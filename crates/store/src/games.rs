//! Games: one row per game, wherever it was played, with its moves as one compressed blob. Lists read the summary
//! columns only and never touch the blob, so a list of a thousand games costs a thousand small rows.

use dameless_core::{
    Error, GameId, PlayerKey, Side, SidePair, Speed, TimeControl, Timestamp, TrialId,
    game::{Ending, Game, GameEnd, GameStatus, GameSummary, Move, Offers, Opening, Outcome, Seat, Source},
};
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};

use crate::{
    DbResult, Store, from_json, from_opt_json, from_opt_word, from_word, opt_json, pack, to_json, unpack, word,
};

/// The kinds of [`Source`], as the store files them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Local,
    Lichess,
    Import,
    Trial,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Lichess => "lichess",
            Self::Import => "import",
            Self::Trial => "trial",
        }
    }

    pub fn of(source: &Source) -> Self {
        match source {
            Source::Local => Self::Local,
            Source::Lichess { .. } => Self::Lichess,
            Source::Import { .. } => Self::Import,
            Source::Trial { .. } => Self::Trial,
        }
    }

    /// Reads `local`, `lichess`, `import` or `trial`.
    pub fn parse(text: &str) -> Option<Self> {
        [Self::Local, Self::Lichess, Self::Import, Self::Trial].into_iter().find(|k| k.as_str() == text.trim())
    }
}

/// Whose copy of a Lichess game a source is: the connection that played it, or the player whose history it was read
/// from. A Lichess game is stored once per owner.
fn owner(source: &Source) -> Option<String> {
    match source {
        Source::Lichess { connection, .. } => Some(connection.to_string()),
        Source::Import { owner, .. } => Some(owner.to_string()),
        Source::Local | Source::Trial { .. } => None,
    }
}

fn trial_of(source: &Source) -> Option<TrialId> {
    match source {
        Source::Trial { trial, .. } => Some(*trial),
        _ => None,
    }
}

fn is_live(status: GameStatus) -> bool {
    matches!(status, GameStatus::Starting | GameStatus::Live)
}

/// A game to store: everything but its id and its review.
#[derive(Clone, Debug, PartialEq)]
pub struct NewGame {
    pub white: Seat,
    pub black: Seat,
    pub source: Source,
    pub time: TimeControl,
    pub status: GameStatus,
    pub outcome: Option<Outcome>,
    pub ending: Option<Ending>,
    pub opening: Option<Opening>,
    /// None for the standard start.
    pub start_fen: Option<String>,
    pub moves: Vec<Move>,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    /// The game on lichess.org, made by whoever knows which Lichess it was.
    pub url: Option<String>,
}

impl NewGame {
    /// A game about to start: no moves, no result.
    pub fn starting(white: Seat, black: Seat, source: Source, time: TimeControl, started_at: Timestamp) -> Self {
        Self {
            white,
            black,
            source,
            time,
            status: GameStatus::Starting,
            outcome: None,
            ending: None,
            opening: None,
            start_fen: None,
            moves: Vec::new(),
            started_at,
            ended_at: None,
            url: None,
        }
    }

    /// The speed lists filter on: Lichess's own for Lichess games, else from the clock.
    fn speed(&self) -> Option<Speed> {
        match &self.source {
            Source::Lichess { speed, .. } | Source::Import { speed, .. } => Some(*speed),
            _ => self.time.speed(),
        }
    }

    /// The side You played: the seat whose player is You.
    fn you(&self) -> Option<Side> {
        if self.white.is_you() {
            Some(Side::White)
        } else if self.black.is_you() {
            Some(Side::Black)
        } else {
            None
        }
    }
}

/// Changes to a game's summary while it is played. Fields left `None` stay as they are. Moves, takebacks and the end
/// have their own calls; offers, clocks and what You may do live in memory with the game being played.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameUpdate {
    pub status: Option<GameStatus>,
    pub white: Option<Seat>,
    pub black: Option<Seat>,
    pub opening: Option<Opening>,
    pub url: Option<String>,
    pub started_at: Option<Timestamp>,
}

