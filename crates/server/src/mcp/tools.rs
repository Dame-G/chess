//! The MCP tools, made from the OpenAPI document: one tool per operation, so the API and the tools cannot drift.
//!
//! A tool is named by its operation id. Its description is the route's summary and description, the CLI command that
//! calls the same route, and what the tool touches beyond this machine. Its input schema is an object holding the
//! route's path and query parameters under their own names, and its JSON body (when it takes one) under `body`.
//! Component schemas the input refers to are copied into the schema's `$defs`, so each tool's schema stands alone.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// Operations that are not tools: the event stream (a tool answers once; `wait_for_work` and `wait_for_idea` cover
/// waiting), and the API token, which only pairs the hosted UI and which an MCP client already holds.
pub const EXCLUDED: &[&str] = &["follow_events", "token"];

/// What an operation touches beyond this app, written into its description and its annotations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Only this app and this machine.
    Local,
    /// Reads from Lichess or another site; changes nothing there.
    ReadsOutside,
    /// Acts on the user's real Lichess account: only when the user asked.
    RealAccount,
    /// Acts on the outside world in another way (a push, a download): only when the user asked.
    Outside,
}

/// Operations that reach beyond the machine, and how. Everything not listed is [`Reach::Local`].
const REACH: &[(&str, Reach)] = &[
    // Your account.
    ("login", Reach::RealAccount),
    ("set_connection_token", Reach::RealAccount),
    ("logout", Reach::RealAccount),
    ("seek", Reach::RealAccount),
    ("cancel_seek", Reach::RealAccount),
    ("challenge", Reach::RealAccount),
    ("accept_challenge", Reach::RealAccount),
    ("decline_challenge", Reach::RealAccount),
    ("cancel_challenge", Reach::RealAccount),
    // In a Lichess game these play, offer, resign or abort on Lichess.
    ("act_in_game", Reach::RealAccount),
    ("stop_game", Reach::RealAccount),
    // Downloads.
    ("setup", Reach::Outside),
    // Reads from GitHub (the sizes of the built-in engines' downloads).
    ("setup_plan", Reach::ReadsOutside),
    // Reads from Lichess.
    ("import_player", Reach::ReadsOutside),
    ("top_players", Reach::ReadsOutside),
    ("get_player", Reach::ReadsOutside),
    ("explorer", Reach::ReadsOutside),
    ("tablebase", Reach::ReadsOutside),
];

/// Operations that undo or destroy something (beyond their own effect being reversible by another call): deleting,
/// stopping, resigning, forgetting a token.
const DESTRUCTIVE: &[&str] = &[
    "delete_drill",
    "remove_engine",
    "stop_work",
    "stop_game",
    "stop_eval",
    "logout",
    "decline_challenge",
    "cancel_challenge",
    "act_in_game",
];

/// Assistance the fair-play policy refuses (409 `fair_play`) while the user plays on Lichess.
const FAIR_PLAY: &[&str] =
    &["start_eval", "explorer", "tablebase", "review_games", "get_repertoire", "get_prep", "import_player"];

/// Extra words for operations whose summary alone could mislead an agent.
const NOTES: &[(&str, &str)] = &[
    (
        "act_in_game",
        "In a Lichess game this plays on the user's real account: only the move or action the user chose, never one \
         an engine or you picked for them (Lichess's fair-play rules forbid it). In a local game against an engine it \
         is free.",
    ),
    ("stop_game", "In a Lichess game of the user's this resigns it (or aborts it before both sides moved) on Lichess."),
    (
        "setup",
        "Downloads the built-in engines (Stockfish, lc0 and Maia's nets; about a hundred MB, `setup_plan` gives the \
         sizes). Only when the user asked, after they saw the sizes.",
    ),
    ("add_engine", "Starts the binary at `path` to check it speaks UCI: only a path the user gave."),
];

/// Where a parameter goes in the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    Path,
    Query,
}

/// One parameter of an operation.
#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub place: Place,
    pub required: bool,
}

/// One operation as a tool: what the list shows, and what a call needs to make the request.
#[derive(Clone, Debug)]
pub struct Operation {
    pub id: String,
    pub method: String,
    pub path: String,
    pub params: Vec<Param>,
    /// Whether the operation reads a JSON body (`body` in the tool's input).
    pub body: bool,
    pub description: String,
    pub schema: Map<String, Value>,
    pub reach: Reach,
    pub read_only: bool,
    pub destructive: bool,
    pub idempotent: bool,
}

/// How an operation reaches beyond the machine.
pub fn reach(id: &str) -> Reach {
    REACH.iter().find(|(op, _)| *op == id).map_or(Reach::Local, |(_, r)| *r)
}

/// The operation ids the tables above name, to check them against the document.
pub fn named() -> impl Iterator<Item = &'static str> {
    REACH
        .iter()
        .map(|(id, _)| *id)
        .chain(DESTRUCTIVE.iter().copied())
        .chain(FAIR_PLAY.iter().copied())
        .chain(NOTES.iter().map(|(id, _)| *id))
        .chain(EXCLUDED.iter().copied())
}

