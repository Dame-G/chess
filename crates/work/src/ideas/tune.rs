//! The `Tune` executor: a tune idea's SPSA stage, ported from the old harness's `tune.rs`.
//!
//! It builds the idea's branch with the `spsa` feature, reads the tunables the build declares, and scales the clock
//! (copies = games at once; dev and base are one build). Then it runs like OpenBench's fleet: the cores it is granted
//! are split into workers of two games, each worker plays one iteration's game pairs between θ+ and θ−, and each
//! result is applied as it comes back (`core::spsa::Scheduler`). Every update is a `tune_steps` row, a `tune.progress`
//! event and a checkpoint, so a stopped or restarted tune carries on from its last update.
//!
//! When the iterations are done, the rounded values become the new defaults in `src/parameters.rs` on a new branch
//! `tune/<idea id>`, which is built and given its `Bench:` line; that is the idea's version, and the funnel goes on.

use async_trait::async_trait;
use dameless_core::{
    Error, IdeaId,
    events::{Payload, TuneProgress},
    ideas::{Idea, TrialStatus},
    scaling::{self, Scaling},
    spsa::{self, Draw, TuneParam, TuneSpec, TuneStep},
    time::TimeControl,
    version::Line,
    work::{Progress, WorkKind},
};
use dameless_engine::{
    fastchess::{Adjudication, Book, BookFormat, Each, Limit, Player, Round},
    git::CommitFiles,
};
use dameless_hub::{Executor, WorkContext};
use dameless_store::{IdeaUpdate, NewVersion, now};
use futures::{StreamExt, stream::FuturesUnordered};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{Ideas, TrialWork};
use crate::{toolchain::require_tools, versions::VersionRef};

/// Where the tunables a build declares live.
pub const PARAMETERS: &str = "src/parameters.rs";

pub(super) struct TuneExecutor {
    pub ideas: Ideas,
}

/// The meta key holding a tune's parameter names, in the order of its steps' values.
pub(super) fn params_key(idea: IdeaId) -> String {
    format!("tune.{}.params", idea.0)
}

/// Where a tune stands: kept as the work's checkpoint, and rebuilt from the stored steps when there is none.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TuneState {
    params: Vec<TuneParam>,
    applied: u32,
    scaling: Option<Scaling>,
    played: TimeControl,
}

