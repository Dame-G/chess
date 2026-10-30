//! SQLite: schema, migrations, one writer, repositories.
//!
//! The database runs in WAL mode with foreign keys on. Every write goes through one thread that owns the only write
//! connection, so writes never fight over the lock and each is its own transaction; reads run on a small pool of
//! query-only connections beside it, which WAL lets see the last committed state while a write is going on. Callers
//! are on tokio: [`Store`]'s methods are async and do their blocking work off the runtime's threads.
//!
//! Repositories are typed with `core`'s types; no SQL leaves this crate. Each repository is a module of `impl Store`
//! methods: [`games`], [`reviews`], [`positions`], [`engines`], [`work`], [`records`] (connections, players, meta), and
//! for the player side [`drills`] (and their answers) and [`levels`] (games at a level, calibrated ratings).

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use dameless_core::{Error, Timestamp};
use rusqlite::{Connection, Transaction};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{Semaphore, oneshot};

pub mod drills;
pub mod engines;
pub mod games;
pub mod levels;
mod migrate;
pub mod positions;
pub mod records;
pub mod reviews;
pub mod work;

pub use engines::EngineRecord;
pub use games::{GameFilter, GameUpdate, NewGame, SourceKind};
pub use records::{ConnectionField, ConnectionRecord};
pub use work::{NewWork, WorkLists, WorkUpdate};

/// Read connections kept open beside the writer.
const READERS: usize = 4;

/// How long a connection waits for a lock before giving up. With one writer this only matters for checkpoints and
/// for another process opening the file.
const BUSY_TIMEOUT: Duration = Duration::from_secs(10);

/// What a closure run on a connection can fail with. It becomes `core`'s [`Error`] when it leaves the store: as
/// `internal`, unless it already is a domain error.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DbError {
    #[error("database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("stored JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("stored blob: {0}")]
    Blob(#[from] std::io::Error),
    #[error(transparent)]
    Domain(#[from] Error),
}

impl From<DbError> for Error {
    fn from(e: DbError) -> Self {
        match e {
            DbError::Domain(e) => e,
            other => Error::internal(other.to_string()),
        }
    }
}

pub(crate) type DbResult<T> = Result<T, DbError>;

type WriteJob = Box<dyn FnOnce(&mut Connection) + Send>;

/// The database. Cheap to clone: every clone shares the writer thread and the readers.
#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    writer: Mutex<mpsc::Sender<WriteJob>>,
    readers: Mutex<Vec<Connection>>,
    permits: Semaphore,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("path", &self.inner.path).finish_non_exhaustive()
    }
}

impl Store {
    /// Opens (or creates) the database at `path`, applies the migrations it has not seen, and starts the writer.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        let open = |e: rusqlite::Error| Error::internal(format!("open {}: {e}", path.display()));
        let mut writer = Connection::open(&path).map_err(open)?;
        configure(&writer).map_err(open)?;
        writer.pragma_update(None, "journal_mode", "WAL").map_err(open)?;
        writer.pragma_update(None, "synchronous", "NORMAL").map_err(open)?;
        migrate::apply(&mut writer).map_err(Error::from)?;

        let mut readers = Vec::with_capacity(READERS);
        for _ in 0..READERS {
            let conn = Connection::open(&path).map_err(open)?;
            configure(&conn).map_err(open)?;
            conn.pragma_update(None, "query_only", true).map_err(open)?;
            readers.push(conn);
        }

        let (tx, rx) = mpsc::channel::<WriteJob>();
        std::thread::Builder::new()
            .name("dameless-store-writer".into())
            .spawn(move || {
                // Runs until every clone of the store is dropped and the channel closes.
                while let Ok(job) = rx.recv() {
                    job(&mut writer);
                }
            })
            .map_err(|e| Error::internal(format!("start the database writer: {e}")))?;

        Ok(Self {
            inner: Arc::new(Inner {
                path,
                writer: Mutex::new(tx),
                readers: Mutex::new(readers),
                permits: Semaphore::new(READERS),
            }),
        })
    }

    /// The database file.
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// Runs `f` in a transaction on the write connection, after every write queued before it. The transaction commits
    /// when `f` returns `Ok` and rolls back otherwise.
    pub(crate) async fn write<T, F>(&self, f: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction) -> DbResult<T> + Send + 'static,
    {
        let (done, result) = oneshot::channel();
        let job: WriteJob = Box::new(move |conn| {
            let outcome = (|| -> DbResult<T> {
                let tx = conn.transaction()?;
                let value = f(&tx)?;
                tx.commit()?;
                Ok(value)
            })();
            let _ = done.send(outcome);
        });
        let sent = self.inner.writer.lock().unwrap_or_else(|p| p.into_inner()).send(job);
        if sent.is_err() {
            return Err(Error::internal("the database writer has stopped"));
        }
        match result.await {
            Ok(outcome) => outcome.map_err(Error::from),
            Err(_) => Err(Error::internal("the database writer stopped during a write")),
        }
    }

    /// Runs `f` on one of the read connections, off the runtime's threads.
    pub(crate) async fn read<T, F>(&self, f: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> DbResult<T> + Send + 'static,
    {
        let _permit = self.inner.permits.acquire().await.map_err(|_| Error::internal("the database is closed"))?;
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            // Holding a permit means a connection is free.
            let conn = inner.readers.lock().unwrap_or_else(|p| p.into_inner()).pop();
            let Some(conn) = conn else { return Err(Error::internal("no free database reader")) };
            let outcome = f(&conn);
            inner.readers.lock().unwrap_or_else(|p| p.into_inner()).push(conn);
            outcome.map_err(Error::from)
        })
        .await
        .map_err(|e| Error::internal(format!("a database read panicked: {e}")))?
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

