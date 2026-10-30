//! Work: background items with their spec (what to do, owned by the `work` crate), a checkpoint (where to carry on
//! after a restart) and their progress.

use chess_core::{
    Error, Timestamp, WorkId,
    work::{Priority, Progress, Subject, Work, WorkKind, WorkStatus},
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{DbResult, Store, from_json, from_opt_json, from_word, now, opt_json, to_json, word};

/// Work to queue.
#[derive(Clone, Debug, PartialEq)]
pub struct NewWork {
    pub kind: WorkKind,
    pub priority: Priority,
    pub title: String,
    pub subject: Subject,
    pub spec: serde_json::Value,
}

/// Changes to a work item. Fields left `None` stay as they are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkUpdate {
    pub status: Option<WorkStatus>,
    pub title: Option<String>,
    pub cores: Option<u32>,
    pub progress: Option<Progress>,
    /// `Some(None)` clears it when work resumes.
    pub paused_for: Option<Option<String>>,
    pub error: Option<String>,
    pub started_at: Option<Timestamp>,
    pub ended_at: Option<Timestamp>,
}

/// The queue as people see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkLists {
    /// Running and paused, oldest first.
    pub running: Vec<Work>,
    /// In the order it will run: by priority, then oldest first.
    pub queued: Vec<Work>,
    /// Finished, newest first.
    pub recent: Vec<Work>,
}

const WORK: &str = "SELECT id, kind, status, priority, title, subject, cores, progress, paused_for, error, created_at, \
                    started_at, ended_at FROM work";

fn work_from_row(row: &Row<'_>) -> DbResult<Work> {
    let rank: u8 = row.get(3)?;
    Ok(Work {
        id: WorkId(row.get(0)?),
        kind: from_word(&row.get::<_, String>(1)?)?,
        status: from_word(&row.get::<_, String>(2)?)?,
        priority: Priority::from_rank(rank)
            .ok_or_else(|| Error::internal(format!("work has an unknown priority {rank}")))?,
        title: row.get(4)?,
        subject: from_json(&row.get::<_, String>(5)?)?,
        cores: row.get(6)?,
        progress: from_opt_json(row.get(7)?)?,
        paused_for: row.get(8)?,
        error: row.get(9)?,
        created_at: Timestamp(row.get(10)?),
        started_at: row.get::<_, Option<String>>(11)?.map(Timestamp),
        ended_at: row.get::<_, Option<String>>(12)?.map(Timestamp),
    })
}

fn work_where(conn: &Connection, clause: &str, args: impl rusqlite::Params) -> DbResult<Vec<Work>> {
    let mut stmt = conn.prepare_cached(&format!("{WORK} {clause}"))?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(work_from_row(row)?);
    }
    Ok(out)
}

fn work_or_missing(conn: &Connection, id: WorkId) -> DbResult<Work> {
    work_where(conn, "WHERE id = ?1", [id.0])?.pop().ok_or_else(|| Error::not_found(format!("No work {id}")).into())
}

const QUEUED_ORDER: &str = "WHERE status = 'queued' ORDER BY priority, id";