/// Every operation of the document but the excluded ones, in the document's order of paths.
pub fn operations(doc: &Value, commands: &[(&str, &str)]) -> Vec<Operation> {
    let components = doc.pointer("/components/schemas").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut out = Vec::new();
    let Some(paths) = doc.get("paths").and_then(Value::as_object) else { return out };
    for (path, methods) in paths {
        let Some(methods) = methods.as_object() else { continue };
        for (method, op) in methods {
            let Some(id) = op.get("operationId").and_then(Value::as_str) else { continue };
            if EXCLUDED.contains(&id) {
                continue;
            }
            out.push(operation(id, method, path, op, &components, commands));
        }
    }
    out
}

fn operation(
    id: &str,
    method: &str,
    path: &str,
    op: &Value,
    components: &Map<String, Value>,
    commands: &[(&str, &str)],
) -> Operation {
    let mut properties = Map::new();
    let mut required = Vec::new();
    let mut params = Vec::new();
    for p in op.get("parameters").and_then(Value::as_array).into_iter().flatten() {
        let (Some(name), Some(place)) = (p.get("name").and_then(Value::as_str), p.get("in").and_then(Value::as_str))
        else {
            continue;
        };
        let place = match place {
            "path" => Place::Path,
            "query" => Place::Query,
            _ => continue,
        };
        let is_required = place == Place::Path || p.get("required").and_then(Value::as_bool).unwrap_or(false);
        let mut schema = p.get("schema").cloned().unwrap_or_else(|| json!({}));
        if let (Some(obj), Some(text)) = (schema.as_object_mut(), p.get("description").and_then(Value::as_str)) {
            obj.entry("description").or_insert_with(|| Value::String(text.to_owned()));
        }
        if place == Place::Query {
            mention_lists(&mut schema, name);
        }
        properties.insert(name.to_owned(), schema);
        if is_required {
            required.push(Value::String(name.to_owned()));
        }
        params.push(Param { name: name.to_owned(), place, required: is_required });
    }

    let body_schema = op.pointer("/requestBody/content/application~1json/schema").cloned();
    let body = body_schema.is_some();
    if let Some(mut schema) = body_schema {
        let body_required = op.pointer("/requestBody/required").and_then(Value::as_bool).unwrap_or(false);
        if let Some(obj) = schema.as_object_mut() {
            let text = if body_required { "The JSON body." } else { "The JSON body (optional)." };
            obj.entry("description").or_insert_with(|| Value::String(text.to_owned()));
        }
        properties.insert("body".into(), schema);
        if body_required {
            required.push(Value::String("body".into()));
        }
    }

    let mut schema = Map::new();
    schema.insert("type".into(), Value::String("object".into()));
    schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".into(), Value::Array(required));
    }
    schema.insert("additionalProperties".into(), Value::Bool(false));
    let mut schema = Value::Object(schema);
    let defs = definitions(&schema, components);
    rewrite_refs(&mut schema);
    let mut schema = match schema {
        Value::Object(map) => map,
        _ => unreachable!("built as an object"),
    };
    if !defs.is_empty() {
        let mut defs: Map<String, Value> = defs.into_iter().collect();
        for def in defs.values_mut() {
            rewrite_refs(def);
        }
        schema.insert("$defs".into(), Value::Object(defs));
    }

    let reach = reach(id);
    let read_only = method == "get";
    Operation {
        id: id.to_owned(),
        method: method.to_ascii_uppercase(),
        path: path.to_owned(),
        params,
        body,
        description: describe(id, method, path, op, reach, commands),
        schema,
        reach,
        read_only,
        destructive: !read_only && DESTRUCTIVE.contains(&id),
        idempotent: read_only || matches!(method, "put" | "delete"),
    }
}

/// The description: the route's own words, then the CLI command, what it reaches and fair play.
fn describe(id: &str, method: &str, path: &str, op: &Value, reach: Reach, commands: &[(&str, &str)]) -> String {
    let mut parts = Vec::new();
    for key in ["summary", "description"] {
        if let Some(text) = op.get(key).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()) {
            parts.push(text.replace('\n', " "));
        }
    }
    match reach {
        Reach::RealAccount => {
            parts.push("Acts on the user's real Lichess account: only when the user asked.".into());
        }
        Reach::Outside => parts.push("Acts on the outside world: only when the user asked.".into()),
        Reach::ReadsOutside => parts.push("Reads from Lichess (network).".into()),
        Reach::Local => {}
    }
    if let Some((_, note)) = NOTES.iter().find(|(op, _)| *op == id) {
        parts.push((*note).to_owned());
    }
    if FAIR_PLAY.contains(&id) {
        parts.push(
            "Refused (409 fair_play) while the user plays on Lichess; never work around it, and never analyse a \
             position from a game the user is playing."
                .into(),
        );
    }
    let route = format!("Route: {} {path}.", method.to_ascii_uppercase());
    match commands.iter().find(|(op, _)| *op == id) {
        Some((_, line)) => parts.push(format!("{route} CLI: `{line}`.")),
        None => parts.push(route),
    }
    parts.join(" ")
}

