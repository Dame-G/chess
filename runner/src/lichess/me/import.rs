//! Importing your Lichess history: your finished games (`GET /api/games/user/{username}`, NDJSON with clocks, the
//! opening and Lichess's evaluations where the game was analysed) become finished matches with `account: "me"`. Each
//! Lichess game is stored once, and a run with no `since` carries on from the newest game imported before.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::San};
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use super::super::{meta_get, meta_set, rules as bot_rules};
use crate::{
    api::AppState,
    engine::uci::Score,
    matches::{
        self, LichessAccount, LichessGame, Match, MatchMove, MatchStatus, MatchTime, Player,
        rules::{self as match_rules, GameResult, Reason, Side},
    },
};

/// The `createdAt` (ms) of the newest game imported, so the next run asks only for newer ones.
pub const SINCE_KEY: &str = "lichess.me.import.since";

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct ImportRequest {
    /// At most this many games, newest first. All when absent.
    #[serde(default)]
    pub max: Option<u32>,
    /// Only games started at or after this time, in milliseconds since 1970. When absent: after the newest game
    /// imported before, or all of them the first time.
    #[serde(default)]
    pub since: Option<i64>,
}

fn player(v: &Value) -> Player {
    let name = match v["aiLevel"].as_u64() {
        Some(level) => format!("Stockfish level {level}"),
        None => v["user"]["name"].as_str().unwrap_or("Anonymous").to_owned(),
    };
    Player {
        lichess: v["user"]["name"].as_str().map(str::to_owned),
        name,
        rating: v["rating"].as_u64().map(|r| r as u32),
        title: v["user"]["title"].as_str().map(str::to_owned),
        ..Player::default()
    }
}

/// An evaluation from White's side as the mover's own score.
fn mover_score(eval: &Value, mover: Side) -> Option<Score> {
    let sign = if mover == Side::White { 1 } else { -1 };
    if let Some(cp) = eval["eval"].as_i64() {
        return Some(Score::Cp(sign * cp as i32));
    }
    eval["mate"].as_i64().map(|m| Score::Mate(sign * m as i32))
}

fn iso(ms: i64) -> String {
    super::super::quota::iso(ms)
}

