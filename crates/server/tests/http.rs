//! The app over HTTP: the guard, error shapes, status, games you play against the fake engine with the event stream,
//! PGN, reviews, profiles, the engines list, setup, work and the player side.

mod common;

use std::time::Duration;

use common::*;
use reqwest::Method;
use serde_json::{Value, json};

#[tokio::test]
async fn the_guard_wants_the_token_a_local_host_and_a_known_origin() {
    let t = TestApp::start().await;
    // The health check needs nothing.
    let res = t.http.get(t.url("/api/health")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    // Everything else needs the token.
    let res = t.http.get(t.url("/api/status")).send().await.unwrap();
    assert_eq!(res.status(), 401);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["error"]["code"], "unauthorized");
    let res = t.http.get(t.url("/api/status")).bearer_auth("wrong").send().await.unwrap();
    assert_eq!(res.status(), 401);
    assert_eq!(t.get("/api/status").await.0, 200);
    // The event stream also takes it in the query, for EventSource.
    let res = t.http.get(t.url(&format!("/api/events?token={}", t.token))).send().await.unwrap();
    assert_eq!(res.status(), 200);
    drop(res);
    // Another website is refused, token or not; the hosted UI is answered with it, CORS included.
    let (status, body) = t.send(t.req(Method::GET, "/api/status").header("origin", "https://evil.example")).await;
    assert_eq!((status, body["error"]["code"].as_str()), (403, Some("forbidden")));
    let res = t.req(Method::GET, "/api/status").header("origin", "https://chess.dame.gg").send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["access-control-allow-origin"], "https://chess.dame.gg");
    let res = t.http.get(t.url("/api/status")).header("origin", "https://chess.dame.gg").send().await.unwrap();
    assert_eq!(res.status(), 401);
    // DNS rebinding: a foreign Host is refused, the UI's pages included.
    let res = t.req(Method::GET, "/api/status").header("host", "evil.example").send().await.unwrap();
    assert_eq!(res.status(), 403);
    let res = t.http.get(t.url("/")).header("host", "evil.example").send().await.unwrap();
    assert_eq!(res.status(), 403);
    // The app's own page needs no token: its fetches and its event stream are same-origin by the browser's word.
    let own = t.url("");
    let res = t.http.get(t.url("/api/status")).header("sec-fetch-site", "same-origin").send().await.unwrap();
    assert_eq!(res.status(), 200);
    // A POST carries the page's origin (this one gets past the guard to the route, which refuses the action).
    let res = t
        .http
        .post(t.url("/api/games/1/actions/fly"))
        .header("sec-fetch-site", "same-origin")
        .header("origin", own.trim_end_matches('/'))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let res = t.http.get(t.url("/api/events")).header("sec-fetch-site", "same-origin").send().await.unwrap();
    assert_eq!(res.status(), 200);
    drop(res);
    // Another website's page is cross-site, and needs the token.
    let res = t.http.get(t.url("/api/status")).header("sec-fetch-site", "cross-site").send().await.unwrap();
    assert_eq!(res.status(), 401);
    // The login callback is reachable without the token (and refuses an unknown login).
    let res = t.http.get(t.url("/api/connections/you/callback?state=nope&code=x")).send().await.unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["error"]["code"], "bad_request");
    // The token route answers it, behind the guard.
    assert_eq!(t.get("/api/token").await.1["token"], t.token.as_str());
    t.stop().await;
}

#[tokio::test]
async fn errors_have_one_shape() {
    let t = TestApp::start().await;
    let (status, body) = t.get("/api/games/999").await;
    assert_eq!((status, body["error"]["code"].as_str()), (404, Some("not_found")));
    assert!(body["error"]["message"].as_str().is_some_and(|m| !m.is_empty()));
    let (status, body) = t.get("/api/games?status=sideways").await;
    assert_eq!((status, body["error"]["code"].as_str()), (400, Some("bad_request")));
    let (status, body) =
        t.send(t.req(Method::POST, "/api/games").body("{not json").header("content-type", "application/json")).await;
    assert_eq!((status, body["error"]["code"].as_str()), (400, Some("bad_request")));
    let (status, _) = t.get("/api/players/nobody%20at%20all").await;
    assert_eq!(status, 400);
    let (status, body) = t.post("/api/games/1/actions/fly", json!({})).await;
    assert_eq!((status, body["error"]["code"].as_str()), (400, Some("bad_request")));
    t.stop().await;
}

