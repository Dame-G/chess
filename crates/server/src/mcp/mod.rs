//! `/mcp`: the API as MCP tools, for Claude (or any MCP client) to drive the app with the same verbs as the UI and the
//! CLI.
//!
//! - **Transport.** Streamable HTTP, served statelessly by `rmcp` (no sessions; the 2026-07-28 protocol, and older
//!   clients' `initialize` too), answering plain JSON. It sits behind the same guard as the API: the same bearer
//!   token, the same `Host` and `Origin` checks.
//! - **Tools.** One per operation of the OpenAPI document ([`tools`]), named by its operation id. A call is made into
//!   a request and sent through the app's own router (its handlers, its guard), so a tool does exactly what its route
//!   does and answers with the route's JSON. A route's error is a tool error carrying the status and the error body.
//! - **Two more tools** wait without polling from the client: `wait_for_work` and `wait_for_idea`.
//!
//! The server's instructions carry the rules an agent must keep: fair play, and the user's real accounts.

pub mod tools;

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
    middleware,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
    },
};
use serde_json::{Map, Value, json};
use tower::ServiceExt;

use crate::App;
use tools::{Operation, Place, Reach};

/// What the server tells a client when it connects: how to work with the app, and the rules.
pub const INSTRUCTIONS: &str = "\
Dameless is a chess app with two players at one board: the user and their engine, Dameless (a fork of Reckless). \
Every tool is one route of the app's API, named by its operation id; the description names the CLI command that does \
the same. Results are the route's JSON.

How to work:
- Start with `get_dameless` (the champion, the logbook of ideas, the bot) and `status` (machine, cores, fair play).
- Engine work is ideas: commit a change on a branch of the fork with an up-to-date `Bench:` line, then `create_idea` \
or `create_batch` with a hypothesis, notes and `author: \"claude\"`. Ideas run as work in the background; \
`wait_for_idea` and `wait_for_work` wait for them.
- Read verdicts honestly: the short (STC [0, 4], alpha 0.05, beta 0.20) trial decides, and its kept Elo tends to read \
high; a gain under about 2 Elo needs the long trial before it is trusted. A dropped regression check (`regressions` in \
`get_dameless`) means the champion is measurably worse than five promotions ago: report it first. Promote (`promote_idea`) only kept ideas, when the user wants it. \
Never `push` unless the user asked for that push.
- On a new machine, check the harness first (`harness_status`, `check_harness`).

Fair play (a hard rule): while the user plays a game on Lichess, the app refuses analysis (engine evaluation, the \
explorer, the tablebase, reviews, the repertoire, other players' imports) with 409 `fair_play`. Never work around \
it, and never analyse or suggest moves for a position from a game the user is playing. In a Lichess game, \
`act_in_game` only ever sends the move or action the user chose.

Real accounts: tools whose description says they act on the user's real Lichess account (challenges, seeks, the bot \
going online or seeking, tokens, logging out, moves in Lichess games) or on the outside world (push, downloads) are \
used only when the user asked for that action.";

/// How long the wait tools sleep between looks.
const POLL: Duration = Duration::from_secs(2);

/// The longest a wait tool waits in one call.
const MAX_WAIT_SECONDS: u64 = 3600;

/// The longest text a tool answers with; a longer one (a long log) keeps its end.
const MAX_TEXT: usize = 200_000;

/// The operations, made once from the document.
fn operations() -> &'static [Operation] {
    static OPS: OnceLock<Vec<Operation>> = OnceLock::new();
    OPS.get_or_init(|| {
        let doc = serde_json::to_value(crate::api::openapi()).expect("the OpenAPI document is JSON");
        tools::operations(&doc, crate::cli::COMMANDS)
    })
}

/// The `/mcp` route, behind the API's guard.
pub fn router(app: Arc<App>) -> Router<Arc<App>> {
    let server = Server(Arc::new(Shared { app: app.clone(), router: OnceLock::new() }));
    // The app's own `host` and `guard` layers check the Host, the Origin and the token, as for every API route.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .disable_allowed_hosts()
        .with_cancellation_token(app.shutdown.child_token());
    let service =
        StreamableHttpService::new(move || Ok(server.clone()), Arc::new(NeverSessionManager::default()), config);
    Router::new().route_service("/mcp", service).layer(middleware::from_fn_with_state(app, crate::api::guard::guard))
}

struct Shared {
    app: Arc<App>,
    /// The app's router, made on first use: tool calls go through it.
    router: OnceLock<Router>,
}

/// The MCP server: one per request (stateless), all sharing the app.
#[derive(Clone)]
pub struct Server(Arc<Shared>);

/// A route's answer.
struct Answer {
    status: StatusCode,
    text: String,
}

