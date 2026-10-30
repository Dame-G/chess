//! The levels: every engine has its own ladder. Which entrant (`level:<id>` or `you`) sat at each seat of a game
//! played at an engine's levels, and each engine's last calibration fit. The fit itself is `core::levels::fit`, over
//! [`Store::level_games`].

use chess_core::{
    Error, GameId, Timestamp,
    levels::Estimate,
    levels::{Entrant, EntrantRating, LevelGame},
};
use rusqlite::{OptionalExtension, params};

use crate::{DbError, DbResult, Store, from_word, now};

fn entrant(text: &str) -> DbResult<Entrant> {
    text.parse().map_err(|e: chess_core::levels::BadEntrant| DbError::Domain(Error::internal(e.to_string())))
}

impl Store {
    /// Records who sat at a game played at `engine`'s levels. `calibration` marks the background matches between
    /// levels. Recording a game again replaces its seats.
    pub async fn record_level_game(
        &self,
        game: GameId,
        engine: &str,
        white: Entrant,
        black: Entrant,
        calibration: bool,
    ) -> Result<(), Error> {
        let engine = engine.to_owned();
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO level_games (game, white, black, calibration, engine) VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT (game) DO UPDATE SET white = excluded.white, black = excluded.black, \
                 calibration = excluded.calibration, engine = excluded.engine",
                params![game.0, white.to_string(), black.to_string(), calibration, engine],
            )?;
            Ok(())
        })
        .await
    }

    /// The entrants of a game played at a level, white then black; `None` for any other game.
    pub async fn level_seats(&self, game: GameId) -> Result<Option<(Entrant, Entrant)>, Error> {
        self.read(move |conn| {
            let row: Option<(String, String)> = conn
                .query_row("SELECT white, black FROM level_games WHERE game = ?1", [game.0], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            row.map(|(w, b)| Ok((entrant(&w)?, entrant(&b)?))).transpose()
        })
        .await
    }

    /// Every finished game played at `engine`'s levels, oldest first: what its calibration fits.
    pub async fn level_games(&self, engine: &str) -> Result<Vec<LevelGame>, Error> {
        let engine = engine.to_owned();
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT l.white, l.black, g.outcome FROM level_games l JOIN games g ON g.id = l.game \
                 WHERE l.engine = ?1 AND g.status = 'finished' AND g.outcome IS NOT NULL ORDER BY g.started_at, g.id",
            )?;
            let mut rows = stmt.query([engine])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(LevelGame {
                    white: entrant(&row.get::<_, String>(0)?)?,
                    black: entrant(&row.get::<_, String>(1)?)?,
                    outcome: from_word(&row.get::<_, String>(2)?)?,
                });
            }
            Ok(out)
        })
        .await
    }

    /// Replaces `engine`'s calibrated ratings with a new fit.
    pub async fn put_level_ratings(&self, engine: &str, ratings: Vec<EntrantRating>) -> Result<(), Error> {
        let engine = engine.to_owned();
        self.write(move |tx| {
            let at = now();
            tx.execute("DELETE FROM level_ratings WHERE engine = ?1", [&engine])?;
            for r in &ratings {
                tx.execute(
                    "INSERT INTO level_ratings (engine, entrant, elo, low, high, games, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        engine,
                        r.entrant.to_string(),
                        r.estimate.elo,
                        r.estimate.low,
                        r.estimate.high,
                        r.games,
                        at.as_str()
                    ],
                )?;
            }
            Ok(())
        })
        .await
    }

    /// `engine`'s last fit, levels by id then You, and when it was made (`None` before the first).
    pub async fn level_ratings(&self, engine: &str) -> Result<(Vec<EntrantRating>, Option<Timestamp>), Error> {
        let engine = engine.to_owned();
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT entrant, elo, low, high, games, updated_at FROM level_ratings WHERE engine = ?1",
            )?;
            let mut rows = stmt.query([engine])?;
            let mut out = Vec::new();
            let mut updated: Option<Timestamp> = None;
            while let Some(row) = rows.next()? {
                out.push(EntrantRating {
                    entrant: entrant(&row.get::<_, String>(0)?)?,
                    estimate: Estimate { elo: row.get(1)?, low: row.get(2)?, high: row.get(3)? },
                    games: row.get(4)?,
                });
                let at = Timestamp(row.get(5)?);
                if updated.as_ref().is_none_or(|u| at > *u) {
                    updated = Some(at);
                }
            }
            out.sort_by_key(|r| r.entrant);
            Ok((out, updated))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use chess_core::{
        PlayerKey,
        game::{Ending, GameEnd, Outcome},
        levels::{fit, ladder, pair_results, priors},
    };

    use super::*;
    use crate::{games::tests::local, testing};

    #[tokio::test]
    async fn level_games_feed_the_fit_and_ratings_are_kept() {
        let (store, _dir) = testing::store();
        let finished = |i: i64, outcome| {
            let store = store.clone();
            async move {
                let id = store
                    .insert_game(local(PlayerKey::Engine("stockfish".into()), PlayerKey::Engine("stockfish".into()), i))
                    .await
                    .unwrap();
                store
                    .finish_game(id, GameEnd::new(outcome, Ending::Checkmate), Timestamp::from_unix_ms(i))
                    .await
                    .unwrap();
                id
            }
        };
        let g1 = finished(1, Outcome::WhiteWins).await;
        let g2 = finished(2, Outcome::Draw).await;
        let live = store.insert_game(local(PlayerKey::You, PlayerKey::Engine("stockfish".into()), 3)).await.unwrap();
        let unrelated = finished(4, Outcome::BlackWins).await;

        store.record_level_game(g1, "stockfish", Entrant::Level(2), Entrant::Level(1), true).await.unwrap();
        store.record_level_game(g2, "stockfish", Entrant::Level(1), Entrant::Level(3), false).await.unwrap();
        store.record_level_game(g2, "stockfish", Entrant::Level(1), Entrant::Level(2), true).await.unwrap();
        store.record_level_game(live, "stockfish", Entrant::You, Entrant::Level(1), false).await.unwrap();
        // Another engine's levels are another ladder.
        store.record_level_game(unrelated, "reckless", Entrant::Level(1), Entrant::Level(2), true).await.unwrap();

        assert_eq!(store.level_seats(g2).await.unwrap(), Some((Entrant::Level(1), Entrant::Level(2))));
        let games = store.level_games("stockfish").await.unwrap();
        assert_eq!(
            games,
            [
                LevelGame { white: Entrant::Level(2), black: Entrant::Level(1), outcome: Outcome::WhiteWins },
                LevelGame { white: Entrant::Level(1), black: Entrant::Level(2), outcome: Outcome::Draw },
            ]
        );

        assert_eq!(store.level_games("reckless").await.unwrap().len(), 1);
        assert_eq!(store.level_games("nobody").await.unwrap(), []);

        assert_eq!(store.level_ratings("stockfish").await.unwrap(), (Vec::new(), None));
        let ratings = fit(&priors(&ladder()[..2], Some((1500, None))), &pair_results(&games));
        store.put_level_ratings("stockfish", ratings.clone()).await.unwrap();
        store.put_level_ratings("reckless", ratings[..1].to_vec()).await.unwrap();
        let (stored, at) = store.level_ratings("stockfish").await.unwrap();
        assert_eq!(stored, ratings);
        assert!(at.is_some());
        store.put_level_ratings("stockfish", ratings[..1].to_vec()).await.unwrap();
        assert_eq!(store.level_ratings("stockfish").await.unwrap().0.len(), 1);
        assert_eq!(store.level_ratings("reckless").await.unwrap().0.len(), 1);
    }
}
