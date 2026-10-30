//! A mock Lichess to point a running app at, for walking the Lichess flows by hand: `LICHESS_URL=http://127.0.0.1:9797`.
//!
//! ```text
//! cargo run -p chess-lichess --features mock --example mock_server [-- <port>]
//! ```
//!
//! It knows you, `Dame`, by a (fake) token, `lip_mock_you`. The OAuth page grants every login as `Dame` at once.
//! Opponents in games from challenges and seeks answer with their first legal move and take draw offers and takebacks.
//! Bots Alpha, Beta, Gamma and Delta accept challenges, Dame has a few games to import, and the explorer, the tablebase
//! and a leaderboard answer.
//!
//! The control API, on `<port> + 1`, scripts the rest (JSON bodies; seats are `you`, `user:<name>:<rating>` or
//! `ai:<level>`, sides `white` or `black`):
//!
//! ```text
//! GET  /state                                     games, open challenges and seeks
//! POST /games         { white, black, clock?: [180, 2], days?, rated?, reply?: true, accepts_offers? }  → { id }
//! POST /games/{id}/move           { uci }         the side to move plays it (the scripted opponent)
//! POST /games/{id}/offer-draw     { side }        and offer-takeback, accept-draw, accept-takeback
//! POST /games/{id}/gone           { side, seconds }   the side leaves; /back brings it back
//! POST /games/{id}/end            { status: "resign", winner?: "white" }
//! POST /seeks/{id}/take           { opponent }    someone takes your seek
//! POST /challenges                { from, to: "dame", clock: [300, 3], rated?, color? }  → { id }
//! POST /challenges/{id}/accept    and /decline { reason? }, /cancel
//! POST /answers                   { user, answer: "accept" | "ignore" | <decline reason> }   how a user answers ours
//! POST /reply                     { reply: true, accepts_offers: true }   how opponents in new games play
//! ```

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chess_lichess::{
    Color, GameStatus, Side,
    challenge::{ChallengeRequest, DeclineReason},
    testing::{ChallengeAnswer, GameSetup, MockAccount, MockLichess, Reply, Seat, exported_game},
};
use serde_json::{Value, json};

const YOU_TOKEN: &str = "lip_mock_you";

type Mock = Arc<MockLichess>;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(9797);
    let mock = MockLichess::start_on(&format!("127.0.0.1:{port}")).await?;
    script(&mock);
    let mock: Mock = Arc::new(mock);
    let control = tokio::net::TcpListener::bind(("127.0.0.1", port + 1)).await?;
    println!("mock Lichess at {} (LICHESS_URL); control API at http://127.0.0.1:{}", mock.url(), port + 1);
    println!("token: you {YOU_TOKEN} (Dame); logins are granted as Dame");
    axum::serve(control, router(mock)).await
}

/// The accounts and the world they live in.
fn script(mock: &MockLichess) {
    mock.add_account(
        YOU_TOKEN,
        MockAccount::user("Dame").rating("blitz", 1650).rating("rapid", 1720).rating("classical", 1780).played(),
    );
    mock.grant_logins_as(YOU_TOKEN);
    mock.reply_by_default(Reply::FirstLegal, true);
    for name in ["Alpha", "Beta", "Gamma", "Delta"] {
        mock.answer_challenges(name, ChallengeAnswer::Accept);
    }
    let day = 86_400_000;
    let now = chess_lichess::now_ms();
    mock.set_games_export(
        "dame",
        (0..6)
            .map(|i| {
                let (w, b) = if i % 2 == 0 { ("Dame", "Alice") } else { ("Bob", "Dame") };
                exported_game(&format!("ex{i:06}"), now - (i + 1) * day, w, b)
            })
            .collect(),
    );
    mock.set_user(json!({ "id": "alice", "username": "Alice", "perfs": { "blitz": { "rating": 1790, "games": 400 } },
        "count": { "all": 410, "rated": 400, "win": 200, "draw": 20, "loss": 190 }, "createdAt": now - 400 * day,
        "seenAt": now, "url": "https://lichess.org/@/Alice" }));
    mock.set_leaderboard(
        "blitz",
        ["Magnus", "Hikaru", "Alireza"]
            .iter()
            .enumerate()
            .map(|(i, n)| {
                json!({ "id": n.to_lowercase(), "username": n, "title": "GM",
                "perfs": { "blitz": { "rating": 3200 - 40 * i as u32, "progress": 5 } } })
            })
            .collect(),
    );
    let explorer = json!({ "white": 1200, "draws": 300, "black": 900, "opening": { "eco": "C20", "name": "King's Pawn Game" },
        "moves": [
            { "uci": "e2e4", "san": "e4", "white": 600, "draws": 150, "black": 450, "averageRating": 1900 },
            { "uci": "d2d4", "san": "d4", "white": 500, "draws": 120, "black": 380, "averageRating": 1950 },
            { "uci": "g1f3", "san": "Nf3", "white": 100, "draws": 30, "black": 70, "averageRating": 2000 }
        ], "topGames": [], "recentGames": [] });
    for db in ["lichess", "masters", "player"] {
        mock.set_explorer(db, vec![explorer.clone()]);
    }
    mock.set_tablebase(json!({ "category": "win", "dtz": 1, "dtm": 1, "checkmate": false, "stalemate": false,
        "insufficient_material": false, "moves": [] }));
}

