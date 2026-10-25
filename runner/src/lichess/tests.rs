//! The bot against a tiny Lichess of its own: an event stream that sends two challenges and then the game the bot
//! accepted, a game stream that answers every move of ours with one of its own, and a stand-in engine that plays the
//! first legal move. Nothing leaves the machine and no engine binary is needed.

use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Form, Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, Position, uci::UciMove};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

use super::{
    game::{Brain, BrainSpec},
    *,
};
use crate::{
    config::Config,
    engine::uci::{Info, Score},
    matches::{
        self, MatchStatus, Source,
        rules::{GameResult, Reason, Side},
    },
    store::Store,
};

// ---------------------------------------------------------------------------------------------------------------------
// A stand-in engine
// ---------------------------------------------------------------------------------------------------------------------

/// Plays the first legal move, after one progress report; or, when `crash`, fails as an engine that died would.
struct FirstLegal {
    crash: bool,
}

fn position(moves: &[String]) -> Chess {
    let mut pos = Chess::default();
    for m in moves {
        let m = m.parse::<UciMove>().unwrap().to_move(&pos).unwrap();
        pos.play_unchecked(m);
    }
    pos
}

fn first_legal(pos: &Chess) -> String {
    pos.legal_moves()[0].to_uci(CastlingMode::Standard).to_string()
}

impl Brain for FirstLegal {
    fn think<'a>(
        &'a mut self,
        _start: &'a str,
        moves: &'a [String],
        go: &'a str,
        info: &'a mut (dyn FnMut(&Info) + Send),
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        Box::pin(async move {
            assert!(go.starts_with("go wtime "), "{go}");
            if self.crash {
                anyhow::bail!("engine exited during the search");
            }
            let best = first_legal(&position(moves));
            info(&Info {
                depth: 7,
                seldepth: 9,
                multipv: 1,
                score: Score::Cp(31),
                nodes: 1234,
                nps: 100_000,
                time: 12,
                pv: vec![best.clone()],
            });
            Ok(best)
        })
    }

    fn quit(self: Box<Self>) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}

fn stand_in(crash: bool) -> Brains {
    Arc::new(move |_: BrainSpec| Box::pin(async move { Ok(Box::new(FirstLegal { crash }) as Box<dyn Brain>) }))
}

/// How the mock's opponent ends the game.
#[derive(Clone, Copy, Default, PartialEq)]
enum Mode {
    /// Resigns after the fifth ply.
    #[default]
    Resigns,
    /// Leaves after the fifth ply; the bot then claims the win.
    Leaves,
}

// ---------------------------------------------------------------------------------------------------------------------
// A tiny Lichess
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Mock {
    /// Every POST, as `path` or `path reason=...`.
    posts: Mutex<Vec<String>>,
    events: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    game: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    moves: Mutex<Vec<String>>,
    user_agent: Mutex<String>,
    mode: Mode,
}

/// After this many plies the opponent resigns.
const PLIES: usize = 5;

fn ndjson(rx: mpsc::UnboundedReceiver<Value>) -> Body {
    let lines = UnboundedReceiverStream::new(rx).map(|v| Ok::<_, Infallible>(Bytes::from(format!("{v}\n"))));
    Body::from_stream(lines)
}

fn challenge(id: &str, variant: &str) -> Value {
    json!({ "type": "challenge", "challenge": {
        "id": id, "url": format!("https://lichess.org/{id}"), "status": "created",
        "challenger": { "id": "alice", "name": "Alice", "rating": 1650, "title": null },
        "destUser": { "id": "recklessbot", "name": "RecklessBot", "title": "BOT", "rating": 2000 },
        "variant": { "key": variant, "name": variant }, "rated": true, "speed": "blitz",
        "timeControl": { "type": "clock", "limit": 180, "increment": 2, "show": "3+2" },
        "color": "random", "finalColor": "white", "perf": { "name": "Blitz" }
    }})
}