#[async_trait]
impl Executor for TuneExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Tune
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let w: TrialWork = cx.spec()?;
        let ideas = &self.ideas;
        let hub = ideas.hub();
        let idea = ideas.get(w.idea).await?;
        let spec = ideas.spec(w.idea).await?.tune.ok_or_else(|| Error::internal("the idea has no tune settings"))?;
        let mut trial = ideas.trial(w.idea, w.trial).await?;
        if trial.status != TrialStatus::Running {
            trial.status = TrialStatus::Running;
            trial.started_at.get_or_insert_with(now);
            trial.time = Some(spec.time);
            ideas.save_trial(&trial).await?;
        }
        ideas.mark_running(w.idea).await?;
        cx.set_title(format!("Tuning {}", idea.branch)).await?;

        let versions = ideas.versions();
        let mut tuned = versions.resolve(&VersionRef::Name(idea.branch.clone())).await?;
        // Tuned with the net its base plays with (a champion can be code plus a trained net).
        tuned.net = hub.store.version(idea.base).await?.and_then(|v| v.net);
        let (build, bin) = versions.binary_of(&tuned, &["spsa"]).await?;
        let _hold = versions.in_use().hold(build.id, format!("tune:{}", idea.id));
        let tc = versions.toolchain();
        let (fastchess, book) = require_tools(&tc.tools(&hub.config.tools_dir())).map_err(Error::conflict)?;

        let mut state = match cx.checkpoint::<TuneState>() {
            Some(s) => s,
            None => self.start(&cx, &idea, &spec, &bin, build.bench).await?,
        };
        if trial.played.is_none() {
            trial.scaling = state.scaling;
            trial.played = Some(state.played);
            ideas.save_trial(&trial).await?;
        }
        let limit = match state.played {
            TimeControl::Clock { initial_ms, increment_ms } => {
                Limit::Clock { seconds: initial_ms as f64 / 1000.0, increment: increment_ms as f64 / 1000.0 }
            }
            TimeControl::Nodes { nodes } => Limit::Nodes { nodes },
            other => return Err(Error::internal(format!("tunes cannot play {}", other.seconds_text()))),
        };

        let mut scheduler = spsa::Scheduler::new(spec.schedule(), state.params.clone(), state.applied);
        let n = scheduler.params.len();
        let dir = hub.config.games_dir().join(format!("tune-{}", idea.id));
        while scheduler.applied() < spec.iterations {
            if !cx.safe_point(None).await {
                return Err(cx.stopped());
            }
            let want = spec.concurrency.unwrap_or_else(|| hub.ledger.total()).max(1);
            let Some(grant) = cx.cores(1, want).await else { return Err(cx.stopped()) };
            let slots = spsa::workers(grant.cores());
            cx.log(format!("{} workers of {} pairs per iteration on {} cores", slots.len(), spec.pairs, grant.cores()));
            let launch = |worker: usize, batch: spsa::Batch| {
                let round = Round {
                    fastchess: fastchess.clone(),
                    dev: Player { name: "plus".into(), path: bin.clone(), options: batch.plus.clone() },
                    base: Player { name: "minus".into(), path: bin.clone(), options: batch.minus.clone() },
                    each: Each::reckless(limit, spec.hash, 1),
                    concurrency: slots[worker],
                    pairs: spec.pairs,
                    book: Some(Book { path: book.clone(), format: BookFormat::Epd }),
                    adjudication: Adjudication::reckless(),
                    pgn: dir.join(format!("worker-{worker}.pgn")),
                    seed: Some(super::trial::seed()),
                    recover: true,
                    strict: false,
                    sprt: None,
                    rating_interval: 1000,
                };
                let cancel = cx.cancel.clone();
                async move {
                    // A tune's rounds are two games each: they finish rather than wind down.
                    let never = CancellationToken::new();
                    let result = tc.round(&round, &cancel, &never, &mut |_| {}, &mut |_| {}).await;
                    (worker, batch, result)
                }
            };
            let mut running = FuturesUnordered::new();
            for worker in 0..slots.len() {
                if let Some(batch) = scheduler.next(draw(n)) {
                    running.push(launch(worker, batch));
                }
            }
            // Results are applied as they come back and the worker gets the next batch, until a pause is asked: then
            // the batches in flight finish, the cores go back, and the loop waits at the safe point above.
            while let Some((worker, batch, result)) = running.next().await {
                let totals = match result {
                    Ok(r) => r.totals,
                    Err(e) if dameless_engine::is_cancelled(&e) || cx.cancel.is_cancelled() => {
                        return Err(cx.stopped());
                    }
                    Err(e) => return Err(Error::internal(format!("fastchess failed: {e:#}"))),
                };
                let penta = dameless_core::stats::Pentanomial(totals.penta.map(u64::from));
                let result = penta.wins_minus_losses();
                let k = scheduler.apply(&batch, result);
                if !cx.pause_asked()
                    && let Some(next) = scheduler.next(draw(n))
                {
                    running.push(launch(worker, next));
                }
                let values = scheduler.values();
                hub.store
                    .append_tune_step(idea.id, TuneStep { iteration: k, values: values.clone(), result, penta })
                    .await?;
                state.params = scheduler.params.clone();
                state.applied = k;
                cx.save_checkpoint(&state).await?;
                hub.bus.emit(Payload::TuneProgress(TuneProgress { idea: idea.id, iteration: k, values, result }));
                trial.penta = trial.penta.add(&penta);
                trial.wdl = [trial.wdl[0] + totals.wins, trial.wdl[1] + totals.draws, trial.wdl[2] + totals.losses];
                let sides =
                    batch.plus.iter().zip(&batch.minus).map(|((name, hi), (_, lo))| format!("{name} {hi}/{lo}"));
                cx.log(format!(
                    "iteration {k}/{} (drawn at {}, worker {worker}): {}: {:?} → {result:+}",
                    spec.iterations,
                    batch.k,
                    sides.collect::<Vec<_>>().join(", "),
                    penta.0
                ));
                cx.progress(Progress {
                    done: u64::from(k),
                    total: Some(u64::from(spec.iterations)),
                    unit: "iterations".into(),
                    eta_seconds: None,
                    text: Some(format!("{k} of {} iterations", spec.iterations)),
                })
                .await?;
            }
            drop(grant);
            ideas.save_trial(&trial).await?;
        }

        let detail = self.write_branch(&cx, &idea, &tuned, &state.params).await?;
        cx.log(&detail);
        trial.status = TrialStatus::Passed;
        trial.detail = Some(detail.clone());
        trial.ended_at = Some(now());
        ideas.save_trial(&trial).await?;
        Ok(serde_json::json!({ "passed": true, "detail": detail }))
    }
}