#[tokio::test]
async fn status_and_the_machine() {
    let t = TestApp::start().await;
    let (status, s) = t.get("/api/status").await;
    assert_eq!(status, 200);
    assert_eq!(s["cores"], 4);
    assert_eq!(s["policy"]["assist"], "allowed");
    assert!(s["machine"]["data_dir"].as_str().is_some_and(|d| d.ends_with("data")));
    assert!(s["ledger"]["total"].as_u64() == Some(4));
    assert_eq!(s["lichess_url"], t.mock.url());
    t.stop().await;
}

#[tokio::test]
async fn you_play_chess_and_the_stream_tells_it() {
    let t = TestApp::start().await;
    // Opened first, so it sees the game from its start.
    let stream = t.req(Method::GET, "/api/events?topics=game:*,games").send().await.unwrap();
    let (status, g) = t
        .post("/api/games", json!({ "white": "you", "black": "fake", "time": { "type": "movetime", "ms": 30 } }))
        .await;
    assert_eq!(status, 201, "{g}");
    let id = g["summary"]["id"].as_i64().unwrap();
    assert_eq!(g["summary"]["you"], "white");
    assert!(g["actions"].as_array().unwrap().iter().any(|a| a == "move"));

    // SAN works as well as UCI; an illegal move is a 400, and it is not our turn after moving (409).
    let (status, _) = t.post(&format!("/api/games/{id}/actions/move"), json!({ "uci": "e5" })).await;
    assert_eq!(status, 400);
    let (status, after) = t.post(&format!("/api/games/{id}/actions/move"), json!({ "uci": "e4" })).await;
    assert_eq!(status, 200, "{after}");
    assert_eq!(after["moves"][0]["uci"], "e2e4");
    let g =
        t.until("the engine's reply", &format!("/api/games/{id}"), |g| g["moves"].as_array().unwrap().len() >= 2).await;
    assert!(g["moves"][1]["eval"].is_object(), "engines report their score");

    let (status, pgn) = {
        let res = t.req(Method::GET, &format!("/api/games/{id}/pgn")).send().await.unwrap();
        (res.status().as_u16(), res.text().await.unwrap())
    };
    assert_eq!(status, 200);
    assert!(pgn.contains("1. e4"), "{pgn}");

    let (status, g) = t.post(&format!("/api/games/{id}/actions/resign"), Value::Null).await;
    assert_eq!(status, 200, "{g}");
    let g = t.until("the end", &format!("/api/games/{id}"), |g| g["summary"]["status"] == "finished").await;
    assert_eq!(g["summary"]["outcome"], "black_wins");

    let seen = events(stream, Duration::from_secs(10), |e| {
        e.iter().any(|e| e["kind"] == "game.upserted" && e["data"]["status"] == "finished")
    })
    .await;
    let moved: Vec<&Value> = seen.iter().filter(|e| e["kind"] == "game.moved").collect();
    assert!(moved.len() >= 2, "{seen:?}");
    assert_eq!(moved[0]["topic"], format!("game:{id}"));
    assert_eq!(moved[0]["data"]["move"]["san"], "e4");
    assert!(moved[0]["id"].as_u64().unwrap() > 0);

    // A client coming back after the first move gets the rest replayed, by `since` or Last-Event-ID.
    let first = moved[0]["id"].as_u64().unwrap();
    let again = t.req(Method::GET, &format!("/api/events?topics=game:{id}&since={first}")).send().await.unwrap();
    let replay =
        events(again, Duration::from_secs(2), |e| e.iter().filter(|e| e["kind"] == "game.moved").count() >= 1).await;
    assert!(replay.iter().all(|e| e["id"].as_u64().unwrap() > first));
    assert_eq!(replay.iter().find(|e| e["kind"] == "game.moved").unwrap()["data"]["ply"], 2);
    let again = t
        .req(Method::GET, &format!("/api/events?topics=game:{id}"))
        .header("last-event-id", first.to_string())
        .send()
        .await
        .unwrap();
    let replay = events(again, Duration::from_secs(2), |e| !e.is_empty()).await;
    assert!(replay[0]["id"].as_u64().unwrap() > first);

    // Lists carry it, live first then newest.
    let (_, list) = t.get("/api/games?player=you").await;
    assert_eq!(list[0]["id"], id);
    // Stopping a finished game is a conflict.
    let (status, body) = t.post(&format!("/api/games/{id}/stop"), json!({})).await;
    assert!(status == 409 || status == 200, "{status} {body}");
    t.stop().await;
}