/// Which games a list holds. Everything empty or `None` is every game, newest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameFilter {
    /// Games where this player sat on either side.
    pub player: Option<PlayerKey>,
    /// Only these kinds of source; empty for all.
    pub sources: Vec<SourceKind>,
    /// Only games in these states; empty for all.
    pub statuses: Vec<GameStatus>,
    /// Only these speeds; empty for all.
    pub speeds: Vec<Speed>,
    pub rated: Option<bool>,
    /// One trial's games.
    pub trial: Option<TrialId>,
    /// Games started at or after this moment.
    pub since: Option<Timestamp>,
    /// The page after this game: the cursor is the last game of the previous page.
    pub before: Option<GameId>,
    /// At most this many; 0 means [`DEFAULT_LIMIT`].
    pub limit: u32,
    /// Live games before finished ones (then newest first within each). The cursor follows the same order.
    pub live_first: bool,
}

/// How many games a list holds when the filter gives no limit.
pub const DEFAULT_LIMIT: u32 = 50;
/// The most a list holds, whatever the filter asks.
pub const MAX_LIMIT: u32 = 5000;

/// The summary columns, in the order [`summary_from_row`] reads them, with the newest review's accuracies.
const SUMMARY: &str = "SELECT g.id, g.source, g.white, g.black, g.time, g.status, g.outcome, g.ending, g.opening, \
     g.plies, g.started_at, g.ended_at, g.you_side, g.url, r.white_accuracy, r.black_accuracy \
     FROM games g LEFT JOIN reviews r ON r.id = \
     (SELECT id FROM reviews WHERE game = g.id ORDER BY created_at DESC, id DESC LIMIT 1)";

pub(crate) fn summary_from_row(row: &Row<'_>) -> DbResult<GameSummary> {
    let white_accuracy: Option<f64> = row.get(14)?;
    let black_accuracy: Option<f64> = row.get(15)?;
    Ok(GameSummary {
        id: GameId(row.get(0)?),
        source: from_json(&row.get::<_, String>(1)?)?,
        white: from_json(&row.get::<_, String>(2)?)?,
        black: from_json(&row.get::<_, String>(3)?)?,
        time: from_json(&row.get::<_, String>(4)?)?,
        status: from_word(&row.get::<_, String>(5)?)?,
        outcome: from_opt_word(row.get(6)?)?,
        ending: from_opt_word(row.get(7)?)?,
        opening: from_opt_json(row.get(8)?)?,
        plies: row.get(9)?,
        started_at: Timestamp(row.get(10)?),
        ended_at: row.get::<_, Option<String>>(11)?.map(Timestamp),
        you: from_opt_word(row.get(12)?)?,
        url: row.get(13)?,
        accuracy: white_accuracy.zip(black_accuracy).map(|(white, black)| SidePair::new(white as f32, black as f32)),
    })
}

pub(crate) fn summary_by_id(conn: &Connection, id: GameId) -> DbResult<Option<GameSummary>> {
    let mut stmt = conn.prepare_cached(&format!("{SUMMARY} WHERE g.id = ?1"))?;
    let mut rows = stmt.query([id.0])?;
    rows.next()?.map(summary_from_row).transpose()
}

fn missing(id: GameId) -> Error {
    Error::not_found(format!("No game {id}"))
}

fn insert(conn: &Connection, g: &NewGame) -> DbResult<GameId> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO games (source_kind, source, source_owner, white, black, white_key, black_key, time, speed, \
         rated, status, live, outcome, ending, opening, start_fen, plies, moves, you_side, lichess_id, trial, url, \
         started_at, ended_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, \
         ?23, ?24)",
    )?;
    stmt.execute(params![
        SourceKind::of(&g.source).as_str(),
        to_json(&g.source)?,
        owner(&g.source),
        to_json(&g.white)?,
        to_json(&g.black)?,
        g.white.player.to_string(),
        g.black.player.to_string(),
        to_json(&g.time)?,
        g.speed().map(|s| word(&s)),
        g.source.rated(),
        word(&g.status),
        is_live(g.status),
        g.outcome.map(|o| word(&o)),
        g.ending.map(|e| word(&e)),
        opt_json(&g.opening)?,
        g.start_fen,
        g.moves.len() as i64,
        pack(&g.moves)?,
        g.you().map(|s| word(&s)),
        g.source.lichess_id(),
        trial_of(&g.source).map(|t| t.0),
        g.url,
        g.started_at.as_str(),
        g.ended_at.as_ref().map(Timestamp::as_str),
    ])?;
    Ok(GameId(conn.last_insert_rowid()))
}

