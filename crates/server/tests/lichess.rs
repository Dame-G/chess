//! The Lichess connections over HTTP, against the mock: a token stored, your game mirrored onto the board, fair play
//! refusing analysis while it runs, logging in, the leaderboard and imports.

mod common;

use common::*;
use dameless_lichess::testing::{GameSetup, MockAccount, Reply, Seat};
use serde_json::{Value, json};

#[tokio::test]
async fn your_lichess_game_turns_analysis_off() {
    let t = TestApp::start().await;
    t.mock.add_account("you-tok", MockAccount::user("Dame").rating("rapid", 1800));
    let (status, c) = t.get("/api/connections").await;
    assert_eq!(status, 200);
    assert_eq!(c["you"]["configured"], false);

    let (status, bad) = t.put("/api/connections/you/token", json!({ "token": "nope" })).await;
    assert_eq!(status, 400, "{bad}");
    let (status, you) = t.put("/api/connections/you/token", json!({ "token": "you-tok" })).await;
    assert_eq!(status, 200, "{you}");
    assert_eq!(you["account"]["username"], "Dame");
    t.until("the connection", "/api/connections", |c| c["you"]["state"] == "online").await;

    // The explorer works while nothing is live.
    let (status, _) = t.get("/api/explorer?db=masters").await;
    assert_eq!(status, 200);
    let (status, _) = t.get("/api/tablebase?fen=8/8/8/8/8/8/6k1/4K2R%20w%20K%20-%200%201").await;
    assert_eq!(status, 200);

    let lid = t.mock.start_game(
        GameSetup::new(Seat::account("dame"), Seat::user("Alice", 1700))
            .clock(600, 5)
            .rated(true)
            .reply(Reply::FirstLegal),
    );
    let live = t
        .until("the game to be mirrored", "/api/games?status=live,starting", |g| {
            g.as_array().unwrap().iter().any(|g| g["source"]["id"] == lid.as_str())
        })
        .await;
    let game = live.as_array().unwrap().iter().find(|g| g["source"]["id"] == lid.as_str()).unwrap().clone();
    assert_eq!(game["white"]["player"], "you");
    assert_eq!(game["black"]["player"], "lichess:alice");
    let (_, s) = t.get("/api/status").await;
    assert_eq!(s["policy"]["assist"], "off");

    let (status, e) = t.post("/api/eval", json!({ "client": "tab" })).await;
    assert_eq!((status, e["error"]["code"].as_str()), (409, Some("fair_play")));
    let (status, e) = t.get("/api/explorer").await;
    assert_eq!((status, e["error"]["code"].as_str()), (409, Some("fair_play")));
    let (status, _) = t.get("/api/tablebase?fen=8/8/8/8/8/8/6k1/4K2R%20w%20K%20-%200%201").await;
    assert_eq!(status, 409);
    let (status, _) = t.post("/api/players/lichess:alice/import", Value::Null).await;
    assert_eq!(status, 409, "another player's history is analysis help");
    let (status, _) = t.get("/api/players/you/repertoire?side=white").await;
    assert_eq!(status, 409);

    // Your move goes to Lichess through the same action route.
    let id = game["id"].as_i64().unwrap();
    let (status, g) = t.post(&format!("/api/games/{id}/actions/move"), json!({ "uci": "e2e4" })).await;
    assert_eq!(status, 200, "{g}");
    let (status, g) = t.post(&format!("/api/games/{id}/stop"), json!({})).await;
    assert_eq!(status, 200, "{g}");
    t.until("analysis back on", "/api/status", |s| s["policy"]["assist"] == "allowed").await;

    let (status, out) = t.delete("/api/connections/you").await;
    assert_eq!((status, out["configured"].as_bool()), (200, Some(false)));
    t.stop().await;
}

#[tokio::test]
async fn logging_in_starts_at_lichess_and_comes_back() {
    let t = TestApp::start().await;
    let (status, login) = t.post("/api/connections/you/login", json!({})).await;
    assert_eq!(status, 200, "{login}");
    let url = login["url"].as_str().unwrap();
    assert!(url.starts_with(t.mock.url()), "{url}");
    let port = t.url.rsplit(':').next().unwrap();
    let callback = format!("127.0.0.1%3A{port}%2Fapi%2Fconnections%2Fyou%2Fcallback");
    assert!(url.contains(&callback), "the redirect is this app's callback: {url}");
    // Nothing logs in without a body too.
    let (status, _) = t.send(t.req(reqwest::Method::POST, "/api/connections/you/login")).await;
    assert_eq!(status, 200);
    let (status, _) = t.post("/api/connections/you/login", json!({ "return_to": "https://evil.example/" })).await;
    assert_eq!(status, 400);
    t.stop().await;
}

#[tokio::test]
async fn public_reads_and_imports() {
    let t = TestApp::start().await;
    assert_eq!(t.put("/api/connections/bot/token", json!({ "token": "x" })).await.0, 400, "there is no bot");
    let (status, top) = t.get("/api/players/top?speed=blitz&nb=3").await;
    assert_eq!(status, 200, "{top}");
    assert_eq!(t.get("/api/players/top?speed=fast").await.0, 400);
    let (status, w) = t.post("/api/players/lichess:alice/import", json!({ "max": 5 })).await;
    assert_eq!(status, 202, "{w}");
    assert_eq!(w["kind"], "import");
    assert_eq!(t.post("/api/players/engine:fake/import", json!({})).await.0, 400);
    t.stop().await;
}