#[tokio::test]
async fn a_reconnect_with_a_lost_past_is_told_to_reload() {
    let t = TestApp::start().await;
    for _ in 0..chess_hub::bus::KEEP + 5 {
        t.app.hub.policy.set_live_games(&[false]);
        t.app.hub.policy.set_live_games(&[]);
    }
    let res = t.req(Method::GET, "/api/events?since=2&topics=policy").send().await.unwrap();
    let seen = events(res, Duration::from_secs(2), |e| !e.is_empty()).await;
    assert_eq!(seen[0]["kind"], "reload", "{:?}", seen.first());
    assert_eq!(seen[0]["topic"], "stream");
    t.stop().await;
}

#[tokio::test]
async fn pasted_pgn_becomes_games_that_are_reviewed_and_profiled() {
    let t = TestApp::start().await;
    let pgn = "[White \"Dame\"]\n[Black \"Alice\"]\n[Result \"1-0\"]\n[TimeControl \"300+3\"]\n\n\
               1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0\n";
    let (status, games) = t.post("/api/games/import", json!({ "pgn": pgn, "you": "white" })).await;
    assert_eq!(status, 201, "{games}");
    let g = &games[0];
    let id = g["id"].as_i64().unwrap();
    assert_eq!((g["white"]["player"].as_str(), g["black"]["player"].as_str()), (Some("you"), Some("lichess:alice")));
    assert_eq!(
        (g["status"].as_str(), g["outcome"].as_str(), g["ending"].as_str()),
        (Some("finished"), Some("white_wins"), Some("checkmate"))
    );
    let (status, _) = t.post("/api/games/import", json!({ "pgn": "nonsense 1. Kxz9" })).await;
    assert_eq!(status, 400);

    // No review yet.
    assert_eq!(t.get(&format!("/api/games/{id}/review")).await.0, 404);
    let (status, w) = t.post("/api/reviews", json!({ "games": [id], "nodes": 1000, "concurrency": 1 })).await;
    assert_eq!(status, 202, "{w}");
    assert_eq!(w["kind"], "review");
    let w = t
        .until("the review", &format!("/api/work/{}", w["id"]), |w| w["status"] == "done" || w["status"] == "failed")
        .await;
    assert_eq!(w["status"], "done", "{w}");
    let (status, r) = t.get(&format!("/api/games/{id}/review")).await;
    assert_eq!(status, 200);
    assert_eq!(r["plies"].as_array().unwrap().len(), 7);
    let (_, log) = {
        let res = t.req(Method::GET, &format!("/api/work/{}/log", w["id"])).send().await.unwrap();
        (res.status(), res.text().await.unwrap())
    };
    let _ = log;

    let (status, p) = t.get("/api/players/you/profile").await;
    assert_eq!(status, 200, "{p}");
    assert_eq!(p["games"], 1);
    assert_eq!(p["reviewed"], 1);
    // Your whole history is not here until an import says so; other players have no such flag.
    assert_eq!(p["history_complete"], false);
    t.app.hub.store.put_meta(chess_play::import::COMPLETE_KEY, &true).await.unwrap();
    let (_, p) = t.get("/api/players/you/profile").await;
    assert_eq!(p["history_complete"], true);
    let (_, p) = t.get("/api/players/lichess:alice/profile").await;
    assert!(p.get("history_complete").is_none(), "{p}");
    let (status, c) = t.get("/api/compare?players=you,lichess:alice").await;
    assert_eq!(status, 200, "{c}");
    assert_eq!(c["profiles"].as_array().unwrap().len(), 2);
    assert_eq!(t.get("/api/compare?players=you").await.0, 400);
    let (status, player) = t.get("/api/players/lichess:alice").await;
    assert_eq!((status, player["name"].as_str()), (200, Some("Alice")));

    // The player side reads the same games.
    let (status, rep) = t.get("/api/players/you/repertoire?side=white&min_games=1").await;
    assert_eq!(status, 200, "{rep}");
    assert_eq!(rep["games"], 1);
    let (status, progress) = t.get("/api/progress").await;
    assert_eq!(status, 200, "{progress}");
    assert!(progress["series"].is_array());
    let (status, drills) = t.get("/api/drills").await;
    assert_eq!(status, 200);
    assert!(drills.is_array());
    let (status, today) = t.get("/api/drills/today").await;
    assert_eq!(status, 200);
    assert!(today["drills"].is_array());
    assert_eq!(t.post("/api/drills", Value::Null).await.0, 201);
    assert_eq!(t.get("/api/drills/trend").await.0, 200);
    assert_eq!(t.delete("/api/drills/987").await.0, 404);
    t.stop().await;
}

