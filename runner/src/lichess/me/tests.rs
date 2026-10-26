//! Your account against a tiny Lichess: the OAuth exchange, a Board API game played through the match routes, the
//! import of your history, the fair-play lock, and the explorer and tablebase behind it. Nothing leaves the machine.

use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Form, Router,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::{Value, json};
use shakmaty::{CastlingMode, Chess, Position, uci::UciMove};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

use super::*;
use crate::{
    config::Config,
    matches::{self, Action, LichessAccount, MatchStatus, Reply, Source, rules::Side},
    store::Store,
};

#[derive(Default)]
struct Mock {
    /// The PKCE challenge of the login under way, which the token exchange checks.
    challenge: Mutex<String>,
    posts: Mutex<Vec<String>>,
    events: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    game: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    moves: Mutex<Vec<String>>,
    /// The `since` of each games export asked for.
    exports: Mutex<Vec<Option<i64>>>,
}

const TOKEN: &str = "lio_me_token";

fn ndjson(rx: mpsc::UnboundedReceiver<Value>) -> Body {
    let lines = UnboundedReceiverStream::new(rx).map(|v| Ok::<_, Infallible>(Bytes::from(format!("{v}\n"))));
    Body::from_stream(lines)
}

fn bearer(headers: &HeaderMap) -> String {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_owned()
}

fn position(moves: &[String]) -> Chess {
    let mut pos = Chess::default();
    for m in moves {
        let m = m.parse::<UciMove>().unwrap().to_move(&pos).unwrap();
        pos.play_unchecked(m);
    }
    pos
}

fn game_state(moves: &[String], status: &str, extra: Value) -> Value {
    let mut v = json!({ "type": "gameState", "moves": moves.join(" "), "wtime": 600_000, "btime": 600_000,
        "winc": 5000, "binc": 5000, "status": status });
    if let (Some(v), Some(extra)) = (v.as_object_mut(), extra.as_object()) {
        v.extend(extra.clone());
    }
    v
}

async fn token(State(m): State<Arc<Mock>>, Form(f): Form<HashMap<String, String>>) -> axum::response::Response {
    let ok = f.get("grant_type").map(String::as_str) == Some("authorization_code")
        && f.get("code").map(String::as_str) == Some("liu_code")
        && f.get("client_id").map(String::as_str) == Some(oauth::CLIENT_ID)
        && f.get("redirect_uri").is_some_and(|r| r.ends_with("/api/lichess/me/callback"))
        && f.get("code_verifier").is_some_and(|v| oauth::challenge_of(v) == *m.challenge.lock().unwrap());
    if !ok {
        return (StatusCode::BAD_REQUEST, axum::Json(json!({ "error": "invalid_grant" }))).into_response();
    }
    axum::Json(json!({ "token_type": "Bearer", "access_token": TOKEN, "expires_in": 31_536_000 })).into_response()
}

async fn token_test(body: String) -> axum::Json<Value> {
    let mut out = serde_json::Map::new();
    out.insert(
        body.clone(),
        json!({ "scopes": "board:play,challenge:read,challenge:write", "userId": "dame", "expires": 1_900_000_000_000i64 }),
    );
    axum::Json(Value::Object(out))
}

async fn revoke(State(m): State<Arc<Mock>>) -> StatusCode {
    m.posts.lock().unwrap().push("revoke".into());
    StatusCode::NO_CONTENT
}

