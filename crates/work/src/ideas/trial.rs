//! The `Trial` executor: one stage of an idea that is not a tune.
//!
//! - **Fingerprint:** both versions built, their benches compared as the kind wants (changed, or identical), and each
//!   against its commit's `Bench:` line.
//! - **Speed:** paired `bench` runs, alternating which goes first, as the old nonfunctional stage.
//! - **Games:** the clock is scaled once at the start (`copies` = planned games at once × threads, both builds; a copy
//!   with another node count fails the trial as non-deterministic), then rounds: cores from the ledger, one fastchess
//!   round of about four pairs per core, the round's pairs added to the trial, the SPRT checked, the games stored, a
//!   safe point. A stopped or restarted trial carries on from the rounds it has, which live on the trial itself.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use dameless_core::{
    Error, TrialId,
    chess::pgn,
    events::{Payload, TrialProgress},
    game::{GameStatus, Seat, Source},
    ideas::{self as funnel, Bench, FingerprintRule, Idea, Preset, StageRule, StageVerdict, Trial, TrialStatus},
    scaling,
    stats::{Pentanomial, Verdict},
    time::TimeControl,
    version::{Build, Version},
    work::{Progress, WorkKind},
};
use dameless_engine::fastchess::{self, Adjudication, Book, BookFormat, Each, Limit, Player, Round};
use dameless_hub::{Executor, WorkContext};
use dameless_store::{NewGame, now};

use super::{Ideas, TrialWork};
use crate::toolchain::require_tools;

pub(super) struct TrialExecutor {
    pub ideas: Ideas,
}

/// trial.progress is sent at most this often while a round plays.
const PROGRESS_EVERY: Duration = Duration::from_secs(1);

/// The two sides of a trial, built.
struct Sides {
    base: (Version, Build, PathBuf),
    dev: (Version, Build, PathBuf),
}

#[async_trait]
impl Executor for TrialExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Trial
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let w: TrialWork = cx.spec()?;
        let ideas = &self.ideas;
        let idea = ideas.get(w.idea).await?;
        let spec = ideas.spec(w.idea).await?;
        let plan = *spec.funnel.get(w.index).ok_or_else(|| Error::internal("the trial is not in its idea's funnel"))?;
        let mut trial = ideas.trial(w.idea, w.trial).await?;
        if trial.status != TrialStatus::Running {
            trial.status = TrialStatus::Running;
            trial.started_at.get_or_insert_with(now);
            ideas.save_trial(&trial).await?;
        }
        ideas.mark_running(w.idea).await?;
        cx.set_title(format!("Trying {} · {}", idea.branch, trial.stage.label())).await?;
        cx.log(format!("Idea {}: {} · {}", idea.id, idea.title, trial.stage.label()));

        let sides = self.sides(&idea).await?;
        let _hold = [
            ideas.versions().in_use().hold(sides.base.1.id, format!("trial:{}", trial.id)),
            ideas.versions().in_use().hold(sides.dev.1.id, format!("trial:{}", trial.id)),
        ];
        let verdict = match plan.rule {
            StageRule::Fingerprint { rule } => fingerprint(rule, &sides),
            StageRule::Speed { runs } => self.speed(&cx, &sides, runs).await?,
            StageRule::Games { preset, bounds, pairs } => {
                self.games(&cx, &idea, &mut trial, &sides, preset, bounds, pairs).await?
            }
            StageRule::Tune => return Err(Error::internal("a tune stage runs as tune work")),
            StageRule::Train => return Err(Error::internal("a training stage runs as train work")),
        };
        cx.log(format!("{}: {}", if verdict.passed { "Passed" } else { "Failed" }, verdict.detail));
        trial.status = if verdict.passed { TrialStatus::Passed } else { TrialStatus::Failed };
        trial.detail = Some(verdict.detail.clone());
        trial.ended_at = Some(now());
        ideas.save_trial(&trial).await?;
        Ok(serde_json::json!({ "passed": verdict.passed, "detail": verdict.detail }))
    }
}