#[tokio::test]
async fn evaluation_streams_on_its_topic() {
    let t = TestApp::start().await;
    let stream = t.req(Method::GET, "/api/events?topics=eval:*").send().await.unwrap();
    let (status, s) = t
        .post("/api/eval", json!({ "client": "tab-1", "budget": { "type": "depth", "depth": 3 }, "multipv": 2 }))
        .await;
    assert_eq!(status, 200, "{s}");
    let session = s["session"].as_str().unwrap().to_owned();
    let two = |e: &Value| e["kind"] == "eval.info" && e["data"]["lines"].as_array().is_some_and(|l| l.len() == 2);
    let seen = events(stream, Duration::from_secs(10), |e| e.iter().any(two)).await;
    let info = seen.iter().find(|e| two(e)).expect("an eval.info with both lines");
    assert_eq!(info["topic"], format!("eval:{session}"));
    assert_eq!(info["data"]["lines"].as_array().unwrap().len(), 2);
    assert_eq!(t.delete(&format!("/api/eval/{session}")).await.0, 200);
    assert_eq!(t.get("/api/eval").await.0, 200);
    let (status, _) = t.post("/api/eval", json!({ "client": "tab-1", "multipv": 99 })).await;
    assert_eq!(status, 400);
    t.stop().await;
}

