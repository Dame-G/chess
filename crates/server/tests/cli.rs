//! The CLI: every route has a command, every listed command parses, and the binary talks to a running app.

mod common;

use std::process::Command;

use chess::cli::{COMMANDS, Cli};
use clap::Parser;
use common::*;
use serde_json::Value;

#[test]
fn every_route_has_a_command() {
    let doc = serde_json::to_value(chess::api::openapi()).unwrap();
    let mut ops = Vec::new();
    for item in doc["paths"].as_object().unwrap().values() {
        for op in item.as_object().unwrap().values() {
            if let Some(id) = op["operationId"].as_str() {
                ops.push(id.to_owned());
            }
        }
    }
    let missing: Vec<&String> = ops.iter().filter(|op| !COMMANDS.iter().any(|(id, _)| id == op)).collect();
    assert!(missing.is_empty(), "routes without a command: {missing:?}");
    let stale: Vec<&str> = COMMANDS.iter().map(|(id, _)| *id).filter(|id| !ops.iter().any(|op| op == id)).collect();
    assert!(stale.is_empty(), "commands for routes that are gone: {stale:?}");
    for (id, line) in COMMANDS {
        let args: Vec<&str> = line.split_whitespace().collect();
        if let Err(e) = Cli::try_parse_from(&args) {
            panic!("{id}: `{line}` does not parse: {e}");
        }
    }
    // The callback stays out of the document.
    assert!(doc["paths"].get("/api/connections/you/callback").is_none());
}

#[test]
fn the_event_union_covers_every_kind() {
    let doc = serde_json::to_value(chess::api::openapi()).unwrap();
    let union = doc["components"]["schemas"]["EventData"].to_string();
    for kind in chess::api::events::kinds() {
        assert!(union.contains(&format!("\"{kind}\"")), "the Event schema has no {kind}");
    }
}

fn cli(t: &TestApp, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_chess"))
        .args(args)
        .env("CHESS_URL", &t.url)
        .env("CHESS_TOKEN", &t.token)
        .env("CHESS_DATA", &t.data)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_binary_is_a_client() {
    let t = TestApp::start().await;
    let (ok, out, err) = tokio::task::block_in_place(|| cli(&t, &["status", "--json"]));
    assert!(ok, "{err}");
    let status: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(status["cores"], 4);

    let (ok, out, _) = tokio::task::block_in_place(|| cli(&t, &["status"]));
    assert!(ok && out.contains("cores"), "{out}");

    // An error exits non-zero with the app's message on stderr.
    let (ok, _, err) = tokio::task::block_in_place(|| cli(&t, &["game", "999", "--json"]));
    assert!(!ok);
    assert!(err.contains("No game 999"), "{err}");

    // Play a move against the fake engine and get the reply.
    let (ok, out, err) = tokio::task::block_in_place(|| {
        cli(&t, &["play", "--white", "you", "--black", "fake", "--movetime", "20", "--json"])
    });
    assert!(ok, "{err}");
    let game: Value = serde_json::from_str(&out).unwrap();
    let id = game["summary"]["id"].to_string();
    let (ok, out, err) = tokio::task::block_in_place(|| cli(&t, &["move", &id, "d4", "--json"]));
    assert!(ok, "{err}");
    let after: Value = serde_json::from_str(&out).unwrap();
    assert!(after["moves"].as_array().unwrap().len() >= 2, "{after}");
    let (ok, _, err) = tokio::task::block_in_place(|| cli(&t, &["resign", &id]));
    assert!(ok, "{err}");

    // The token comes from the data directory when not given.
    let out = Command::new(env!("CARGO_BIN_EXE_chess"))
        .args(["token"])
        .env("CHESS_URL", &t.url)
        .env_remove("CHESS_TOKEN")
        .env("CHESS_DATA", &t.data)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), t.token);

    // Evaluation streams until its budget is reached.
    let (ok, out, err) = tokio::task::block_in_place(|| cli(&t, &["eval", "--depth", "3", "--multipv", "2", "--json"]));
    assert!(ok, "{err}");
    let done: Value = serde_json::from_str(&out).unwrap();
    assert!(done["last"]["lines"].is_array(), "{out}");
    t.stop().await;
}
