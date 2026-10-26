//! Experiments: a change (dev ref) against a baseline (base ref), run through a funnel of stages. This is the unit
//! the harness is built around: an agent submits experiments, alone or as a batch, waits, and reads the verdicts.

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{
    api::AppState,
    engine::{build, fastchess},
    funnel::{self, Kind, MatchSpec, Stage},
    jobs::{self, JobSpec},
    stats::{self, Estimate, Pentanomial, Verdict},
    tools,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Queued,
    Running,
    /// Every stage passed.
    Passed,
    /// A stage failed: see `summary`.
    Failed,
    Cancelled,
}

impl ExperimentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "passed" => Self::Passed,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }
}

/// What a finished stage found.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct StageResult {
    pub stage: String,
    pub passed: bool,
    /// One line on why, with the numbers.
    pub detail: String,
    /// The match, for a match stage.
    pub test: Option<i64>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Experiment {
    pub id: i64,
    /// The batch it was submitted with, if any.
    pub batch: Option<String>,
    pub name: String,
    pub hypothesis: String,
    pub kind: Kind,
    /// The baseline ref.
    pub base: String,
    /// The changed ref.
    pub dev: String,
    pub base_build: Option<i64>,
    pub dev_build: Option<i64>,
    pub funnel: Vec<Stage>,
    pub status: ExperimentStatus,
    /// Results of the stages run so far, in order.
    pub results: Vec<StageResult>,
    /// The stage running now, while running.
    pub current: Option<String>,
    /// Why it stopped, once finished.
    pub summary: Option<String>,
    pub job: Option<i64>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

const COLUMNS: &str = "id, batch, name, hypothesis, kind, base_ref, dev_ref, base_build, dev_build, funnel, status, \
                       results, current, summary, job_id, created_at, finished_at";

fn json<T: serde::de::DeserializeOwned>(r: &Row, i: usize) -> rusqlite::Result<T> {
    serde_json::from_str(&r.get::<_, String>(i)?)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e)))
}

fn from_row(r: &Row) -> rusqlite::Result<Experiment> {
    Ok(Experiment {
        id: r.get(0)?,
        batch: r.get(1)?,
        name: r.get(2)?,
        hypothesis: r.get(3)?,
        kind: serde_json::from_value(serde_json::Value::String(r.get(4)?)).unwrap_or(Kind::Gainer),
        base: r.get(5)?,
        dev: r.get(6)?,
        base_build: r.get(7)?,
        dev_build: r.get(8)?,
        funnel: json(r, 9)?,
        status: ExperimentStatus::parse(&r.get::<_, String>(10)?),
        results: json(r, 11)?,
        current: r.get(12)?,
        summary: r.get(13)?,
        job: r.get(14)?,
        created_at: r.get(15)?,
        finished_at: r.get(16)?,
    })
}

pub fn list(c: &Connection, batch: Option<&str>, limit: i64) -> Result<Vec<Experiment>> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLUMNS} FROM experiments WHERE ?1 IS NULL OR batch = ?1 ORDER BY id DESC LIMIT ?2"
    ))?;
    Ok(stmt.query_map(params![batch, limit], from_row)?.collect::<rusqlite::Result<_>>()?)
}

pub fn get(c: &Connection, id: i64) -> Result<Option<Experiment>> {
    Ok(c.query_row(&format!("SELECT {COLUMNS} FROM experiments WHERE id = ?1"), [id], from_row).optional()?)
}

/// An experiment to run.
#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct NewExperiment {
    /// A short name. Defaults to the dev ref.
    pub name: Option<String>,
    /// What the change is expected to do, and why.
    #[serde(default)]
    pub hypothesis: String,
    #[serde(default = "gainer")]
    pub kind: Kind,
    /// The baseline. Defaults to `main`.
    #[serde(default = "main")]
    pub base: String,
    pub dev: String,
    /// Add an LTC confirmation to the default funnel.
    #[serde(default)]
    pub ltc: bool,
    /// Run these stages instead of the default funnel for the kind.
    pub stages: Option<Vec<Stage>>,
    /// Shorthand for a single match with these settings, instead of a funnel.
    pub preset: Option<funnel::Preset>,
    /// SPRT bounds in normalized Elo for `preset`, such as `[0, 3]`. Left out (and no `pairs`), `[0, 3]`.
    pub bounds: Option<[f64; 2]>,
    /// Play this many game pairs for `preset` instead of an SPRT.
    pub pairs: Option<u32>,
}