impl TrialExecutor {
    async fn sides(&self, idea: &Idea) -> Result<Sides, Error> {
        let versions = self.ideas.versions();
        let store = &versions.hub().store;
        let base =
            store.version(idea.base).await?.ok_or_else(|| Error::not_found("The idea's base version is gone"))?;
        let dev_id = idea.version.ok_or_else(|| Error::internal("The idea has no version to test yet"))?;
        let dev = store.version(dev_id).await?.ok_or_else(|| Error::not_found("The idea's version is gone"))?;
        let (base_build, base_bin) = versions.binary_of(&base, &[]).await?;
        let (dev_build, dev_bin) = versions.binary_of(&dev, &[]).await?;
        Ok(Sides { base: (base, base_build, base_bin), dev: (dev, dev_build, dev_bin) })
    }

    /// Paired bench runs, alternating which build goes first so drift in the machine cancels out.
    async fn speed(&self, cx: &WorkContext, sides: &Sides, runs: u32) -> Result<StageVerdict, Error> {
        let tc = self.ideas.versions().toolchain();
        let Some(_grant) = cx.cores(1, 1).await else { return Err(cx.stopped()) };
        let bench = |path: &Path| {
            let path = path.to_owned();
            async move {
                tc.bench(&path).await.map(|b| b.nps as f64).map_err(|e| Error::internal(format!("bench failed: {e:#}")))
            }
        };
        let mut pairs = Vec::new();
        for i in 0..runs.max(3) {
            if cx.cancel.is_cancelled() {
                return Err(cx.stopped());
            }
            let (b, d) = if i % 2 == 0 {
                let b = bench(&sides.base.2).await?;
                (b, bench(&sides.dev.2).await?)
            } else {
                let d = bench(&sides.dev.2).await?;
                (bench(&sides.base.2).await?, d)
            };
            cx.log(format!("speed run {}: base {b} nps, the change {d} nps", i + 1));
            pairs.push((b, d));
        }
        Ok(funnel::speed(&pairs))
    }