fn state(moves: &[String], status: &str, winner: Option<&str>) -> Value {
    let n = moves.len() as u64;
    json!({ "type": "gameState", "moves": moves.join(" "), "wtime": 180_000 - n * 500, "btime": 180_000 - n * 400,
        "winc": 2000, "binc": 2000, "status": status, "winner": winner })
}

async fn account(State(m): State<Arc<Mock>>, headers: HeaderMap) -> axum::Json<Value> {
    *m.user_agent.lock().unwrap() =
        headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
    axum::Json(json!({ "id": "recklessbot", "username": "RecklessBot", "title": "BOT" }))
}

async fn event_stream(State(m): State<Arc<Mock>>) -> Body {
    let (tx, rx) = mpsc::unbounded_channel();
    tx.send(challenge("c1", "standard")).unwrap();
    tx.send(challenge("c2", "chess960")).unwrap();
    *m.events.lock().unwrap() = Some(tx);
    ndjson(rx)
}

async fn accept(State(m): State<Arc<Mock>>, Path(id): Path<String>) -> axum::Json<Value> {
    m.posts.lock().unwrap().push(format!("accept {id}"));
    let tx = m.events.lock().unwrap().clone().unwrap();
    tx.send(json!({ "type": "gameStart", "game": { "gameId": "g1", "fullId": "g1abcd", "color": "white" } })).unwrap();
    axum::Json(json!({ "ok": true }))
}