impl NewExperiment {
    pub fn funnel(&self) -> Vec<Stage> {
        let Some(preset) = self.preset else {
            return self.stages.clone().unwrap_or_else(|| funnel::default_funnel(self.kind, self.ltc));
        };
        let sprt = match (self.bounds, self.pairs) {
            (Some([elo0, elo1]), _) => Some(funnel::bounds(elo0, elo1)),
            (None, Some(_)) => None,
            (None, None) => Some(funnel::bounds(0.0, 3.0)),
        };
        let name = serde_json::to_value(preset).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
        vec![Stage::Match { name, spec: preset.spec(sprt, self.pairs) }]
    }
}

fn gainer() -> Kind {
    Kind::Gainer
}
fn main() -> String {
    "main".into()
}

pub const NO_STAGES: &str = "an experiment needs at least one stage";

/// Queues experiments: each gets a job, and the jobs run in order.
pub async fn submit(state: &Arc<AppState>, new: Vec<NewExperiment>, batch: Option<String>) -> Result<Vec<Experiment>> {
    // Checked before any is queued, so a bad entry does not leave half a batch behind.
    if new.iter().any(|n| n.funnel().is_empty()) {
        bail!(NO_STAGES);
    }
    let mut out = Vec::new();
    for n in new {
        let funnel = n.funnel();
        let batch = batch.clone();
        let id = state
            .store
            .call(move |c| {
                c.execute(
                    "INSERT INTO experiments (batch, name, hypothesis, kind, base_ref, dev_ref, funnel, status, results)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'queued', '[]')",
                    params![
                        batch,
                        n.name.clone().unwrap_or_else(|| n.dev.clone()),
                        n.hypothesis,
                        serde_json::to_value(n.kind)?.as_str().unwrap_or("gainer"),
                        n.base,
                        n.dev,
                        serde_json::to_string(&funnel)?,
                    ],
                )?;
                Ok(c.last_insert_rowid())
            })
            .await?;
        let job = jobs::submit(state, JobSpec::Experiment { id }).await?;
        let experiment = state
            .store
            .call(move |c| {
                c.execute("UPDATE experiments SET job_id = ?2 WHERE id = ?1", params![id, job.id])?;
                Ok(get(c, id)?.expect("just inserted"))
            })
            .await?;
        announce(state, &experiment);
        out.push(experiment);
    }
    Ok(out)
}

/// Cancels a queued experiment or stops a running one. `None` when there is no such experiment.
pub async fn cancel(state: &Arc<AppState>, id: i64) -> Result<Option<Experiment>> {
    let Some(experiment) = state.store.call(move |c| get(c, id)).await? else { return Ok(None) };
    let job = match experiment.job {
        Some(job) => jobs::cancel(state, job).await?,
        None => None,
    };
    // A job that had not started ends the experiment here; a running one ends it as it stops.
    if experiment.status == ExperimentStatus::Queued && job.is_none_or(|j| j.status == jobs::JobStatus::Cancelled) {
        return finish(state, id, ExperimentStatus::Cancelled, "cancelled before it started").await.map(Some);
    }
    Ok(Some(experiment))
}

pub fn announce(state: &AppState, e: &Experiment) {
    state.events.emit("experiment.updated", serde_json::to_value(e).unwrap());
}

async fn update(state: &AppState, id: i64, f: impl FnOnce(&Connection) -> Result<()> + Send + 'static) -> Result<()> {
    let e = state
        .store
        .call(move |c| {
            f(c)?;
            Ok(get(c, id)?.expect("experiment exists"))
        })
        .await?;
    announce(state, &e);
    Ok(())
}

/// Marks an experiment finished, unless it already is.
pub async fn finish(state: &AppState, id: i64, status: ExperimentStatus, summary: &str) -> Result<Experiment> {
    let summary = summary.to_owned();
    let e = state
        .store
        .call(move |c| {
            c.execute(
                "UPDATE experiments SET status = ?2, summary = ?3, current = NULL,
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND status IN ('queued', 'running')",
                params![id, status.as_str(), summary],
            )?;
            Ok(get(c, id)?.expect("experiment exists"))
        })
        .await?;
    announce(state, &e);
    Ok(e)
}

pub struct Run<'a> {
    pub state: &'a Arc<AppState>,
    pub cancel: &'a CancellationToken,
    pub log: &'a (dyn Fn(&str) + Sync),
}

