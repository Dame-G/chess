//! The runner's database: one SQLite file in the data directory, in WAL mode, behind one connection. Schema changes
//! are numbered migrations applied in order; `PRAGMA user_version` records how far a database has come.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // 1: a key-value table for runner-wide settings.
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    // 2: builds and the job queue.
    "CREATE TABLE builds (
        id INTEGER PRIMARY KEY,
        key TEXT NOT NULL UNIQUE,
        ref TEXT NOT NULL,
        commit_sha TEXT NOT NULL,
        subject TEXT NOT NULL,
        features TEXT NOT NULL,
        network TEXT,
        bench INTEGER NOT NULL,
        bench_expected INTEGER,
        nps INTEGER NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );
    CREATE TABLE jobs (
        id INTEGER PRIMARY KEY,
        spec TEXT NOT NULL,
        status TEXT NOT NULL,
        result TEXT,
        error TEXT,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        started_at TEXT,
        finished_at TEXT
    );
    CREATE INDEX jobs_status ON jobs (status, id);",
    // 3: experiments, their matches, and the matches' progress.
    "CREATE TABLE experiments (
        id INTEGER PRIMARY KEY,
        batch TEXT,
        name TEXT NOT NULL,
        hypothesis TEXT NOT NULL DEFAULT '',
        kind TEXT NOT NULL,
        base_ref TEXT NOT NULL,
        dev_ref TEXT NOT NULL,
        base_build INTEGER REFERENCES builds (id),
        dev_build INTEGER REFERENCES builds (id),
        funnel TEXT NOT NULL,
        status TEXT NOT NULL,
        results TEXT NOT NULL,
        current TEXT,
        summary TEXT,
        job_id INTEGER REFERENCES jobs (id),
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        finished_at TEXT
    );
    CREATE INDEX experiments_batch ON experiments (batch, id);
    CREATE TABLE tests (
        id INTEGER PRIMARY KEY,
        experiment_id INTEGER NOT NULL REFERENCES experiments (id),
        stage TEXT NOT NULL,
        spec TEXT NOT NULL,
        base_build INTEGER NOT NULL REFERENCES builds (id),
        dev_build INTEGER NOT NULL REFERENCES builds (id),
        running INTEGER NOT NULL,
        penta TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        finished_at TEXT
    );
    CREATE TABLE test_points (
        test_id INTEGER NOT NULL REFERENCES tests (id),
        games INTEGER NOT NULL,
        llr REAL,
        nelo REAL,
        PRIMARY KEY (test_id, games)
    );",
    // 4: engine-vs-engine matches you can watch. `data` is the whole game as JSON, rewritten after every move.
    "CREATE TABLE matches (
        id INTEGER PRIMARY KEY,
        status TEXT NOT NULL,
        data TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );",
    // 5: SPSA tunes and every iteration's parameters, for trajectory charts. `inputs` is the request's parameter list;
    // `params` the resolved parameters with their current values.
    "CREATE TABLE tunes (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        ref TEXT NOT NULL,
        build_id INTEGER REFERENCES builds (id),
        inputs TEXT NOT NULL,
        params TEXT NOT NULL,
        spec TEXT NOT NULL,
        iterations INTEGER NOT NULL,
        pairs INTEGER NOT NULL,
        concurrency INTEGER NOT NULL,
        alpha REAL NOT NULL,
        gamma REAL NOT NULL,
        a_ratio REAL NOT NULL,
        iteration INTEGER NOT NULL,
        status TEXT NOT NULL,
        summary TEXT,
        job_id INTEGER REFERENCES jobs (id),
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        finished_at TEXT
    );
    CREATE TABLE tune_iterations (
        tune_id INTEGER NOT NULL REFERENCES tunes (id),
        iteration INTEGER NOT NULL,
        vals TEXT NOT NULL,
        penta TEXT NOT NULL,
        result INTEGER NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        PRIMARY KEY (tune_id, iteration)
    );",
    // 6: a deleted build keeps its row (experiments, tests, tunes and matches name it) but loses its binary. Its key
    // is renamed on deletion so a later build of the same tree is a new row.
    "ALTER TABLE builds ADD COLUMN deleted_at TEXT;",
    // 7: how a test's or a tune's clock was scaled to the machine's speed (`engine::scale::Calibration` as JSON), or
    // NULL when it was played as written.
    "ALTER TABLE tests ADD COLUMN scaling TEXT;
    ALTER TABLE tunes ADD COLUMN scaling TEXT;",
    // 8: game reviews (one per game, build and node budget; `summary` and `plies` as JSON) and the evaluations of
    // positions they searched, by FEN without move counters, build, nodes and MultiPV.
    "CREATE TABLE reviews (
        id INTEGER PRIMARY KEY,
        match_id INTEGER NOT NULL REFERENCES matches (id),
        build_id INTEGER NOT NULL REFERENCES builds (id),
        nodes INTEGER NOT NULL,
        summary TEXT NOT NULL,
        plies TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        UNIQUE (match_id, build_id, nodes)
    );
    CREATE INDEX reviews_match ON reviews (match_id, id);
    CREATE TABLE position_evals (
        fen TEXT NOT NULL,
        build_id INTEGER NOT NULL,
        nodes INTEGER NOT NULL,
        multipv INTEGER NOT NULL,
        data TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        PRIMARY KEY (fen, build_id, nodes, multipv)
    ) WITHOUT ROWID;",
];

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // With WAL, NORMAL loses at most the last transactions on a power cut and never corrupts; FULL syncs on every
        // commit, and a match is rewritten after every move.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Another process on the file (the sqlite3 shell, a backup) makes a write wait a little instead of failing.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Runs `f` on the connection off the async threads. A closure that panicked (an `expect` on a missing row) fails
    /// its own call only: the lock is taken back from the poison, and a transaction it left open rolled back as it
    /// unwound.
    pub async fn call<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || f(&mut conn.lock().unwrap_or_else(|e| e.into_inner()))).await?
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql).with_context(|| format!("migration {}", i + 1))?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrates_a_new_database() {
        let store = Store::memory().unwrap();
        let version: i64 = store.call(|c| Ok(c.pragma_query_value(None, "user_version", |r| r.get(0))?)).await.unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
    }

    #[tokio::test]
    async fn a_call_that_panics_does_not_break_the_next() {
        let store = Store::memory().unwrap();
        assert!(store.call(|_| -> Result<()> { panic!("a missing row") }).await.is_err());
        assert_eq!(store.call(|c| Ok(c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?)).await.unwrap(), 1);
    }
}
