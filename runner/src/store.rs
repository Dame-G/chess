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
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Runs `f` on the connection off the async threads.
    pub async fn call<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || f(&mut conn.lock().unwrap())).await?
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
}
