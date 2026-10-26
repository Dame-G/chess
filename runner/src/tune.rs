//! SPSA tuning: a build's declared tunables (Reckless's `define!` macro, built with the `spsa` feature) are moved by
//! simultaneous perturbation, the way OpenBench tunes. Each iteration plays a few game pairs between the parameters
//! nudged one way and the other, and steps the parameters toward the side that scored better. A tune is a job in the
//! queue, so it never shares the machine with a test; its parameters after every iteration are kept, for charts.
//!
//! ## Filling the machine
//!
//! An iteration is small (OpenBench's default is 8 pairs, 16 games), so one fastchess run per iteration at the tune's
//! full concurrency spends much of each iteration waiting on its last few games with most slots idle. Instead the
//! tune runs the way OpenBench's fleet does: its `concurrency` slots are split into workers of two (one game pair at a
//! time, both games at once), and each worker plays one iteration's batch of `pairs_per_iteration` pairs. A batch is
//! drawn when a worker frees up — its own random Δ, θ± around θ as it stands at that moment, and `c_k`, `R_k` for
//! `k` = updates applied so far + 1, as OpenBench computes the iteration from results received — and its step
//! `R_k · c_k · (W − L) · Δ` is added to θ as it stands when the batch finishes ([`Scheduler`]). So several batches
//! are in flight at once, each a few updates behind, exactly as with OpenBench's many workers; every update is still
//! one stored iteration (numbered in the order they are applied) with its own `tune.progress` event, and a tune of
//! `N` iterations plays `N` batches. Each worker writes its games to its own `games-<worker>.pgn`.
//!
//! Applying a tune bakes its current values into the engine as new defaults on a branch of the fork, with the right
//! `Bench:` line, and can queue the confirmation OpenBench-style tuning asks for: a gainer at SPRT [0, 4].

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, bail};
use futures::stream::{FuturesUnordered, StreamExt};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{
    api::AppState,
    engine::{build, fastchess, git},
    experiments::{self, NewExperiment},
    funnel::{self, Kind, MatchSpec, Preset, TimeControl},
    jobs::{self, JobSpec},
    proc,
    stats::Pentanomial,
    tools,
};

/// The SPSA arithmetic, as OpenBench does it.
///
/// OpenBench (`OpenBench/workloads/get_workload.py` and `submit_results.py`, and the SPSA set-up in
/// `OpenBench/views.py`), with `N` iterations, `A = A_ratio · N` and each parameter's `c_end` and `r_end`:
///
/// ```text
/// c       = c_end · N^γ                      a       = r_end · c_end² · (A + N)^α
/// c_k     = c / k^γ                          a_k     = a / (A + k)^α           (k = 1, 2, … N)
/// R_k     = a_k / c_k²
/// θ±      = clamp(θ ± c_k · Δ), rounded for integers, Δ = ±1 per parameter at random
/// θ      ← clamp(θ + R_k · c_k · (W − L) · Δ)
/// ```
///
/// where `W − L` counts games won minus games lost by θ+ against θ−. So `c_k` shrinks to `c_end` and `R_k` to `r_end`
/// at the last iteration, and the step is the usual SPSA gradient estimate `a_k · (W − L) / c_k` along Δ.
pub mod spsa {
    use serde::{Deserialize, Serialize};
    use utoipa::ToSchema;

    /// OpenBench's defaults.
    pub const ALPHA: f64 = 0.602;
    pub const GAMMA: f64 = 0.101;
    pub const A_RATIO: f64 = 0.1;
    pub const R_END: f64 = 0.002;

    /// One tunable: its range and step sizes, and where it starts.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
    pub struct Param {
        pub name: String,
        pub start: f64,
        pub min: f64,
        pub max: f64,
        /// The perturbation at the last iteration.
        pub c_end: f64,
        /// The learning rate `a_k / c_k²` at the last iteration.
        pub r_end: f64,
    }

    /// The schedule of a tune: how `c_k` and `R_k` shrink over its iterations.
    #[derive(Clone, Copy, Debug)]
    pub struct Schedule {
        pub iterations: u32,
        pub alpha: f64,
        pub gamma: f64,
        pub a_ratio: f64,
    }

    impl Schedule {
        fn big_a(&self) -> f64 {
            self.a_ratio * self.iterations as f64
        }

        /// `c_k` for `p` at iteration `k`, counted from 1.
        pub fn c(&self, p: &Param, k: u32) -> f64 {
            let c = p.c_end * (self.iterations as f64).powf(self.gamma);
            c / (k as f64).powf(self.gamma)
        }

        /// `R_k = a_k / c_k²` for `p` at iteration `k`, counted from 1.
        pub fn r(&self, p: &Param, k: u32) -> f64 {
            let a = p.r_end * p.c_end * p.c_end * (self.big_a() + self.iterations as f64).powf(self.alpha);
            let a_k = a / (self.big_a() + k as f64).powf(self.alpha);
            a_k / self.c(p, k).powi(2)
        }
    }

    /// The values the two engines play with: θ + c·Δ and θ − c·Δ, clamped and rounded (every tunable is an integer).
    pub fn perturb(p: &Param, value: f64, c: f64, flip: f64) -> (f64, f64) {
        let side = |v: f64| v.clamp(p.min, p.max).round();
        (side(value + c * flip), side(value - c * flip))
    }