/// A game's moves, decompressed.
fn moves_of(conn: &Connection, id: GameId) -> DbResult<Vec<Move>> {
    let blob: Option<Vec<u8>> =
        conn.query_row("SELECT moves FROM games WHERE id = ?1", [id.0], |r| r.get(0)).optional()?;
    unpack(&blob.ok_or_else(|| missing(id))?)
}

fn set_moves(conn: &Connection, id: GameId, moves: &[Move]) -> DbResult<()> {
    conn.execute(
        "UPDATE games SET moves = ?2, plies = ?3 WHERE id = ?1",
        params![id.0, pack(moves)?, moves.len() as i64],
    )?;
    Ok(())
}

fn summary_or_missing(conn: &Connection, id: GameId) -> DbResult<GameSummary> {
    summary_by_id(conn, id)?.ok_or_else(|| missing(id).into())
}

impl Store {
    /// Stores a new game.
    pub async fn insert_game(&self, game: NewGame) -> Result<GameId, Error> {
        self.write(move |tx| insert(tx, &game)).await
    }

    /// Stores many games in one transaction: imports and a trial round's PGN.
    pub async fn insert_games(&self, games: Vec<NewGame>) -> Result<Vec<GameId>, Error> {
        self.write(move |tx| games.iter().map(|g| insert(tx, g)).collect()).await
    }

    /// The full record. Offers, clocks and actions are those of a game at rest (none): the game being played
    /// overlays its own.
    pub async fn game(&self, id: GameId) -> Result<Option<Game>, Error> {
        self.read(move |conn| {
            let Some(summary) = summary_by_id(conn, id)? else { return Ok(None) };
            let (start_fen, blob): (Option<String>, Vec<u8>) =
                conn.query_row("SELECT start_fen, moves FROM games WHERE id = ?1", [id.0], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?;
            Ok(Some(Game {
                summary,
                start_fen,
                moves: unpack(&blob)?,
                clocks: None,
                offers: Offers::default(),
                actions: Vec::new(),
            }))
        })
        .await
    }

    pub async fn game_summary(&self, id: GameId) -> Result<Option<GameSummary>, Error> {
        self.read(move |conn| summary_by_id(conn, id)).await
    }

    /// Summaries, newest first (live first when asked), without reading any moves.
    pub async fn games(&self, filter: GameFilter) -> Result<Vec<GameSummary>, Error> {
        self.read(move |conn| {
            let mut sql = format!("{SUMMARY} WHERE 1 = 1");
            let mut args: Vec<Value> = Vec::new();
            let arg = |v: Value, args: &mut Vec<Value>| {
                args.push(v);
                format!("?{}", args.len())
            };
            if let Some(player) = &filter.player {
                let p = arg(Value::Text(player.to_string()), &mut args);
                sql += &format!(" AND (g.white_key = {p} OR g.black_key = {p})");
            }
            let any_of = |column: &str, values: Vec<String>, args: &mut Vec<Value>| {
                if values.is_empty() {
                    return String::new();
                }
                let marks: Vec<String> = values.into_iter().map(|v| arg(Value::Text(v), args)).collect();
                format!(" AND {column} IN ({})", marks.join(", "))
            };
            sql += &any_of("g.source_kind", filter.sources.iter().map(|k| k.as_str().to_owned()).collect(), &mut args);
            sql += &any_of("g.status", filter.statuses.iter().map(word).collect(), &mut args);
            sql += &any_of("g.speed", filter.speeds.iter().map(word).collect(), &mut args);
            if let Some(rated) = filter.rated {
                sql += &format!(" AND g.rated = {}", arg(Value::Integer(rated.into()), &mut args));
            }
            if let Some(trial) = filter.trial {
                sql += &format!(" AND g.trial = {}", arg(Value::Integer(trial.0), &mut args));
            }
            if let Some(since) = &filter.since {
                sql += &format!(" AND g.started_at >= {}", arg(Value::Text(since.0.clone()), &mut args));
            }
            if let Some(before) = filter.before {
                let b = arg(Value::Integer(before.0), &mut args);
                sql += &if filter.live_first {
                    format!(
                        " AND (g.live, g.started_at, g.id) < (SELECT live, started_at, id FROM games WHERE id = {b})"
                    )
                } else {
                    format!(" AND (g.started_at, g.id) < (SELECT started_at, id FROM games WHERE id = {b})")
                };
            }
            sql += if filter.live_first {
                " ORDER BY g.live DESC, g.started_at DESC, g.id DESC"
            } else {
                " ORDER BY g.started_at DESC, g.id DESC"
            };
            let limit = if filter.limit == 0 { DEFAULT_LIMIT } else { filter.limit.min(MAX_LIMIT) };
            sql += &format!(" LIMIT {limit}");

            let mut stmt = conn.prepare(&sql)?;
            let mut rows = stmt.query(params_from_iter(args))?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(summary_from_row(row)?);
            }
            Ok(out)
        })
        .await
    }