/// Runs an experiment's funnel. Returns its final status; errors (a failed build, a crashed engine) propagate.
pub async fn run(r: Run<'_>, id: i64) -> Result<ExperimentStatus> {
    let state = r.state;
    let e = state.store.call(move |c| get(c, id)).await?.context("no such experiment")?;
    update(state, id, move |c| {
        c.execute("UPDATE experiments SET status = 'running' WHERE id = ?1", [id])?;
        Ok(())
    })
    .await?;
    (r.log)(&format!("experiment {id}: {} ({} vs {})", e.name, e.dev, e.base));

    let cx = || build::BuildContext {
        data: &state.config.data,
        reckless: &state.config.reckless,
        store: &state.store,
        cancel: r.cancel,
        log: r.log,
    };
    let base = build::build(cx(), &e.base).await.context("building base")?;
    let dev = build::build(cx(), &e.dev).await.context("building dev")?;
    let (base_id, dev_id) = (base.id, dev.id);
    update(state, id, move |c| {
        c.execute(
            "UPDATE experiments SET base_build = ?2, dev_build = ?3 WHERE id = ?1",
            params![id, base_id, dev_id],
        )?;
        Ok(())
    })
    .await?;

    let mut results = Vec::new();
    for stage in &e.funnel {
        let name = stage.name().to_owned();
        update(state, id, move |c| {
            c.execute("UPDATE experiments SET current = ?2 WHERE id = ?1", params![id, name])?;
            Ok(())
        })
        .await?;
        (r.log)(&format!("stage {}", stage.name()));
        let result = match stage {
            Stage::Bench { functional } => bench_stage(&base, &dev, *functional),
            Stage::Speed { runs } => speed_stage(&r, &base, &dev, *runs).await?,
            Stage::Match { name, spec } => match_stage(&r, id, name, spec, &base, &dev).await?,
        };
        (r.log)(&format!("{}: {} ({})", result.stage, if result.passed { "passed" } else { "failed" }, result.detail));
        let passed = result.passed;
        let summary = format!("{}: {}", result.stage, result.detail);
        results.push(result);
        let json = serde_json::to_string(&results)?;
        update(state, id, move |c| {
            c.execute("UPDATE experiments SET results = ?2 WHERE id = ?1", params![id, json])?;
            Ok(())
        })
        .await?;
        if !passed {
            finish(state, id, ExperimentStatus::Failed, &summary).await?;
            return Ok(ExperimentStatus::Failed);
        }
    }
    finish(state, id, ExperimentStatus::Passed, &format!("passed {} of {} stages", results.len(), e.funnel.len()))
        .await?;
    Ok(ExperimentStatus::Passed)
}

fn bench_stage(base: &build::Build, dev: &build::Build, functional: bool) -> StageResult {
    // A build whose bench differs from its commit's `Bench:` line is suspect (a stale line, or search that is not
    // deterministic), as Reckless's CI would say: testing with it would test something other than the commit claims.
    for (side, build) in [("base", base), ("dev", dev)] {
        if let Some(expected) = build.bench_expected.filter(|e| *e != build.bench) {
            let detail = format!(
                "{side} benches {} but its commit says Bench: {expected}: fix the Bench: line, or find the non-determinism",
                build.bench
            );
            return StageResult { stage: "bench".into(), passed: false, detail, test: None };
        }
    }
    let (b, d) = (base.bench, dev.bench);
    let (passed, detail) = match (functional, b == d) {
        (true, false) => (true, format!("bench {b} → {d}")),
        (true, true) => (false, format!("bench unchanged at {b}: the change does not alter search (nonfunctional?)")),
        (false, true) => (true, format!("bench unchanged at {b}")),
        (false, false) => (false, format!("bench changed {b} → {d}: the change alters search")),
    };
    StageResult { stage: "bench".into(), passed, detail, test: None }
}

/// Benches the builds in turn and compares their speed. Each run benches base then dev back to back, and the
/// comparison is of those pairs, so drift in the machine (boost clocks, other load) cancels out. Dev fails when it is
/// slower by more than 1% and by more than twice the standard error.
async fn speed_stage(r: &Run<'_>, base: &build::Build, dev: &build::Build, runs: u32) -> Result<StageResult> {
    let data = &r.state.config.data;
    let mut ratios = Vec::new();
    for i in 0..runs.max(3) {
        if r.cancel.is_cancelled() {
            bail!("cancelled");
        }
        // Alternate the order, so neither build always runs on a warmer machine.
        let (b, d) = if i % 2 == 0 {
            let b = build::bench(&build::binary(data, base)).await?.1 as f64;
            (b, build::bench(&build::binary(data, dev)).await?.1 as f64)
        } else {
            let d = build::bench(&build::binary(data, dev)).await?.1 as f64;
            (build::bench(&build::binary(data, base)).await?.1 as f64, d)
        };
        (r.log)(&format!("speed run {}: base {b} nps, dev {d} nps", i + 1));
        ratios.push((d / b - 1.0) * 100.0);
    }
    let (diff, se) = mean_and_error(&ratios);
    let passed = !(diff < -1.0 && diff < -2.0 * se);
    let detail = format!("dev speed {diff:+.2}% ± {:.2}% over {} paired runs", 2.0 * se, ratios.len());
    Ok(StageResult { stage: "speed".into(), passed, detail, test: None })
}