impl Store {
    /// Queues work; returns it.
    pub async fn insert_work(&self, w: NewWork) -> Result<Work, Error> {
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO work (kind, status, priority, title, subject, spec, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    word(&w.kind),
                    word(&WorkStatus::Queued),
                    w.priority.rank(),
                    w.title,
                    to_json(&w.subject)?,
                    to_json(&w.spec)?,
                    now().as_str()
                ],
            )?;
            work_or_missing(tx, WorkId(tx.last_insert_rowid()))
        })
        .await
    }

    pub async fn work(&self, id: WorkId) -> Result<Option<Work>, Error> {
        self.read(move |conn| Ok(work_where(conn, "WHERE id = ?1", [id.0])?.pop())).await
    }

    /// What the work was queued to do.
    pub async fn work_spec(&self, id: WorkId) -> Result<Option<serde_json::Value>, Error> {
        self.read(move |conn| {
            let text: Option<String> =
                conn.query_row("SELECT spec FROM work WHERE id = ?1", [id.0], |r| r.get(0)).optional()?;
            text.as_deref().map(from_json).transpose()
        })
        .await
    }

    /// Where the work carries on from after a restart, if it saved a checkpoint.
    pub async fn work_checkpoint(&self, id: WorkId) -> Result<Option<serde_json::Value>, Error> {
        self.read(move |conn| {
            let text: Option<Option<String>> =
                conn.query_row("SELECT checkpoint FROM work WHERE id = ?1", [id.0], |r| r.get(0)).optional()?;
            from_opt_json(text.flatten())
        })
        .await
    }

    pub async fn set_work_checkpoint(&self, id: WorkId, checkpoint: serde_json::Value) -> Result<(), Error> {
        self.write(move |tx| {
            let n =
                tx.execute("UPDATE work SET checkpoint = ?2 WHERE id = ?1", params![id.0, to_json(&checkpoint)?])?;
            if n == 0 { Err(Error::not_found(format!("No work {id}")).into()) } else { Ok(()) }
        })
        .await
    }

    /// Changes a work item; returns it as it now is.
    pub async fn update_work(&self, id: WorkId, u: WorkUpdate) -> Result<Work, Error> {
        self.write(move |tx| {
            work_or_missing(tx, id)?;
            tx.execute(
                "UPDATE work SET status = coalesce(?2, status), title = coalesce(?3, title), \
                 cores = coalesce(?4, cores), progress = coalesce(?5, progress), error = coalesce(?6, error), \
                 started_at = coalesce(?7, started_at), ended_at = coalesce(?8, ended_at) WHERE id = ?1",
                params![
                    id.0,
                    u.status.map(|s| word(&s)),
                    u.title,
                    u.cores,
                    opt_json(&u.progress)?,
                    u.error,
                    u.started_at.as_ref().map(Timestamp::as_str),
                    u.ended_at.as_ref().map(Timestamp::as_str),
                ],
            )?;
            if let Some(paused_for) = &u.paused_for {
                tx.execute("UPDATE work SET paused_for = ?2 WHERE id = ?1", params![id.0, paused_for])?;
            }
            work_or_missing(tx, id)
        })
        .await
    }

    /// Queued work in the order it runs: by priority, then oldest first.
    pub async fn queued_work(&self) -> Result<Vec<Work>, Error> {
        self.read(move |conn| work_where(conn, QUEUED_ORDER, [])).await
    }

    /// Running, queued and the `recent` latest finished work.
    pub async fn work_lists(&self, recent: u32) -> Result<WorkLists, Error> {
        self.read(move |conn| {
            Ok(WorkLists {
                running: work_where(conn, "WHERE status IN ('running', 'paused') ORDER BY id", [])?,
                queued: work_where(conn, QUEUED_ORDER, [])?,
                recent: work_where(
                    conn,
                    "WHERE status IN ('done', 'failed', 'stopped') ORDER BY coalesce(ended_at, created_at) DESC, id DESC \
                     LIMIT ?1",
                    [recent],
                )?,
            })
        })
        .await
    }

    /// At startup: work that was running when the app stopped goes back to the queue to resume from its checkpoint.
    /// Returns those items, now queued, with no cores.
    pub async fn requeue_running_work(&self) -> Result<Vec<Work>, Error> {
        self.write(move |tx| {
            let ids: Vec<i64> = {
                let mut stmt = tx.prepare("SELECT id FROM work WHERE status = 'running' ORDER BY id")?;
                stmt.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
            };
            tx.execute("UPDATE work SET status = 'queued', cores = 0 WHERE status = 'running'", [])?;
            ids.into_iter().map(|id| work_or_missing(tx, WorkId(id))).collect()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use chess_core::{GameId, PlayerKey};

    use super::*;
    use crate::testing;

    fn new(priority: Priority, title: &str) -> NewWork {
        NewWork {
            kind: WorkKind::Review,
            priority,
            title: title.into(),
            subject: Subject::Game { id: GameId(1) },
            spec: serde_json::json!({ "games": [1] }),
        }
    }

    #[tokio::test]
    async fn work_queues_by_priority_then_age() {
        let (store, _dir) = testing::store();
        let idea = store.insert_work(new(Priority::Background, "idea")).await.unwrap();
        let review = store.insert_work(new(Priority::Review, "review")).await.unwrap();
        let idea2 = store
            .insert_work(NewWork {
                subject: Subject::Player { key: PlayerKey::You },
                ..new(Priority::Background, "idea 2")
            })
            .await
            .unwrap();
        assert_eq!(idea.status, WorkStatus::Queued);
        let order: Vec<_> = store.queued_work().await.unwrap().into_iter().map(|w| w.id).collect();
        assert_eq!(order, [review.id, idea.id, idea2.id]);
        assert_eq!(store.work_spec(review.id).await.unwrap(), Some(serde_json::json!({ "games": [1] })));
        assert_eq!(store.work(idea2.id).await.unwrap().unwrap().subject, Subject::Player { key: PlayerKey::You });
    }

    #[tokio::test]
    async fn work_progresses_and_resumes_after_a_restart() {
        let (store, _dir) = testing::store();
        let w = store.insert_work(new(Priority::Review, "Reviewing 20 games")).await.unwrap();
        let done = store.insert_work(new(Priority::Review, "done")).await.unwrap();
        let progress = Progress { done: 3, total: Some(20), unit: "games".into(), eta_seconds: None, text: None };
        let running = store
            .update_work(
                w.id,
                WorkUpdate {
                    status: Some(WorkStatus::Running),
                    cores: Some(3),
                    progress: Some(progress.clone()),
                    paused_for: Some(Some("Your Lichess game".into())),
                    started_at: Some(now()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            (running.cores, running.progress, running.paused_for.as_deref()),
            (3, Some(progress), Some("Your Lichess game"))
        );
        let cleared =
            store.update_work(w.id, WorkUpdate { paused_for: Some(None), ..Default::default() }).await.unwrap();
        assert_eq!(cleared.paused_for, None);
        store.set_work_checkpoint(w.id, serde_json::json!({ "reviewed": [1, 2, 3] })).await.unwrap();
        store
            .update_work(
                done.id,
                WorkUpdate { status: Some(WorkStatus::Done), ended_at: Some(now()), ..Default::default() },
            )
            .await
            .unwrap();

        let lists = store.work_lists(10).await.unwrap();
        assert_eq!((lists.running.len(), lists.queued.len(), lists.recent.len()), (1, 0, 1));

        let resumed = store.requeue_running_work().await.unwrap();
        assert_eq!(
            resumed.iter().map(|w| (w.id, w.status, w.cores)).collect::<Vec<_>>(),
            [(w.id, WorkStatus::Queued, 0)]
        );
        assert_eq!(store.work_checkpoint(w.id).await.unwrap(), Some(serde_json::json!({ "reviewed": [1, 2, 3] })));
        assert!(store.requeue_running_work().await.unwrap().is_empty());
        assert_eq!(store.work_checkpoint(done.id).await.unwrap(), None);
        assert_eq!(store.update_work(WorkId(99), WorkUpdate::default()).await.unwrap_err().status(), 404);
    }
}