    #[allow(clippy::too_many_arguments)]
    async fn games(
        &self,
        cx: &WorkContext,
        idea: &Idea,
        trial: &mut Trial,
        sides: &Sides,
        preset: Preset,
        bounds: Option<[f64; 2]>,
        pairs: Option<u32>,
    ) -> Result<StageVerdict, Error> {
        let ideas = &self.ideas;
        let hub = ideas.hub();
        let tc = ideas.versions().toolchain();
        let settings = preset.settings();
        let (fastchess, book) = require_tools(&tc.tools(&hub.config.tools_dir())).map_err(Error::conflict)?;

        if trial.played.is_none() {
            trial.time = Some(settings.time);
            if hub.config.scaling.applies(&settings.time) {
                let games_at_once = (hub.ledger.free().max(2) / settings.threads).max(1);
                let copies = games_at_once * settings.threads;
                cx.log(format!("Scaling the clock: {copies} benches at once of each build"));
                let measure = |side: &(Version, Build, PathBuf)| {
                    let (bin, nodes) = (side.2.clone(), side.1.bench);
                    async move {
                        tc.measure(&bin, copies, nodes, &cx.cancel).await.map_err(|e| {
                            if dameless_engine::is_cancelled(&e) {
                                cx.stopped()
                            } else {
                                Error::internal(format!("bench failed: {e:#}"))
                            }
                        })
                    }
                };
                let base = measure(&sides.base).await?;
                let dev = measure(&sides.dev).await?;
                for (who, m) in [("base", &base), ("the change", &dev)] {
                    if !m.deterministic() {
                        let counts: Vec<String> = m.copies.iter().map(|c| c.nodes.to_string()).collect();
                        return Ok(StageVerdict {
                            passed: false,
                            detail: format!(
                                "{who}'s bench is not deterministic: {} copies gave {} nodes, expected {}",
                                m.copies.len(),
                                counts.join(", "),
                                m.nodes
                            ),
                        });
                    }
                }
                let (s, played) = scaling::calibrate(
                    settings.time,
                    hub.config.scaling.reference_nps,
                    base.mean_nps,
                    dev.mean_nps,
                    copies,
                );
                cx.log(format!(
                    "Playing {} as {} (×{:.2})",
                    settings.time.seconds_text(),
                    played.seconds_text(),
                    s.scale
                ));
                trial.scaling = Some(s);
                trial.played = Some(played);
            } else {
                trial.played = Some(settings.time);
            }
            ideas.save_trial(trial).await?;
        }
        let played = trial.played.unwrap_or(settings.time);
        let limit = match played {
            TimeControl::Clock { initial_ms, increment_ms } => {
                Limit::Clock { seconds: initial_ms as f64 / 1000.0, increment: increment_ms as f64 / 1000.0 }
            }
            TimeControl::Nodes { nodes } => Limit::Nodes { nodes },
            other => return Err(Error::internal(format!("trials cannot play {}", other.seconds_text()))),
        };

        let store = &hub.store;
        let (mut rounds, _) = store.trial_rounds(trial.id).await?.unwrap_or((0, None));
        let mut stored = store.count_trial_games(trial.id).await?;
        let dir = hub.config.games_dir().join(format!("trial-{}", trial.id));
        let seats = (seat(&sides.dev), seat(&sides.base));
        let started = Instant::now();
        let played_before = trial.penta.pairs();
        loop {
            if let Some(b) = bounds {
                let s = funnel::sprt(b);
                if s.verdict(s.llr(&trial.penta)) != Verdict::Continue {
                    break;
                }
            }
            let remaining = pairs.map(|p| u64::from(p).saturating_sub(trial.penta.pairs()));
            if remaining == Some(0) {
                break;
            }
            if !cx.safe_point(None).await {
                return Err(cx.stopped());
            }
            let min = settings.threads.max(2);
            let Some(grant) = cx.cores(min, hub.ledger.total().max(min)).await else { return Err(cx.stopped()) };
            let concurrency = (grant.cores() / settings.threads).max(1);
            let mut n = u64::from(4 * concurrency);
            if let Some(r) = remaining {
                n = n.min(r);
            }
            let round = Round {
                fastchess: fastchess.clone(),
                dev: Player { name: "dev".into(), path: sides.dev.2.clone(), options: vec![] },
                base: Player { name: "base".into(), path: sides.base.2.clone(), options: vec![] },
                each: Each::reckless(limit, settings.hash, settings.threads),
                concurrency,
                pairs: n as u32,
                book: Some(Book { path: book.clone(), format: BookFormat::Epd }),
                adjudication: Adjudication::reckless(),
                pgn: dir.join(format!("round-{}.pgn", rounds + 1)),
                seed: Some(seed()),
                recover: true,
                strict: false,
                sprt: None,
                rating_interval: 10,
            };
            cx.log(format!("Round {}: {n} pairs, {concurrency} games at once", rounds + 1));
            let before = trial.penta;
            let mut last_sent: Option<Instant> = None;
            let (bus, id, idea_id, wdl) = (hub.bus.clone(), trial.id, idea.id, trial.wdl);
            let mut on_progress = |p: fastchess::Progress| {
                if let fastchess::Progress::Totals(t) = p
                    && last_sent.is_none_or(|s| s.elapsed() >= PROGRESS_EVERY)
                {
                    last_sent = Some(Instant::now());
                    let penta = before.add(&penta_of(t.penta));
                    let (llr, elo, _) = Trial::statistics(&penta, bounds);
                    let wdl = [wdl[0] + t.wins, wdl[1] + t.draws, wdl[2] + t.losses];
                    bus.emit(Payload::TrialProgress(TrialProgress { id, idea: idea_id, penta, wdl, llr, elo }));
                }
            };
            let mut log = |line: &str| cx.log(line);
            let result = match tc.round(&round, &cx.cancel, &mut on_progress, &mut log).await {
                Ok(r) => r,
                Err(e) if dameless_engine::is_cancelled(&e) || cx.cancel.is_cancelled() => return Err(cx.stopped()),
                Err(e) => return Err(Error::internal(format!("fastchess failed: {e:#}"))),
            };
            drop(grant);
            let t = result.totals;
            trial.penta = trial.penta.add(&penta_of(t.penta));
            trial.wdl = [trial.wdl[0] + t.wins, trial.wdl[1] + t.draws, trial.wdl[2] + t.losses];
            (trial.llr, trial.elo, trial.nelo) = Trial::statistics(&trial.penta, bounds);
            rounds += 1;
            ideas.save_trial(trial).await?;
            store.set_trial_rounds(trial.id, rounds, Some(dir.display().to_string())).await?;
            stored += self.store_games(trial.id, stored, &result.pgn, &seats, played).await?;
            cx.save_checkpoint(&serde_json::json!({ "rounds": rounds, "pairs": trial.penta.pairs() })).await?;
            hub.bus.emit(Payload::TrialProgress(TrialProgress {
                id: trial.id,
                idea: idea.id,
                penta: trial.penta,
                wdl: trial.wdl,
                llr: trial.llr,
                elo: trial.elo,
            }));
            let done = trial.penta.pairs();
            let text = match (trial.llr, trial.elo) {
                (Some(l), _) => format!("{done} pairs, LLR {:.2} of {:.2}", l.value, l.upper),
                (None, Some(e)) => format!("{done} pairs, {} Elo", e.text()),
                _ => format!("{done} pairs"),
            };
            let total = pairs.map(u64::from);
            let eta =
                total.and_then(|t| Progress::eta(done - played_before, t - played_before, started.elapsed().as_secs()));
            cx.progress_now(Progress { done, total, unit: "pairs".into(), eta_seconds: eta, text: Some(text) }).await?;
        }
        Ok(funnel::games(&trial.penta, bounds, trial.scaling.map(|s| (played, s.scale))))
    }

