//! Importing history from Lichess, as work (`WorkKind::Import`, network only): your finished games, or another
//! player's. Each game becomes a finished game here (`Source::Import`, owned by whose history it is), stored once per
//! owner, with its clocks, Lichess's evaluations where the game was analysed, and the opening. Your history, asked for
//! with nothing capped or filtered, comes in whole: the games newer than the newest imported before, then (until
//! [`COMPLETE_KEY`] says it is all here) every game older than the oldest imported before. Another player's is
//! assistance, so it waits while you play on Lichess.
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

/// Whether your whole history is here: an import went back until Lichess had no older game. Later imports of your
/// history then only fetch what is new.
pub const COMPLETE_KEY: &str = "lichess.you.import.complete";

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
    /// Games started at or after this time (ms since 1970). Your history: when absent (with no `max`, `speeds` or
    /// `rated`), the games newer than the newest imported before and, until your whole history is here, every game
    /// older than the oldest imported before.
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default)]
    pub speeds: Vec<Speed>,
    #[serde(default)]
    pub rated: Option<bool>,
}

impl ImportSpec {
    /// Whether this import brings in your whole history: your games, nothing filtered or capped.
    fn whole_history(&self) -> bool {
        self.player == PlayerKey::You
            && self.max.is_none()
            && self.since.is_none()
            && self.speeds.is_empty()
            && self.rated.is_none()
    }
}

/// Which part of your history an import is reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    /// Games newer than the newest imported before (all of them the first time).
    #[default]
    New,
    /// Games older than the oldest imported before, until Lichess has none.
    Older,
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
    #[serde(default)]
    phase: Phase,
    /// While reading older games: the oldest game stored when that began (ms), which they are all older than.
    #[serde(default)]
    older_than: Option<i64>,
    /// Your games stored when the import began, and how many games your Lichess account has (when Lichess said): what
    /// the progress counts against while your whole history comes in.
    #[serde(default)]
    had: u32,
    #[serde(default)]
    total: Option<u32>,
}

/// How a run of the export ended.
enum Read {
    Done,
    Stopped,
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

/// What one import reads from: the client, whose history and whether reading it is assistance.
struct Reading<'a> {
    client: &'a dameless_lichess::Lichess,
    username: &'a str,
    owner_id: String,
    lichess_url: String,
    assist: Option<AssistKind>,
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
        let store = &cx.hub.store;
        let whole = spec.whole_history();
        let complete = whole && store.meta::<bool>(COMPLETE_KEY).await?.unwrap_or(false);
        let fresh = cx.checkpoint::<Checkpoint>().is_none();
        let mut cp: Checkpoint = cx.checkpoint().unwrap_or_default();
        if whole && !complete && fresh {
            // What the progress counts against: one request, and none of it matters if Lichess does not answer.
            cp.had = store.imported_history(PlayerKey::You).await?.0;
            cp.total = client.account().await.ok().and_then(|a| a.count).map(|c| c.all).filter(|&n| n > 0);
        }
        let source = Reading {
            client: &client,
            username: &username,
            owner_id: username.to_lowercase(),
            lichess_url: self.connections.lichess_url().to_owned(),
            assist,
        };

