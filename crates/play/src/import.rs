//! Importing history from Lichess, as work (`WorkKind::Import`, network only): your finished games, or another
//! player's. Each game becomes a finished game here (`Source::Import`, owned by whose history it is), stored once per
//! owner, with its clocks, Lichess's evaluations where the game was analysed, and the opening. Your history carries on
//! from the newest game imported before; another player's is assistance, so it waits while you play on Lichess.
//!
//! The export can be long and can break: the adapter reopens it below the oldest game read, and this saves a
//! checkpoint after every batch, so a restarted import carries on where it stopped.

use async_trait::async_trait;
use dameless_core::{
    Error, PlayerKey, Score, Side, Speed, TimeControl, Timestamp,
    chess::Position,
    events::AssistKind,
    game::{Move, Opening, Seat, Source},
    work::{Progress, WorkKind},
};
use dameless_hub::{Executor, WorkContext};
use dameless_lichess::{
    Error as LichessError,
    import::{ExportPlayer, ExportQuery, ExportedGame},
};
use dameless_store::NewGame;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{clients::account_of, connections::Connections, error, lichess_game::side};

/// Where your history's import carries on from: the newest game's start (ms since 1970).
pub const SINCE_KEY: &str = "lichess.you.import.since";

/// Games stored at once.
const BATCH: usize = 100;

/// What an import is asked to do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ImportSpec {
    /// `you` (your history) or `lichess:<name>` (theirs).
    pub player: PlayerKey,
    /// At most this many games, newest first. All when absent.
    #[serde(default)]
    pub max: Option<u32>,
    /// Games started at or after this time (ms since 1970). Your history: when absent, after the newest game imported
    /// before.
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default)]
    pub speeds: Vec<Speed>,
    #[serde(default)]
    pub rated: Option<bool>,
}

/// Where an import stands, saved after each batch.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Checkpoint {
    read: u32,
    imported: u32,
    skipped: u32,
    other: u32,
    /// The oldest game read (ms): a restart asks for games before it.
    oldest: Option<i64>,
    newest: Option<i64>,
}

/// Runs imports.
pub struct ImportExecutor {
    connections: Connections,
}

impl ImportExecutor {
    pub fn new(connections: Connections) -> Self {
        Self { connections }
    }
}

