//! The engines list: one row per engine you added and per built-in once installed (a built-in that is not installed has
//! no row). Which engine analyses is `meta`'s `engines.analysis`.

use std::collections::BTreeMap;

use dameless_core::{Error, Timestamp, engines::EngineKind};
use rusqlite::{OptionalExtension, Row, params};

use crate::{DbResult, Store, from_json, from_word, now, to_json, word};

/// The meta key of the analysis engine's id.
pub const ANALYSIS_KEY: &str = "engines.analysis";

/// An engine as stored: what was registered, without what the app works out (installed, analysis, levels).
#[derive(Clone, Debug, PartialEq)]
pub struct EngineRecord {
    pub id: String,
    pub name: String,
    pub kind: EngineKind,
    pub path: String,
    pub args: Vec<String>,
    pub options: BTreeMap<String, String>,
    pub id_name: Option<String>,
    pub nodes: Option<u64>,
    pub rating: Option<u32>,
    pub created_at: Timestamp,
}

impl EngineRecord {
    /// A record made now.
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: EngineKind, path: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            path: path.into(),
            args: Vec::new(),
            options: BTreeMap::new(),
            id_name: None,
            nodes: None,
            rating: None,
            created_at: now(),
        }
    }
}

const ENGINE: &str = "SELECT id, name, kind, path, args, options, id_name, nodes, rating, created_at FROM engines";

fn from_row(row: &Row<'_>) -> DbResult<EngineRecord> {
    Ok(EngineRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: from_word(&row.get::<_, String>(2)?)?,
        path: row.get(3)?,
        args: from_json(&row.get::<_, String>(4)?)?,
        options: from_json(&row.get::<_, String>(5)?)?,
        id_name: row.get(6)?,
        nodes: row.get::<_, Option<i64>>(7)?.map(|n| n as u64),
        rating: row.get::<_, Option<i64>>(8)?.map(|n| n as u32),
        created_at: Timestamp(row.get(9)?),
    })
}

impl Store {
    /// Every stored engine, by name.
    pub async fn engines(&self) -> Result<Vec<EngineRecord>, Error> {
        self.read(|conn| {
            let mut stmt = conn.prepare_cached(&format!("{ENGINE} ORDER BY name COLLATE NOCASE, id"))?;
            let mut rows = stmt.query([])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(from_row(row)?);
            }
            Ok(out)
        })
        .await
    }

    pub async fn engine(&self, id: &str) -> Result<Option<EngineRecord>, Error> {
        let id = id.to_owned();
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(&format!("{ENGINE} WHERE id = ?1"))?;
            stmt.query_row([id], |r| Ok(from_row(r))).optional()?.transpose()
        })
        .await
    }

    /// Adds an engine, or replaces the one with its id (a built-in installed again keeps when it was first added).
    pub async fn put_engine(&self, e: EngineRecord) -> Result<(), Error> {
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO engines (id, name, kind, path, args, options, id_name, nodes, rating, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name, kind = excluded.kind, path = excluded.path, \
                 args = excluded.args, options = excluded.options, id_name = excluded.id_name, nodes = excluded.nodes, \
                 rating = excluded.rating",
                params![
                    e.id,
                    e.name,
                    word(&e.kind),
                    e.path,
                    to_json(&e.args)?,
                    to_json(&e.options)?,
                    e.id_name,
                    e.nodes.map(|n| n as i64),
                    e.rating.map(i64::from),
                    e.created_at.as_str(),
                ],
            )?;
            Ok(())
        })
        .await
    }

    /// Removes an engine; whether there was one.
    pub async fn delete_engine(&self, id: &str) -> Result<bool, Error> {
        let id = id.to_owned();
        self.write(move |tx| Ok(tx.execute("DELETE FROM engines WHERE id = ?1", [id])? > 0)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[tokio::test]
    async fn engines_are_added_replaced_and_removed() {
        let (store, _dir) = testing::store();
        assert!(store.engines().await.unwrap().is_empty());
        let mut sf = EngineRecord::new("stockfish", "Stockfish", EngineKind::Builtin, "/tools/stockfish/sf");
        sf.id_name = Some("Stockfish 19".into());
        store.put_engine(sf.clone()).await.unwrap();
        let mut maia = EngineRecord::new("maia-1500", "Maia 1500", EngineKind::Builtin, "/tools/lc0/lc0");
        maia.args = vec!["--weights=/tools/maia/maia-1500.pb.gz".into()];
        maia.options.insert("Threads".into(), "1".into());
        maia.nodes = Some(1);
        maia.rating = Some(1500);
        store.put_engine(maia.clone()).await.unwrap();
        assert_eq!(store.engines().await.unwrap(), [maia.clone(), sf.clone()]);

        let newer = EngineRecord { id_name: Some("Stockfish 20".into()), ..sf.clone() };
        store.put_engine(newer.clone()).await.unwrap();
        assert_eq!(store.engine("stockfish").await.unwrap(), Some(newer));
        assert!(store.delete_engine("maia-1500").await.unwrap());
        assert!(!store.delete_engine("maia-1500").await.unwrap());
        assert_eq!(store.engine("maia-1500").await.unwrap(), None);
    }
}