async fn account(headers: HeaderMap) -> axum::response::Response {
    match bearer(&headers).as_str() {
        TOKEN => axum::Json(json!({ "id": "dame", "username": "Dame",
            "perfs": { "blitz": { "rating": 1800 }, "rapid": { "rating": 1850 } } }))
        .into_response(),
        "lip_bot" => axum::Json(json!({ "id": "somebot", "username": "SomeBot", "title": "BOT" })).into_response(),
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn event_stream(State(m): State<Arc<Mock>>) -> Body {
    let (tx, rx) = mpsc::unbounded_channel();
    *m.events.lock().unwrap() = Some(tx);
    ndjson(rx)
}

async fn game_stream(State(m): State<Arc<Mock>>, Path(id): Path<String>) -> Body {
    let (tx, rx) = mpsc::unbounded_channel();
    let moves = m.moves.lock().unwrap().clone();
    tx.send(json!({
        "type": "gameFull", "id": id, "rated": false, "speed": "rapid", "variant": { "key": "standard" },
        "clock": { "initial": 600_000, "increment": 5000 },
        "white": { "id": "dame", "name": "Dame", "rating": 1850 },
        "black": { "id": "alice", "name": "Alice", "rating": 1700 },
        "initialFen": "startpos", "state": game_state(&moves, "started", json!({}))
    }))
    .unwrap();
    *m.game.lock().unwrap() = Some(tx);
    ndjson(rx)
}

/// Our move; Alice answers with her first legal move, then offers a draw after the fourth ply.
async fn board_move(State(m): State<Arc<Mock>>, Path((_id, uci)): Path<(String, String)>) -> axum::Json<Value> {
    m.posts.lock().unwrap().push(format!("move {uci}"));
    let tx = m.game.lock().unwrap().clone().unwrap();
    let mut moves = m.moves.lock().unwrap();
    position(&moves).play(uci.parse::<UciMove>().unwrap().to_move(&position(&moves)).unwrap()).unwrap();
    moves.push(uci);
    tx.send(game_state(&moves, "started", json!({}))).unwrap();
    let reply = position(&moves).legal_moves()[0].to_uci(CastlingMode::Standard).to_string();
    moves.push(reply);
    let offer = if moves.len() >= 4 { json!({ "bdraw": true }) } else { json!({}) };
    tx.send(game_state(&moves, "started", offer)).unwrap();
    axum::Json(json!({ "ok": true }))
}

/// Draws, takebacks, resigning, aborting: Lichess answers on the game stream.
async fn board_action(State(m): State<Arc<Mock>>, Path(rest): Path<(String, String)>) -> axum::Json<Value> {
    let (_, action) = rest;
    m.posts.lock().unwrap().push(action.clone());
    let tx = m.game.lock().unwrap().clone().unwrap();
    let moves = m.moves.lock().unwrap().clone();
    match action.as_str() {
        "resign" => tx.send(game_state(&moves, "resign", json!({ "winner": "black" }))).unwrap(),
        "abort" => tx.send(game_state(&moves, "aborted", json!({}))).unwrap(),
        _ => {}
    }
    axum::Json(json!({ "ok": true }))
}

async fn draw(State(m): State<Arc<Mock>>, Path((_id, answer)): Path<(String, String)>) -> axum::Json<Value> {
    m.posts.lock().unwrap().push(format!("draw {answer}"));
    let tx = m.game.lock().unwrap().clone().unwrap();
    let moves = m.moves.lock().unwrap().clone();
    if answer == "yes" {
        tx.send(game_state(&moves, "draw", json!({}))).unwrap();
    }
    axum::Json(json!({ "ok": true }))
}

fn exported(id: &str, created: i64) -> Value {
    json!({
        "id": id, "rated": true, "variant": "standard", "speed": "blitz", "createdAt": created,
        "lastMoveAt": created + 60_000, "status": "mate", "winner": "white",
        "players": {
            "white": { "user": { "name": "Dame", "id": "dame" }, "rating": 1800 },
            "black": { "user": { "name": "Bob", "id": "bob" }, "rating": 1790 }
        },
        "opening": { "eco": "C20", "name": "King's Pawn Game" },
        "moves": "e4 e5 Bc4 Nc6 Qh5 Nf6 Qxf7#",
        "clocks": [18003, 18003, 17800, 17900, 17500, 17000, 17300],
        "analysis": [{ "eval": 30 }, { "eval": 35 }, { "eval": 20 }, { "eval": 25 }, { "eval": 10 }, { "mate": 1 }],
        "clock": { "initial": 180, "increment": 2, "totalTime": 260 }
    })
}

async fn export(State(m): State<Arc<Mock>>, Query(q): Query<HashMap<String, String>>) -> String {
    let since = q.get("since").and_then(|s| s.parse::<i64>().ok());
    m.exports.lock().unwrap().push(since);
    [exported("imp00001", 1_700_000_000_000), exported("imp00002", 1_700_000_100_000)]
        .into_iter()
        .filter(|g| since.is_none_or(|s| g["createdAt"].as_i64().unwrap() >= s))
        .map(|g| format!("{g}\n"))
        .collect()
}

async fn explorer(headers: HeaderMap) -> axum::response::Response {
    if bearer(&headers) != TOKEN {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    axum::Json(json!({ "white": 10, "draws": 2, "black": 7, "moves": [] })).into_response()
}

async fn tablebase() -> axum::Json<Value> {
    axum::Json(json!({ "category": "win", "dtz": 1 }))
}

async fn serve() -> (String, Arc<Mock>) {
    let m = Arc::new(Mock::default());
    let app = Router::new()
        .route("/api/token", post(token).delete(revoke))
        .route("/api/token/test", post(token_test))
        .route("/api/account", get(account))
        .route("/api/stream/event", get(event_stream))
        .route("/api/board/game/stream/{id}", get(game_stream))
        .route("/api/board/game/{id}/move/{uci}", post(board_move))
        .route("/api/board/game/{id}/draw/{answer}", post(draw))
        .route("/api/board/game/{id}/{action}", post(board_action))
        .route("/api/games/user/{name}", get(export))
        .route("/explorer/lichess", get(explorer))
        .route("/tablebase/standard", get(tablebase))
        .with_state(m.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, m)
}

fn runner(lichess_url: String) -> Arc<AppState> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).unwrap();
    let data = std::env::temp_dir().join(format!("chess-lichess-me-test-{}", u64::from_le_bytes(bytes)));
    std::fs::create_dir_all(&data).unwrap();
    let config = Config {
        data,
        reckless: ".".into(),
        listen: "127.0.0.1:7878".parse().unwrap(),
        token: "runner-token".into(),
        lichess_url,
    };
    Arc::new(AppState::new(config, Store::memory().unwrap(), crate::lichess::engines()))
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !ok() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

async fn until_match(state: &AppState, id: i64, what: &str, ok: impl Fn(&crate::matches::Match) -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !ok(&matches::get(state, id).await.unwrap().unwrap()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// Logs in through the OAuth flow, as the browser would, and answers where the browser was sent.
async fn log_in(state: &Arc<AppState>, mock: &Mock) -> String {
    let started = oauth::start(
        state,
        Some("127.0.0.1:7878"),
        oauth::LoginRequest { return_to: Some("https://chess.dame.gg/lichess".into()) },
    )
    .unwrap();
    let url = reqwest::Url::parse(&started.url).unwrap();
    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["redirect_uri"], "http://127.0.0.1:7878/api/lichess/me/callback");
    assert_eq!(q["scope"], "board:play challenge:read challenge:write");
    *mock.challenge.lock().unwrap() = q["code_challenge"].clone();
    let query = serde_json::from_value(json!({ "code": "liu_code", "state": q["state"] })).unwrap();
    let res = oauth::callback(State(state.clone()), Query(query)).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let location = res.headers()["location"].to_str().unwrap().to_owned();
    // The state is single-use.
    let again = serde_json::from_value(json!({ "code": "liu_code", "state": q["state"] })).unwrap();
    assert_eq!(oauth::callback(State(state.clone()), Query(again)).await.status(), StatusCode::BAD_REQUEST);
    location
}

#[tokio::test]
async fn logs_in_with_pkce_and_refuses_a_bot_or_a_strange_return_address() {
    let (url, mock) = serve().await;
    let state = runner(url);
    let bad = oauth::start(&state, None, oauth::LoginRequest { return_to: Some("https://evil.example/".into()) });
    assert_eq!(bad.unwrap_err().status, StatusCode::BAD_REQUEST);

    let location = log_in(&state, &mock).await;
    assert_eq!(location, "https://chess.dame.gg/lichess?lichess=connected");
    let status = state.me.status();
    assert!(status.configured);
    assert_eq!(status.account.as_ref().map(|a| (a.username.as_str(), a.ratings.rapid)), Some(("Dame", Some(1850))));
    assert!(status.missing_scopes.is_empty(), "{:?}", status.missing_scopes);
    assert!(status.expires_at.is_some());
    assert_eq!(std::fs::read_to_string(token_path(&state.config.data)).unwrap(), TOKEN);
    assert!(!serde_json::to_string(&status).unwrap().contains(TOKEN));
    until("the event stream", || state.me.status().state == BotState::Online).await;

    // A BOT account is the bot's business, never yours.
    let refused = set_token(&state, "lip_bot").await.unwrap_err();
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.message.contains("BOT"), "{}", refused.message);

    let status = logout(&state).await.unwrap();
    assert!(!status.configured);
    assert_eq!(status.state, BotState::Off);
    assert!(mock.posts.lock().unwrap().contains(&"revoke".to_owned()));
    assert!(!token_path(&state.config.data).exists());
    let _ = std::fs::remove_dir_all(&state.config.data);
}

#[tokio::test]
async fn a_game_is_played_through_the_match_routes_and_locks_analysis_meanwhile() {
    let (url, mock) = serve().await;
    let state = runner(url);
    log_in(&state, &mock).await;
    until("the event stream", || mock.events.lock().unwrap().is_some()).await;
    assert!(
        explorer::tablebase(&state, explorer::TablebaseQuery { fen: "8/8/8/8/8/8/k7/2K4R w - - 0 1".into() })
            .await
            .is_ok()
    );
    let (_, mut events) = state.events.subscribe(0);

    let tx = mock.events.lock().unwrap().clone().unwrap();
    tx.send(json!({ "type": "gameStart", "game": { "gameId": "m1", "speed": "rapid", "variant": { "key": "standard" },
        "compat": { "board": true, "bot": false } } }))
        .unwrap();
    until("the game's record", || !state.me.status().playing.is_empty()).await;
    let id = state.me.status().playing[0];

    // Fair play: no engine, no tablebase, no explorer while the game runs.
    assert!(state.me.status().fair_play_lock);
    let req = serde_json::from_value(json!({ "fen": "startpos" })).unwrap();
    let refused = crate::analysis::analyse(&state, req).await.unwrap_err();
    assert_eq!(
        (refused.status, refused.message.as_str()),
        (StatusCode::CONFLICT, "Analysis is off while you're playing on Lichess")
    );
    let req = serde_json::from_value(json!({ "fen": "startpos" })).unwrap();
    assert_eq!(crate::analysis::start_live(&state, req).await.unwrap_err().status, StatusCode::CONFLICT);
    let tb = explorer::TablebaseQuery { fen: "8/8/8/8/8/8/k7/2K4R w - - 0 1".into() };
    assert_eq!(explorer::tablebase(&state, tb).await.unwrap_err().status, StatusCode::CONFLICT);
    assert_eq!(
        explorer::explorer(&state, explorer::ExplorerQuery::default()).await.unwrap_err().status,
        StatusCode::CONFLICT
    );

    // You move through the match routes; Lichess plays it and Alice answers.
    assert_eq!(matches::act(&state, id, Action::Move("e2e4".into())).await.unwrap(), Reply::Moved);
    until_match(&state, id, "Alice's reply", |m| m.moves.len() == 2).await;
    let m = matches::get(&state, id).await.unwrap().unwrap();
    assert_eq!(m.moves[0].san, "e4");
    assert!(m.white.human && m.white.name == "Dame");
    assert_eq!((m.black.name.as_str(), m.black.rating), ("Alice", Some(1700)));
    assert_eq!(m.lichess.as_ref().unwrap().account, LichessAccount::Me);
    let illegal = matches::act(&state, id, Action::Move("e2e4".into())).await.unwrap_err();
    assert_eq!(illegal.status, StatusCode::BAD_REQUEST);
    let nothing = matches::act(&state, id, Action::DeclineDraw).await.unwrap_err();
    assert_eq!(nothing.status, StatusCode::CONFLICT);

    // After the fourth ply Alice offers a draw, and taking it ends the game.
    assert_eq!(matches::act(&state, id, Action::Move("d2d4".into())).await.unwrap(), Reply::Moved);
    until_match(&state, id, "the draw offer", |m| m.lichess.as_ref().unwrap().draw_offer == Some(Side::Black)).await;
    assert_eq!(matches::act(&state, id, Action::Draw).await.unwrap(), Reply::Draw { accepted: true });
    until("the end", || state.me.status().playing.is_empty()).await;
    let m = matches::find_lichess(&state, "m1", LichessAccount::Me).await.unwrap().unwrap();
    assert_eq!(m.status, MatchStatus::Finished);
    assert_eq!(m.reason, Some(crate::matches::rules::Reason::Agreement));
    assert_eq!(m.moves.len(), 4);
    let posts = mock.posts.lock().unwrap().clone();
    assert!(posts.contains(&"move e2e4".to_owned()) && posts.contains(&"draw yes".to_owned()), "{posts:?}");

    // The lock lifts with the game.
    assert!(!state.me.status().fair_play_lock);
    let tb = explorer::TablebaseQuery { fen: "8/8/8/8/8/8/k7/2K4R w - - 0 1".into() };
    assert_eq!(explorer::tablebase(&state, tb).await.unwrap()["category"], "win");
    let opening = explorer::explorer(&state, explorer::ExplorerQuery::default()).await.unwrap();
    assert_eq!(opening["white"], 10);
    let mut locks = Vec::new();
    let mut kinds = Vec::new();
    while let Ok(e) = events.try_recv() {
        if e.kind == "lichess.me.fair_play" {
            locks.push(e.data["locked"].as_bool().unwrap());
        }
        kinds.push(e.kind);
    }
    assert_eq!(locks, [true, false]);
    for kind in ["match.started", "match.move", "match.offer", "match.ended", "lichess.me.status"] {
        assert!(kinds.iter().any(|k| k == kind), "no {kind} in {kinds:?}");
    }
    let mine = matches::list(&state, 10, Some(Source::Lichess), Some(LichessAccount::Me), None).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert!(matches::list(&state, 10, None, Some(LichessAccount::Bot), None).await.unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&state.config.data);
}

#[tokio::test]
async fn importing_twice_stores_each_game_once_and_carries_on_from_the_newest() {
    let (url, mock) = serve().await;
    let state = runner(url);
    log_in(&state, &mock).await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let log = |_: &str| {};
    let first = import::run(&state, &cancel, &log, &import::ImportRequest::default()).await.unwrap();
    assert_eq!((first["imported"].as_u64(), first["skipped"].as_u64()), (Some(2), Some(0)));
    // The next run asks only for newer games, and finds none.
    let second = import::run(&state, &cancel, &log, &import::ImportRequest::default()).await.unwrap();
    assert_eq!(second["imported"], 0);
    assert_eq!(mock.exports.lock().unwrap().clone(), [None, Some(1_700_000_100_001)]);
    // Asking again from the start finds both, already stored.
    let again = import::ImportRequest { max: None, since: Some(0) };
    let third = import::run(&state, &cancel, &log, &again).await.unwrap();
    assert_eq!((third["imported"].as_u64(), third["skipped"].as_u64()), (Some(0), Some(2)));

    let list = matches::list(&state, 10, None, Some(LichessAccount::Me), None).await.unwrap();
    assert_eq!(list.len(), 2);
    let m = matches::get(&state, list[0].id).await.unwrap().unwrap();
    let l = m.lichess.as_ref().unwrap();
    assert_eq!((l.opening.as_deref(), l.our_side), (Some("C20 King's Pawn Game"), Side::White));
    assert_eq!(
        (m.result, m.reason),
        (crate::matches::rules::GameResult::White, Some(crate::matches::rules::Reason::Checkmate))
    );
    assert!(m.white.human);
    assert_eq!(m.moves.len(), 7);
    assert_eq!(m.moves[6].san, "Qxf7#");
    // Lichess's evaluations are from White's side; a move's is the mover's.
    assert_eq!(m.moves[1].eval, Some(crate::engine::uci::Score::Cp(-35)));
    assert_eq!(m.moves[5].eval, Some(crate::engine::uci::Score::Mate(-1)));
    assert_eq!(m.moves[2].clock_white_ms, Some(178_000));
    assert!(m.created_at.starts_with("2023-11-14T22:1"), "{}", m.created_at);
    let _ = std::fs::remove_dir_all(&state.config.data);
}

#[tokio::test]
async fn games_found_must_be_ones_the_board_api_plays() {
    let (url, mock) = serve().await;
    let state = runner(url);
    log_in(&state, &mock).await;
    until("the event stream", || state.me.status().state == BotState::Online).await;
    let seek = |clock_seconds, increment| MeSeekRequest {
        clock_seconds,
        increment,
        rated: false,
        color: ChallengeColor::Random,
        rating_range: None,
    };
    // Blitz cannot be sought, only challenged.
    assert_eq!(super::seek(&state, seek(180, 2)).await.unwrap_err().status, StatusCode::BAD_REQUEST);
    let challenge = |clock_seconds, rated| MeChallengeRequest {
        username: None,
        ai_level: Some(3),
        my_bot: false,
        clock_seconds,
        increment: 0,
        rated,
        color: ChallengeColor::White,
    };
    assert_eq!(super::challenge(&state, challenge(60, false)).await.unwrap_err().status, StatusCode::BAD_REQUEST);
    assert_eq!(super::challenge(&state, challenge(300, true)).await.unwrap_err().status, StatusCode::BAD_REQUEST);
    let mut neither = challenge(300, false);
    neither.ai_level = None;
    assert_eq!(super::challenge(&state, neither).await.unwrap_err().status, StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_dir_all(&state.config.data);
}