    /// The new value after an iteration whose θ+ side won `result` more games than it lost.
    pub fn step(p: &Param, value: f64, r: f64, c: f64, flip: f64, result: i64) -> f64 {
        (value + r * c * result as f64 * flip).clamp(p.min, p.max)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn param() -> Param {
            Param { name: "x".into(), start: 100.0, min: 50.0, max: 150.0, c_end: 5.0, r_end: 0.002 }
        }

        fn schedule() -> Schedule {
            Schedule { iterations: 1000, alpha: ALPHA, gamma: GAMMA, a_ratio: A_RATIO }
        }

        #[test]
        fn ends_at_c_end_and_r_end() {
            let (p, s) = (param(), schedule());
            assert!((s.c(&p, 1000) - 5.0).abs() < 1e-9);
            assert!((s.r(&p, 1000) - 0.002).abs() < 1e-12);
        }

        #[test]
        fn starts_wider_and_shrinks() {
            let (p, s) = (param(), schedule());
            // c_1 = c_end · N^γ = 5 · 1000^0.101.
            assert!((s.c(&p, 1) - 5.0 * 1000f64.powf(0.101)).abs() < 1e-9);
            assert!(s.c(&p, 1) > s.c(&p, 500) && s.c(&p, 500) > s.c(&p, 1000));
            // a_1 / a_N = ((A + N) / (A + 1))^α with A = 100.
            let a = |k| s.r(&p, k) * s.c(&p, k).powi(2);
            assert!((a(1) / a(1000) - (1100f64 / 101.0).powf(0.602)).abs() < 1e-9);
        }

        #[test]
        fn perturbs_both_ways_within_bounds() {
            let p = param();
            assert_eq!(perturb(&p, 100.0, 6.4, 1.0), (106.0, 94.0));
            assert_eq!(perturb(&p, 100.0, 6.4, -1.0), (94.0, 106.0));
            assert_eq!(perturb(&p, 148.0, 6.4, 1.0), (150.0, 142.0));
        }

        #[test]
        fn steps_toward_the_winning_side() {
            let p = param();
            // θ+ was θ + c (flip +1) and won 4 more games: θ moves up by R·c·4.
            assert!((step(&p, 100.0, 0.01, 6.0, 1.0, 4) - 100.24).abs() < 1e-9);
            // θ+ was θ − c (flip −1) and won: θ moves down.
            assert!((step(&p, 100.0, 0.01, 6.0, -1.0, 4) - 99.76).abs() < 1e-9);
            assert_eq!(step(&p, 149.9, 1.0, 6.0, 1.0, 10), 150.0);
        }
    }
}

pub use spsa::Param;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TuneStatus {
    Queued,
    Running,
    /// Every iteration was played.
    Done,
    Failed,
    Cancelled,
}

impl TuneStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }
}

/// A parameter being tuned, with where it is now.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct TuneParam {
    #[serde(flatten)]
    pub param: Param,
    /// The current value, unrounded.
    pub value: f64,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Tune {
    pub id: i64,
    pub name: String,
    /// The ref (or build id) that was asked for.
    #[serde(rename = "ref")]
    pub refname: String,
    /// The `spsa` build being tuned, once built.
    pub build: Option<i64>,
    /// The parameters, in the engine's order, with their current values. Empty until the build is made (when the
    /// request left them to the engine's declared list).
    pub params: Vec<TuneParam>,
    /// Time control, hash and threads of each iteration's games; `pairs` is the pairs per iteration.
    pub spec: MatchSpec,
    pub iterations: u32,
    pub pairs_per_iteration: u32,
    /// Games played at once.
    pub concurrency: u32,
    pub alpha: f64,
    pub gamma: f64,
    pub a_ratio: f64,
    /// Iterations finished.
    pub iteration: u32,
    pub status: TuneStatus,
    /// Why it stopped, once finished.
    pub summary: Option<String>,
    pub job: Option<i64>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

/// One finished iteration.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TuneIteration {
    /// Counted from 1.
    pub iteration: u32,
    /// Every parameter after this iteration's step, in the order of the tune's `params`.
    pub values: Vec<f64>,
    /// θ+ against θ−, as game pairs from θ+'s side.
    pub penta: Pentanomial,
    /// Games won minus games lost by θ+.
    pub result: i64,
}

/// A parameter to tune. Anything left out comes from the engine's declaration (`spsa` command), so `{ "name": "x" }`
/// tunes `x` as declared.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct ParamInput {
    pub name: String,
    pub start: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub c_end: Option<f64>,
    pub r_end: Option<f64>,
}

/// A tune to run.
#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct NewTune {
    /// A short name. Defaults to the build.
    pub name: Option<String>,
    /// The ref to tune (built with the `spsa` feature), or the id of a build whose commit to tune.
    pub build: String,
    /// The parameters to tune. Left out or empty, every tunable the engine declares.
    #[serde(default)]
    pub params: Vec<ParamInput>,
    /// Default 1000.
    pub iterations: Option<u32>,
    /// Game pairs per iteration. Default 8, as OpenBench batches SPSA.
    pub pairs_per_iteration: Option<u32>,
    /// The time control and hash of the games. Default `stc` (8+0.08, 16 MiB).
    pub preset: Option<Preset>,
    /// Play this many nodes per move instead of the preset's clock.
    pub nodes: Option<u64>,
    /// Hash in MiB, instead of the preset's.
    pub hash: Option<u32>,
    /// Games at once. Default: the machine's threads less two, divided by the preset's threads.
    pub concurrency: Option<u32>,
    /// Default 0.602.
    pub alpha: Option<f64>,
    /// Default 0.101.
    pub gamma: Option<f64>,
    /// `A` as a fraction of the iterations. Default 0.1.
    pub a_ratio: Option<f64>,
}

