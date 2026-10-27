//! Reviews: one row per game, version and node budget, the summary as JSON and the judged plies as a compressed blob.
//! Each side's accuracy has its own column, so game lists show it without reading the summary.

use dameless_core::{
    Error, GameId, PlayerKey, ReviewId, Timestamp, VersionId,
    game::GameStatus,
    profile::{ProfileFilter, ProfileGame},
    review::{Review, ReviewSummary, ReviewedMove},
};
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};

use crate::{DbResult, Store, from_json, games, now, pack, to_json, unpack, word};

const REVIEW: &str = "SELECT id, game, version, nodes, summary, created_at, plies FROM reviews";

/// A review row; its plies are read only when `plies` is set, else left empty.
fn review_from_row(row: &Row<'_>, plies: bool) -> DbResult<Review> {
    Ok(Review {
        id: ReviewId(row.get(0)?),
        game: GameId(row.get(1)?),
        version: VersionId(row.get(2)?),
        nodes: row.get::<_, i64>(3)? as u64,
        summary: from_json(&row.get::<_, String>(4)?)?,
        created_at: Timestamp(row.get(5)?),
        plies: if plies { unpack(&row.get::<_, Vec<u8>>(6)?)? } else { Vec::new() },
    })
}

fn newest(conn: &Connection, game: GameId, plies: bool) -> DbResult<Option<Review>> {
    let mut stmt =
        conn.prepare_cached(&format!("{REVIEW} WHERE game = ?1 ORDER BY created_at DESC, id DESC LIMIT 1"))?;
    let mut rows = stmt.query([game.0])?;
    rows.next()?.map(|r| review_from_row(r, plies)).transpose()
}

impl Store {
    /// Stores a review, replacing the one of the same game, version and budget. Returns it as stored.
    pub async fn put_review(
        &self,
        game: GameId,
        summary: ReviewSummary,
        plies: Vec<ReviewedMove>,
    ) -> Result<Review, Error> {
        self.write(move |tx| {
            let accuracy = summary.accuracy();
            let created_at = now();
            tx.execute(
                "INSERT INTO reviews (game, version, nodes, white_accuracy, black_accuracy, summary, plies, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT (game, version, nodes) DO UPDATE SET white_accuracy = excluded.white_accuracy, \
                 black_accuracy = excluded.black_accuracy, summary = excluded.summary, plies = excluded.plies, \
                 created_at = excluded.created_at",
                params![
                    game.0,
                    summary.version.0,
                    summary.nodes as i64,
                    accuracy.white.map(f64::from),
                    accuracy.black.map(f64::from),
                    to_json(&summary)?,
                    pack(&plies)?,
                    created_at.as_str(),
                ],
            )?;
            let id: i64 = tx.query_row(
                "SELECT id FROM reviews WHERE game = ?1 AND version = ?2 AND nodes = ?3",
                params![game.0, summary.version.0, summary.nodes as i64],
                |r| r.get(0),
            )?;
            Ok(Review {
                id: ReviewId(id),
                game,
                version: summary.version,
                nodes: summary.nodes,
                summary,
                plies,
                created_at,
            })
        })
        .await
    }