    /// Adds a move to a game; returns how many plies it now has.
    pub async fn append_move(&self, id: GameId, mv: Move) -> Result<u32, Error> {
        self.write(move |tx| {
            let mut moves = moves_of(tx, id)?;
            moves.push(mv);
            set_moves(tx, id, &moves)?;
            Ok(moves.len() as u32)
        })
        .await
    }

    /// Takes moves back so that `plies` are left (a takeback); returns how many are left.
    pub async fn truncate_moves(&self, id: GameId, plies: u32) -> Result<u32, Error> {
        self.write(move |tx| {
            let mut moves = moves_of(tx, id)?;
            moves.truncate(plies as usize);
            set_moves(tx, id, &moves)?;
            Ok(moves.len() as u32)
        })
        .await
    }

    /// Replaces every move at once, as a Lichess game resumed after a restart does with the moves Lichess sends.
    pub async fn replace_moves(&self, id: GameId, moves: Vec<Move>) -> Result<(), Error> {
        self.write(move |tx| {
            summary_or_missing(tx, id)?;
            set_moves(tx, id, &moves)
        })
        .await
    }

    /// Changes summary fields; returns the summary as it now is.
    pub async fn update_game(&self, id: GameId, update: GameUpdate) -> Result<GameSummary, Error> {
        self.write(move |tx| {
            summary_or_missing(tx, id)?;
            if let Some(status) = update.status {
                tx.execute(
                    "UPDATE games SET status = ?2, live = ?3 WHERE id = ?1",
                    params![id.0, word(&status), is_live(status)],
                )?;
            }
            for (side, seat) in [("white", &update.white), ("black", &update.black)] {
                if let Some(seat) = seat {
                    let you = tx.query_row("SELECT you_side FROM games WHERE id = ?1", [id.0], |r| {
                        r.get::<_, Option<String>>(0)
                    })?;
                    // A seat changing hands keeps `you_side` true to the seats.
                    let you = match (seat.is_you(), you) {
                        (true, _) => Some(side.to_owned()),
                        (false, Some(s)) if s == side => None,
                        (false, other) => other,
                    };
                    tx.execute(
                        &format!("UPDATE games SET {side} = ?2, {side}_key = ?3, you_side = ?4 WHERE id = ?1"),
                        params![id.0, to_json(seat)?, seat.player.to_string(), you],
                    )?;
                }
            }
            if let Some(opening) = &update.opening {
                tx.execute("UPDATE games SET opening = ?2 WHERE id = ?1", params![id.0, to_json(opening)?])?;
            }
            if let Some(url) = &update.url {
                tx.execute("UPDATE games SET url = ?2 WHERE id = ?1", params![id.0, url])?;
            }
            if let Some(at) = &update.started_at {
                tx.execute("UPDATE games SET started_at = ?2 WHERE id = ?1", params![id.0, at.as_str()])?;
            }
            summary_or_missing(tx, id)
        })
        .await
    }