impl TuneExecutor {
    /// A fresh tune: the declared tunables, those asked for, and the clock scaled to the machine.
    async fn start(
        &self,
        cx: &WorkContext,
        idea: &Idea,
        spec: &TuneSpec,
        bin: &std::path::Path,
        bench: Option<u64>,
    ) -> Result<TuneState, Error> {
        let hub = self.ideas.hub();
        let tc = self.ideas.versions().toolchain();
        let raw = tc.spsa(bin).await.map_err(|e| Error::internal(format!("listing the tunables failed: {e:#}")))?;
        let declared = spsa::parse_openbench(&raw);
        if declared.is_empty() && spec.params.is_empty() {
            return Err(Error::bad_request(format!("{} declares no tunables (src/parameters.rs)", idea.branch)));
        }
        let tunables = spsa::select(&declared, &spec.params).map_err(Error::bad_request)?;
        cx.log(format!("Tuning {} parameters for {} iterations", tunables.len(), spec.iterations));
        let names: Vec<String> = tunables.iter().map(|t| t.name.clone()).collect();
        hub.store.put_meta(&params_key(idea.id), &names).await?;
        let mut params: Vec<TuneParam> = tunables.into_iter().map(TuneParam::start).collect();
        // A tune restarted without its checkpoint carries on from its last stored step.
        let steps = hub.store.tune_steps(idea.id).await?;
        let applied = steps.last().map_or(0, |s| s.iteration);
        if let Some(last) = steps.last() {
            for (p, v) in params.iter_mut().zip(&last.values) {
                p.value = *v;
            }
        }
        let (scaling, played) = if hub.config.scaling.applies(&spec.time) {
            let copies = spec.concurrency.unwrap_or_else(|| hub.ledger.free().max(1));
            let m = tc.measure(bin, copies, bench, &cx.cancel).await.map_err(|e| {
                if dameless_engine::is_cancelled(&e) {
                    cx.stopped()
                } else {
                    Error::internal(format!("bench failed: {e:#}"))
                }
            })?;
            if !m.deterministic() {
                return Err(Error::internal(format!("{}'s bench is not deterministic under load", idea.branch)));
            }
            let reference = hub.config.scaling.reference_nps;
            let (s, played) = scaling::calibrate(spec.time, reference, m.mean_nps, m.mean_nps, copies);
            cx.log(format!("Playing {} as {} (×{:.2})", spec.time.seconds_text(), played.seconds_text(), s.scale));
            (Some(s), played)
        } else {
            (None, spec.time)
        };
        let state = TuneState { params, applied, scaling, played };
        cx.save_checkpoint(&state).await?;
        Ok(state)
    }

    /// Writes the tuned values into `src/parameters.rs` on `tune/<idea id>`, builds it, adds its `Bench:` line, and makes
    /// it the idea's version. Returns the trial's sentence.
    async fn write_branch(
        &self,
        cx: &WorkContext,
        idea: &Idea,
        tuned: &dameless_core::version::Version,
        params: &[TuneParam],
    ) -> Result<String, Error> {
        let versions = self.ideas.versions();
        let store = &versions.hub().store;
        let repo = versions.repo();
        let git = |e: anyhow::Error| Error::internal(format!("git: {e:#}"));
        let branch = format!("tune/{}", idea.id);
        let source = repo.read_file(&tuned.commit, PARAMETERS).await.map_err(git)?;
        let (source, changes) = spsa::rewrite_defaults(&source, params).map_err(Error::internal)?;
        for line in &changes {
            cx.log(line);
        }
        if !repo.has_branch(&branch).await {
            if changes.is_empty() {
                return Err(Error::internal("no parameter moved from its default: nothing to apply"));
            }
            let message = format!(
                "Apply SPSA tune of idea {}: {}\n\n{} parameters, {} iterations.\n\n{}\n",
                idea.id,
                idea.title,
                params.len(),
                store.tune_steps(idea.id).await?.len(),
                changes.join("\n")
            );
            let files: [(&str, &[u8]); 1] = [(PARAMETERS, source.as_bytes())];
            repo.commit_files(&CommitFiles { branch: &branch, from: &tuned.commit, files: &files, message: &message })
                .await
                .map_err(git)?;
        }
        let commit = repo.resolve(&branch).await.map_err(git)?;
        let version = store
            .upsert_version(NewVersion {
                name: branch.clone(),
                commit: commit.sha.clone(),
                branch: Some(branch.clone()),
                line: Line::Idea { idea: idea.id },
                parent: Some(idea.base),
                bench: commit.bench,
                net: None,
            })
            .await?;
        // The Bench: line is the commit's own: measured with its own net.
        let (build, _) = versions.binary_of(&version, &[]).await?;
        let base_net = store.version(idea.base).await?.and_then(|v| v.net);
        let bench = build.bench.ok_or_else(|| Error::internal("the tuned build has no bench"))?;
        let commit = if commit.bench == Some(bench) {
            commit.sha
        } else {
            let message = format!("{}\n\nBench: {bench}\n", commit.message.trim_end());
            let sha = repo.reword(&branch, &message).await.map_err(git)?;
            versions.alias_build(&build, version.id, &sha).await?;
            sha
        };
        let version = store
            .upsert_version(NewVersion {
                name: branch.clone(),
                commit,
                branch: Some(branch.clone()),
                line: Line::Idea { idea: idea.id },
                parent: Some(idea.base),
                bench: if base_net.is_some() { None } else { Some(bench) },
                net: base_net,
            })
            .await?;
        versions.hub().bus.emit(Payload::VersionUpserted(Box::new(version.clone())));
        store.update_idea(idea.id, IdeaUpdate { version: Some(version.id), ..Default::default() }).await?;
        self.ideas.emit_idea(idea.id).await?;
        Ok(format!("{} parameters moved, written to {branch} with Bench: {bench}", changes.len()))
    }
}

/// A batch's randomness.
fn draw(n: usize) -> Draw {
    let mut bytes = vec![0u8; n * 8];
    let _ = getrandom::fill(&mut bytes);
    let words: Vec<u64> = bytes.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c)).collect();
    Draw::from_words(&words)
}