#[async_trait]
impl Executor for ImportExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Import
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let spec: ImportSpec = cx.spec()?;
        let (client, username, assist) = match &spec.player {
            PlayerKey::You => {
                let client = self
                    .connections
                    .client(dameless_core::ConnectionKey::You)
                    .ok_or_else(|| Error::conflict("your Lichess account is not connected: log in first"))?;
                let name = match self.connections.you().account() {
                    Some(a) => a.username,
                    None => account_of(&client.account().await.map_err(error::lichess)?).username,
                };
                (client, name, None)
            }
            PlayerKey::Lichess(name) => {
                (self.connections.yours_or_anonymous(), name.clone(), Some(AssistKind::PlayerImport))
            }
            other => return Err(Error::bad_request(format!("{other}'s games are not on Lichess"))),
        };
        if !cx.safe_point(assist).await {
            return Err(cx.stopped());
        }
        let mut cp: Checkpoint = cx.checkpoint().unwrap_or_default();
        let since = match (spec.since, &spec.player) {
            (Some(s), _) => Some(s),
            (None, PlayerKey::You) => cx.hub.store.meta::<i64>(SINCE_KEY).await?.map(|s| s + 1),
            (None, _) => None,
        };
        let remaining = spec.max.map(|m| m.saturating_sub(cp.read));
        let query = ExportQuery {
            since,
            until: cp.oldest.map(|o| o - 1),
            max: remaining,
            perf_type: spec.speeds.iter().map(|s| s.lichess_key().to_owned()).collect(),
            rated: spec.rated,
        };
        cx.log(format!(
            "Importing {username}'s games{}.",
            since.map(|s| format!(" since {}", Timestamp::from_unix_ms(s))).unwrap_or_default()
        ));
        let lichess_url = self.connections.lichess_url().to_owned();
        let owner_id = username.to_lowercase();
        let mut batch: Vec<NewGame> = Vec::new();
        if remaining != Some(0) {
            let (tx, mut breaks) = tokio::sync::mpsc::unbounded_channel::<String>();
            let mut export = client.export_games(&username, query).on_break(move |b| {
                let _ = tx.send(format!("{} games read; the export broke ({}), going on.", b.read, b.error));
            });
            loop {
                let next = tokio::select! {
                    _ = cx.cancel.cancelled() => {
                        self.flush(&cx, &mut batch, &mut cp).await?;
                        return Err(cx.stopped());
                    }
                    next = export.next() => next,
                };
                while let Ok(line) = breaks.try_recv() {
                    cx.log(line);
                }
                match next {
                    Ok(Some(g)) => {
                        cp.read += 1;
                        cp.oldest = Some(cp.oldest.map_or(g.created_at, |o| o.min(g.created_at)));
                        cp.newest = Some(cp.newest.map_or(g.created_at, |n| n.max(g.created_at)));
                        match to_game(&g, &spec.player, &owner_id, &lichess_url) {
                            Ok(Some(game)) => batch.push(game),
                            Ok(None) => cp.other += 1,
                            Err(why) => {
                                cx.log(format!("Skipped {}: {why}", g.id));
                                cp.other += 1;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(LichessError::Unexpected(why)) => {
                        cx.log(format!("Skipped a game that does not read: {why}"));
                        cp.other += 1;
                    }
                    Err(e) => {
                        self.flush(&cx, &mut batch, &mut cp).await?;
                        return Err(error::lichess(e));
                    }
                }
                if batch.len() >= BATCH {
                    self.flush(&cx, &mut batch, &mut cp).await?;
                    cx.progress_now(progress(&cp, spec.max)).await?;
                    if !cx.safe_point(assist).await {
                        return Err(cx.stopped());
                    }
                }
            }
        }
        self.flush(&cx, &mut batch, &mut cp).await?;
        cx.progress_now(progress(&cp, spec.max)).await?;
        if spec.player == PlayerKey::You
            && let Some(newest) = cp.newest
        {
            let before = cx.hub.store.meta::<i64>(SINCE_KEY).await?.unwrap_or(0);
            cx.hub.store.put_meta(SINCE_KEY, &newest.max(before)).await?;
        }
        cx.log(format!(
            "Done: {} new, {} already here, {} not standard chess or unreadable.",
            cp.imported, cp.skipped, cp.other
        ));
        Ok(serde_json::json!({
            "player": spec.player,
            "imported": cp.imported,
            "skipped": cp.skipped,
            "other": cp.other,
            "newest": cp.newest.map(Timestamp::from_unix_ms),
        }))
    }
}

impl ImportExecutor {
    /// Stores the games read that are not here yet, and saves where the import stands.
    async fn flush(&self, cx: &WorkContext, batch: &mut Vec<NewGame>, cp: &mut Checkpoint) -> Result<(), Error> {
        let mut fresh: Vec<NewGame> = Vec::new();
        for g in batch.drain(..) {
            let twice = fresh.iter().any(|f| f.source == g.source);
            if twice || cx.hub.store.find_source(g.source.clone()).await?.is_some() {
                cp.skipped += 1;
            } else {
                fresh.push(g);
            }
        }
        if !fresh.is_empty() {
            cp.imported += fresh.len() as u32;
            cx.hub.store.insert_games(fresh).await?;
        }
        cx.save_checkpoint(cp).await?;
        cx.log(format!("{} games read, {} stored.", cp.read, cp.imported));
        Ok(())
    }
}

fn progress(cp: &Checkpoint, max: Option<u32>) -> Progress {
    Progress {
        done: u64::from(cp.read),
        total: max.map(u64::from),
        unit: "games".into(),
        eta_seconds: None,
        text: Some(format!("{} games read, {} new", cp.read, cp.imported)),
    }
}

fn seat_of(p: &ExportPlayer) -> Seat {
    let seat = match (&p.ai_level, &p.user) {
        (Some(level), _) => Seat::new(PlayerKey::Stockfish((*level).clamp(1, 8)), format!("Stockfish level {level}")),
        (None, Some(u)) => {
            let key = PlayerKey::lichess(&u.id).unwrap_or(PlayerKey::Lichess("anonymous".into()));
            let name = if u.name.is_empty() { u.id.clone() } else { u.name.clone() };
            Seat { title: u.title.clone(), ..Seat::new(key, name) }
        }
        (None, None) => Seat::new(PlayerKey::Lichess("anonymous".into()), "Anonymous"),
    };
    Seat { rating: p.rating, ..seat }
}

/// One exported game as a finished game owned by `owner` (whose user id is `owner_id`), or `None` for one that is not
/// standard chess or not over. An error says why a game does not read.
pub fn to_game(
    g: &ExportedGame,
    owner: &PlayerKey,
    owner_id: &str,
    lichess_url: &str,
) -> Result<Option<NewGame>, String> {
    if !matches!(g.variant.as_str(), "standard" | "fromPosition" | "") {
        return Ok(None);
    }
    if !g.status.is_over() {
        return Ok(None);
    }
    let Some(our) = g.side_of(owner_id).map(side) else {
        return Err(format!("{owner_id} plays neither side"));
    };
    let (mut white, mut black) = (seat_of(&g.players.white), seat_of(&g.players.black));
    if *owner == PlayerKey::You {
        let seat = if our == Side::White { &mut white } else { &mut black };
        seat.player = PlayerKey::You;
    }
    let mut pos =
        Position::from_start(g.initial_fen.as_deref().filter(|f| *f != "startpos")).map_err(|e| e.to_string())?;
    let start_fen = (!pos.is_standard_start()).then(|| pos.fen());
    let time = match (g.clock, g.days_per_turn) {
        (Some(c), _) => TimeControl::clock(u64::from(c.initial) * 1000, u64::from(c.increment) * 1000),
        (None, Some(days)) => TimeControl::Correspondence { days },
        (None, None) => TimeControl::Correspondence { days: 0 },
    };
    let increment = g.clock.map_or(0, |c| u64::from(c.increment) * 1000);
    let initial = g.clock.map(|c| u64::from(c.initial) * 1000);
    let mut left = [initial, initial];
    let mut moves = Vec::new();
    for (i, san) in g.moves.split_whitespace().enumerate() {
        let mover = pos.turn();
        let played = pos.play(san).map_err(|_| format!("{san} is not legal at ply {}", i + 1))?;
        // The mover's clock after the move, and what it spent: what the clock lost, less the increment it got back.
        let after = g.clocks.get(i).map(|cs| cs * 10);
        let before = left[mover.index()];
        let think = match (before, after) {
            (Some(b), Some(a)) if i >= 2 => Some((b + increment).saturating_sub(a)),
            _ => None,
        };
        if after.is_some() {
            left[mover.index()] = after;
        }
        let eval = g.analysis.get(i).and_then(|a| match (a.eval, a.mate) {
            (Some(cp), _) => Some(Score::Cp(cp)),
            (None, Some(m)) => Some(Score::Mate(m)),
            _ => None,
        });
        moves.push(Move { clock_ms: after, think_ms: think, eval, ..Move::new(played.uci, played.san) });
    }
    let winner = g.winner.map(side);
    let Some(end) = dameless_core::lichess::ending(g.status.key(), winner, Some(&pos)) else {
        return Ok(None);
    };
    let opening =
        g.opening.as_ref().and_then(|o| Some(Opening { eco: o.eco.clone(), name: o.name.clone()?, ply: o.ply }));
    let speed = Speed::parse(&g.speed).or_else(|| time.speed()).unwrap_or(Speed::Correspondence);
    let source = Source::Import { owner: owner.clone(), id: g.id.clone(), rated: g.rated, speed };
    Ok(Some(NewGame {
        white,
        black,
        url: source.url(lichess_url),
        source,
        time,
        status: end.status(),
        outcome: end.outcome,
        ending: Some(end.ending),
        opening,
        start_fen,
        moves,
        started_at: Timestamp::from_unix_ms(g.created_at),
        ended_at: g.last_move_at.map(Timestamp::from_unix_ms),
    }))
}

#[cfg(test)]
mod tests {
    use dameless_core::game::{Ending, GameStatus as Status, Outcome};
    use dameless_lichess::GameStatus;

    use super::*;

    fn game() -> ExportedGame {
        serde_json::from_value(dameless_lichess::testing::exported_game("imp00001", 1_700_000_000_000, "Dame", "Bob"))
            .unwrap()
    }

    #[test]
    fn maps_an_exported_game() {
        let g = to_game(&game(), &PlayerKey::You, "dame", "https://lichess.org").unwrap().unwrap();
        assert_eq!(g.white.player, PlayerKey::You);
        assert_eq!(g.black.player, PlayerKey::Lichess("bob".into()));
        assert_eq!(g.black.rating, Some(1790));
        assert_eq!(
            (g.status, g.outcome, g.ending),
            (Status::Finished, Some(Outcome::WhiteWins), Some(Ending::Checkmate))
        );
        assert_eq!(g.moves.len(), 7);
        assert_eq!(g.moves[0].uci, "e2e4");
        assert_eq!(g.moves[6].san, "Qxf7#");
        assert_eq!(g.moves[0].clock_ms, Some(180_030));
        assert_eq!(g.moves[0].think_ms, None);
        // White's second move: 180.03 s + 2 s increment, down to 178.00 s.
        assert_eq!(g.moves[2].think_ms, Some(4_030));
        // Lichess's evaluations are White's: mate in one for White after Black's sixth ply.
        assert_eq!(g.moves[5].eval, Some(Score::Mate(1)));
        assert_eq!(g.opening.as_ref().unwrap().ply, Some(2));
        assert_eq!(g.time, TimeControl::clock(180_000, 2_000));
        assert_eq!(g.url.as_deref(), Some("https://lichess.org/imp00001"));
        assert!(matches!(&g.source, Source::Import { owner: PlayerKey::You, speed: Speed::Blitz, rated: true, .. }));
        // Another player's history keeps their key.
        let theirs =
            to_game(&game(), &PlayerKey::Lichess("bob".into()), "bob", "https://lichess.org").unwrap().unwrap();
        assert_eq!(theirs.white.player, PlayerKey::Lichess("dame".into()));
        assert!(to_game(&game(), &PlayerKey::You, "carol", "x").is_err());
    }

    #[test]
    fn skips_games_that_are_not_standard_or_not_over() {
        let mut g = game();
        g.variant = "chess960".into();
        assert_eq!(to_game(&g, &PlayerKey::You, "dame", "x"), Ok(None));
        let mut g = game();
        g.status = GameStatus::Started;
        assert_eq!(to_game(&g, &PlayerKey::You, "dame", "x"), Ok(None));
    }
}
