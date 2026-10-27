//! Drills and their answers. A drill is unique by its position's key, so the same weak spot found twice (or in two
//! games) stays one drill. Grading is `core::drills::grade`; this stores what it decides, together with the answer, in
//! one transaction.

use std::collections::HashSet;

use dameless_core::{
    Error, GameId, Timestamp,
    drills::{Drill, DrillAnswer, DrillId, NewDrill, Theme},
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{DbResult, Store, from_json, from_word, now, to_json, word};

const DRILL: &str = "SELECT id, game, ply, fen, key, side, best_moves, best_san, line, played, played_san, swing, \
                     opening, phase, due, interval_days, ease, reps, lapses, created_at, answered_at FROM drills";

fn drill_from_row(row: &Row<'_>) -> DbResult<Drill> {
    Ok(Drill {
        id: DrillId(row.get(0)?),
        game: GameId(row.get(1)?),
        ply: row.get(2)?,
        fen: row.get(3)?,
        key: row.get(4)?,
        side: from_word(&row.get::<_, String>(5)?)?,
        best_moves: from_json(&row.get::<_, String>(6)?)?,
        best_san: from_json(&row.get::<_, String>(7)?)?,
        line: from_json(&row.get::<_, String>(8)?)?,
        played: row.get(9)?,
        played_san: row.get(10)?,
        swing: row.get(11)?,
        theme: Theme { opening: row.get(12)?, phase: from_word(&row.get::<_, String>(13)?)? },
        due: Timestamp(row.get(14)?),
        interval_days: row.get(15)?,
        ease: row.get(16)?,
        reps: row.get(17)?,
        lapses: row.get(18)?,
        created_at: Timestamp(row.get(19)?),
        answered_at: row.get::<_, Option<String>>(20)?.map(Timestamp),
    })
}

fn get(conn: &Connection, id: DrillId) -> DbResult<Option<Drill>> {
    let mut stmt = conn.prepare_cached(&format!("{DRILL} WHERE id = ?1"))?;
    let mut rows = stmt.query([id.0])?;
    rows.next()?.map(drill_from_row).transpose()
}

impl Store {
    /// Stores new drills, due at once. A drill whose position is already a drill is skipped. Returns the drills
    /// stored, in the order given.
    pub async fn insert_drills(&self, drills: Vec<NewDrill>) -> Result<Vec<Drill>, Error> {
        self.write(move |tx| {
            let created_at = now();
            let mut out = Vec::new();
            for d in drills {
                let inserted = tx.execute(
                    "INSERT INTO drills (key, game, ply, fen, side, best_moves, best_san, line, played, played_san, \
                     swing, opening, phase, due, interval_days, ease, reps, lapses, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0, ?15, 0, 0, ?14) \
                     ON CONFLICT (key) DO NOTHING",
                    params![
                        d.key,
                        d.game.0,
                        d.ply,
                        d.fen,
                        word(&d.side),
                        to_json(&d.best_moves)?,
                        to_json(&d.best_san)?,
                        to_json(&d.line)?,
                        d.played,
                        d.played_san,
                        d.swing,
                        d.theme.opening,
                        word(&d.theme.phase),
                        created_at.as_str(),
                        dameless_core::drills::START_EASE,
                    ],
                )?;
                if inserted == 1 {
                    out.push(d.into_drill(DrillId(tx.last_insert_rowid()), created_at.clone()));
                }
            }
            Ok(out)
        })
        .await
    }

    pub async fn drill(&self, id: DrillId) -> Result<Option<Drill>, Error> {
        self.read(move |conn| get(conn, id)).await
    }

    /// Every drill, the soonest due first.
    pub async fn drills(&self) -> Result<Vec<Drill>, Error> {
        self.read(|conn| {
            let mut stmt = conn.prepare_cached(&format!("{DRILL} ORDER BY due, id"))?;
            let mut rows = stmt.query([])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(drill_from_row(row)?);
            }
            Ok(out)
        })
        .await
    }

    /// The position keys that are drills already, for `core::drills::dedupe`.
    pub async fn drill_keys(&self) -> Result<HashSet<String>, Error> {
        self.read(|conn| {
            let mut stmt = conn.prepare_cached("SELECT key FROM drills")?;
            let keys = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<HashSet<_>, _>>()?;
            Ok(keys)
        })
        .await
    }

    /// Stores an answer and the drill's schedule after it (`graded`, from `core::drills::grade`), together. Returns the
    /// drill as stored; not found when the drill is gone.
    pub async fn record_drill_answer(&self, graded: Drill, answer: DrillAnswer) -> Result<Drill, Error> {
        self.write(move |tx| {
            if graded.id != answer.drill {
                return Err(Error::bad_request("the answer is for another drill").into());
            }
            let changed = tx.execute(
                "UPDATE drills SET due = ?2, interval_days = ?3, ease = ?4, reps = ?5, lapses = ?6, answered_at = ?7 \
                 WHERE id = ?1",
                params![
                    graded.id.0,
                    graded.due.as_str(),
                    graded.interval_days,
                    graded.ease,
                    graded.reps,
                    graded.lapses,
                    graded.answered_at.as_ref().map(Timestamp::as_str),
                ],
            )?;
            if changed == 0 {
                return Err(Error::not_found(format!("no drill {}", graded.id)).into());
            }
            let first = tx
                .query_row("SELECT 1 FROM drill_answers WHERE drill = ?1 LIMIT 1", [graded.id.0], |_| Ok(()))
                .optional()?
                .is_none();
            tx.execute(
                "INSERT INTO drill_answers (drill, at, answer, outcome, first) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![answer.drill.0, answer.at.as_str(), answer.answer, word(&answer.outcome), first],
            )?;
            Ok(get(tx, graded.id)?.expect("updated above"))
        })
        .await
    }

    /// Answers, oldest first; those at or after `since` when it is given.
    pub async fn drill_answers(&self, since: Option<Timestamp>) -> Result<Vec<DrillAnswer>, Error> {
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT drill, at, answer, outcome FROM drill_answers WHERE (?1 IS NULL OR at >= ?1) ORDER BY at, id",
            )?;
            let mut rows = stmt.query([since.as_ref().map(Timestamp::as_str)])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(DrillAnswer {
                    drill: DrillId(row.get(0)?),
                    at: Timestamp(row.get(1)?),
                    answer: row.get(2)?,
                    outcome: from_word(&row.get::<_, String>(3)?)?,
                });
            }
            Ok(out)
        })
        .await
    }

    /// New drills started (answered for the first time) at or after `since`: what counts against the day's cap.
    pub async fn drills_started_since(&self, since: Timestamp) -> Result<u32, Error> {
        self.read(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM drill_answers WHERE first = 1 AND at >= ?1",
                [since.as_str()],
                |r| r.get::<_, u32>(0),
            )?)
        })
        .await
    }

    /// Removes a drill and its answers. Whether there was one.
    pub async fn delete_drill(&self, id: DrillId) -> Result<bool, Error> {
        self.write(move |tx| Ok(tx.execute("DELETE FROM drills WHERE id = ?1", [id.0])? == 1)).await
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::{
        PlayerKey, Side,
        drills::{DrillOutcome, check, grade, today},
        review::judge::Phase,
    };

    use super::*;
    use crate::{games::tests::local, testing};

    fn new(game: GameId, key: &str, swing: f64) -> NewDrill {
        NewDrill {
            game,
            ply: 7,
            fen: format!("{key} 0 1"),
            key: key.into(),
            side: Side::White,
            best_moves: vec!["e2e4".into(), "d2d4".into()],
            best_san: vec!["e4".into(), "d4".into()],
            line: vec!["e7e5".into()],
            played: "a2a3".into(),
            played_san: "a3".into(),
            swing,
            theme: Theme { opening: Some("Italian Game".into()), phase: Phase::Opening },
        }
    }

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -";
    const OTHER: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq -";

    #[tokio::test]
    async fn drills_are_stored_once_per_position() {
        let (store, _dir) = testing::store();
        let game = store.insert_game(local(PlayerKey::You, PlayerKey::Dameless, 0)).await.unwrap();
        let first = store.insert_drills(vec![new(game, START, 10.0), new(game, START, 20.0)]).await.unwrap();
        assert_eq!(first.len(), 1);
        let again = store.insert_drills(vec![new(game, START, 30.0), new(game, OTHER, 5.0)]).await.unwrap();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].key, OTHER);

        let all = store.drills().await.unwrap();
        assert_eq!(all.len(), 2);
        let stored = store.drill(first[0].id).await.unwrap().unwrap();
        assert_eq!(stored, first[0]);
        assert_eq!(stored.theme.phase, Phase::Opening);
        assert!(stored.is_new());
        assert_eq!(store.drill_keys().await.unwrap(), [START.to_owned(), OTHER.to_owned()].into());

        assert!(store.delete_drill(stored.id).await.unwrap());
        assert!(!store.delete_drill(stored.id).await.unwrap());
        assert!(store.drill(stored.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn answers_move_the_schedule_and_count_new_drills_started() {
        let (store, _dir) = testing::store();
        let game = store.insert_game(local(PlayerKey::You, PlayerKey::Dameless, 0)).await.unwrap();
        let d = store.insert_drills(vec![new(game, START, 10.0), new(game, OTHER, 5.0)]).await.unwrap();
        let t0 = Timestamp::from_unix_ms(1_000_000);

        let answer = |drill: &Drill, mv: &str, at: &Timestamp| DrillAnswer {
            drill: drill.id,
            at: at.clone(),
            answer: mv.into(),
            outcome: check(drill, mv),
        };
        let a = answer(&d[0], "e2e4", &t0);
        assert_eq!(a.outcome, DrillOutcome::Pass);
        let graded = grade(&d[0], a.outcome, &t0);
        let stored = store.record_drill_answer(graded.clone(), a).await.unwrap();
        assert_eq!(stored, graded);
        assert_eq!((stored.reps, stored.interval_days), (1, 1));

        let t1 = Timestamp::from_unix_ms(1_000_000 + 86_400_000);
        let b = answer(&stored, "h2h4", &t1);
        let graded = grade(&stored, b.outcome, &t1);
        let stored = store.record_drill_answer(graded, b).await.unwrap();
        assert_eq!((stored.reps, stored.lapses), (0, 1));

        let answers = store.drill_answers(None).await.unwrap();
        assert_eq!(answers.iter().map(|a| a.outcome).collect::<Vec<_>>(), [DrillOutcome::Pass, DrillOutcome::Fail]);
        assert_eq!(store.drill_answers(Some(t1.clone())).await.unwrap().len(), 1);
        // Only the first answer started a new drill.
        assert_eq!(store.drills_started_since(t0.clone()).await.unwrap(), 1);
        assert_eq!(store.drills_started_since(t1.clone()).await.unwrap(), 0);

        let all = store.drills().await.unwrap();
        let created = all[0].created_at.unix_ms().unwrap();
        // Today's set: the tried drill (due again since its miss) first, then the new one while the cap allows.
        let later = Timestamp::from_unix_ms(created + 2 * 86_400_000);
        assert_eq!(today(&all, &later, 10, 1, 30), [d[0].id, d[1].id]);
        assert_eq!(today(&all, &later, 1, 1, 30), [d[0].id]);

        // A mismatched answer or a drill that is gone is refused.
        let wrong = DrillAnswer { drill: d[1].id, ..answer(&stored, "e2e4", &t1) };
        assert!(store.record_drill_answer(stored.clone(), wrong).await.is_err());
        store.delete_drill(stored.id).await.unwrap();
        let gone = answer(&stored, "e2e4", &t1);
        assert!(store.record_drill_answer(stored, gone).await.is_err());
        assert_eq!(store.drill_answers(None).await.unwrap().len(), 0);
    }
}