        if cp.phase == Phase::New {
            let since = match (spec.since, &spec.player) {
                (Some(s), _) => Some(s),
                (None, PlayerKey::You) => store.meta::<i64>(SINCE_KEY).await?.map(|s| s + 1),
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
            if remaining != Some(0)
                && let Read::Stopped = self.read(&cx, &source, &spec, query, &mut cp).await?
            {
                return Err(cx.stopped());
            }
            if spec.player == PlayerKey::You
                && let Some(newest) = cp.newest
            {
                let before = store.meta::<i64>(SINCE_KEY).await?.unwrap_or(0);
                store.put_meta(SINCE_KEY, &newest.max(before)).await?;
            }
            if whole && !complete {
                // Read from the start with nothing stored before: that was the whole history already.
                let oldest = store.imported_history(PlayerKey::You).await?.1.and_then(|t| t.unix_ms());
                match (since, oldest) {
                    (Some(_), Some(oldest)) => {
                        cp.phase = Phase::Older;
                        cp.older_than = Some(oldest);
                        cx.save_checkpoint(&cp).await?;
                    }
                    _ => store.put_meta(COMPLETE_KEY, &true).await?,
                }
            }
        }

        if cp.phase == Phase::Older
            && let Some(older_than) = cp.older_than
        {
            let until = cp.oldest.map_or(older_than, |o| o.min(older_than)) - 1;
            cx.log(format!("Going back for older games, before {}.", Timestamp::from_unix_ms(until + 1)));
            let query = ExportQuery { until: Some(until), ..ExportQuery::default() };
            if let Read::Stopped = self.read(&cx, &source, &spec, query, &mut cp).await? {
                return Err(cx.stopped());
            }
            store.put_meta(COMPLETE_KEY, &true).await?;
            cx.log("Your whole history is here: later imports fetch only new games.");
        }

        cx.log(format!(
            "Done: {} new, {} already here, {} not standard chess or unreadable.",
            cp.imported, cp.skipped, cp.other
        ));
        let complete = spec.player == PlayerKey::You && store.meta::<bool>(COMPLETE_KEY).await?.unwrap_or(false);
        Ok(serde_json::json!({
            "player": spec.player,
            "imported": cp.imported,
            "skipped": cp.skipped,
            "other": cp.other,
            "newest": cp.newest.map(Timestamp::from_unix_ms),
            "complete": complete,
        }))
    }
}

impl ImportExecutor {
    /// Reads one export to its end, storing the games in batches and saving the checkpoint after each. A Lichess
    /// error is stored up to, then returned; the checkpoint lets a restart carry on below the oldest game read.
    async fn read(
        &self,
        cx: &WorkContext,
        source: &Reading<'_>,
        spec: &ImportSpec,
        query: ExportQuery,
        cp: &mut Checkpoint,
    ) -> Result<Read, Error> {
        let mut batch: Vec<NewGame> = Vec::new();
        let (tx, mut breaks) = tokio::sync::mpsc::unbounded_channel::<String>();
        let mut export = source.client.export_games(source.username, query).on_break(move |b| {
            let _ = tx.send(format!("{} games read; the export broke ({}), going on.", b.read, b.error));
        });
        loop {
            let next = tokio::select! {
                _ = cx.cancel.cancelled() => {
                    self.flush(cx, &mut batch, cp).await?;
                    return Ok(Read::Stopped);
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
                    match to_game(&g, &spec.player, &source.owner_id, &source.lichess_url) {
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
                    self.flush(cx, &mut batch, cp).await?;
                    return Err(error::lichess(e));
                }
            }
            if batch.len() >= BATCH {
                self.flush(cx, &mut batch, cp).await?;
                cx.progress_now(progress(cp, spec.max)).await?;
                if !cx.safe_point(source.assist).await {
                    return Ok(Read::Stopped);
                }
            }
        }
        self.flush(cx, &mut batch, cp).await?;
        cx.progress_now(progress(cp, spec.max)).await?;
        Ok(Read::Done)
    }

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

/// `3,708`.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// How far an import is. Your whole history counts the games of yours here (stored before, and read since) against
/// how many your account has; anything else counts the games read, against `max` when there is one.
fn progress(cp: &Checkpoint, max: Option<u32>) -> Progress {
    let new = thousands(u64::from(cp.imported));
    match (max, cp.total) {
        (None, Some(total)) => {
            let done = u64::from(cp.had) + u64::from(cp.read);
            let total = u64::from(total).max(done);
            Progress {
                done,
                total: Some(total),
                unit: "games".into(),
                eta_seconds: None,
                text: Some(format!("{} of about {} games, {new} new", thousands(done), thousands(total))),
            }
        }
        _ => Progress {
            done: u64::from(cp.read),
            total: max.map(u64::from),
            unit: "games".into(),
            eta_seconds: None,
            text: Some(format!("{} games read, {new} new", thousands(u64::from(cp.read)))),
        },
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