    /// The newest review of a game, or the newest with this version and budget when they are given.
    pub async fn review(
        &self,
        game: GameId,
        version: Option<VersionId>,
        nodes: Option<u64>,
    ) -> Result<Option<Review>, Error> {
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(&format!(
                "{REVIEW} WHERE game = ?1 AND (?2 IS NULL OR version = ?2) AND (?3 IS NULL OR nodes = ?3) \
                 ORDER BY created_at DESC, id DESC LIMIT 1"
            ))?;
            let mut rows = stmt.query(params![game.0, version.map(|v| v.0), nodes.map(|n| n as i64)])?;
            rows.next()?.map(|r| review_from_row(r, true)).transpose()
        })
        .await
    }

    /// Whether a game already has a review with this version and budget: a review skips it.
    pub async fn has_review(&self, game: GameId, version: VersionId, nodes: u64) -> Result<bool, Error> {
        self.read(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT 1 FROM reviews WHERE game = ?1 AND version = ?2 AND nodes = ?3",
                    params![game.0, version.0, nodes as i64],
                    |_| Ok(()),
                )
                .optional()?
                .is_some())
        })
        .await
    }

    /// A player's games as a profile reads them: the newest that pass `filter` (finished or aborted, never live),
    /// each with its start position and newest review. The reviews' plies are read only when `plies` is set: weak
    /// spots and notable moments need them, the figures do not.
    pub async fn profile_games(
        &self,
        player: PlayerKey,
        filter: ProfileFilter,
        plies: bool,
    ) -> Result<Vec<ProfileGame>, Error> {
        self.read(move |conn| {
            let summary = "SELECT g.id, g.source, g.white, g.black, g.time, g.status, g.outcome, g.ending, g.opening, \
                 g.plies, g.started_at, g.ended_at, g.you_side, g.url, r.white_accuracy, r.black_accuracy, g.start_fen \
                 FROM games g LEFT JOIN reviews r ON r.id = \
                 (SELECT id FROM reviews WHERE game = g.id ORDER BY created_at DESC, id DESC LIMIT 1)";
            let mut sql = format!("{summary} WHERE (g.white_key = ?1 OR g.black_key = ?1) AND g.live = 0");
            let mut args = vec![Value::Text(player.to_string())];
            if !filter.speeds.is_empty() {
                let marks: Vec<String> = filter
                    .speeds
                    .iter()
                    .map(|s| {
                        args.push(Value::Text(word(s)));
                        format!("?{}", args.len())
                    })
                    .collect();
                sql += &format!(" AND g.speed IN ({})", marks.join(", "));
            }
            if let Some(rated) = filter.rated {
                args.push(Value::Integer(rated.into()));
                sql += &format!(" AND g.rated = ?{}", args.len());
            }
            if let Some(since) = &filter.since {
                args.push(Value::Text(since.0.clone()));
                sql += &format!(" AND g.started_at >= ?{}", args.len());
            }
            sql += &format!(" ORDER BY g.started_at DESC, g.id DESC LIMIT {}", filter.limit.max(1));

            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(args))?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                let summary = games::summary_from_row(row)?;
                debug_assert!(!matches!(summary.status, GameStatus::Live | GameStatus::Starting));
                let start_fen: Option<String> = row.get(16)?;
                out.push(ProfileGame { summary, start_fen, review: None });
            }
            for g in &mut out {
                if g.summary.accuracy.is_some() || plies {
                    g.review = newest(conn, g.summary.id, plies)?;
                }
            }
            Ok(out)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::{
        Side,
        game::{Ending, GameEnd, Outcome},
        review::{ClassCounts, SideReview, judge::Classification},
        version::Line,
    };

    use super::*;
    use crate::{GameFilter, NewVersion, games::tests::local, testing};

    pub fn summary(version: VersionId, nodes: u64, white: f64, black: f64) -> ReviewSummary {
        let side = |accuracy| SideReview {
            moves: 10,
            accuracy: Some(accuracy),
            counts: ClassCounts { best: 3, ..Default::default() },
            ..Default::default()
        };
        ReviewSummary {
            version,
            nodes,
            white: side(white),
            black: side(black),
            middlegame_ply: None,
            endgame_ply: None,
        }
    }

    async fn version(store: &Store) -> VersionId {
        store.upsert_version(NewVersion::new("v1", "abc", Line::Champion { number: 1 })).await.unwrap().id
    }

    fn ply() -> ReviewedMove {
        serde_json::from_value(serde_json::json!({
            "ply": 1, "side": "white", "uci": "e2e4", "san": "e4", "eval_before": {"cp": 20}, "eval_after": {"cp": 25},
            "best_uci": "e2e4", "best_san": "e4", "best_pv": ["e2e4"], "second_uci": null, "eval_second": null,
            "win_before": 51.0, "win_after": 51.5, "accuracy": 100.0, "cp_loss": 0, "classification": "best",
            "judgement": null, "phase": "opening", "sacrifice": 0, "clock_before_ms": null, "depth": 14
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn reviews_replace_by_version_and_budget_and_show_in_lists() {
        let (store, _dir) = testing::store();
        let v = version(&store).await;
        let mut g = local(PlayerKey::You, PlayerKey::Dameless, 0);
        g.status = dameless_core::game::GameStatus::Finished;
        let game = store.insert_game(g).await.unwrap();
        assert!(store.review(game, None, None).await.unwrap().is_none());

        let first = store.put_review(game, summary(v, 200_000, 80.0, 70.0), vec![ply()]).await.unwrap();
        assert!(store.has_review(game, v, 200_000).await.unwrap());
        assert!(!store.has_review(game, v, 100_000).await.unwrap());
        let again = store.put_review(game, summary(v, 200_000, 81.0, 71.0), vec![ply()]).await.unwrap();
        assert_eq!(again.id, first.id);
        store.put_review(game, summary(v, 100_000, 60.0, 50.0), vec![]).await.unwrap();

        let newest = store.review(game, None, None).await.unwrap().unwrap();
        assert_eq!(newest.nodes, 100_000);
        let asked = store.review(game, Some(v), Some(200_000)).await.unwrap().unwrap();
        assert_eq!(asked.plies[0].classification, Classification::Best);
        assert_eq!(asked.summary.white.accuracy, Some(81.0));

        let list = store.games(GameFilter::default()).await.unwrap();
        assert_eq!(list[0].accuracy.map(|a| (a.white, a.black)), Some((60.0, 50.0)));
        assert_eq!(store.game_summary(game).await.unwrap().unwrap().accuracy.map(|a| a.white), Some(60.0));
    }

    #[tokio::test]
    async fn profiles_read_a_players_finished_games() {
        let (store, _dir) = testing::store();
        let v = version(&store).await;
        let mut ids = Vec::new();
        for i in 0..5 {
            let id = store.insert_game(local(PlayerKey::You, PlayerKey::Dameless, i * 1000)).await.unwrap();
            if i < 4 {
                store
                    .finish_game(id, GameEnd::new(Outcome::WhiteWins, Ending::Checkmate), Timestamp::from_unix_ms(i))
                    .await
                    .unwrap();
            }
            ids.push(id);
        }
        store.put_review(ids[1], summary(v, 1, 90.0, 40.0), vec![ply()]).await.unwrap();
        let other = store.insert_game(local(PlayerKey::Dameless, PlayerKey::Stockfish(2), 0)).await.unwrap();

        let filter = ProfileFilter { limit: 10, ..Default::default() };
        let games = store.profile_games(PlayerKey::You, filter.clone(), false).await.unwrap();
        assert_eq!(games.iter().map(|g| g.summary.id).collect::<Vec<_>>(), [ids[3], ids[2], ids[1], ids[0]]);
        let reviewed = games.iter().find(|g| g.summary.id == ids[1]).unwrap();
        let review = reviewed.review.as_ref().unwrap();
        assert!(review.plies.is_empty());
        assert_eq!(review.summary.side(Side::White).accuracy, Some(90.0));

        let with_plies = store.profile_games(PlayerKey::You, ProfileFilter { limit: 2, ..filter }, true).await.unwrap();
        assert_eq!(with_plies.len(), 2);
        let games =
            store.profile_games(PlayerKey::You, ProfileFilter { limit: 10, ..Default::default() }, true).await.unwrap();
        assert_eq!(games.iter().find(|g| g.summary.id == ids[1]).unwrap().review.as_ref().unwrap().plies.len(), 1);
        assert!(!games.iter().any(|g| g.summary.id == other));
    }
}