/// Query lists are comma-separated strings: say so where the schema is a plain string named like a list.
fn mention_lists(schema: &mut Value, name: &str) {
    let plural = name.ends_with('s') && !matches!(name, "rated" | "status" | "plies" | "moves");
    if let Some(obj) = schema.as_object_mut()
        && plural
        && obj.get("type").and_then(Value::as_str) == Some("string")
        && !obj.contains_key("description")
    {
        obj.insert("description".into(), Value::String("Comma-separated.".into()));
    }
}

/// The component schemas `value` refers to, followed through each other.
fn definitions(value: &Value, components: &Map<String, Value>) -> BTreeMap<String, Value> {
    let mut seen = BTreeSet::new();
    let mut todo = Vec::new();
    refs(value, &mut todo);
    let mut out = BTreeMap::new();
    while let Some(name) = todo.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(schema) = components.get(&name) {
            refs(schema, &mut todo);
            out.insert(name, schema.clone());
        }
    }
    out
}

fn refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(name) = map.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix(COMPONENTS)) {
                out.push(name.to_owned());
            }
            map.values().for_each(|v| refs(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| refs(v, out)),
        _ => {}
    }
}

const COMPONENTS: &str = "#/components/schemas/";

/// `#/components/schemas/X` → `#/$defs/X`.
fn rewrite_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get_mut("$ref")
                && let Some(name) = r.strip_prefix(COMPONENTS)
            {
                *r = format!("#/$defs/{name}");
            }
            map.values_mut().for_each(rewrite_refs);
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite_refs),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Value {
        serde_json::to_value(crate::api::openapi()).unwrap()
    }

    #[test]
    fn every_operation_but_the_excluded_is_a_tool() {
        let doc = doc();
        let ops = operations(&doc, crate::cli::COMMANDS);
        let mut ids = Vec::new();
        for methods in doc["paths"].as_object().unwrap().values() {
            for op in methods.as_object().unwrap().values() {
                ids.push(op["operationId"].as_str().unwrap().to_owned());
            }
        }
        for id in &ids {
            let tool = ops.iter().any(|op| &op.id == id);
            assert_eq!(tool, !EXCLUDED.contains(&id.as_str()), "{id}");
        }
        // The tables name only operations that exist.
        for id in named() {
            assert!(ids.iter().any(|op| op == id), "{id} is not an operation");
        }
    }

    #[test]
    fn schemas_stand_alone() {
        let ops = operations(&doc(), crate::cli::COMMANDS);
        for op in &ops {
            let text = serde_json::to_string(&op.schema).unwrap();
            assert!(!text.contains("#/components/"), "{}: {text}", op.id);
            let mut found = Vec::new();
            refs_any(&Value::Object(op.schema.clone()), &mut found);
            for r in found {
                let name = r.strip_prefix("#/$defs/").unwrap_or_else(|| panic!("{}: {r}", op.id));
                assert!(op.schema["$defs"].get(name).is_some(), "{}: {name} is not defined", op.id);
            }
        }
        let add = ops.iter().find(|op| op.id == "add_engine").unwrap();
        assert_eq!(add.schema["required"], json!(["body"]));
        assert!(add.schema["$defs"].get("NewEngineRequest").is_some());
        let game = ops.iter().find(|op| op.id == "get_game").unwrap();
        assert_eq!(game.params[0].place, Place::Path);
        assert!(game.read_only && !game.destructive);
        assert!(game.description.contains("CLI: `dameless game 12`"));
    }

    fn refs_any(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(r) = map.get("$ref").and_then(Value::as_str) {
                    out.push(r.to_owned());
                }
                map.values().for_each(|v| refs_any(v, out));
            }
            Value::Array(items) => items.iter().for_each(|v| refs_any(v, out)),
            _ => {}
        }
    }

    #[test]
    fn real_accounts_and_the_outside_world_say_so() {
        let ops = operations(&doc(), crate::cli::COMMANDS);
        for id in ["challenge", "seek", "set_connection_token", "logout", "login"] {
            let op = ops.iter().find(|op| op.id == id).unwrap();
            assert_eq!(op.reach, Reach::RealAccount, "{id}");
            assert!(op.description.contains("real Lichess account: only when the user asked"), "{id}");
        }
        let setup = ops.iter().find(|op| op.id == "setup").unwrap();
        assert_eq!(setup.reach, Reach::Outside);
        let plan = ops.iter().find(|op| op.id == "setup_plan").unwrap();
        assert_eq!(plan.reach, Reach::ReadsOutside);
        let eval = ops.iter().find(|op| op.id == "start_eval").unwrap();
        assert!(eval.description.contains("fair_play"));
    }
}
