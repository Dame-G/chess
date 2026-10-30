//! Migrations: numbered SQL files embedded in the binary, applied in order. The number of the last one applied is the
//! database's `user_version`, so each runs once, and a database newer than this binary is refused rather than
//! misread.
//!
//! They run with foreign keys off, as SQLite asks for rebuilding a table (a new table, the rows copied, the old one
//! dropped and the new one renamed): with them on, dropping the old table would delete every row that points at it. A
//! migration deletes what it orphans itself, and the foreign keys are checked once all have run.

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
    include_str!("../migrations/0007_engines.sql"),
];

/// Applies the migrations the database has not seen, each in its own transaction.
pub(crate) fn apply(conn: &mut Connection) -> DbResult<()> {
    apply_to(conn, MIGRATIONS.len())
}

/// Applies migrations up to `to` (a count), for the tests of a migration on an older schema.
fn apply_to(conn: &mut Connection, to: usize) -> DbResult<()> {
    let applied: usize = conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize;
    if applied > MIGRATIONS.len() {
        return Err(Error::internal(format!(
            "the database is at schema {applied}, newer than this build's {}: use a newer dameless",
            MIGRATIONS.len()
        ))
        .into());
    }
    if applied >= to {
        return Ok(());
    }
    let keys: bool = conn.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;
    let outcome = (|| -> DbResult<()> {
        for (i, sql) in MIGRATIONS.iter().enumerate().take(to).skip(applied) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        let broken: i64 = conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?;
        if broken > 0 {
            return Err(
                Error::internal(format!("the migrated database has {broken} rows with dangling references")).into()
            );
        }
        Ok(())
    })();
    conn.pragma_update(None, "foreign_keys", keys)?;
    outcome
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

    fn version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).unwrap()
    }

    #[test]
    fn applies_every_migration_from_empty() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        apply(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
        assert_eq!(
            names(&conn, "table"),
            [
                "connections",
                "drill_answers",
                "drills",
                "engines",
                "games",
                "level_games",
                "level_ratings",
                "meta",
                "players",
                "positions",
                "reviews",
                "work"
            ]
        );
        let indexes = names(&conn, "index");
        for index in [
            "games_white",
            "games_black",
            "games_source",
            "games_lichess",
            "games_newest",
            "reviews_newest",
            "work_status",
            "drills_due",
            "drills_kind",
            "drill_answers_at",
        ] {
            assert!(indexes.iter().any(|i| i == index), "{index} is missing");
        }
        // Foreign keys are back on, and applying again changes nothing.
        assert!(conn.pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0)).unwrap());
        apply(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn refuses_a_newer_database() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(apply(&mut conn).is_err());
    }

    /// A database from before the engines list: Dameless's games, a review by a version, a trial's game, the bot's
    /// Lichess game, a game against Lichess's AI, a calibration game and the rest that migration 7 rewrites.
    #[test]
    fn the_engines_migration_keeps_your_games_and_drops_engine_work() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        apply_to(&mut conn, 6).unwrap();
        let t = "2026-01-01T00:00:00.000Z";
        let seat = |player: &str, name: &str, version: &str| {
            format!(
                r#"{{"player":"{player}","name":"{name}","level":null,"version":{version},"build":null,"rating":null,"title":null}}"#
            )
        };
        let you = seat("you", "You", "null");
        let dameless = seat("dameless", "Dameless v3", "1");
        let ai = seat("stockfish:5", "Stockfish level 5", "null");
        conn.execute_batch(&format!(
            r#"
            INSERT INTO versions (id, name, commit_sha, line, line_kind, created_at)
                VALUES (1, 'v3', 'abcdef1234567890', '{{"type":"champion","number":3}}', 'champion', '{t}');
            INSERT INTO ideas (id, title, hypothesis, kind, author, branch, base, status, tags, spec, created_at)
                VALUES (1, 'x', 'y', 'gain', 'you', 'b', 1, 'kept', '[]', '{{}}', '{t}');
            INSERT INTO trials (id, idea, stage, ord, status, penta, wdl) VALUES (1, 1, 'short', 0, 'passed', '[]', '[]');
            INSERT INTO players (key, name, ratings, updated_at) VALUES ('dameless', 'Dameless', '{{}}', '{t}');
            INSERT INTO connections (key, updated_at) VALUES ('bot', '{t}'), ('you', '{t}');
            INSERT INTO meta (key, value) VALUES ('upstream.last', '{{}}'), ('lichess.you.import.complete', 'true');
            INSERT INTO work (kind, status, priority, title, subject, spec, created_at)
                VALUES ('trial', 'done', 3, 't', '{{}}', '{{}}', '{t}'), ('import', 'done', 1, 'i', '{{}}', '{{}}', '{t}');
            "#
        ))
        .unwrap();
        let game = |id: i64, kind: &str, source: &str, white: &str, black: &str, trial: &str| {
            let key =
                |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap()["player"].as_str().unwrap().to_owned();
            conn.execute(
                &format!(
                    "INSERT INTO games (id, source_kind, source, source_owner, white, black, white_key, black_key, time, \
                     rated, status, live, plies, moves, trial, started_at) \
                     VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, '{{}}', 0, 'finished', 0, 0, x'', {trial}, '{t}')"
                ),
                rusqlite::params![id, kind, source, white, black, key(white), key(black)],
            )
            .unwrap();
        };
        game(1, "local", r#"{"type":"local"}"#, &you, &dameless, "NULL");
        game(2, "trial", r#"{"type":"trial","trial":1,"index":0}"#, &dameless, &dameless, "1");
        game(
            3,
            "lichess",
            r#"{"type":"lichess","connection":"bot","id":"abcd1234","rated":true,"speed":"blitz"}"#,
            &dameless,
            &seat("lichess:bob", "bob", "null"),
            "NULL",
        );
        game(
            4,
            "lichess",
            r#"{"type":"lichess","connection":"you","id":"q","rated":false,"speed":"rapid"}"#,
            &you,
            &ai,
            "NULL",
        );
        game(5, "local", r#"{"type":"local"}"#, &dameless, &dameless, "NULL");
        conn.execute_batch(&format!(
            r#"
            INSERT INTO level_games (game, white, black, calibration) VALUES (5, 'level:1', 'level:2', 1);
            INSERT INTO level_ratings (entrant, elo, low, high, games, updated_at) VALUES ('level:1', 1, 0, 2, 3, '{t}');
            INSERT INTO reviews (game, version, nodes, summary, plies, created_at)
                VALUES (1, 1, 200000, '{{"version":1,"nodes":200000}}', x'', '{t}'),
                       (2, 1, 200000, '{{"version":1,"nodes":200000}}', x'', '{t}');
            INSERT INTO positions (key, commit_sha, nodes, multipv, result) VALUES ('k', 'abc', 1, 1, '{{}}');
            "#
        ))
        .unwrap();

        apply(&mut conn).unwrap();
        assert_eq!(version(&conn), MIGRATIONS.len() as i64);
        let one = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, String>(0)).unwrap();
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();

        // Trial and calibration games are gone; yours, the bot's and the one against Lichess's AI stay.
        let ids: String = conn
            .query_row("SELECT group_concat(id, ',') FROM (SELECT id FROM games ORDER BY id)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ids, "1,3,4");
        // Dameless's seat is Reckless's, labelled with the commit it played.
        assert_eq!(one("SELECT black_key FROM games WHERE id = 1"), "engine:reckless");
        assert_eq!(one("SELECT json_extract(black, '$.name') FROM games WHERE id = 1"), "Reckless v3");
        assert_eq!(one("SELECT json_extract(black, '$.engine') FROM games WHERE id = 1"), "Reckless abcdef1");
        assert_eq!(count("SELECT count(*) FROM games WHERE json_extract(black, '$.version') IS NOT NULL"), 0);
        // The bot's game is Reckless's imported history.
        assert_eq!(one("SELECT source_kind || ' ' || source_owner FROM games WHERE id = 3"), "import engine:reckless");
        let source: serde_json::Value = serde_json::from_str(&one("SELECT source FROM games WHERE id = 3")).unwrap();
        assert_eq!(
            source,
            serde_json::json!({"type":"import","owner":"engine:reckless","id":"abcd1234","rated":true,"speed":"blitz"})
        );
        // Lichess's AI is the Stockfish engine at its level.
        assert_eq!(one("SELECT black_key FROM games WHERE id = 4"), "engine:stockfish");
        assert_eq!(count("SELECT json_extract(black, '$.level') FROM games WHERE id = 4"), 5);
        // The review of your game stays, keyed and labelled by what made it.
        assert_eq!(one("SELECT engine FROM reviews"), "reckless|Reckless abcdef1");
        assert_eq!(one("SELECT json_extract(summary, '$.engine') FROM reviews"), "Reckless abcdef1");
        assert_eq!(count("SELECT count(*) FROM reviews"), 1);
        assert_eq!(count("SELECT count(*) FROM positions"), 0);
        assert_eq!(count("SELECT count(*) FROM level_games") + count("SELECT count(*) FROM level_ratings"), 0);
        assert_eq!(count("SELECT count(*) FROM players"), 0);
        assert_eq!(one("SELECT group_concat(key) FROM connections"), "you");
        assert_eq!(one("SELECT group_concat(key) FROM meta"), "lichess.you.import.complete");
        assert_eq!(one("SELECT kind || ' ' || priority FROM work"), "import 2");
    }
}