/// The mean and its standard error.
fn mean_and_error(v: &[f64]) -> (f64, f64) {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, (var / n).sqrt())
}

async fn match_stage(
    r: &Run<'_>,
    experiment: i64,
    name: &str,
    spec: &MatchSpec,
    base: &build::Build,
    dev: &build::Build,
) -> Result<StageResult> {
    let state = r.state;
    let fastchess = tools::fastchess(&state.config.data, r.log).await?;
    let book = tools::book(&state.config.data, r.log).await?;
    let test = tests::create(state, experiment, name, spec, base.id, dev.id).await?;
    let dir = tests::dir(state, test);
    std::fs::create_dir_all(&dir)?;
    let concurrency = (state.machine.threads.saturating_sub(2) as u32 / spec.threads).max(1);

    let sprt = spec.sprt;
    // One task records progress in order, and the final record waits for it, so a late update never lands after it.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Pentanomial>();
    let writer = tokio::spawn({
        let state = state.clone();
        async move {
            let mut latest = Pentanomial::default();
            while let Some(p) = rx.recv().await {
                latest = p;
                let _ = tests::record(&state, test, p, sprt, false).await;
            }
            latest
        }
    });
    let progress = move |p: Pentanomial| {
        let _ = tx.send(p);
    };
    let outcome = fastchess::run(fastchess::Match {
        fastchess: &fastchess,
        book: &book,
        dev: &build::binary(&state.config.data, dev),
        base: &build::binary(&state.config.data, base),
        dev_options: &[],
        base_options: &[],
        spec,
        concurrency,
        pgn: &dir.join("games.pgn"),
        cancel: r.cancel,
        log: r.log,
        progress: &progress,
    })
    .await;
    drop(progress);

    let totals = match (&outcome, writer.await) {
        (Ok(p), _) => *p,
        (Err(_), latest) => latest.unwrap_or_default(),
    };
    let t = tests::record(state, test, totals, sprt, true).await?;
    outcome?;

    let est =
        t.estimate.map(|e| format!("{:+.1} ± {:.1} Elo ({:+.1} nElo)", e.elo, e.elo_error, e.nelo)).unwrap_or_default();
    let games = totals.pairs() * 2;
    let (passed, detail) = match (sprt, t.verdict) {
        (Some(s), Some(Verdict::Accepted)) => {
            (true, format!("accepted [{}, {}] after {games} games, {est}", s.elo0, s.elo1))
        }
        (Some(s), Some(Verdict::Rejected)) => {
            (false, format!("rejected [{}, {}] after {games} games, {est}", s.elo0, s.elo1))
        }
        (Some(_), _) => (false, format!("undecided after {games} games, {est}")),
        (None, _) => (true, format!("{games} games, {est}")),
    };
    Ok(StageResult { stage: name.into(), passed, detail, test: Some(test) })
}

/// The matches experiments play, with their running totals.
pub mod tests {
    use super::*;

    #[derive(Clone, Debug, Serialize, ToSchema)]
    pub struct Test {
        pub id: i64,
        pub experiment: i64,
        pub stage: String,
        pub spec: MatchSpec,
        pub base_build: i64,
        pub dev_build: i64,
        pub running: bool,
        pub penta: Pentanomial,
        pub games: u64,
        pub llr: Option<f64>,
        pub verdict: Option<Verdict>,
        pub estimate: Option<Estimate>,
        pub created_at: String,
        pub finished_at: Option<String>,
    }

    /// One point of a test's progress, for its chart.
    #[derive(Clone, Debug, Serialize, ToSchema)]
    pub struct Point {
        pub games: u64,
        pub llr: Option<f64>,
        pub nelo: Option<f64>,
    }

    const COLUMNS: &str =
        "id, experiment_id, stage, spec, base_build, dev_build, running, penta, created_at, finished_at";

