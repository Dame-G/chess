//! The job queue. Jobs that load the CPU (builds now; matches, datagen and training later) run one at a time in
//! submission order, so a measurement never shares the machine with another. Each job's output is kept as a log file.

use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::{api::AppState, engine::build};

/// What a job does.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobSpec {
    /// Build a commit of the Reckless checkout and bench it.
    Build {
        #[serde(rename = "ref")]
        refname: String,
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
}

impl Queue {
    pub fn new() -> Self {
        Self { wake: Notify::new(), running: Mutex::new(None) }
    }
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

/// Cancels a queued job, or stops a running one. A finished job is left as it is.
pub async fn cancel(state: &Arc<AppState>, id: i64) -> Result<Job> {
    let job = state.store.call(move |c| get(c, id)).await?;
    let Some(job) = job else { bail!("no job {id}") };
    match job.status {
        JobStatus::Queued => {
            let job = state.store.call(move |c| set_status(c, id, JobStatus::Cancelled)).await?;
            announce(state, &job);
            Ok(job)
        }
        JobStatus::Running => {
            if let Some((running, token)) = &*state.queue.running.lock().unwrap()
                && *running == id
            {
                token.cancel();
            }
            Ok(job)
        }
        _ => Ok(job),
    }
}

fn announce(state: &AppState, job: &Job) {
    state.events.emit("job.updated", serde_json::to_value(job).unwrap());
}

/// Runs queued jobs one at a time, for as long as the runner runs. A job left running by a previous process failed.
pub async fn worker(state: Arc<AppState>) {
    let _ = state
        .store
        .call(|c| {
            c.execute("UPDATE jobs SET status = 'failed', error = 'the runner stopped' WHERE status = 'running'", [])?;
            Ok(())
        })
        .await;
    std::fs::create_dir_all(state.config.data.join("jobs")).ok();

    loop {
        let next = state
            .store
            .call(|c| {
                let id: Option<i64> = c
                    .query_row("SELECT id FROM jobs WHERE status = 'queued' ORDER BY id LIMIT 1", [], |r| r.get(0))
                    .optional()?;
                id.map(|id| set_status(c, id, JobStatus::Running)).transpose()
            })
            .await;
        match next {
            Ok(Some(job)) => run(&state, job).await,
            Ok(None) => state.queue.wake.notified().await,
            Err(e) => {
                tracing::error!("job queue: {e:#}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

async fn run(state: &Arc<AppState>, job: Job) {
    announce(state, &job);
    let token = CancellationToken::new();
    *state.queue.running.lock().unwrap() = Some((job.id, token.clone()));

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
    };

    *state.queue.running.lock().unwrap() = None;
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