/// What `apply` should do with a tune's values.
#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct ApplyTune {
    /// The branch to create in the fork. Default `tune/<id>`; it must not exist yet.
    pub branch: Option<String>,
    /// Queue a gainer experiment of the branch against `base` at SPRT [0, 4]. Default true.
    pub experiment: Option<bool>,
    /// The experiment's baseline. Default `main`.
    pub base: Option<String>,
    /// Add an LTC [0, 4] confirmation to the experiment.
    #[serde(default)]
    pub ltc: bool,
    /// Push the branch to the fork's `origin`.
    #[serde(default)]
    pub push: bool,
}

const COLUMNS: &str = "id, name, ref, build_id, params, spec, iterations, pairs, concurrency, alpha, gamma, a_ratio, \
                       iteration, status, summary, job_id, created_at, finished_at";

fn json<T: serde::de::DeserializeOwned>(r: &Row, i: usize) -> rusqlite::Result<T> {
    serde_json::from_str(&r.get::<_, String>(i)?)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e)))
}

fn from_row(r: &Row) -> rusqlite::Result<Tune> {
    Ok(Tune {
        id: r.get(0)?,
        name: r.get(1)?,
        refname: r.get(2)?,
        build: r.get(3)?,
        params: json(r, 4)?,
        spec: json(r, 5)?,
        iterations: r.get(6)?,
        pairs_per_iteration: r.get(7)?,
        concurrency: r.get(8)?,
        alpha: r.get(9)?,
        gamma: r.get(10)?,
        a_ratio: r.get(11)?,
        iteration: r.get(12)?,
        status: TuneStatus::parse(&r.get::<_, String>(13)?),
        summary: r.get(14)?,
        job: r.get(15)?,
        created_at: r.get(16)?,
        finished_at: r.get(17)?,
    })
}

pub fn list(c: &Connection, limit: i64) -> Result<Vec<Tune>> {
    let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM tunes ORDER BY id DESC LIMIT ?1"))?;
    Ok(stmt.query_map([limit], from_row)?.collect::<rusqlite::Result<_>>()?)
}

pub fn get(c: &Connection, id: i64) -> Result<Option<Tune>> {
    Ok(c.query_row(&format!("SELECT {COLUMNS} FROM tunes WHERE id = ?1"), [id], from_row).optional()?)
}

