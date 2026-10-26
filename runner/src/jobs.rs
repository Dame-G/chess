//! The job queue. Jobs that load the CPU (builds now; matches, datagen and training later) run one at a time in
//! submission order, so a measurement never shares the machine with another. Each job's output is kept as a log file.

use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{api::AppState, engine::build, experiments, tune};

/// What a job does.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobSpec {
    /// Build a commit of the Reckless checkout and bench it.
    Build {
        #[serde(rename = "ref")]
        refname: String,
    },
    /// Run an experiment's funnel.
    Experiment { id: i64 },
    /// Run an SPSA tune's remaining iterations.
    Tune { id: i64 },
    /// Bake a tune's values into a branch of the fork, and queue its confirmation.
    ApplyTune {
        id: i64,
        #[serde(flatten)]
        options: tune::ApplyTune,
    },
    /// Import your finished Lichess games as matches. It needs the network, not the CPU, so it runs at once beside
    /// the queue.
    LichessImport {
        #[serde(flatten)]
        options: crate::lichess::me::import::ImportRequest,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobStatus {
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

    pub fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Job {
    pub id: i64,
    pub spec: JobSpec,
    pub status: JobStatus,
    /// What the job produced, such as `{ "build": 3 }`.
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

const COLUMNS: &str = "id, spec, status, result, error, created_at, started_at, finished_at";

fn from_row(r: &Row) -> rusqlite::Result<Job> {
    let json = |i: usize| -> rusqlite::Result<Option<serde_json::Value>> {
        Ok(r.get::<_, Option<String>>(i)?.and_then(|s| serde_json::from_str(&s).ok()))
    };
    Ok(Job {
        id: r.get(0)?,
        spec: serde_json::from_str(&r.get::<_, String>(1)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?,
        status: JobStatus::parse(&r.get::<_, String>(2)?),
        result: json(3)?,
        error: r.get(4)?,
        created_at: r.get(5)?,
        started_at: r.get(6)?,
        finished_at: r.get(7)?,
    })
}

pub fn list(c: &Connection, limit: i64) -> Result<Vec<Job>> {
    let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM jobs ORDER BY id DESC LIMIT ?1"))?;
    Ok(stmt.query_map([limit], from_row)?.collect::<rusqlite::Result<_>>()?)
}

pub fn get(c: &Connection, id: i64) -> Result<Option<Job>> {
    Ok(c.query_row(&format!("SELECT {COLUMNS} FROM jobs WHERE id = ?1"), [id], from_row).optional()?)
}

fn set_status(c: &Connection, id: i64, status: JobStatus) -> Result<Job> {
    let stamp = match status {
        JobStatus::Running => ", started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        s if s.is_finished() => ", finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        _ => "",
    };
    c.execute(&format!("UPDATE jobs SET status = ?2{stamp} WHERE id = ?1"), params![id, status.as_str()])?;
    Ok(get(c, id)?.expect("job exists"))
}

pub struct Queue {
    wake: Notify,
    running: Mutex<Option<(i64, CancellationToken)>>,
    /// Jobs that run at once beside the queue (they use the network, not the CPU), by id.
    beside: Mutex<std::collections::HashMap<i64, CancellationToken>>,
}

impl Queue {
    pub fn new() -> Self {
        Self { wake: Notify::new(), running: Mutex::new(None), beside: Mutex::new(Default::default()) }
    }
}

/// Starts a job at once, beside the queue, for work that does not load the CPU (an import). It is listed, logged,
/// cancelled and announced like any other.
pub async fn run_now(state: &Arc<AppState>, spec: JobSpec) -> Result<Job> {
    let job = state
        .store
        .call(move |c| {
            c.execute("INSERT INTO jobs (spec, status) VALUES (?1, 'queued')", [serde_json::to_string(&spec)?])?;
            set_status(c, c.last_insert_rowid(), JobStatus::Running)
        })
        .await?;
    let token = CancellationToken::new();
    state.queue.beside.lock().unwrap().insert(job.id, token.clone());
    let (state, started) = (state.clone(), job.clone());
    tokio::spawn(async move { run(&state, started, token).await });
    Ok(job)
}

pub fn log_path(state: &AppState, id: i64) -> PathBuf {
    state.config.data.join("jobs").join(format!("{id}.log"))
}

pub async fn submit(state: &Arc<AppState>, spec: JobSpec) -> Result<Job> {
    let job = state
        .store
        .call(move |c| {
            c.execute("INSERT INTO jobs (spec, status) VALUES (?1, 'queued')", [serde_json::to_string(&spec)?])?;
            Ok(get(c, c.last_insert_rowid())?.expect("just inserted"))
        })
        .await?;
    announce(state, &job);
    state.queue.wake.notify_one();
    Ok(job)
}

/// Cancels a queued job, or stops a running one. A finished job is left as it is. `None` when there is no such job.
pub async fn cancel(state: &Arc<AppState>, id: i64) -> Result<Option<Job>> {
    // Read and, when queued, cancelled in one call, so the worker cannot start it in between.
    let (job, cancelled) = state
        .store
        .call(move |c| {
            let cancelled = c.execute(
                "UPDATE jobs SET status = 'cancelled', finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND status = 'queued'",
                [id],
            )? > 0;
            Ok((get(c, id)?, cancelled))
        })
        .await?;
    let Some(job) = job else { return Ok(None) };
    if cancelled {
        announce(state, &job);
    } else if job.status == JobStatus::Running
        && let Some((running, token)) = &*state.queue.running.lock().unwrap()
        && *running == id
    {
        token.cancel();
    } else if let Some(token) = state.queue.beside.lock().unwrap().get(&id) {
        token.cancel();
    }
    Ok(Some(job))
}

fn announce(state: &AppState, job: &Job) {
    state.events.emit("job.updated", serde_json::to_value(job).unwrap());
}

/// Work left running by a previous process failed: its job, and the experiment, test or tune it was running. A tune
/// keeps every iteration, so one stopped this way can resume where it was.
fn recover(c: &mut Connection) -> Result<()> {
    let tx = c.transaction()?;
    tx.execute("UPDATE jobs SET status = 'failed', error = 'the runner stopped' WHERE status = 'running'", [])?;
    for table in ["experiments", "tunes"] {
        tx.execute(
            &format!(
                "UPDATE {table} SET status = 'failed', summary = 'the runner stopped',
                 finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE status = 'running'"
            ),
            [],
        )?;
    }
    tx.execute("UPDATE experiments SET current = NULL WHERE status = 'failed' AND current IS NOT NULL", [])?;
    tx.execute(
        "UPDATE tests SET running = 0, finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE running = 1",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

/// Runs queued jobs one at a time, for as long as the runner runs.
pub async fn worker(state: Arc<AppState>) {
    if let Err(e) = state.store.call(recover).await {
        tracing::error!("job queue: {e:#}");
    }
    std::fs::create_dir_all(state.config.data.join("jobs")).ok();

    loop {
        let token = CancellationToken::new();
        let next = state
            .store
            .call({
                let (state, token) = (state.clone(), token.clone());
                move |c| {
                    let id: Option<i64> = c
                        .query_row("SELECT id FROM jobs WHERE status = 'queued' ORDER BY id LIMIT 1", [], |r| r.get(0))
                        .optional()?;
                    let Some(id) = id else { return Ok(None) };
                    let job = set_status(c, id, JobStatus::Running)?;
                    // Still holding the connection, so a cancel that reads the job as running also finds its token.
                    *state.queue.running.lock().unwrap() = Some((id, token));
                    Ok(Some(job))
                }
            })
            .await;
        match next {
            Ok(Some(job)) => run(&state, job, token).await,
            Ok(None) => state.queue.wake.notified().await,
            Err(e) => {
                tracing::error!("job queue: {e:#}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

async fn run(state: &Arc<AppState>, job: Job, token: CancellationToken) {
    announce(state, &job);

    let file = Mutex::new(std::fs::File::create(log_path(state, job.id)).ok());
    let id = job.id;
    let log = |line: &str| {
        if let Some(f) = file.lock().unwrap().as_mut() {
            let _ = writeln!(f, "{line}");
        }
        state.events.emit("job.log", serde_json::json!({ "id": id, "line": line }));
    };

    let outcome: Result<serde_json::Value> = match &job.spec {
        JobSpec::Build { refname } => {
            let cx = build::BuildContext {
                data: &state.config.data,
                reckless: &state.config.reckless,
                store: &state.store,
                cancel: &token,
                log: &log,
            };
            build::build(cx, refname).await.map(|b| serde_json::json!({ "build": b.id }))
        }
        JobSpec::Experiment { id } => {
            let run = experiments::Run { state, cancel: &token, log: &log };
            let outcome = experiments::run(run, *id).await;
            // A build error or a crash ends the experiment too; its summary says why.
            let ended = match &outcome {
                Ok(_) => None,
                Err(_) if token.is_cancelled() => {
                    Some((experiments::ExperimentStatus::Cancelled, "cancelled".to_owned()))
                }
                Err(e) => Some((experiments::ExperimentStatus::Failed, format!("{e:#}"))),
            };
            if let Some((status, summary)) = ended {
                let _ = experiments::finish(state, *id, status, &summary).await;
            }
            outcome.map(|status| serde_json::json!({ "experiment": id, "status": status }))
        }
        JobSpec::Tune { id } => {
            let run = tune::Run { state, cancel: &token, log: &log };
            let outcome = tune::run(run, *id).await;
            let ended = match &outcome {
                Ok(_) => None,
                Err(_) if token.is_cancelled() => Some((tune::TuneStatus::Cancelled, "cancelled".to_owned())),
                Err(e) => Some((tune::TuneStatus::Failed, format!("{e:#}"))),
            };
            if let Some((status, summary)) = ended {
                let _ = tune::finish(state, *id, status, &summary).await;
            }
            outcome.map(|status| serde_json::json!({ "tune": id, "status": status }))
        }
        JobSpec::ApplyTune { id, options } => {
            tune::apply(tune::Run { state, cancel: &token, log: &log }, *id, options).await
        }
        JobSpec::LichessImport { options } => crate::lichess::me::import::run(state, &token, &log, options).await,
    };

    {
        let mut running = state.queue.running.lock().unwrap();
        if running.as_ref().is_some_and(|(r, _)| *r == id) {
            *running = None;
        }
    }
    state.queue.beside.lock().unwrap().remove(&id);
    let finished = state
        .store
        .call(move |c| {
            let status = match &outcome {
                Ok(result) => {
                    c.execute("UPDATE jobs SET result = ?2 WHERE id = ?1", params![id, result.to_string()])?;
                    JobStatus::Done
                }
                Err(_) if token.is_cancelled() => JobStatus::Cancelled,
                Err(e) => {
                    c.execute("UPDATE jobs SET error = ?2 WHERE id = ?1", params![id, format!("{e:#}")])?;
                    JobStatus::Failed
                }
            };
            set_status(c, id, status)
        })
        .await;
    match finished {
        Ok(job) => announce(state, &job),
        Err(e) => tracing::error!("job {id}: {e:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn work_left_running_by_a_stopped_runner_fails() {
        let store = crate::store::Store::memory().unwrap();
        store
            .call(|c| {
                c.execute_batch(
                    "INSERT INTO jobs (spec, status) VALUES ('{\"kind\":\"experiment\",\"id\":1}', 'running');
                     INSERT INTO jobs (spec, status) VALUES ('{\"kind\":\"experiment\",\"id\":2}', 'queued');
                     INSERT INTO builds (key, ref, commit_sha, subject, features, bench, nps)
                     VALUES ('k', 'main', 'abc', 's', 'no-syzygy', 1, 1);
                     INSERT INTO experiments (name, kind, base_ref, dev_ref, funnel, status, results, current, job_id)
                     VALUES ('e', 'gainer', 'main', 'x', '[]', 'running', '[]', 'stc', 1);
                     INSERT INTO experiments (name, kind, base_ref, dev_ref, funnel, status, results, job_id)
                     VALUES ('f', 'gainer', 'main', 'y', '[]', 'queued', '[]', 2);
                     INSERT INTO tests (experiment_id, stage, spec, base_build, dev_build, running, penta)
                     VALUES (1, 'stc', '{}', 1, 1, 1, '[0,0,0,0,0]');",
                )?;
                recover(c)?;
                let one = |sql: &str| -> rusqlite::Result<String> { c.query_row(sql, [], |r| r.get(0)) };
                assert_eq!(one("SELECT status FROM jobs WHERE id = 1")?, "failed");
                assert_eq!(one("SELECT status FROM jobs WHERE id = 2")?, "queued");
                assert_eq!(
                    one(
                        "SELECT status || ' ' || summary || ' ' || ifnull(current, '-') FROM experiments WHERE id = 1"
                    )?,
                    "failed the runner stopped -"
                );
                assert_eq!(one("SELECT status FROM experiments WHERE id = 2")?, "queued");
                assert_eq!(one("SELECT CAST(running AS TEXT) FROM tests")?, "0");
                // A build the stopped experiment used can be deleted again.
                assert_eq!(build::user(c, 1)?, None);
                Ok(())
            })
            .await
            .unwrap();
    }
}