    /// Ends a game: its result, why, and when. Returns the summary as it now is.
    pub async fn finish_game(&self, id: GameId, end: GameEnd, ended_at: Timestamp) -> Result<GameSummary, Error> {
        self.write(move |tx| {
            let n = tx.execute(
                "UPDATE games SET status = ?2, live = 0, outcome = ?3, ending = ?4, ended_at = ?5 WHERE id = ?1",
                params![id.0, word(&end.status()), end.outcome.map(|o| word(&o)), word(&end.ending), ended_at.as_str()],
            )?;
            if n == 0 {
                return Err(missing(id).into());
            }
            summary_or_missing(tx, id)
        })
        .await
    }

    /// The stored copy of a Lichess game from the same source kind and owner (the connection that played it, or the
    /// player whose history it came from), to avoid storing it twice.
    pub async fn find_source(&self, source: Source) -> Result<Option<GameSummary>, Error> {
        let Some(lichess_id) = source.lichess_id().map(str::to_owned) else { return Ok(None) };
        self.read(move |conn| {
            let id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM games WHERE lichess_id = ?1 AND source_kind = ?2 AND source_owner IS ?3 \
                     ORDER BY id LIMIT 1",
                    params![lichess_id, SourceKind::of(&source).as_str(), owner(&source)],
                    |r| r.get(0),
                )
                .optional()?;
            id.map(|id| summary_by_id(conn, GameId(id))).transpose().map(Option::flatten)
        })
        .await
    }

    /// Every stored copy of a Lichess game, whoever's.
    pub async fn games_with_lichess_id(&self, lichess_id: &str) -> Result<Vec<GameSummary>, Error> {
        let lichess_id = lichess_id.to_owned();
        self.read(move |conn| {
            let mut stmt = conn.prepare_cached(&format!("{SUMMARY} WHERE g.lichess_id = ?1 ORDER BY g.id"))?;
            let mut rows = stmt.query([lichess_id])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(summary_from_row(row)?);
            }
            Ok(out)
        })
        .await
    }

    /// How many of a trial's games are stored.
    pub async fn count_trial_games(&self, trial: TrialId) -> Result<u32, Error> {
        self.read(move |conn| {
            Ok(conn.query_row("SELECT count(*) FROM games WHERE trial = ?1", [trial.0], |r| r.get(0))?)
        })
        .await
    }

    /// Games still marked live: after a restart, the ones to resume or abort.
    pub async fn live_games(&self) -> Result<Vec<GameSummary>, Error> {
        self.games(GameFilter {
            statuses: vec![GameStatus::Starting, GameStatus::Live],
            limit: MAX_LIMIT,
            ..GameFilter::default()
        })
        .await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Instant;

    use dameless_core::{ConnectionKey, Score, game::Ending};

    use super::*;
    use crate::testing;

    pub fn seat(player: PlayerKey) -> Seat {
        let name = player.to_string();
        Seat::new(player, name)
    }

    pub fn local(white: PlayerKey, black: PlayerKey, started_at: i64) -> NewGame {
        NewGame::starting(
            seat(white),
            seat(black),
            Source::Local,
            TimeControl::clock(180_000, 2_000),
            Timestamp::from_unix_ms(started_at),
        )
    }

    fn moves(n: usize) -> Vec<Move> {
        (0..n)
            .map(|i| Move {
                uci: "e2e4".into(),
                san: "e4".into(),
                clock_ms: Some(170_000 - i as u64 * 100),
                think_ms: Some(1234),
                eval: Some(Score::Cp(i as i32)),
                depth: Some(18),
                nodes: Some(123_456),
            })
            .collect()
    }

    #[tokio::test]
    async fn a_game_is_played_and_finished() {
        let (store, _dir) = testing::store();
        let id = store.insert_game(local(PlayerKey::You, PlayerKey::Dameless, 1_000)).await.unwrap();
        let s = store.game_summary(id).await.unwrap().unwrap();
        assert_eq!((s.status, s.plies, s.you), (GameStatus::Starting, 0, Some(Side::White)));

        store.update_game(id, GameUpdate { status: Some(GameStatus::Live), ..Default::default() }).await.unwrap();
        assert_eq!(store.append_move(id, Move::new("e2e4", "e4")).await.unwrap(), 1);
        assert_eq!(store.append_move(id, Move::new("e7e5", "e5")).await.unwrap(), 2);
        assert_eq!(store.append_move(id, Move::new("g1f3", "Nf3")).await.unwrap(), 3);
        assert_eq!(store.truncate_moves(id, 1).await.unwrap(), 1);
        assert_eq!(store.live_games().await.unwrap().len(), 1);

        let end = GameEnd::new(Outcome::BlackWins, Ending::Resignation);
        let s = store.finish_game(id, end, Timestamp::from_unix_ms(9_000)).await.unwrap();
        assert_eq!(
            (s.status, s.outcome, s.ending),
            (GameStatus::Finished, Some(Outcome::BlackWins), end.outcome.map(|_| Ending::Resignation))
        );
        assert_eq!(s.ended_at, Some(Timestamp::from_unix_ms(9_000)));
        let g = store.game(id).await.unwrap().unwrap();
        assert_eq!(g.moves, vec![Move::new("e2e4", "e4")]);
        assert_eq!(g.summary.plies, 1);
        assert!(store.live_games().await.unwrap().is_empty());

        assert_eq!(store.append_move(GameId(99), Move::new("a2a3", "a3")).await.unwrap_err().status(), 404);
        assert!(store.game(GameId(99)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn seats_and_openings_update() {
        let (store, _dir) = testing::store();
        let id = store.insert_game(local(PlayerKey::You, PlayerKey::Dameless, 1_000)).await.unwrap();
        let mut black = seat(PlayerKey::Dameless);
        black.rating = Some(2800);
        let opening = Opening { eco: Some("C20".into()), name: "King's Pawn Game".into(), ply: Some(2) };
        let s = store
            .update_game(
                id,
                GameUpdate { black: Some(black.clone()), opening: Some(opening.clone()), ..Default::default() },
            )
            .await
            .unwrap();
        assert_eq!((s.black, s.opening, s.you), (black, Some(opening), Some(Side::White)));
        let s = store
            .update_game(id, GameUpdate { white: Some(seat(PlayerKey::Stockfish(3))), ..Default::default() })
            .await
            .unwrap();
        assert_eq!(s.you, None);
    }

    #[tokio::test]
    async fn lists_filter_and_page() {
        let (store, _dir) = testing::store();
        let bob = PlayerKey::Lichess("bob".into());
        let mut ids = Vec::new();
        for i in 0..10 {
            let (w, b) = if i % 2 == 0 { (PlayerKey::You, bob.clone()) } else { (PlayerKey::Dameless, PlayerKey::You) };
            let mut g = local(w, b, i * 1000);
            g.status = GameStatus::Finished;
            ids.push(store.insert_game(g).await.unwrap());
        }
        let mut lichess = local(PlayerKey::Dameless, bob.clone(), 50_000);
        lichess.source =
            Source::Lichess { connection: ConnectionKey::Bot, id: "abcd1234".into(), rated: true, speed: Speed::Blitz };
        lichess.status = GameStatus::Live;
        let live = store.insert_game(lichess).await.unwrap();

        let all = store.games(GameFilter::default()).await.unwrap();
        assert_eq!(all.len(), 11);
        assert_eq!(all[0].id, live);
        assert_eq!(all[1].id, ids[9]);

        let bobs = store.games(GameFilter { player: Some(bob.clone()), ..Default::default() }).await.unwrap();
        assert_eq!(bobs.len(), 6);
        let rated = store.games(GameFilter { rated: Some(true), ..Default::default() }).await.unwrap();
        assert_eq!(rated.iter().map(|g| g.id).collect::<Vec<_>>(), [live]);
        let lichess =
            store.games(GameFilter { sources: vec![SourceKind::Lichess], ..Default::default() }).await.unwrap();
        assert_eq!(lichess.len(), 1);
        let blitz = store.games(GameFilter { speeds: vec![Speed::Blitz], ..Default::default() }).await.unwrap();
        assert_eq!(blitz.len(), 11, "3+2 is blitz too");
        let finished =
            store.games(GameFilter { statuses: vec![GameStatus::Finished], ..Default::default() }).await.unwrap();
        assert_eq!(finished.len(), 10);

        // Pages of four, newest first, with the cursor.
        let mut seen = Vec::new();
        let mut before = None;
        loop {
            let page = store.games(GameFilter { before, limit: 4, ..Default::default() }).await.unwrap();
            if page.is_empty() {
                break;
            }
            before = page.last().map(|g| g.id);
            seen.extend(page.into_iter().map(|g| g.id));
        }
        let mut expected: Vec<_> = ids.iter().rev().copied().collect();
        expected.insert(0, live);
        assert_eq!(seen, expected);

        // Live first, with a live game that is older than finished ones.
        let mut old_live = local(PlayerKey::You, bob, -5_000);
        old_live.status = GameStatus::Live;
        let old_live = store.insert_game(old_live).await.unwrap();
        let page = store.games(GameFilter { live_first: true, limit: 3, ..Default::default() }).await.unwrap();
        assert_eq!(page.iter().map(|g| g.id).collect::<Vec<_>>(), [live, old_live, ids[9]]);
        let next = store
            .games(GameFilter { live_first: true, limit: 2, before: Some(ids[9]), ..Default::default() })
            .await
            .unwrap();
        assert_eq!(next.iter().map(|g| g.id).collect::<Vec<_>>(), [ids[8], ids[7]]);
    }

    #[tokio::test]
    async fn lichess_games_are_found_by_source() {
        let (store, _dir) = testing::store();
        let source =
            Source::Lichess { connection: ConnectionKey::You, id: "q7ZvsdUF".into(), rated: true, speed: Speed::Rapid };
        let mut g = local(PlayerKey::You, PlayerKey::Lichess("bob".into()), 0);
        g.source = source.clone();
        let id = store.insert_game(g.clone()).await.unwrap();
        assert_eq!(store.find_source(source.clone()).await.unwrap().map(|s| s.id), Some(id));
        let bots =
            Source::Lichess { connection: ConnectionKey::Bot, id: "q7ZvsdUF".into(), rated: true, speed: Speed::Rapid };
        assert!(store.find_source(bots.clone()).await.unwrap().is_none());
        g.source = bots;
        store.insert_game(g).await.unwrap();
        assert_eq!(store.games_with_lichess_id("q7ZvsdUF").await.unwrap().len(), 2);
        assert!(store.find_source(Source::Local).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn bulk_inserts_are_fast_and_lists_never_read_moves() {
        let (store, _dir) = testing::store();
        let games: Vec<NewGame> = (0..2000)
            .map(|i| {
                let mut g = local(PlayerKey::Lichess(format!("p{}", i % 7)), PlayerKey::You, i * 1000);
                g.moves = moves(80);
                g.status = GameStatus::Finished;
                g
            })
            .collect();
        let started = Instant::now();
        let ids = store.insert_games(games).await.unwrap();
        let took = started.elapsed();
        assert_eq!(ids.len(), 2000);
        assert!(took.as_secs_f64() < 2.0, "2,000 games took {took:?}");

        // Break one game's moves: lists still read, the full record does not.
        let broken = ids[1999];
        store
            .write(move |tx| Ok(tx.execute("UPDATE games SET moves = x'00' WHERE id = ?1", [broken.0])?))
            .await
            .unwrap();
        let started = Instant::now();
        let list = store.games(GameFilter { limit: 2000, ..Default::default() }).await.unwrap();
        assert_eq!(list.len(), 2000);
        assert_eq!(list[0].id, broken);
        assert_eq!(list[0].plies, 80);
        assert!(started.elapsed().as_secs_f64() < 2.0);
        assert!(store.game(broken).await.is_err());
        assert_eq!(store.game(ids[0]).await.unwrap().unwrap().moves.len(), 80);
    }
}