/// One game of the export as a finished match, or `None` for one that is not standard chess or not over.
pub fn to_match(g: &Value, me: &str) -> Result<Option<Match>> {
    let id = g["id"].as_str().context("a game without an id")?.to_owned();
    if !matches!(g["variant"].as_str().unwrap_or("standard"), "standard" | "fromPosition") {
        return Ok(None);
    }
    let status = g["status"].as_str().unwrap_or_default();
    if matches!(status, "created" | "started") {
        return Ok(None);
    }
    let white = &g["players"]["white"];
    let black = &g["players"]["black"];
    let our = if white["user"]["id"].as_str() == Some(me) {
        Side::White
    } else if black["user"]["id"].as_str() == Some(me) {
        Side::Black
    } else {
        bail!("game {id}: {me} plays neither side");
    };
    let (mut wp, mut bp) = (player(white), player(black));
    match our {
        Side::White => wp.human = true,
        Side::Black => bp.human = true,
    }
    let initial = g["initialFen"].as_str().unwrap_or("startpos");
    let mut pos: Chess = if initial == "startpos" {
        Chess::default()
    } else {
        initial
            .parse::<Fen>()
            .ok()
            .and_then(|f| f.into_position(CastlingMode::Standard).ok())
            .with_context(|| format!("game {id}: a start position that does not read"))?
    };
    let start_fen = if pos == Chess::default() {
        "startpos".to_owned()
    } else {
        Fen::from_position(&pos, EnPassantMode::Legal).to_string()
    };
    let turn0 = Side::from(pos.turn());
    let clock = &g["clock"];
    let time = match (clock["initial"].as_u64(), clock["increment"].as_u64(), g["daysPerTurn"].as_u64()) {
        (Some(initial), Some(increment), _) => {
            MatchTime::Clock { seconds: initial as f64, increment: increment as f64 }
        }
        (_, _, Some(days)) => MatchTime::Movetime { ms: days * 86_400_000 },
        _ => MatchTime::Movetime { ms: 0 },
    };
    // Centiseconds left after each move; the first entry is the clock after White's first move.
    let clocks: Vec<u64> =
        g["clocks"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
    let analysis = g["analysis"].as_array().cloned().unwrap_or_default();
    let increment_ms = clock["increment"].as_u64().unwrap_or(0) * 1000;
    let initial_ms = clock["initial"].as_u64().map(|s| s * 1000);
    let (mut cw, mut cb) = (initial_ms, initial_ms);
    let mut history = vec![match_rules::hash(&pos)];
    let mut moves = Vec::new();
    for (i, san) in g["moves"].as_str().unwrap_or_default().split_whitespace().enumerate() {
        let side = Side::from(pos.turn());
        let m = san
            .parse::<San>()
            .ok()
            .and_then(|s| s.to_move(&pos).ok())
            .with_context(|| format!("game {id}: {san} is not legal at ply {}", i + 1))?;
        let uci = m.to_uci(CastlingMode::Standard).to_string();
        let san = shakmaty::san::SanPlus::from_move_and_play_unchecked(&mut pos, m).to_string();
        history.push(match_rules::hash(&pos));
        let left = clocks.get(i).map(|cs| cs * 10);
        // Time spent: what the mover's clock lost, less the increment it got back.
        let before = match side {
            Side::White => cw,
            Side::Black => cb,
        };
        let took = match (before, left) {
            (Some(b), Some(a)) if i >= 2 => (b + increment_ms).saturating_sub(a),
            _ => 0,
        };
        if left.is_some() {
            match side {
                Side::White => cw = left,
                Side::Black => cb = left,
            }
        }
        let eval = analysis.get(i).and_then(|e| mover_score(e, side));
        moves.push(MatchMove {
            ply: i as u32 + 1,
            side,
            uci,
            san,
            fen_after: Fen::from_position(&pos, EnPassantMode::Legal).to_string(),
            clock_white_ms: if clocks.is_empty() { None } else { cw },
            clock_black_ms: if clocks.is_empty() { None } else { cb },
            eval,
            depth: None,
            nodes: None,
            time_ms: took,
        });
    }
    let winner = match g["winner"].as_str() {
        Some("white") => Some(Side::White),
        Some("black") => Some(Side::Black),
        _ => None,
    };
    let ending = bot_rules::ending(status, winner, &pos, &history)
        .unwrap_or(match_rules::Ending { result: GameResult::Unfinished, reason: Reason::Other });
    let opening = match (g["opening"]["eco"].as_str(), g["opening"]["name"].as_str()) {
        (Some(eco), Some(name)) => Some(format!("{eco} {name}")),
        (None, Some(name)) => Some(name.to_owned()),
        _ => None,
    };
    let created = g["createdAt"].as_i64().unwrap_or(0);
    Ok(Some(Match {
        id: 0,
        white: wp,
        black: bp,
        time,
        start_fen,
        book: false,
        hash: 0,
        threads: 0,
        adjudicate: false,
        minimal: false,
        status: if ending.reason == Reason::Aborted { MatchStatus::Aborted } else { MatchStatus::Finished },
        result: ending.result,
        reason: Some(ending.reason),
        turn: if moves.len() % 2 == 0 { turn0 } else { other(turn0) },
        clock_white_ms: if clocks.is_empty() { None } else { cw },
        clock_black_ms: if clocks.is_empty() { None } else { cb },
        moves,
        created_at: iso(created),
        finished_at: g["lastMoveAt"].as_i64().map(iso),
        lichess: Some(LichessGame {
            url: format!("https://lichess.org/{id}"),
            id,
            rated: g["rated"].as_bool().unwrap_or(false),
            speed: g["speed"].as_str().unwrap_or_default().to_owned(),
            our_side: our,
            account: LichessAccount::Me,
            opening,
            draw_offer: None,
            takeback_offer: None,
        }),
    }))
}

fn other(side: Side) -> Side {
    match side {
        Side::White => Side::Black,
        Side::Black => Side::White,
    }
}

/// How many games to store at once.
const BATCH: usize = 100;

/// The import job: streams the games, stores the new ones in batches, and moves the cursor to the newest.
pub async fn run(
    state: &Arc<AppState>,
    cancel: &CancellationToken,
    log: &(dyn Fn(&str) + Sync),
    req: &ImportRequest,
) -> Result<Value> {
    let client = state.me.client().context("your Lichess account is not connected: log in first")?;
    let account = match state.me.account() {
        Some(a) => a,
        None => super::read_me(&client.get("/api/account").await?),
    };
    let since = match req.since {
        Some(s) => Some(s),
        None => meta_get(state, SINCE_KEY).await?.and_then(|s| s.parse::<i64>().ok()).map(|s| s + 1),
    };
    let mut path = format!(
        "/api/games/user/{}?finished=true&moves=true&clocks=true&evals=true&opening=true&sort=dateDesc",
        account.username
    );
    if let Some(s) = since {
        path += &format!("&since={s}");
    }
    if let Some(max) = req.max {
        path += &format!("&max={}", max.max(1));
    }
    log(&format!(
        "importing {}'s games{}",
        account.username,
        since.map(|s| format!(" since {}", iso(s))).unwrap_or_default()
    ));
    let mut stream = client.stream(&path).await?;
    let (mut stored, mut skipped, mut other, mut seen) = (0usize, 0usize, 0usize, 0usize);
    let mut newest = since.map(|s| s - 1).unwrap_or(0);
    let mut batch = Vec::new();
    loop {
        let line = tokio::select! {
            _ = cancel.cancelled() => bail!("cancelled"),
            line = stream.next() => line?,
        };
        let done = line.is_none();
        if let Some(g) = line {
            seen += 1;
            newest = newest.max(g["createdAt"].as_i64().unwrap_or(0));
            match to_match(&g, &account.id) {
                Ok(Some(m)) => batch.push(m),
                Ok(None) => other += 1,
                Err(e) => {
                    log(&format!("skipped: {e:#}"));
                    other += 1;
                }
            }
        }
        if batch.len() >= BATCH || (done && !batch.is_empty()) {
            let (s, k) = matches::import(state, std::mem::take(&mut batch)).await?;
            stored += s;
            skipped += k;
            log(&format!("{seen} games read, {stored} stored"));
        }
        if done {
            break;
        }
    }
    if newest > 0 {
        meta_set(state, SINCE_KEY, newest.to_string()).await?;
    }
    log(&format!("done: {stored} new, {skipped} already here, {other} not standard chess or unreadable"));
    Ok(json!({ "imported": stored, "skipped": skipped, "other": other, "newest": (newest > 0).then(|| iso(newest)) }))
}