pub fn history(c: &Connection, id: i64) -> Result<Vec<TuneIteration>> {
    let mut stmt =
        c.prepare("SELECT iteration, vals, penta, result FROM tune_iterations WHERE tune_id = ?1 ORDER BY iteration")?;
    Ok(stmt
        .query_map([id], |r| {
            Ok(TuneIteration {
                iteration: r.get(0)?,
                values: json(r, 1)?,
                penta: Pentanomial(json(r, 2)?),
                result: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// Queues a tune.
pub async fn submit(state: &Arc<AppState>, new: NewTune) -> Result<Tune> {
    let iterations = new.iterations.unwrap_or(1000);
    let pairs = new.pairs_per_iteration.unwrap_or(8);
    if iterations == 0 || pairs == 0 {
        bail!("a tune needs at least one iteration of at least one pair");
    }
    let mut spec = new.preset.unwrap_or(Preset::Stc).spec(None, Some(pairs));
    if let Some(nodes) = new.nodes {
        spec.tc = TimeControl::Nodes { nodes };
    }
    if let Some(hash) = new.hash {
        spec.hash = hash;
    }
    let concurrency = new
        .concurrency
        .unwrap_or_else(|| (state.machine.threads.saturating_sub(2) as u32 / spec.threads).max(1))
        .max(1);
    let (alpha, gamma, a_ratio) =
        (new.alpha.unwrap_or(spsa::ALPHA), new.gamma.unwrap_or(spsa::GAMMA), new.a_ratio.unwrap_or(spsa::A_RATIO));
    let name = new.name.clone().unwrap_or_else(|| new.build.clone());
    let id = state
        .store
        .call(move |c| {
            c.execute(
                "INSERT INTO tunes (name, ref, inputs, params, spec, iterations, pairs, concurrency, alpha, gamma,
                 a_ratio, iteration, status)
                 VALUES (?1, ?2, ?3, '[]', ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 'queued')",
                params![
                    name,
                    new.build,
                    serde_json::to_string(&new.params)?,
                    serde_json::to_string(&spec)?,
                    iterations,
                    pairs,
                    concurrency,
                    alpha,
                    gamma,
                    a_ratio
                ],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    queue(state, id).await
}

/// Gives a tune a job. A tune that stopped (failed or cancelled) carries on from its last iteration.
async fn queue(state: &Arc<AppState>, id: i64) -> Result<Tune> {
    let job = jobs::submit(state, JobSpec::Tune { id }).await?;
    update(state, id, move |c| {
        c.execute(
            "UPDATE tunes SET job_id = ?2, status = 'queued', summary = NULL, finished_at = NULL WHERE id = ?1",
            params![id, job.id],
        )?;
        Ok(())
    })
    .await
}

/// Queues a stopped tune again, to carry on where it stopped.
pub async fn resume(state: &Arc<AppState>, id: i64) -> Result<Tune> {
    let tune = state.store.call(move |c| get(c, id)).await?.context("no such tune")?;
    match tune.status {
        TuneStatus::Failed | TuneStatus::Cancelled => queue(state, id).await,
        _ => bail!("tune {id} is {}: only a failed or cancelled tune can resume", tune.status.as_str()),
    }
}

pub async fn cancel(state: &Arc<AppState>, id: i64) -> Result<Tune> {
    let tune = state.store.call(move |c| get(c, id)).await?.context("no such tune")?;
    if let Some(job) = tune.job {
        jobs::cancel(state, job).await?;
    }
    if tune.status == TuneStatus::Queued {
        return finish(state, id, TuneStatus::Cancelled, "cancelled before it started").await;
    }
    Ok(tune)
}

fn announce(state: &AppState, t: &Tune) {
    state.events.emit("tune.updated", serde_json::to_value(t).unwrap());
}

async fn update(state: &AppState, id: i64, f: impl FnOnce(&Connection) -> Result<()> + Send + 'static) -> Result<Tune> {
    let t = state
        .store
        .call(move |c| {
            f(c)?;
            Ok(get(c, id)?.expect("tune exists"))
        })
        .await?;
    announce(state, &t);
    Ok(t)
}

/// Marks a tune finished, unless it already is, and says so on `tune.finished`.
pub async fn finish(state: &AppState, id: i64, status: TuneStatus, summary: &str) -> Result<Tune> {
    let summary = summary.to_owned();
    let t = update(state, id, move |c| {
        c.execute(
            "UPDATE tunes SET status = ?2, summary = ?3, finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1 AND status IN ('queued', 'running')",
            params![id, status.as_str(), summary],
        )?;
        Ok(())
    })
    .await?;
    state.events.emit(
        "tune.finished",
        serde_json::json!({ "id": id, "status": t.status, "summary": t.summary, "iteration": t.iteration }),
    );
    Ok(t)
}

pub struct Run<'a> {
    pub state: &'a Arc<AppState>,
    pub cancel: &'a CancellationToken,
    pub log: &'a (dyn Fn(&str) + Sync),
}

/// Runs a tune's remaining iterations. Errors (a failed build, a crashed engine) propagate.
pub async fn run(r: Run<'_>, id: i64) -> Result<TuneStatus> {
    let state = r.state;
    let tune = update(state, id, move |c| {
        c.execute("UPDATE tunes SET status = 'running' WHERE id = ?1", [id])?;
        Ok(())
    })
    .await?;
    (r.log)(&format!("tune {id}: {} ({})", tune.name, tune.refname));

    let refname = resolve_ref(state, &tune.refname).await?;
    let cx = build::BuildContext {
        data: &state.config.data,
        reckless: &state.config.reckless,
        store: &state.store,
        cancel: r.cancel,
        log: r.log,
    };
    let b = build::build_spsa(cx, &refname).await.context("building with the spsa feature")?;
    let bin = build::binary(&state.config.data, &b);

    let mut params = tune.params.clone();
    if params.is_empty() {
        let inputs: Vec<ParamInput> = state
            .store
            .call(move |c| {
                Ok(serde_json::from_str(
                    &c.query_row("SELECT inputs FROM tunes WHERE id = ?1", [id], |r| r.get::<_, String>(0))?,
                )?)
            })
            .await?;
        let declared = declared(&bin).await?;
        params = select(&declared, &inputs)?.into_iter().map(|param| TuneParam { value: param.start, param }).collect();
        (r.log)(&format!("tuning {} parameters", params.len()));
    }
    let (build_id, json) = (b.id, serde_json::to_string(&params)?);
    update(state, id, move |c| {
        c.execute("UPDATE tunes SET build_id = ?2, params = ?3 WHERE id = ?1", params![id, build_id, json])?;
        Ok(())
    })
    .await?;

    let fastchess = tools::fastchess(&state.config.data, r.log).await?;
    let book = tools::book(&state.config.data, r.log).await?;
    let dir = state.config.data.join("tunes").join(id.to_string());
    std::fs::create_dir_all(&dir)?;
    let schedule =
        spsa::Schedule { iterations: tune.iterations, alpha: tune.alpha, gamma: tune.gamma, a_ratio: tune.a_ratio };

    let slots = workers(tune.concurrency);
    (r.log)(&format!(
        "{} batches of {} pairs at once, {} games at a time",
        slots.len(),
        tune.pairs_per_iteration,
        tune.concurrency
    ));
    let mut scheduler = Scheduler::new(schedule, params, tune.iteration);
    let n = scheduler.params.len();
    let (fastchess, book, bin, spec, dir, cancel, slots) =
        (&fastchess, &book, &bin, &tune.spec, &dir, r.cancel, &slots);
    let launch = move |worker: usize, batch: Batch| async move {
        let penta = fastchess::run(fastchess::Match {
            fastchess,
            book,
            dev: bin,
            base: bin,
            dev_options: &batch.plus,
            base_options: &batch.minus,
            spec,
            concurrency: slots[worker],
            pgn: &dir.join(format!("games-{worker}.pgn")),
            cancel,
            // One line per iteration is enough; fastchess's own failures come back in its error.
            log: &|_| {},
            progress: &|_| {},
        })
        .await;
        (worker, batch, penta)
    };
    let mut running = FuturesUnordered::new();
    for worker in 0..slots.len() {
        if let Some(batch) = scheduler.next(flips(n)?) {
            running.push(launch(worker, batch));
        }
    }

    // Results are applied as they come back; the worker that played one is given the next batch. An error in any
    // batch (or a cancel) drops the rest, which stops their fastchess.
    while let Some((worker, batch, penta)) = running.next().await {
        let penta = penta?;
        let result = wins_minus_losses(&penta);
        let k = scheduler.apply(&batch, result);
        if let Some(next) = scheduler.next(flips(n)?) {
            running.push(launch(worker, next));
        }
        let (plus, minus) = (&batch.plus, &batch.minus);

        let values: Vec<f64> = scheduler.params.iter().map(|p| p.value).collect();
        let (vals, penta_json, params_json) = (
            serde_json::to_string(&values)?,
            serde_json::to_string(&penta.0)?,
            serde_json::to_string(&scheduler.params)?,
        );
        state
            .store
            .call(move |c| {
                let tx = c.transaction()?;
                tx.execute(
                    "INSERT OR REPLACE INTO tune_iterations (tune_id, iteration, vals, penta, result)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, k, vals, penta_json, result],
                )?;
                tx.execute("UPDATE tunes SET iteration = ?2, params = ?3 WHERE id = ?1", params![id, k, params_json])?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        let sides = plus.iter().zip(minus.iter()).map(|((name, hi), (_, lo))| format!("{name} {hi}/{lo}"));
        (r.log)(&format!(
            "iteration {k}/{} (drawn at {}, worker {worker}): θ+/θ− {}: {:?} → {result:+}",
            tune.iterations,
            batch.k,
            sides.collect::<Vec<_>>().join(", "),
            penta.0
        ));
        state.events.emit(
            "tune.progress",
            serde_json::json!({ "id": id, "iteration": k, "values": values, "result": result, "penta": penta }),
        );
    }

    finish(state, id, TuneStatus::Done, &format!("{} iterations", tune.iterations)).await?;
    Ok(TuneStatus::Done)
}

/// The tune's game slots as workers: two each (a game pair at a time, both games at once), and one left over when
/// the concurrency is odd.
fn workers(concurrency: u32) -> Vec<u32> {
    let mut slots = vec![2; (concurrency / 2) as usize];
    if concurrency % 2 == 1 || slots.is_empty() {
        slots.push(1);
    }
    slots
}

/// One iteration's batch of games: θ± as drawn at dispatch, and what its result needs to become a step.
#[derive(Clone, Debug)]
struct Batch {
    /// The `k` its `c_k` and `R_k` belong to.
    k: u32,
    flips: Vec<f64>,
    /// `(c_k, R_k)` per parameter.
    steps: Vec<(f64, f64)>,
    /// θ+ and θ− as UCI options.
    plus: Vec<(String, String)>,
    minus: Vec<(String, String)>,
}

/// Draws batches against the current θ and applies their results to θ as it stands when they come back, as
/// OpenBench's server does for its workers. See the module's doc.
struct Scheduler {
    schedule: spsa::Schedule,
    params: Vec<TuneParam>,
    /// Updates applied, the tune's `iteration`.
    applied: u32,
    /// Batches drawn and not yet applied.
    in_flight: u32,
}

impl Scheduler {
    fn new(schedule: spsa::Schedule, params: Vec<TuneParam>, applied: u32) -> Self {
        Self { schedule, params, applied, in_flight: 0 }
    }

    /// The next batch, drawn with these flips around the current θ; `None` once every iteration left is drawn.
    fn next(&mut self, flips: Vec<f64>) -> Option<Batch> {
        if self.applied + self.in_flight >= self.schedule.iterations {
            return None;
        }
        self.in_flight += 1;
        let k = self.applied + 1;
        let mut batch = Batch { k, flips, steps: Vec::new(), plus: Vec::new(), minus: Vec::new() };
        for (p, &flip) in self.params.iter().zip(&batch.flips) {
            let (c, rk) = (self.schedule.c(&p.param, k), self.schedule.r(&p.param, k));
            let (hi, lo) = spsa::perturb(&p.param, p.value, c, flip);
            batch.plus.push((p.param.name.clone(), format!("{hi}")));
            batch.minus.push((p.param.name.clone(), format!("{lo}")));
            batch.steps.push((c, rk));
        }
        Some(batch)
    }

    /// Steps θ by a finished batch's result (θ+'s wins minus losses) and returns the iteration this update is.
    fn apply(&mut self, batch: &Batch, result: i64) -> u32 {
        for ((p, &flip), &(c, rk)) in self.params.iter_mut().zip(&batch.flips).zip(&batch.steps) {
            p.value = spsa::step(&p.param, p.value, rk, c, flip, result);
        }
        self.in_flight -= 1;
        self.applied += 1;
        self.applied
    }
}

/// A ref to build: a build id names its commit.
async fn resolve_ref(state: &AppState, refname: &str) -> Result<String> {
    let Ok(id) = refname.parse::<i64>() else { return Ok(refname.to_owned()) };
    let b = state.store.call(move |c| build::get(c, id)).await?;
    Ok(b.map(|b| b.commit).unwrap_or_else(|| refname.to_owned()))
}

/// A random ±1 per parameter.
fn flips(n: usize) -> Result<Vec<f64>> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("random flips: {e}"))?;
    Ok(bytes.iter().map(|b| if b & 1 == 1 { 1.0 } else { -1.0 }).collect())
}

/// Games won minus games lost from game-pair counts: pairs scoring 0, ½, 1, 1½, 2 are −2, −1, 0, +1, +2.
fn wins_minus_losses(p: &Pentanomial) -> i64 {
    let [ll, ld, _, wd, ww] = p.0.map(|n| n as i64);
    -2 * ll - ld + wd + 2 * ww
}

/// The tunables a `spsa` build declares, from its `spsa` command (OpenBench's format), or else from its UCI spin
/// options with OpenBench's usual steps.
pub async fn declared(bin: &Path) -> Result<Vec<Param>> {
    let out = proc::output(bin, &["spsa"], None).await.unwrap_or_default();
    let params = parse_openbench(&out);
    if !params.is_empty() {
        return Ok(params);
    }
    let out = proc::output(bin, &["uci"], None).await?;
    let params = parse_uci_options(&out);
    if params.is_empty() {
        bail!("the build declares no tunables: is it built with the spsa feature, and does it define! any?");
    }
    Ok(params)
}

/// Lines like `rfp_quad, int, 1140, 600, 1700, 55, 0.002`.
fn parse_openbench(out: &str) -> Vec<Param> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            let [name, kind, start, min, max, c_end, r_end] = f.as_slice() else { return None };
            if *kind != "int" {
                return None;
            }
            Some(Param {
                name: (*name).to_owned(),
                start: start.parse().ok()?,
                min: min.parse().ok()?,
                max: max.parse().ok()?,
                c_end: c_end.parse().ok()?,
                r_end: r_end.parse().ok()?,
            })
        })
        .collect()
}

/// The engine options that are not tunables.
const STANDARD_OPTIONS: &[&str] = &["Hash", "Threads", "MoveOverhead", "MultiPV"];

/// `option name X type spin default D min A max B`, for every spin option that is not a standard one.
fn parse_uci_options(out: &str) -> Vec<Param> {
    out.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let field = |key: &str| w.iter().position(|x| *x == key).and_then(|i| w.get(i + 1)?.parse::<f64>().ok());
            if w.first() != Some(&"option") || !l.contains("type spin") {
                return None;
            }
            let name = w.get(2)?;
            if STANDARD_OPTIONS.contains(name) {
                return None;
            }
            let (start, min, max) = (field("default")?, field("min")?, field("max")?);
            Some(Param {
                name: (*name).to_owned(),
                start,
                min,
                max,
                c_end: ((max - min) / 20.0).max(0.5),
                r_end: spsa::R_END,
            })
        })
        .collect()
}

/// The parameters to tune: every declared one when `inputs` is empty, else those named, with the overrides given.
fn select(declared: &[Param], inputs: &[ParamInput]) -> Result<Vec<Param>> {
    if inputs.is_empty() {
        return Ok(declared.to_vec());
    }
    inputs
        .iter()
        .map(|i| {
            let d = declared.iter().find(|d| d.name == i.name);
            let field = |given: Option<f64>, from: fn(&Param) -> f64, what: &str| {
                given
                    .or(d.map(from))
                    .with_context(|| format!("{} is not declared by the engine: give its {what}", i.name))
            };
            let p = Param {
                name: i.name.clone(),
                start: field(i.start, |d| d.start, "start")?,
                min: field(i.min, |d| d.min, "min")?,
                max: field(i.max, |d| d.max, "max")?,
                c_end: field(i.c_end, |d| d.c_end, "c_end")?,
                r_end: i.r_end.or(d.map(|d| d.r_end)).unwrap_or(spsa::R_END),
            };
            if !(p.min <= p.start && p.start <= p.max) || p.c_end <= 0.0 {
                bail!("{}: needs min ≤ start ≤ max and c_end > 0", p.name);
            }
            Ok(p)
        })
        .collect()
}

/// Writes a tune's current values into the engine as new defaults, on a new branch, with its `Bench:` line; pushes it
/// and queues the [0, 4] confirmation when asked. Returns what it made.
pub async fn apply(r: Run<'_>, id: i64, opts: &ApplyTune) -> Result<serde_json::Value> {
    let state = r.state;
    let tune = state.store.call(move |c| get(c, id)).await?.context("no such tune")?;
    let build_id = tune.build.context("the tune has no build yet: it has not started")?;
    let b = state.store.call(move |c| build::get(c, build_id)).await?.context("the tune's build is gone")?;
    if tune.params.is_empty() {
        bail!("the tune has no parameters yet");
    }
    let branch = opts.branch.clone().unwrap_or_else(|| format!("tune/{id}"));
    let repo = &state.config.reckless;
    if git::git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).await.is_ok() {
        bail!("branch {branch} already exists in {}: pick another name", repo.display());
    }

    let worktree = state.config.data.join("apply-worktree");
    let _ = std::fs::remove_dir_all(&worktree);
    git::git(repo, &["worktree", "prune"]).await?;
    let path = worktree.to_string_lossy().into_owned();
    (r.log)(&format!("branch {branch} from {} ({})", &b.commit[..8], b.subject));
    git::git(repo, &["worktree", "add", "-b", &branch, &path, &b.commit]).await?;

    let made = async {
        let file = worktree.join("src").join("parameters.rs");
        let source = std::fs::read_to_string(&file).with_context(|| format!("read {}", file.display()))?;
        let (source, changes) = rewrite_defaults(&source, &tune.params)?;
        std::fs::write(&file, source)?;
        for line in &changes {
            (r.log)(line);
        }
        if changes.is_empty() {
            bail!("no parameter moved from its default: nothing to apply");
        }
        let message = format!(
            "Apply SPSA tune {id}: {}\n\n{} of {} iterations, {} pairs each.\n\n{}\n",
            tune.name,
            tune.iteration,
            tune.iterations,
            tune.pairs_per_iteration,
            changes.join("\n")
        );
        git::git(&worktree, &["commit", "-q", "-m", &message, "--", "src/parameters.rs"]).await?;

        // Builds are keyed by tree, so the reworded commit below is this same build.
        let cx = build::BuildContext {
            data: &state.config.data,
            reckless: repo,
            store: &state.store,
            cancel: r.cancel,
            log: r.log,
        };
        let built = build::build(cx, &branch).await?;
        let message = format!("{message}\nBench: {}\n", built.bench);
        git::git(&worktree, &["commit", "-q", "--amend", "-m", &message]).await?;
        let commit = git::git(&worktree, &["rev-parse", "HEAD"]).await?;
        anyhow::Ok((commit, built.bench))
    }
    .await;
    let _ = git::git(repo, &["worktree", "remove", "--force", &path]).await;
    let (commit, bench) = match made {
        Ok(made) => made,
        Err(e) => {
            let _ = git::git(repo, &["branch", "-D", &branch]).await;
            return Err(e);
        }
    };
    (r.log)(&format!("{branch} at {} with Bench: {bench}", &commit[..8]));

    if opts.push {
        (r.log)(&format!("pushing {branch} to origin"));
        git::git(repo, &["push", "-u", "origin", &branch]).await?;
    }

    let mut experiment = None;
    if opts.experiment.unwrap_or(true) {
        let new = NewExperiment {
            name: Some(format!("tune {id}: {}", tune.name)),
            hypothesis: format!(
                "SPSA tune {id} ({} iterations of {} parameters) gains Elo; confirming at [0, 4] as OpenBench does.",
                tune.iteration,
                tune.params.len()
            ),
            kind: Kind::Gainer,
            base: opts.base.clone().unwrap_or_else(|| "main".into()),
            dev: branch.clone(),
            ltc: opts.ltc,
            stages: Some(funnel::tuned_funnel(opts.ltc)),
            preset: None,
            bounds: None,
            pairs: None,
        };
        let created = experiments::submit(state, vec![new], None).await?;
        experiment = created.first().map(|e| e.id);
        if let Some(e) = experiment {
            (r.log)(&format!("queued experiment {e}"));
        }
    }

    Ok(serde_json::json!({ "tune": id, "branch": branch, "commit": commit, "bench": bench, "experiment": experiment }))
}

/// Replaces each parameter's default in a `define!` block (`i32 name: default, …;` or `i32 name: default;`) with its
/// tuned value, rounded unless the default is written with a decimal point. Returns the new source and one line per
/// changed parameter.
fn rewrite_defaults(source: &str, params: &[TuneParam]) -> Result<(String, Vec<String>)> {
    let mut source = source.to_owned();
    let mut changes = Vec::new();
    for p in params {
        let name = &p.param.name;
        let re = regex_lite::Regex::new(&format!(
            r"(?m)^(\s*[A-Za-z0-9_]+\s+{}\s*:\s*)(-?[0-9][0-9_.]*)",
            regex_lite::escape(name)
        ))?;
        let mut found = re.captures_iter(&source);
        let (Some(caps), None) = (found.next(), found.next()) else {
            bail!("{name} is not declared exactly once in src/parameters.rs");
        };
        let whole = caps.get(0).unwrap().range();
        let (prefix, old) = (caps[1].to_owned(), caps[2].to_owned());
        let new = if old.contains('.') { format!("{:.4}", p.value) } else { format!("{}", p.value.round() as i64) };
        if new != old {
            changes.push(format!("{name}: {old} -> {new}"));
            source.replace_range(whole, &format!("{prefix}{new}"));
        }
    }
    Ok((source, changes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_wins_minus_losses_from_pairs() {
        assert_eq!(wins_minus_losses(&Pentanomial([1, 2, 3, 4, 5])), 10); // -2 - 2 + 4 + 10
        assert_eq!(wins_minus_losses(&Pentanomial([0, 0, 7, 0, 0])), 0);
    }

    #[test]
    fn splits_the_slots_into_workers_of_two() {
        assert_eq!(workers(14), [2; 7]);
        assert_eq!(workers(5), [2, 2, 1]);
        assert_eq!(workers(1), [1]);
        assert_eq!(workers(0), [1]);
    }

    fn scheduler(iterations: u32, applied: u32) -> Scheduler {
        let param = Param { name: "x".into(), start: 100.0, min: 0.0, max: 200.0, c_end: 4.0, r_end: 0.002 };
        let schedule = spsa::Schedule { iterations, alpha: spsa::ALPHA, gamma: spsa::GAMMA, a_ratio: spsa::A_RATIO };
        Scheduler::new(schedule, vec![TuneParam { value: param.start, param }], applied)
    }

    #[test]
    fn batches_in_flight_share_theta_and_k_until_a_result_lands() {
        let mut s = scheduler(10, 0);
        let a = s.next(vec![1.0]).unwrap();
        let b = s.next(vec![-1.0]).unwrap();
        // Both drawn against θ = 100 at k = 1, with opposite flips.
        assert_eq!((a.k, b.k), (1, 1));
        assert_eq!(a.plus, b.minus);
        assert_eq!(a.minus, b.plus);

        // b lands first: θ moves by b's own step, and becomes iteration 1.
        let (c, rk) = b.steps[0];
        assert_eq!(s.apply(&b, 4), 1);
        let after_b = 100.0 - rk * c * 4.0;
        assert!((s.params[0].value - after_b).abs() < 1e-9);

        // A batch drawn now sees the moved θ and k = 2.
        let c2 = s.next(vec![1.0]).unwrap();
        assert_eq!(c2.k, 2);
        let (cc, _) = c2.steps[0];
        assert_eq!(c2.plus[0].1, format!("{}", (after_b + cc).round()));

        // a lands next, applied to θ as it stands (not to the θ it was drawn at), with a's c and R, as iteration 2.
        let (ca, ra) = a.steps[0];
        assert_eq!(s.apply(&a, -2), 2);
        assert!((s.params[0].value - (after_b - ra * ca * 2.0)).abs() < 1e-9);
    }

    #[test]
    fn draws_exactly_the_iterations_left() {
        // A resumed tune with 7 of 10 applied: three batches, however many workers ask.
        let mut s = scheduler(10, 7);
        let drawn: Vec<Batch> = (0..7).filter_map(|_| s.next(vec![1.0])).collect();
        assert_eq!(drawn.len(), 3);
        assert!(drawn.iter().all(|b| b.k == 8));
        let numbers: Vec<u32> = drawn.iter().map(|b| s.apply(b, 0)).collect();
        assert_eq!(numbers, [8, 9, 10]);
        assert!(s.next(vec![1.0]).is_none());
    }

    #[test]
    fn reads_declared_tunables() {
        let ob = "asp_delta, int, 23, 10, 40, 1.5, 0.002\nrfp_quad, int, 1140, 600, 1700, 55, 0.002\n";
        let p = parse_openbench(ob);
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].name.as_str(), p[1].start, p[1].c_end), ("rfp_quad", 1140.0, 55.0));

        let uci = "id name Reckless\noption name Hash type spin default 16 min 1 max 262144\n\
                   option name Minimal type check default false\n\
                   option name rfp_quad type spin default 1140 min 600 max 1700\nuciok\n";
        let p = parse_uci_options(uci);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].min, p[0].max, p[0].c_end), (600.0, 1700.0, 55.0));
    }

    #[test]
    fn selects_named_parameters_with_overrides() {
        let declared = parse_openbench("a, int, 10, 0, 20, 1, 0.002\nb, int, 5, 0, 9, 0.5, 0.002\n");
        assert_eq!(select(&declared, &[]).unwrap().len(), 2);
        let input = ParamInput { name: "b".into(), max: Some(8.0), ..Default::default() };
        let p = select(&declared, &[input]).unwrap();
        assert_eq!((p.len(), p[0].max, p[0].start), (1, 8.0, 5.0));
        assert!(select(&declared, &[ParamInput { name: "zzz".into(), ..Default::default() }]).is_err());
    }

    #[test]
    fn rewrites_defaults_in_place() {
        let src = "define! {\r\n    i32 asp_delta: 23, 10, 40, 1.5;\r\n    i32 rfp_quad: 1140, 600, 1700, 55.0;\r\n    f32 tm1: 3.1838;\r\n}\r\n";
        let tp = |name: &str, value| TuneParam {
            param: Param { name: name.into(), start: 0.0, min: 0.0, max: 0.0, c_end: 1.0, r_end: 0.002 },
            value,
        };
        let (out, changes) =
            rewrite_defaults(src, &[tp("asp_delta", 24.6), tp("rfp_quad", 1140.2), tp("tm1", 3.2)]).unwrap();
        assert!(out.contains("i32 asp_delta: 25, 10, 40, 1.5;\r\n"), "{out}");
        assert!(out.contains("i32 rfp_quad: 1140, 600"));
        assert!(out.contains("f32 tm1: 3.2000;"));
        assert_eq!(changes, ["asp_delta: 23 -> 25", "tm1: 3.1838 -> 3.2000"]);
        assert!(rewrite_defaults(src, &[tp("nope", 1.0)]).is_err());
    }
}