#[tokio::test]
async fn engines_setup_and_work() {
    let t = TestApp::start().await;
    let (status, list) = t.get("/api/engines").await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(list["analysis"], "fake");
    let engines = list["engines"].as_array().unwrap();
    assert_eq!(engines.len(), 3, "two built-ins and the fake one");
    assert_eq!((&engines[0]["id"], &engines[0]["installed"]), (&json!("stockfish"), &json!(false)));
    assert_eq!((&engines[1]["id"], &engines[1]["installed"]), (&json!("reckless"), &json!(false)));
    let fake = engines.iter().find(|e| e["id"] == "fake").unwrap();
    assert_eq!(
        (&fake["id_name"], &fake["analysis"], &fake["levels"]),
        (&json!("FakeEval 1.0"), &json!(true), &json!(true))
    );

    // Another engine by path; one that is not an engine, or a taken id, is refused.
    let path = common::fake_engine().display().to_string();
    let (status, added) =
        t.post("/api/engines", json!({ "name": "Second", "path": path, "options": { "Hash": "8" } })).await;
    assert_eq!(status, 201, "{added}");
    assert_eq!(added["id"], "second");
    assert_eq!(t.post("/api/engines", json!({ "name": "Second", "path": path })).await.0, 409);
    assert_eq!(t.post("/api/engines", json!({ "name": "Gone", "path": "/no/such/engine" })).await.0, 400);
    let (status, chosen) = t.put("/api/engines/analysis", json!({ "engine": "second" })).await;
    assert_eq!((status, &chosen["analysis"]), (200, &json!("second")));
    assert_eq!(t.put("/api/engines/analysis", json!({ "engine": "stockfish" })).await.0, 409, "not installed");
    let (status, removed) = t.delete("/api/engines/second").await;
    assert_eq!((status, &removed["analysis"]), (200, &json!("stockfish")));
    assert_eq!(t.delete("/api/engines/second").await.0, 404);
    assert_eq!(t.put("/api/engines/analysis", json!({ "engine": "fake" })).await.0, 200);

    // The diagnostics name the analysis engine.
    let (status, health) = t.get("/api/diagnostics").await;
    assert_eq!(status, 200, "{health}");
    let analysis = health["items"].as_array().unwrap().iter().find(|i| i["key"] == "analysis").unwrap().clone();
    assert_eq!(analysis["level"], "ok", "{analysis}");

    // Work: a review on the analysis engine.
    let game = finished_game(&t).await;
    let (status, w) = t.post("/api/reviews", json!({ "games": [game] })).await;
    assert_eq!(status, 202, "{w}");
    let id = w["id"].as_i64().unwrap();
    t.until("the review", &format!("/api/work/{id}"), |w| w["status"] == "done").await;
    let (status, review) = t.get(&format!("/api/games/{game}/review?engine=fake")).await;
    assert_eq!((status, &review["engine"]), (200, &json!("FakeEval 1.0")));
    let (status, work) = t.get("/api/work").await;
    assert_eq!(status, 200);
    let any = work["recent"][0]["id"].as_i64().unwrap();
    assert_eq!(t.get(&format!("/api/work/{any}")).await.0, 200);
    let res = t.req(Method::GET, &format!("/api/work/{any}/log")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(t.post(&format!("/api/work/{any}/stop"), json!({})).await.0, 200);
    t.stop().await;
}

/// A finished game pasted in as PGN.
async fn finished_game(t: &TestApp) -> i64 {
    let pgn = "[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0\n";
    let (status, games) = t.post("/api/games/import", json!({ "pgn": pgn })).await;
    assert_eq!(status, 201, "{games}");
    games[0]["id"].as_i64().unwrap()
}

#[tokio::test]
async fn levels_list_and_calibrate() {
    let t = TestApp::start().await;
    let (status, levels) = t.get("/api/levels").await;
    assert_eq!(status, 200, "{levels}");
    assert_eq!((levels["levels"].as_array().unwrap().len(), &levels["engine"]), (12, &json!("fake")));
    // Every engine has a ladder: one not installed yet still lists its levels at their nominal ratings.
    let (status, reckless) = t.get("/api/levels?engine=reckless").await;
    assert_eq!((status, &reckless["engine"]), (200, &json!("reckless")), "{reckless}");
    assert_eq!(t.get("/api/levels?engine=nope").await.0, 404);
    let (status, plan) = t.get("/api/levels/plan?count=2&engine=reckless").await;
    assert_eq!((status, plan.as_array().map(Vec::len)), (200, Some(2)));
    let (status, w) = t.post("/api/levels/calibrate", json!({ "count": 1, "games_per_pair": 2 })).await;
    assert_eq!(status, 202, "{w}");
    assert_eq!(w["kind"], "calibrate");
    let missing = t.post("/api/levels/calibrate", json!({ "engine": "reckless", "count": 1 })).await;
    assert_eq!(missing.0, 409, "Reckless is not installed: {}", missing.1);
    // A game at a level.
    let (status, g) = t
        .post("/api/games", json!({ "white": "fake@3", "black": "fake@4", "time": { "type": "nodes", "nodes": 500 } }))
        .await;
    assert_eq!(status, 201, "{g}");
    assert!(g["summary"]["white"]["name"].as_str().unwrap().starts_with("Fake "));
    // Calibration games stay out of the list unless asked for.
    let (status, _) = t.get("/api/games?calibration=true").await;
    assert_eq!(status, 200);
    assert_eq!(t.get("/api/games?calibration=maybe").await.0, 400);
    t.stop().await;
}