impl Server {
    /// Sends a request through the app's router, with the app's token.
    async fn send(&self, method: Method, uri: &str, body: Option<Vec<u8>>) -> Result<Answer, ErrorData> {
        let router = self.0.router.get_or_init(|| crate::api::router(self.0.app.clone())).clone();
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "127.0.0.1")
            .header(header::AUTHORIZATION, format!("Bearer {}", self.0.app.hub.token));
        if body.is_some() {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
        let req = req
            .body(body.map(Body::from).unwrap_or_else(Body::empty))
            .map_err(|e| ErrorData::internal_error(format!("make the request: {e}"), None))?;
        let res = router.oneshot(req).await.map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .map_err(|e| ErrorData::internal_error(format!("read the answer: {e}"), None))?;
        Ok(Answer { status, text: String::from_utf8_lossy(&bytes).into_owned() })
    }

    async fn call_operation(&self, op: &Operation, args: Map<String, Value>) -> Result<CallToolResult, ErrorData> {
        let (uri, body) = match request(op, args) {
            Ok(r) => r,
            Err(message) => return Ok(CallToolResult::error(vec![ContentBlock::text(message)])),
        };
        let method =
            Method::from_bytes(op.method.as_bytes()).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let answer = self.send(method, &uri, body).await?;
        Ok(result(answer))
    }

    /// `wait_for_work` and `wait_for_idea`: look every few seconds until the item ends or the time is up.
    async fn wait(&self, args: &Map<String, Value>, idea: bool) -> Result<CallToolResult, ErrorData> {
        let Some(id) = args.get("id").and_then(Value::as_i64) else {
            return Ok(CallToolResult::error(vec![ContentBlock::text("`id` (an integer) is required")]));
        };
        let timeout = args.get("timeout_seconds").and_then(Value::as_u64).unwrap_or(600).min(MAX_WAIT_SECONDS);
        let (uri, ended): (String, &[&str]) = if idea {
            (format!("/api/ideas/{id}"), &["kept", "dropped", "stopped", "failed"])
        } else {
            (format!("/api/work/{id}"), &["done", "failed", "stopped"])
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
        loop {
            let answer = self.send(Method::GET, &uri, None).await?;
            if !answer.status.is_success() {
                return Ok(result(answer));
            }
            let item: Value = serde_json::from_str(&answer.text).unwrap_or(Value::Null);
            let finished = item.get("status").and_then(Value::as_str).is_some_and(|s| ended.contains(&s));
            let now = tokio::time::Instant::now();
            if finished || now >= deadline {
                let key = if idea { "idea" } else { "work" };
                let text = json!({ "finished": finished, key: item }).to_string();
                return Ok(CallToolResult::success(vec![ContentBlock::text(text)]));
            }
            tokio::time::sleep(POLL.min(deadline - now)).await;
        }
    }
}

/// The request a call makes: the URI with its path and query, and the JSON body.
fn request(op: &Operation, mut args: Map<String, Value>) -> Result<(String, Option<Vec<u8>>), String> {
    let mut path = op.path.clone();
    let mut query = Vec::new();
    for param in &op.params {
        let value = args.remove(&param.name).filter(|v| !v.is_null());
        let Some(value) = value else {
            if param.required {
                return Err(format!("`{}` is required", param.name));
            }
            continue;
        };
        let text = plain(&value).ok_or_else(|| format!("`{}` must be a string, number or boolean", param.name))?;
        match param.place {
            Place::Path => path = path.replace(&format!("{{{}}}", param.name), &encode(&text)),
            Place::Query => query.push(format!("{}={}", param.name, encode(&text))),
        }
    }
    let body = match args.remove("body") {
        Some(Value::Null) | None => None,
        Some(_) if !op.body => return Err(format!("{} takes no body", op.id)),
        Some(body) => Some(serde_json::to_vec(&body).map_err(|e| e.to_string())?),
    };
    if let Some(name) = args.keys().next() {
        let known: Vec<&str> = op.params.iter().map(|p| p.name.as_str()).chain(op.body.then_some("body")).collect();
        return Err(format!("unknown argument `{name}`; {} takes: {}", op.id, known.join(", ")));
    }
    let uri = if query.is_empty() { path } else { format!("{path}?{}", query.join("&")) };
    Ok((uri, body))
}

/// A parameter's value as text: lists are comma-separated, as the API reads them.
fn plain(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(items) => items.iter().map(plain).collect::<Option<Vec<_>>>().map(|v| v.join(",")),
        _ => None,
    }
}

