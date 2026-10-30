//! Migrations: numbered SQL files embedded in the binary, applied in order. The number of the last one applied is the
//! database's `user_version`, so each runs once, and a database newer than this binary is refused rather than
//! misread.

use dameless_core::Error;
use rusqlite::Connection;

use crate::DbResult;

/// Every migration, in order. A new one is appended here, never edited in place once released.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_player.sql"),
    include_str!("../migrations/0003_training.sql"),
    include_str!("../migrations/0004_upstream.sql"),
    include_str!("../migrations/0005_error_rates.sql"),
    include_str!("../migrations/0006_drill_themes.sql"),
];

/// Applies the migrations the database has not seen, each in its own transaction.
pub(crate) fn apply(conn: &mut Connection) -> DbResult<()> {
    let applied: usize = conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
    if applied > MIGRATIONS.len() {
        return Err(Error::internal(format!(
            "the database is at schema {applied}, newer than this build's {}: use a newer dameless",
            MIGRATIONS.len()
        ))
        .into());
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(conn: &Connection, kind: &str) -> Vec<String> {
        let mut s = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = ?1 AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap();
        s.query_map([kind], |r| r.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
    }

    #[test]
    fn applies_every_migration_from_empty() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap(),
            MIGRATIONS.len() as i64
        );
        assert_eq!(
            names(&conn, "table"),
            [
                "builds",
                "connections",
                "datasets",
                "drill_answers",
                "drills",
                "games",
                "ideas",
                "level_games",
                "level_ratings",
                "meta",
                "nets",
                "players",
                "positions",
                "reviews",
                "train_steps",
                "trials",
                "tune_steps",
                "versions",
                "work"
            ]
        );
        let indexes = names(&conn, "index");
        for index in [
            "games_white",
            "games_black",
            "games_source",
            "games_trial",
            "games_lichess",
            "games_newest",
            "reviews_newest",
            "builds_cache",
            "work_status",
            "drills_due",
            "drills_kind",
            "drill_answers_at",
        ] {
            assert!(indexes.iter().any(|i| i == index), "{index} is missing");
        }
        // Applying again changes nothing.
        apply(&mut conn).unwrap();
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap(),
            MIGRATIONS.len() as i64
        );
    }

    #[test]
    fn refuses_a_newer_database() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(apply(&mut conn).is_err());
    }
}