fn router(mock: Mock) -> Router {
    Router::new()
        .route("/state", get(state))
        .route("/games", post(start_game))
        .route("/games/{id}/{action}", post(game_action))
        .route("/seeks/{id}/take", post(take_seek))
        .route("/challenges", post(challenge))
        .route("/challenges/{id}/{action}", post(challenge_action))
        .route("/answers", post(answers))
        .route("/reply", post(reply))
        .with_state(mock)
}

fn bad(why: impl std::fmt::Display) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": why.to_string() }))).into_response()
}

fn seat(v: &Value) -> Result<Seat, String> {
    let s = v.as_str().ok_or("a seat is a string")?;
    match s.split(':').collect::<Vec<_>>().as_slice() {
        ["you"] => Ok(Seat::account("dame")),
        ["user", name, rating] => Ok(Seat::user(name, rating.parse().map_err(|_| "a rating is a number")?)),
        ["ai", level] => Ok(Seat::ai(level.parse().map_err(|_| "a level is a number")?)),
        _ => Err(format!("unknown seat {s}: you, user:<name>:<rating> or ai:<level>")),
    }
}

fn side(v: &Value) -> Result<Side, String> {
    serde_json::from_value(v.clone()).map_err(|_| "side is white or black".to_owned())
}

async fn state(State(mock): State<Mock>) -> Json<Value> {
    let ids: Vec<String> = mock.ongoing_games("dame");
    let games: Vec<Value> = ids
        .iter()
        .filter_map(|id| mock.game(id))
        .map(|g| {
            json!({ "id": g.id, "white": format!("{:?}", g.white), "black": format!("{:?}", g.black),
            "moves": g.moves, "draw_offer": g.draw_offer, "takeback_offer": g.takeback_offer })
        })
        .collect();
    let seeks: Vec<Value> =
        mock.seeks().into_iter().filter(|s| s.open).map(|s| json!({ "id": s.id, "form": s.form })).collect();
    Json(json!({ "games": games, "challenges": mock.challenges(), "seeks": seeks }))
}

async fn start_game(State(mock): State<Mock>, Json(b): Json<Value>) -> Response {
    let (white, black) = match (seat(&b["white"]), seat(&b["black"])) {
        (Ok(w), Ok(b)) => (w, b),
        (Err(e), _) | (_, Err(e)) => return bad(e),
    };
    let mut setup = GameSetup::new(white, black).rated(b["rated"].as_bool().unwrap_or(false));
    if let Some(days) = b["days"].as_u64() {
        setup = setup.correspondence(days as u32);
    } else if let Some([l, i]) = b["clock"].as_array().map(Vec::as_slice) {
        setup = setup.clock(l.as_u64().unwrap_or(180) as u32, i.as_u64().unwrap_or(2) as u32);
    }
    if b["reply"].as_bool().unwrap_or(false) {
        setup = setup.reply(Reply::FirstLegal);
    }
    if b["accepts_offers"].as_bool().unwrap_or(false) {
        setup = setup.accepts_offers();
    }
    Json(json!({ "id": mock.start_game(setup) })).into_response()
}