/// Percent-encodes a path segment or a query value (branch names hold `/`, player keys `:`).
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A route's answer as a tool result: its body on success, its status and body as a tool error otherwise.
fn result(answer: Answer) -> CallToolResult {
    let mut text = answer.text;
    if text.len() > MAX_TEXT {
        let mut cut = text.len() - MAX_TEXT;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        text = format!("[{} bytes left out]\n{}", cut, &text[cut..]);
    }
    if answer.status.is_success() {
        if text.trim().is_empty() {
            text = format!("Done ({}).", answer.status.as_u16());
        }
        CallToolResult::success(vec![ContentBlock::text(text)])
    } else {
        CallToolResult::error(vec![ContentBlock::text(format!("HTTP {}: {text}", answer.status.as_u16()))])
    }
}

/// The tool an operation is.
fn tool(op: &Operation) -> Tool {
    let annotations = ToolAnnotations::new()
        .read_only(op.read_only)
        .destructive(op.destructive)
        .idempotent(op.idempotent)
        .open_world(op.reach != Reach::Local);
    Tool::new(op.id.clone(), op.description.clone(), Arc::new(op.schema.clone())).with_annotations(annotations)
}

/// The two tools that wait.
fn wait_tools() -> [Tool; 2] {
    let schema = |what: &str| {
        let Value::Object(map) = json!({
            "type": "object",
            "properties": {
                "id": { "type": "integer", "description": format!("The {what}'s id.") },
                "timeout_seconds": {
                    "type": "integer", "minimum": 0, "maximum": MAX_WAIT_SECONDS,
                    "description": "How long to wait at most (default 600)."
                }
            },
            "required": ["id"],
            "additionalProperties": false
        }) else {
            unreachable!("an object")
        };
        Arc::new(map)
    };
    let annotations = || ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false);
    [
        Tool::new(
            "wait_for_work",
            "Waits until a work item (a build, trial, tune, review, import, ...) is done, failed or stopped, or until \
             `timeout_seconds` pass, looking every two seconds. Answers `{ finished, work }`; call again to keep \
             waiting. Work paused for a live game stays unfinished until it resumes.",
            schema("work item"),
        )
        .with_annotations(annotations()),
        Tool::new(
            "wait_for_idea",
            "Waits until an idea ends (kept, dropped, stopped or failed), or until `timeout_seconds` pass, looking \
             every two seconds. Answers `{ finished, idea }` with its trials and verdict; call again to keep waiting.",
            schema("idea"),
        )
        .with_annotations(annotations()),
    ]
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("dameless", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools: Vec<Tool> = operations().iter().map(tool).collect();
        tools.extend(wait_tools());
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request.arguments.unwrap_or_default();
        let result = match &*request.name {
            "wait_for_work" => self.wait(&args, false).await?,
            "wait_for_idea" => self.wait(&args, true).await?,
            name => match operations().iter().find(|op| op.id == name) {
                Some(op) => self.call_operation(op, args).await?,
                None => return Err(ErrorData::invalid_params(format!("there is no tool {name}"), None)),
            },
        };
        Ok(result.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(id: &str) -> &'static Operation {
        operations().iter().find(|op| op.id == id).unwrap()
    }

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn calls_become_requests() {
        let (uri, body) = request(op("get_game"), args(json!({ "id": 12 }))).unwrap();
        assert_eq!((uri.as_str(), body), ("/api/games/12", None));
        let (uri, _) =
            request(op("list_games"), args(json!({ "player": "you", "speed": ["blitz", "rapid"] }))).unwrap();
        assert_eq!(uri, "/api/games?player=you&speed=blitz,rapid");
        let (uri, _) = request(op("get_player"), args(json!({ "key": "lichess:alice" }))).unwrap();
        assert_eq!(uri, "/api/players/lichess%3Aalice");
        let (uri, body) = request(op("create_idea"), args(json!({ "body": { "title": "x" } }))).unwrap();
        assert_eq!((uri.as_str(), body.unwrap()), ("/api/ideas", br#"{"title":"x"}"#.to_vec()));
        assert!(request(op("get_game"), args(json!({}))).unwrap_err().contains("`id` is required"));
        assert!(request(op("get_game"), args(json!({ "id": 1, "nope": 2 }))).unwrap_err().contains("unknown argument"));
        assert!(request(op("get_game"), args(json!({ "id": 1, "body": {} }))).unwrap_err().contains("takes no body"));
    }

    #[test]
    fn long_answers_keep_their_end() {
        let text = format!("{}end", "x".repeat(MAX_TEXT + 10));
        let r = result(Answer { status: StatusCode::OK, text });
        let shown = serde_json::to_string(&r.content).unwrap();
        assert!(shown.contains("bytes left out") && shown.contains("end"));
    }
}