async fn decline(
    State(m): State<Arc<Mock>>,
    Path(id): Path<String>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> axum::Json<Value> {
    m.posts.lock().unwrap().push(format!("decline {id} reason={}", form.get("reason").cloned().unwrap_or_default()));
    axum::Json(json!({ "ok": true }))
}

async fn game_stream(State(m): State<Arc<Mock>>, Path(id): Path<String>) -> Body {
    assert_eq!(id, "g1");
    let (tx, rx) = mpsc::unbounded_channel();
    let moves = m.moves.lock().unwrap().clone();
    tx.send(json!({
        "type": "gameFull", "id": "g1", "rated": true, "speed": "blitz", "variant": { "key": "standard" },
        "clock": { "initial": 180_000, "increment": 2000 },
        "white": { "id": "recklessbot", "name": "RecklessBot", "title": "BOT", "rating": 2000 },
        "black": { "id": "alice", "name": "Alice", "title": null, "rating": 1650 },
        "initialFen": "startpos", "state": state(&moves, "started", None)
    }))
    .unwrap();
    tx.send(json!({ "type": "chatLine", "username": "Alice", "text": "good luck", "room": "player" })).unwrap();
    *m.game.lock().unwrap() = Some(tx);
    ndjson(rx)
}

async fn play_move(State(m): State<Arc<Mock>>, Path((id, uci)): Path<(String, String)>) -> axum::Json<Value> {
    assert_eq!(id, "g1");
    m.posts.lock().unwrap().push(format!("move {uci}"));
    let tx = m.game.lock().unwrap().clone().unwrap();
    let mut moves = m.moves.lock().unwrap();
    // A move that is not legal would be refused by Lichess; the test fails instead.
    position(&moves).play(uci.parse::<UciMove>().unwrap().to_move(&position(&moves)).unwrap()).unwrap();
    moves.push(uci);
    tx.send(state(&moves, "started", None)).unwrap();
    if moves.len() >= PLIES {
        match m.mode {
            Mode::Resigns => tx.send(state(&moves, "resign", Some("white"))).unwrap(),
            Mode::Leaves => tx.send(json!({ "type": "opponentGone", "gone": true, "claimWinInSeconds": 0 })).unwrap(),
        }
    } else {
        let reply = first_legal(&position(&moves));
        moves.push(reply);
        tx.send(state(&moves, "started", None)).unwrap();
    }
    axum::Json(json!({ "ok": true }))
}

/// Claiming the win, resigning and aborting: Lichess ends the game, and says so on the game stream.
async fn end_game(State(m): State<Arc<Mock>>, Path((id, action)): Path<(String, String)>) -> axum::Json<Value> {
    m.posts.lock().unwrap().push(format!("{action} {id}"));
    let tx = m.game.lock().unwrap().clone().unwrap();
    let moves = m.moves.lock().unwrap().clone();
    let end = match action.as_str() {
        "claim-victory" => state(&moves, "timeout", Some("white")),
        "resign" => state(&moves, "resign", Some("black")),
        _ => state(&moves, "aborted", None),
    };
    tx.send(end).unwrap();
    axum::Json(json!({ "ok": true }))
}

async fn mock(mode: Mode) -> (String, Arc<Mock>) {
    let m = Arc::new(Mock { mode, ..Mock::default() });
    let app = Router::new()
        .route("/api/bot/game/{id}/{action}", post(end_game))
        .route("/api/account", get(account))
        .route("/api/stream/event", get(event_stream))
        .route("/api/challenge/{id}/accept", post(accept))
        .route("/api/challenge/{id}/decline", post(decline))
        .route("/api/bot/game/stream/{id}", get(game_stream))
        .route("/api/bot/game/{id}/move/{uci}", post(play_move))
        .with_state(m.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, m)
}

async fn runner(lichess_url: String, crash: bool) -> Arc<AppState> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).unwrap();
    let data = std::env::temp_dir().join(format!("chess-lichess-test-{}", u64::from_le_bytes(bytes)));
    std::fs::create_dir_all(&data).unwrap();
    let config = Config {
        data,
        reckless: ".".into(),
        listen: "127.0.0.1:0".parse().unwrap(),
        token: "runner-token".into(),
        lichess_url,
    };
    let store = Store::memory().unwrap();
    store
        .call(|c| {
            c.execute(
                "INSERT INTO builds (key, ref, commit_sha, subject, features, bench, nps)
                 VALUES ('tree-no-syzygy', 'main', 'abcdef1234567890', 'A test build', 'no-syzygy', 1, 1)",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    Arc::new(AppState::new(config, store, stand_in(crash)))
}

/// Sets up the bot against a mock, takes it online, and waits for game `g1` to end.
async fn play_out(mode: Mode, crash: bool) -> (Arc<AppState>, Arc<Mock>, crate::matches::Match) {
    let (url, mock) = mock(mode).await;
    let state = runner(url, crash).await;
    set_token(&state, "lip_test_token").await.unwrap();
    put_settings(&state, Settings { build: Some("main".into()), ..Settings::default() }).await.unwrap();
    start(&state).await.unwrap();
    let game = finished(&state).await;
    (state, mock, game)
}

async fn finished(state: &AppState) -> crate::matches::Match {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(m) = matches::find_lichess(state, "g1").await.unwrap()
                && m.status != MatchStatus::Running
            {
                break m;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the game should finish")
}

#[tokio::test]
async fn claims_the_win_when_the_opponent_leaves() {
    let (state, mock, game) = play_out(Mode::Leaves, false).await;
    assert!(mock.posts.lock().unwrap().contains(&"claim-victory g1".to_owned()));
    assert_eq!((game.result, game.reason), (GameResult::White, Some(Reason::Abandoned)));
    let _ = std::fs::remove_dir_all(&state.config.data);
}

#[tokio::test]
async fn leaves_the_game_when_the_engine_dies() {
    let (state, mock, game) = play_out(Mode::Resigns, true).await;
    // Before both sides have moved, leaving is an abort.
    assert!(mock.posts.lock().unwrap().contains(&"abort g1".to_owned()));
    assert_eq!((game.status, game.reason), (MatchStatus::Aborted, Some(Reason::Aborted)));
    assert!(game.moves.is_empty());
    let _ = std::fs::remove_dir_all(&state.config.data);
}

#[tokio::test]
async fn accepts_a_challenge_declines_another_and_plays_the_game_into_a_match() {
    let (url, mock) = mock(Mode::Resigns).await;
    let state = runner(url, false).await;
    let (_, mut events) = state.events.subscribe(0);

    let status = set_token(&state, "lip_test_token").await.unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(status.account.as_ref().map(|a| (a.username.as_str(), a.bot)), Some(("RecklessBot", true)));
    assert!(std::fs::read_to_string(token_path(&state.config.data)).unwrap().contains("lip_test_token"));
    // The token is never part of what the API answers.
    assert!(!serde_json::to_string(&state.lichess.status()).unwrap().contains("lip_test_token"));

    let refused = start(&state).await;
    assert!(refused.is_err(), "no build is chosen yet");
    put_settings(&state, Settings { build: Some("main".into()), ..Settings::default() }).await.unwrap();
    start(&state).await.unwrap_or_else(|e| panic!("{}", e.message));

    // The game ends when the opponent resigns after the fifth ply.
    let game = finished(&state).await;

    let posts = mock.posts.lock().unwrap().clone();
    assert!(posts.contains(&"accept c1".to_owned()), "{posts:?}");
    assert!(posts.contains(&"decline c2 reason=variant".to_owned()), "{posts:?}");
    assert_eq!(posts.iter().filter(|p| p.starts_with("move ")).count(), 3, "{posts:?}");
    assert!(mock.user_agent.lock().unwrap().starts_with("Reckless-Lab/"));

    assert_eq!(game.status, MatchStatus::Finished);
    assert_eq!(game.result, GameResult::White);
    assert_eq!(game.reason, Some(Reason::Resigned));
    assert_eq!(game.moves.len(), PLIES);
    let l = game.lichess.as_ref().unwrap();
    assert_eq!(
        (l.id.as_str(), l.url.as_str(), l.rated, l.speed.as_str()),
        ("g1", "https://lichess.org/g1", true, "blitz")
    );
    assert_eq!(l.our_side, Side::White);
    assert_eq!(game.white.build, Some(1));
    assert_eq!(game.white.lichess.as_deref(), Some("RecklessBot"));
    assert_eq!(game.black.build, None);
    assert_eq!((game.black.name.as_str(), game.black.rating), ("Alice", Some(1650)));
    // Our moves carry the engine's report; the opponent's carry none. Clocks are Lichess's.
    for mv in &game.moves {
        match mv.side {
            Side::White => assert_eq!((mv.eval, mv.depth), (Some(Score::Cp(31)), Some(7))),
            Side::Black => assert_eq!(mv.eval, None),
        }
    }
    let last = game.moves.last().unwrap();
    assert_eq!(last.clock_white_ms, Some(180_000 - PLIES as u64 * 500));
    assert!(matches!(game.time, crate::matches::MatchTime::Clock { seconds: 180.0, increment: 2.0 }));

    // It lists among the Lichess games, not the local ones, and the bot is no longer playing it.
    let lichess = matches::list(&state, 10, Some(Source::Lichess)).await.unwrap();
    assert_eq!(lichess.len(), 1);
    assert!(matches::list(&state, 10, Some(Source::Local)).await.unwrap().is_empty());
    assert!(state.lichess.status().playing.is_empty());
    assert_eq!(state.lichess.status().state, BotState::Online);

    // What a client following the events saw.
    let mut kinds = Vec::new();
    while let Ok(e) = events.try_recv() {
        if e.kind == "lichess.challenge" {
            kinds.push(format!("{} {}", e.data["id"].as_str().unwrap(), e.data["decision"].as_str().unwrap()));
        } else {
            kinds.push(e.kind);
        }
    }
    for expected in ["c1 accepted", "c2 declined", "match.started", "match.move", "match.info", "match.ended"] {
        assert!(kinds.iter().any(|k| k == expected), "no {expected} in {kinds:?}");
    }
    assert!(kinds.iter().any(|k| k == "lichess.status"));

    stop(&state).await.unwrap();
    assert_eq!(state.lichess.status().state, BotState::Off);
    clear_token(&state).await.unwrap();
    assert!(!token_path(&state.config.data).exists());
    let _ = std::fs::remove_dir_all(&state.config.data);
}