    fn from_row(r: &Row) -> rusqlite::Result<Test> {
        let spec: MatchSpec = json(r, 3)?;
        let penta = Pentanomial(json(r, 7)?);
        let llr = spec.sprt.map(|s| s.llr(&penta));
        Ok(Test {
            id: r.get(0)?,
            experiment: r.get(1)?,
            stage: r.get(2)?,
            base_build: r.get(4)?,
            dev_build: r.get(5)?,
            running: r.get(6)?,
            games: penta.pairs() * 2,
            verdict: spec.sprt.zip(llr).map(|(s, l)| s.verdict(l)),
            llr,
            estimate: stats::estimate(&penta),
            penta,
            spec,
            created_at: r.get(8)?,
            finished_at: r.get(9)?,
        })
    }

    pub fn get(c: &Connection, id: i64) -> Result<Option<Test>> {
        Ok(c.query_row(&format!("SELECT {COLUMNS} FROM tests WHERE id = ?1"), [id], from_row).optional()?)
    }

    pub fn for_experiment(c: &Connection, experiment: i64) -> Result<Vec<Test>> {
        let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM tests WHERE experiment_id = ?1 ORDER BY id"))?;
        Ok(stmt.query_map([experiment], from_row)?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn points(c: &Connection, id: i64) -> Result<Vec<Point>> {
        let mut stmt = c.prepare("SELECT games, llr, nelo FROM test_points WHERE test_id = ?1 ORDER BY games")?;
        Ok(stmt
            .query_map([id], |r| Ok(Point { games: r.get::<_, i64>(0)? as u64, llr: r.get(1)?, nelo: r.get(2)? }))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn dir(state: &AppState, id: i64) -> PathBuf {
        state.config.data.join("tests").join(id.to_string())
    }

    pub async fn create(
        state: &AppState,
        experiment: i64,
        stage: &str,
        spec: &MatchSpec,
        base: i64,
        dev: i64,
    ) -> Result<i64> {
        let (stage, spec) = (stage.to_owned(), serde_json::to_string(spec)?);
        state
            .store
            .call(move |c| {
                c.execute(
                    "INSERT INTO tests (experiment_id, stage, spec, base_build, dev_build, running, penta)
                     VALUES (?1, ?2, ?3, ?4, ?5, 1, '[0,0,0,0,0]')",
                    params![experiment, stage, spec, base, dev],
                )?;
                Ok(c.last_insert_rowid())
            })
            .await
    }

    /// Stores running totals and a chart point, and tells clients.
    pub async fn record(
        state: &AppState,
        id: i64,
        p: Pentanomial,
        sprt: Option<stats::Sprt>,
        finished: bool,
    ) -> Result<Test> {
        let llr = sprt.map(|s| s.llr(&p));
        let nelo = stats::estimate(&p).map(|e| e.nelo);
        let t = state
            .store
            .call(move |c| {
                let done =
                    if finished { ", running = 0, finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')" } else { "" };
                c.execute(
                    &format!("UPDATE tests SET penta = ?2{done} WHERE id = ?1"),
                    params![id, serde_json::to_string(&p.0)?],
                )?;
                c.execute(
                    "INSERT OR REPLACE INTO test_points (test_id, games, llr, nelo) VALUES (?1, ?2, ?3, ?4)",
                    params![id, (p.pairs() * 2) as i64, llr, nelo],
                )?;
                Ok(get(c, id)?.expect("test exists"))
            })
            .await?;
        state.events.emit("test.progress", serde_json::to_value(&t)?);
        Ok(t)
    }
}

#[cfg(test)]
mod bench_tests {
    use super::*;

    fn build(bench: u64, expected: Option<u64>) -> build::Build {
        build::Build {
            id: 1,
            refname: "x".into(),
            commit: "c".into(),
            subject: "s".into(),
            features: "no-syzygy".into(),
            network: None,
            bench,
            bench_expected: expected,
            nps: 1,
            created_at: String::new(),
            deleted_at: None,
            key: String::new(),
        }
    }

    #[test]
    fn bench_stage_wants_a_change_only_for_functional_changes() {
        let (main, same, other) = (build(100, Some(100)), build(100, None), build(200, Some(200)));
        assert!(bench_stage(&main, &other, true).passed);
        assert!(!bench_stage(&main, &same, true).passed);
        assert!(bench_stage(&main, &same, false).passed);
        assert!(!bench_stage(&main, &other, false).passed);
    }

    #[test]
    fn bench_stage_fails_a_build_that_disagrees_with_its_bench_line() {
        let main = build(100, Some(100));
        let r = bench_stage(&main, &build(200, Some(150)), true);
        assert!(!r.passed && r.detail.contains("dev benches 200") && r.detail.contains("Bench: 150"), "{}", r.detail);
        let r = bench_stage(&build(100, Some(90)), &build(100, Some(100)), false);
        assert!(!r.passed && r.detail.starts_with("base"), "{}", r.detail);
    }
}
