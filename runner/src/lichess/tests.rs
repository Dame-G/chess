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
    /// Sends no challenges of its own on the event stream.
    quiet: bool,
    /// What `/api/bot/online` lists.
    bots: Vec<Value>,
    /// How the bots answer our challenges, in turn; once these run out, they ignore them.
    answers: Mutex<std::collections::VecDeque<Answer>>,
    /// Every challenge of ours, as `user initial+increment rated=...`.
    challenges: Mutex<Vec<String>>,
}

#[derive(Clone, Copy)]
enum Answer {
    Decline(&'static str),
    Accept,
    Ignore,
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
    axum::Json(json!({ "id": "recklessbot", "username": "RecklessBot", "title": "BOT",
        "perfs": { "blitz": { "rating": 2000, "prov": false }, "bullet": { "rating": 2400, "prov": true } } }))
}

async fn event_stream(State(m): State<Arc<Mock>>) -> Body {
    let (tx, rx) = mpsc::unbounded_channel();
    if !m.quiet {
        tx.send(challenge("c1", "standard")).unwrap();
        tx.send(challenge("c2", "chess960")).unwrap();
    }
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
    let (tx, rx) = mpsc::unbounded_channel();
    let moves = m.moves.lock().unwrap().clone();
    tx.send(json!({
        "type": "gameFull", "id": id, "rated": true, "speed": "blitz", "variant": { "key": "standard" },
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

async fn play_move(State(m): State<Arc<Mock>>, Path((_id, uci)): Path<(String, String)>) -> axum::Json<Value> {
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

fn online_bot(name: &str, blitz: Option<u32>) -> Value {
    let perfs = match blitz {
        Some(r) => json!({ "blitz": { "rating": r, "games": 500 }, "bullet": { "rating": 1800, "games": 50 } }),
        None => json!({ "bullet": { "rating": 1800, "games": 50 } }),
    };
    json!({ "id": name.to_lowercase(), "username": name, "title": "BOT", "perfs": perfs })
}

async fn online(State(m): State<Arc<Mock>>) -> String {
    m.bots.iter().map(|b| format!("{b}\n")).collect()
}

/// A challenge of ours. The answer goes out on the event stream before the reply, as it can on Lichess.
async fn send_challenge(
    State(m): State<Arc<Mock>>,
    Path(user): Path<String>,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> axum::Json<Value> {
    let n = {
        let mut c = m.challenges.lock().unwrap();
        c.push(format!("{user} {}+{} rated={}", form["clock.limit"], form["clock.increment"], form["rated"]));
        c.len()
    };
    let id = format!("s{n}");
    let answer = m.answers.lock().unwrap().pop_front().unwrap_or(Answer::Ignore);
    let tx = m.events.lock().unwrap().clone().unwrap();
    let ours = json!({ "id": "recklessbot", "name": "RecklessBot", "title": "BOT" });
    match answer {
        Answer::Decline(reason) => tx
            .send(json!({ "type": "challengeDeclined", "challenge": {
                "id": id, "challenger": ours, "destUser": { "id": user.to_lowercase(), "name": user },
                "declineReason": "Not now", "declineReasonKey": reason } }))
            .unwrap(),
        Answer::Accept => {
            tx.send(json!({ "type": "gameStart", "game": { "gameId": id, "color": "white" } })).unwrap();
        }
        Answer::Ignore => {}
    }
    axum::Json(json!({ "id": id, "url": format!("https://lichess.org/{id}"), "status": "created" }))
}

async fn cancel_challenge(State(m): State<Arc<Mock>>, Path(id): Path<String>) -> axum::Json<Value> {
    m.posts.lock().unwrap().push(format!("cancel {id}"));
    let tx = m.events.lock().unwrap().clone().unwrap();
    let ours = json!({ "id": "recklessbot", "name": "RecklessBot", "title": "BOT" });
    tx.send(json!({ "type": "challengeCanceled", "challenge": { "id": id, "challenger": ours } })).unwrap();
    axum::Json(json!({ "ok": true }))
}

async fn mock(mode: Mode) -> (String, Arc<Mock>) {
    serve(Mock { mode, ..Mock::default() }).await
}

async fn serve(m: Mock) -> (String, Arc<Mock>) {
    let m = Arc::new(m);
    let app = Router::new()
        .route("/api/bot/online", get(online))
        .route("/api/challenge/{user}", post(send_challenge))
        .route("/api/challenge/{id}/cancel", post(cancel_challenge))
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
    let game = finished(&state, "g1").await;
    (state, mock, game)
}

async fn finished(state: &AppState, id: &str) -> crate::matches::Match {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(m) = matches::find_lichess(state, id).await.unwrap()
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
    let game = finished(&state, "g1").await;

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

// ---------------------------------------------------------------------------------------------------------------------
// Looking for games
// ---------------------------------------------------------------------------------------------------------------------

/// The bot online against a quiet mock whose bots answer our challenges with `answers`, seeking at 3+2 with short
/// pauses; `answer` is how long a challenge may go unanswered.
async fn seeker(answers: &[Answer], answer: Duration) -> (Arc<AppState>, Arc<Mock>) {
    let bots = vec![
        online_bot("RecklessBot", Some(2000)),
        online_bot("Alpha", Some(1950)),
        online_bot("Beta", Some(2080)),
        online_bot("Gamma", None),
    ];
    let m = Mock { quiet: true, bots, answers: Mutex::new(answers.iter().copied().collect()), ..Mock::default() };
    let (url, mock) = serve(m).await;
    let state = runner(url, false).await;
    state.lichess.inner.lock().unwrap().seek.timing = seek::Timing {
        polite: Duration::from_millis(50),
        answer,
        rate_limited: Duration::from_millis(200),
        retry: Duration::from_millis(200),
    };
    set_token(&state, "lip_test_token").await.unwrap();
    let settings = Settings { build: Some("main".into()), seek_clocks: vec!["3+2".into()], ..Settings::default() };
    put_settings(&state, settings).await.unwrap();
    assert!(seek::start(&state, SeekMode::Once).await.is_err(), "not while offline");
    start(&state).await.unwrap();
    (state, mock)
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

fn challenged(mock: &Mock) -> Vec<String> {
    mock.challenges.lock().unwrap().clone()
}

fn target(challenge: &str) -> &str {
    challenge.split(' ').next().unwrap()
}

#[tokio::test]
async fn seeking_once_tries_another_bot_after_a_decline_and_stops_at_the_game() {
    let (state, mock) = seeker(&[Answer::Decline("tooFast"), Answer::Accept], Duration::from_secs(10)).await;
    let (_, mut events) = state.events.subscribe(0);
    let status = seek::start(&state, SeekMode::Once).await.unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(status.seek.mode, SeekMode::Once);

    let game = finished(&state, "s2").await;
    let sent = challenged(&mock);
    assert_eq!(sent.len(), 2, "{sent:?}");
    // Never ourselves nor the bot without a blitz rating, and not the one that declined again.
    let targets: Vec<&str> = sent.iter().map(|c| target(c)).collect();
    assert!(targets.iter().all(|t| ["Alpha", "Beta"].contains(t)), "{targets:?}");
    assert_ne!(targets[0], targets[1]);
    assert!(sent.iter().all(|c| c.ends_with(" 180+2 rated=true")), "{sent:?}");
    assert_eq!(game.lichess.as_ref().unwrap().id, "s2");

    let seek = state.lichess.status().seek;
    assert_eq!(seek.mode, SeekMode::Off);
    assert!(seek.pending.is_none());
    assert!(seek.note.as_deref().unwrap().contains("accepted"), "{:?}", seek.note);
    // Nothing more is sent after the game.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(challenged(&mock).len(), 2);

    let mut notes = Vec::new();
    while let Ok(e) = events.try_recv() {
        if e.kind == "lichess.status"
            && let Some(note) = e.data["seek"]["note"].as_str()
        {
            notes.push(note.to_owned());
        }
    }
    let declined = format!("{} declined (too fast). Trying another bot.", targets[0]);
    assert!(notes.contains(&declined), "{notes:?}");
    stop(&state).await.unwrap();
    let _ = std::fs::remove_dir_all(&state.config.data);
}

#[tokio::test]
async fn an_unanswered_challenge_is_withdrawn_and_another_bot_tried() {
    let (state, mock) = seeker(&[Answer::Ignore, Answer::Ignore], Duration::from_millis(300)).await;
    seek::start(&state, SeekMode::Once).await.unwrap();
    until("both bots to be tried and given up on", || {
        let posts = mock.posts.lock().unwrap();
        posts.contains(&"cancel s1".to_owned()) && posts.contains(&"cancel s2".to_owned())
    })
    .await;
    let sent = challenged(&mock);
    assert_ne!(target(&sent[0]), target(&sent[1]));
    // Both ignored us, so for an hour there is nobody left to ask.
    until("the search to say there is nobody", || {
        state.lichess.status().seek.note.as_deref()
            == Some("No bots to play at 3+2 are online; trying again in a minute.")
    })
    .await;
    assert_eq!(challenged(&mock).len(), 2);
    assert_eq!(state.lichess.status().seek.mode, SeekMode::Once);
    stop(&state).await.unwrap();
    assert_eq!(state.lichess.status().seek.mode, SeekMode::Off, "stopping the bot stops the search");
    let _ = std::fs::remove_dir_all(&state.config.data);
}

#[tokio::test]
async fn seeking_always_goes_on_after_a_game_until_stopped() {
    let (state, mock) = seeker(&[Answer::Accept, Answer::Ignore], Duration::from_secs(30)).await;
    seek::start(&state, SeekMode::Always).await.unwrap();
    assert_eq!(meta_get(&state, seek::SEEK_KEY).await.unwrap().as_deref(), Some("always"));

    finished(&state, "s1").await;
    until("a challenge after the game", || state.lichess.status().seek.pending.is_some_and(|p| p.id == "s2")).await;
    let status = state.lichess.status();
    assert_eq!(status.seek.mode, SeekMode::Always);
    let pending = status.seek.pending.unwrap();
    assert_eq!((pending.clock.initial, pending.clock.increment, pending.rated), (180, 2, true));
    assert_eq!(pending.url, "https://lichess.org/s2");

    // Stopping the search withdraws the challenge waiting for an answer.
    let status = seek::stop(&state).await.unwrap();
    assert_eq!(status.seek.mode, SeekMode::Off);
    assert!(status.seek.pending.is_none());
    assert!(mock.posts.lock().unwrap().contains(&"cancel s2".to_owned()));
    assert_eq!(meta_get(&state, seek::SEEK_KEY).await.unwrap().as_deref(), Some("off"));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(challenged(&mock).len(), 2);
    stop(&state).await.unwrap();
    let _ = std::fs::remove_dir_all(&state.config.data);
}
