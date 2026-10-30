//! The position cache: every search a review makes, by position (FEN without the move counters), the engine,
//! the node budget and MultiPV, so a position shared by many games (openings, mostly) is searched once.

use std::collections::HashMap;

use dameless_core::{Error, review::PositionEval};
use rusqlite::{OptionalExtension, params};

use crate::{Store, from_json, to_json};

/// What a cached search is keyed by besides its position.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SearchKey {
    /// The engine searched with: its cache key (`core::engines::Engine::cache_key`), `stockfish|Stockfish 19`.
    pub engine: String,
    pub nodes: u64,
    pub multipv: u32,
}

impl Store {
    /// The cached search of one position.
    pub async fn position(&self, key: &str, search: &SearchKey) -> Result<Option<PositionEval>, Error> {
        let (key, search) = (key.to_owned(), search.clone());
        self.read(move |conn| {
            let text: Option<String> = conn
                .query_row(
                    "SELECT result FROM positions WHERE key = ?1 AND engine = ?2 AND nodes = ?3 AND multipv = ?4",
                    params![key, search.engine, search.nodes as i64, search.multipv],
                    |r| r.get(0),
                )
                .optional()?;
            text.as_deref().map(from_json).transpose()
        })
        .await
    }

    /// The cached searches of many positions at once; positions not cached are missing from the map.
    pub async fn positions(
        &self,
        keys: Vec<String>,
        search: &SearchKey,
    ) -> Result<HashMap<String, PositionEval>, Error> {
        let search = search.clone();
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT result FROM positions WHERE key = ?1 AND engine = ?2 AND nodes = ?3 AND multipv = ?4",
            )?;
            let mut out = HashMap::new();
            for key in keys {
                let text: Option<String> = stmt
                    .query_row(params![key, search.engine, search.nodes as i64, search.multipv], |r| r.get(0))
                    .optional()?;
                if let Some(text) = text {
                    out.insert(key, from_json(&text)?);
                }
            }
            Ok(out)
        })
        .await
    }

    /// Caches searches, replacing any of the same keys.
    pub async fn put_positions(&self, evals: Vec<(String, PositionEval)>, search: &SearchKey) -> Result<(), Error> {
        let search = search.clone();
        self.write(move |tx| {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO positions (key, engine, nodes, multipv, result) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (key, eval) in &evals {
                stmt.execute(params![key, search.engine, search.nodes as i64, search.multipv, to_json(eval)?])?;
            }
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::{Score, review::EvalLine};

    use super::*;
    use crate::testing;

    #[tokio::test]
    async fn caches_by_position_engine_nodes_and_multipv() {
        let (store, _dir) = testing::store();
        let search = SearchKey { engine: "stockfish|Stockfish 19".into(), nodes: 200_000, multipv: 2 };
        let eval = PositionEval {
            lines: vec![EvalLine { uci: "e2e4".into(), score: Score::Cp(30), pv: vec!["e2e4".into(), "e7e5".into()] }],
            depth: 14,
            nodes: 200_100,
        };
        assert!(store.position("start", &search).await.unwrap().is_none());
        store.put_positions(vec![("start".into(), eval.clone())], &search).await.unwrap();
        assert_eq!(store.position("start", &search).await.unwrap(), Some(eval.clone()));
        let other = SearchKey { multipv: 1, ..search.clone() };
        assert!(store.position("start", &other).await.unwrap().is_none());
        let found = store.positions(vec!["start".into(), "elsewhere".into()], &search).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found["start"], eval);
    }
}