async fn game_action(State(mock): State<Mock>, Path((id, action)): Path<(String, String)>, body: String) -> Response {
    let b: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    if mock.game(&id).is_none() {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no such game" }))).into_response();
    }
    let the_side = || side(&b["side"]);
    match action.as_str() {
        "move" => {
            if let Err(e) = mock.try_move(&id, b["uci"].as_str().unwrap_or_default()) {
                return bad(e);
            }
        }
        "offer-draw" => match the_side() {
            Ok(s) => mock.offer_draw(&id, s),
            Err(e) => return bad(e),
        },
        "offer-takeback" => match the_side() {
            Ok(s) => mock.offer_takeback(&id, s),
            Err(e) => return bad(e),
        },
        "accept-draw" => mock.accept_draw(&id),
        "accept-takeback" => mock.accept_takeback(&id),
        "gone" => match the_side() {
            Ok(s) => mock.opponent_gone(&id, s, b["seconds"].as_u64().unwrap_or(0)),
            Err(e) => return bad(e),
        },
        "back" => mock.opponent_back(&id),
        "end" => {
            let Ok(status) = serde_json::from_value::<GameStatus>(b["status"].clone()) else {
                return bad("status is a Lichess status: resign, mate, outoftime, draw, aborted, …");
            };
            let winner = b.get("winner").and_then(|w| side(w).ok());
            mock.end_game(&id, status, winner);
        }
        _ => return bad(format!("unknown action {action}")),
    }
    Json(json!({ "ok": true })).into_response()
}

async fn take_seek(State(mock): State<Mock>, Path(id): Path<String>, Json(b): Json<Value>) -> Response {
    if !mock.seeks().iter().any(|s| s.id == id && s.open) {
        return bad("no open seek with that id");
    }
    match seat(b.get("opponent").unwrap_or(&json!("user:Alice:1790"))) {
        Ok(opponent) => Json(json!({ "id": mock.take_seek(&id, opponent) })).into_response(),
        Err(e) => bad(e),
    }
}

async fn challenge(State(mock): State<Mock>, Json(b): Json<Value>) -> Response {
    let from = match seat(b.get("from").unwrap_or(&json!("user:Alice:1790"))) {
        Ok(s) => s,
        Err(e) => return bad(e),
    };
    let to = b["to"].as_str().unwrap_or("dame");
    let (l, i) = match b["clock"].as_array().map(Vec::as_slice) {
        Some([l, i]) => (l.as_u64().unwrap_or(300) as u32, i.as_u64().unwrap_or(3) as u32),
        _ => (300, 3),
    };
    let color = serde_json::from_value::<Color>(b["color"].clone()).unwrap_or_default();
    let req = ChallengeRequest::clock(b["rated"].as_bool().unwrap_or(false), l, i).color(color);
    Json(json!({ "id": mock.send_challenge(from, to, &req) })).into_response()
}

async fn challenge_action(
    State(mock): State<Mock>,
    Path((id, action)): Path<(String, String)>,
    body: String,
) -> Response {
    let b: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let done = match action.as_str() {
        "accept" => mock.accept_challenge(&id).is_some(),
        "decline" => {
            let reason = serde_json::from_value(b["reason"].clone()).unwrap_or(DeclineReason::Generic);
            mock.decline_challenge(&id, reason)
        }
        "cancel" => mock.cancel_challenge(&id),
        _ => return bad(format!("unknown action {action}")),
    };
    if done { Json(json!({ "ok": true })).into_response() } else { bad("no open challenge with that id") }
}

async fn answers(State(mock): State<Mock>, Json(b): Json<Value>) -> Response {
    let user = b["user"].as_str().unwrap_or_default();
    let answer = match b["answer"].as_str().unwrap_or("accept") {
        "accept" => ChallengeAnswer::Accept,
        "ignore" => ChallengeAnswer::Ignore,
        reason => match serde_json::from_value(json!(reason)) {
            Ok(r) => ChallengeAnswer::Decline(r),
            Err(_) => return bad("answer is accept, ignore or a decline reason (later, tooFast, …)"),
        },
    };
    mock.answer_challenges(user, answer);
    Json(json!({ "ok": true })).into_response()
}

async fn reply(State(mock): State<Mock>, Json(b): Json<Value>) -> Json<Value> {
    let reply = if b["reply"].as_bool().unwrap_or(true) { Reply::FirstLegal } else { Reply::None };
    mock.reply_by_default(reply, b["accepts_offers"].as_bool().unwrap_or(false));
    Json(json!({ "ok": true }))
}
