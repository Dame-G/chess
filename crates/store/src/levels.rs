//! The levels of the ladder: which entrant (`level:<id>` or `you`) sat at each seat of a game played at a level, and the last
//! calibration fit. The fit itself is `core::levels::fit`, over [`Store::level_games`].

use dameless_core::{
    Error, GameId, Timestamp,
    levels::{Entrant, EntrantRating, LevelGame},
    levels::Estimate,
};
use rusqlite::{OptionalExtension, params};

use crate::{DbError, DbResult, Store, from_word, now};

fn entrant(text: &str) -> DbResult<Entrant> {
    text.parse().map_err(|e: dameless_core::levels::BadEntrant| DbError::Domain(Error::internal(e.to_string())))
}

impl Store {
    /// Records who sat at a game played at a level. `calibration` marks the background matches between levels.
    /// Recording a game again replaces its seats.
    pub async fn record_level_game(
        &self,
        game: GameId,
        white: Entrant,
        black: Entrant,
        calibration: bool,
    ) -> Result<(), Error> {
        self.write(move |tx| {
            tx.execute(
                "INSERT INTO level_games (game, white, black, calibration) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (game) DO UPDATE SET white = excluded.white, black = excluded.black, \
                 calibration = excluded.calibration",
                params![game.0, white.to_string(), black.to_string(), calibration],
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

    /// Every finished game played at a level, oldest first: what the calibration fits.
    pub async fn level_games(&self) -> Result<Vec<LevelGame>, Error> {
        self.read(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT l.white, l.black, g.outcome FROM level_games l JOIN games g ON g.id = l.game \
                 WHERE g.status = 'finished' AND g.outcome IS NOT NULL ORDER BY g.started_at, g.id",
            )?;
            let mut rows = stmt.query([])?;
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

    /// Replaces the calibrated ratings with a new fit.
    pub async fn put_level_ratings(&self, ratings: Vec<EntrantRating>) -> Result<(), Error> {
        self.write(move |tx| {
            let at = now();
            tx.execute("DELETE FROM level_ratings", [])?;
            for r in &ratings {
                tx.execute(
                    "INSERT INTO level_ratings (entrant, elo, low, high, games, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
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

    /// The last fit, levels by id then You, and when it was made (`None` before the first).
    pub async fn level_ratings(&self) -> Result<(Vec<EntrantRating>, Option<Timestamp>), Error> {
        self.read(|conn| {
            let mut stmt =
                conn.prepare_cached("SELECT entrant, elo, low, high, games, updated_at FROM level_ratings")?;
            let mut rows = stmt.query([])?;
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
    use dameless_core::{
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
                let id = store.insert_game(local(PlayerKey::Engine("stockfish".into()), PlayerKey::Engine("stockfish".into()), i)).await.unwrap();
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

        store.record_level_game(g1, Entrant::Level(2), Entrant::Level(1), true).await.unwrap();
        store.record_level_game(g2, Entrant::Level(1), Entrant::Level(3), false).await.unwrap();
        store.record_level_game(g2, Entrant::Level(1), Entrant::Level(2), true).await.unwrap();
        store.record_level_game(live, Entrant::You, Entrant::Level(1), false).await.unwrap();

        assert_eq!(store.level_seats(g2).await.unwrap(), Some((Entrant::Level(1), Entrant::Level(2))));
        assert_eq!(store.level_seats(unrelated).await.unwrap(), None);
        let games = store.level_games().await.unwrap();
        assert_eq!(
            games,
            [
                LevelGame { white: Entrant::Level(2), black: Entrant::Level(1), outcome: Outcome::WhiteWins },
                LevelGame { white: Entrant::Level(1), black: Entrant::Level(2), outcome: Outcome::Draw },
            ]
        );

        assert_eq!(store.level_ratings().await.unwrap(), (Vec::new(), None));
        let ratings = fit(&priors(&ladder()[..2], Some((1500, None))), &pair_results(&games));
        store.put_level_ratings(ratings.clone()).await.unwrap();
        let (stored, at) = store.level_ratings().await.unwrap();
        assert_eq!(stored, ratings);
        assert!(at.is_some());
        store.put_level_ratings(ratings[..1].to_vec()).await.unwrap();
        assert_eq!(store.level_ratings().await.unwrap().0.len(), 1);
    }
}