    /// Reads a round's PGN into games (`Source::Trial`, numbered on from `first`); returns how many were stored.
    async fn store_games(
        &self,
        trial: TrialId,
        first: u32,
        pgn_path: &Path,
        (dev, base): &(Seat, Seat),
        played: TimeControl,
    ) -> Result<u32, Error> {
        let hub = self.ideas.hub();
        let Ok(text) = std::fs::read_to_string(pgn_path) else { return Ok(0) };
        let games = pgn::read(&text).map_err(Error::from)?;
        let when = now();
        let new: Vec<NewGame> = games
            .into_iter()
            .enumerate()
            .map(|(i, g)| {
                let dev_white = g.header("White") != Some("base");
                let (white, black) = if dev_white { (dev.clone(), base.clone()) } else { (base.clone(), dev.clone()) };
                let end = g.end();
                NewGame {
                    white,
                    black,
                    source: Source::Trial { trial, index: first + i as u32 },
                    time: played,
                    status: if end.as_ref().is_some_and(|e| e.outcome.is_some()) {
                        GameStatus::Finished
                    } else {
                        GameStatus::Aborted
                    },
                    outcome: end.as_ref().and_then(|e| e.outcome),
                    ending: end.map(|e| e.ending),
                    opening: None,
                    start_fen: g.start_fen,
                    moves: g.moves,
                    started_at: when.clone(),
                    ended_at: Some(when.clone()),
                    url: None,
                }
            })
            .collect();
        let count = new.len() as u32;
        if count == 0 {
            return Ok(0);
        }
        let ids = hub.store.insert_games(new).await?;
        // One summary a round is enough: the Games rail folds a trial's games into one row.
        if let Some(last) = ids.last()
            && let Some(summary) = hub.store.game_summary(*last).await?
        {
            hub.bus.emit(Payload::GameUpserted(Box::new(summary)));
        }
        Ok(count)
    }
}

/// The fingerprint stage from the two builds' benches and their commits' `Bench:` lines.
fn fingerprint(rule: FingerprintRule, sides: &Sides) -> StageVerdict {
    let bench =
        |(v, b, _): &(Version, Build, PathBuf)| Bench { measured: b.bench.unwrap_or_default(), expected: v.bench };
    funnel::fingerprint(rule, bench(&sides.base), bench(&sides.dev))
}

/// A Dameless seat at a version and build.
fn seat((v, b, _): &(Version, Build, PathBuf)) -> Seat {
    Seat { version: Some(v.id), build: Some(b.id), ..Seat::dameless(&v.name) }
}

fn penta_of(p: [u32; 5]) -> Pentanomial {
    Pentanomial(p.map(u64::from))
}

/// A random seed for a round's opening order.
pub(super) fn seed() -> u64 {
    let mut bytes = [0u8; 8];
    let _ = getrandom::fill(&mut bytes);
    u64::from_le_bytes(bytes)
}