/// Now, as the store writes timestamps.
pub fn now() -> Timestamp {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();
    Timestamp::from_unix_ms(ms)
}

// ---------------------------------------------------------------------------------------------------------------------
// Column helpers
// ---------------------------------------------------------------------------------------------------------------------

/// A value as a JSON column.
pub(crate) fn to_json<T: Serialize + ?Sized>(value: &T) -> DbResult<String> {
    Ok(serde_json::to_string(value)?)
}

/// A JSON column as a value.
pub(crate) fn from_json<T: DeserializeOwned>(text: &str) -> DbResult<T> {
    Ok(serde_json::from_str(text)?)
}

/// An optional JSON column.
pub(crate) fn opt_json<T: Serialize>(value: &Option<T>) -> DbResult<Option<String>> {
    value.as_ref().map(to_json).transpose()
}

pub(crate) fn from_opt_json<T: DeserializeOwned>(text: Option<String>) -> DbResult<Option<T>> {
    text.as_deref().map(from_json).transpose()
}

/// A plain-string enum (`finished`, `white_wins`) as its serde name, for the columns the database filters on.
pub(crate) fn word<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

/// A word column back into its enum.
pub(crate) fn from_word<T: DeserializeOwned>(text: &str) -> DbResult<T> {
    Ok(serde_json::from_value(serde_json::Value::String(text.to_owned()))?)
}

pub(crate) fn from_opt_word<T: DeserializeOwned>(text: Option<String>) -> DbResult<Option<T>> {
    text.as_deref().map(from_word).transpose()
}

/// A value as zstd-compressed JSON: how a game's moves and a review's plies are kept, one blob per row.
pub(crate) fn pack<T: Serialize + ?Sized>(value: &T) -> DbResult<Vec<u8>> {
    let json = serde_json::to_vec(value)?;
    Ok(zstd::bulk::compress(&json, 3)?)
}

pub(crate) fn unpack<T: DeserializeOwned>(blob: &[u8]) -> DbResult<T> {
    let json = zstd::decode_all(blob)?;
    Ok(serde_json::from_slice(&json)?)
}

#[cfg(test)]
pub(crate) mod testing {
    use super::Store;

    /// A store in a fresh temporary directory, removed when the guard drops.
    pub fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("dameless.db")).unwrap();
        (store, dir)
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::game::{Move, Outcome};

    use super::*;

    #[tokio::test]
    async fn writes_commit_and_failed_writes_roll_back() {
        let (store, _dir) = testing::store();
        store
            .write(|tx| {
                tx.execute("INSERT INTO meta (key, value) VALUES ('a', '1')", [])?;
                Ok(())
            })
            .await
            .unwrap();
        let failed = store
            .write(|tx| -> DbResult<()> {
                tx.execute("INSERT INTO meta (key, value) VALUES ('b', '2')", [])?;
                Err(Error::conflict("no").into())
            })
            .await;
        assert_eq!(failed.unwrap_err(), Error::conflict("no"));
        let keys = store
            .read(|c| {
                let mut s = c.prepare("SELECT key FROM meta ORDER BY key")?;
                let keys = s.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                Ok(keys)
            })
            .await
            .unwrap();
        assert_eq!(keys, ["a"]);
    }

    #[tokio::test]
    async fn readers_cannot_write() {
        let (store, _dir) = testing::store();
        let r = store.read(|c| Ok(c.execute("INSERT INTO meta (key, value) VALUES ('x', '1')", [])?)).await;
        assert!(r.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn many_reads_at_once_share_the_pool() {
        let (store, _dir) = testing::store();
        let handles: Vec<_> = (0..32i64)
            .map(|i| {
                let s = store.clone();
                tokio::spawn(async move {
                    s.read(move |c| Ok(c.query_row("SELECT ?1", [i], |r| r.get::<_, i64>(0))?)).await
                })
            })
            .collect();
        for (i, h) in handles.into_iter().enumerate() {
            assert_eq!(h.await.unwrap().unwrap(), i as i64);
        }
    }

    #[test]
    fn blobs_and_words_round_trip() {
        let moves = vec![Move::new("e2e4", "e4")];
        let blob = pack(&moves).unwrap();
        assert_eq!(unpack::<Vec<Move>>(&blob).unwrap(), moves);
        assert_eq!(word(&Outcome::WhiteWins), "white_wins");
        assert_eq!(from_word::<Outcome>("draw").unwrap(), Outcome::Draw);
    }
}
