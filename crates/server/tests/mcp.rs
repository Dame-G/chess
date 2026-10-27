//! `/mcp` over HTTP: the guard, the tool list made from the OpenAPI document, and calls that read and write through
//! the app's routes.

mod common;

use common::*;
use serde_json::{Value, json};

/// A JSON-RPC request in the 2026-07-28 protocol: stateless, its version and client in `_meta`.
fn rpc(id: u64, method: &str, mut params: Value) -> Value {
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "1.0" },
        "io.modelcontextprotocol/clientCapabilities": {}
    });
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

async fn mcp(t: &TestApp, token: Option<&str>, body: &Value) -> (u16, Value) {
    let method = body["method"].as_str().unwrap_or_default().to_owned();
    let mut req = t
        .http
        .post(t.url("/mcp"))
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2026-07-28")
        .header("mcp-method", method)
        .json(body);
    if body["method"] == "tools/call" {
        req = req.header("mcp-name", body["params"]["name"].as_str().unwrap());
    }
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    t.send(req).await
}

async fn call(t: &TestApp, name: &str, arguments: Value) -> Value {
    let (status, res) =
        mcp(t, Some(&t.token), &rpc(9, "tools/call", json!({ "name": name, "arguments": arguments }))).await;
    assert_eq!(status, 200, "{res}");
    res["result"].clone()
}

fn text(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap_or(Value::Null)
}

#[tokio::test]
async fn mcp_wants_the_token() {
    let t = TestApp::start().await;
    let list = rpc(1, "tools/list", json!({}));
    let (status, body) = mcp(&t, None, &list).await;
    assert_eq!((status, body["error"]["code"].as_str()), (401, Some("unauthorized")));
    assert_eq!(mcp(&t, Some("wrong"), &list).await.0, 401);
    // Another website is refused as for the API.
    let res = t
        .http
        .post(t.url("/mcp"))
        .bearer_auth(&t.token)
        .header("origin", "https://evil.example")
        .json(&list)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    t.stop().await;
}

#[tokio::test]
async fn tools_are_the_api() {
    let t = TestApp::start().await;
    let (status, body) = mcp(&t, Some(&t.token), &rpc(1, "tools/list", json!({}))).await;
    assert_eq!(status, 200, "{body}");
    let tools = body["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    // One tool per operation, but the event stream and the token, plus the two waits.
    let doc = serde_json::to_value(dameless::api::openapi()).unwrap();
    let mut ops = 0;
    for methods in doc["paths"].as_object().unwrap().values() {
        for op in methods.as_object().unwrap().values() {
            let id = op["operationId"].as_str().unwrap();
            assert_eq!(names.contains(&id), !dameless::mcp::tools::EXCLUDED.contains(&id), "{id}");
            ops += 1;
        }
    }
    assert_eq!(names.len(), ops - dameless::mcp::tools::EXCLUDED.len() + 2);
    assert!(names.contains(&"wait_for_work") && names.contains(&"wait_for_idea"));
    let tool = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap();
    assert_eq!(tool("status")["annotations"]["readOnlyHint"], true);
    let challenge = tool("challenge");
    assert_eq!(challenge["annotations"]["openWorldHint"], true);
    assert!(challenge["description"].as_str().unwrap().contains("real Lichess account"));
    assert_eq!(tool("push")["annotations"]["destructiveHint"], true);
    assert_eq!(tool("create_idea")["inputSchema"]["required"], json!(["body"]));
    t.stop().await;
}

#[tokio::test]
async fn calls_read_and_write_through_the_routes() {
    let t = TestApp::start().await;
    // A read.
    let status = call(&t, "status", json!({})).await;
    assert_eq!(status["isError"], false, "{status}");
    assert!(text(&status)["machine"].is_object());
    // A write: a pasted game, then the list has it.
    let pgn = "[White \"A\"]\n[Black \"B\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0\n";
    let imported = call(&t, "import_pgn", json!({ "body": { "pgn": pgn } })).await;
    assert_eq!(imported["isError"], false, "{imported}");
    let id = text(&imported)[0]["id"].as_i64().unwrap();
    let game = text(&call(&t, "get_game", json!({ "id": id })).await);
    assert_eq!(game["moves"].as_array().unwrap().len(), 7);
    // A route's error is a tool error with its status.
    let missing = call(&t, "get_game", json!({ "id": 999_999 })).await;
    assert_eq!(missing["isError"], true);
    assert!(missing["content"][0]["text"].as_str().unwrap().starts_with("HTTP 404"));
    // A bad call says what is wrong.
    let bad = call(&t, "get_game", json!({})).await;
    assert_eq!(bad["isError"], true);
    // Waiting: a version's build, which the fake toolchain finishes at once.
    let (code, version) = t.post("/api/versions", json!({ "ref": "deeper" })).await;
    assert_eq!(code, 201, "{version}");
    // The build is queued just after the version is answered: wait until it is listed.
    let has_build = |w: &Value| {
        ["running", "queued", "recent"]
            .iter()
            .any(|k| w[*k].as_array().is_some_and(|a| a.iter().any(|w| w["kind"] == "build")))
    };
    let work = t.until("the build queued", "/api/work", has_build).await;
    let build = work["running"]
        .as_array()
        .unwrap()
        .iter()
        .chain(work["queued"].as_array().unwrap())
        .chain(work["recent"].as_array().unwrap())
        .find(|w| w["kind"] == "build")
        .map(|w| w["id"].as_i64().unwrap())
        .expect("a build was queued");
    let waited = text(&call(&t, "wait_for_work", json!({ "id": build, "timeout_seconds": 30 })).await);
    assert_eq!(waited["finished"], true, "{waited}");
    assert_eq!(waited["work"]["status"], "done");
    t.stop().await;
}

#[tokio::test]
async fn older_clients_initialize() {
    let t = TestApp::start().await;
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "old", "version": "1" } } });
    let res = t
        .http
        .post(t.url("/mcp"))
        .bearer_auth(&t.token)
        .header("accept", "application/json, text/event-stream")
        .json(&init)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["result"]["instructions"].as_str().unwrap().contains("Fair play"), "{body}");
    let list = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} });
    let res = t
        .http
        .post(t.url("/mcp"))
        .bearer_auth(&t.token)
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18")
        .json(&list)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["result"]["tools"].as_array().is_some_and(|t| t.len() > 50), "{body}");
    t.stop().await;
}
