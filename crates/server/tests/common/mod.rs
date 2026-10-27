//! What the app's tests share: the whole app on a temporary data directory, over a temporary engine repository, with
//! a fake toolchain whose "builds" are the fake UCI engine, a mock Lichess, and an HTTP client with the token.

#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

use anyhow::{Result, bail};
use async_trait::async_trait;
use dameless::{App, Options};
use dameless_engine::{
    bench::Measurement,
    build::{Bench, BuildSpec, Built},
    fastchess::{Progress, Round, RoundResult},
    tools::Installed,
};
use dameless_hub::Config;
use dameless_lichess::testing::MockLichess;
use dameless_play::Timing;
use dameless_work::Toolchain;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// The node count every fake build benches, which the test repository's commits carry.
pub const BENCH: u64 = 1_000_000;

/// The fake UCI engine (analysis's, built as this crate's `dameless-fake-engine`).
pub fn fake_engine() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dameless-fake-engine"))
}

/// Builds by copying the fake engine; benches at [`BENCH`]; plays no fastchess rounds.
pub struct FakeToolchain;

#[async_trait]
impl Toolchain for FakeToolchain {
    async fn build(
        &self,
        spec: &BuildSpec,
        _cancel: &CancellationToken,
        log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<Built> {
        let sha = git(&spec.repo, &["rev-parse", &format!("{}^{{commit}}", spec.commit)]);
        std::fs::create_dir_all(&spec.out_dir)?;
        let binary = spec.out_dir.join(dameless_engine::exe("reckless"));
        std::fs::copy(fake_engine(), &binary)?;
        log("fake build");
        Ok(Built {
            binary,
            commit: sha.clone(),
            tree: sha,
            features: spec.features.clone(),
            network: None,
            log: String::new(),
        })
    }

    async fn bench(&self, _binary: &Path) -> Result<Bench> {
        Ok(Bench { nodes: BENCH, nps: 660_000 })
    }

    async fn measure(
        &self,
        _binary: &Path,
        copies: u32,
        expected: Option<u64>,
        _cancel: &CancellationToken,
    ) -> Result<Measurement> {
        let benches: Vec<Bench> = (0..copies.max(1)).map(|_| Bench { nodes: BENCH, nps: 660_000 }).collect();
        Ok(Measurement::from_benches(&benches, expected))
    }

    async fn spsa(&self, _binary: &Path) -> Result<String> {
        Ok("rfp_margin, int, 80, 40, 160, 8, 0.002\n".into())
    }

    async fn round(
        &self,
        _round: &Round,
        _cancel: &CancellationToken,
        _on_progress: &mut (dyn FnMut(Progress) + Send),
        _log: &mut (dyn for<'s> FnMut(&'s str) + Send),
    ) -> Result<RoundResult> {
        bail!("the test toolchain plays no rounds")
    }

    fn tools(&self, _dir: &Path) -> Installed {
        Installed { fastchess: None, book: None }
    }
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// An engine repository with `main` at one commit and a branch `deeper` with a change, both with `Bench:` lines.
fn repo(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "Test"]);
    git(dir, &["config", "user.email", "test@example.com"]);
    git(dir, &["config", "core.autocrlf", "false"]);
    std::fs::write(dir.join("src/search.rs"), "fn search() {}\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", &format!("Start\n\nBench: {BENCH}")]);
    git(dir, &["checkout", "-q", "-b", "deeper"]);
    std::fs::write(dir.join("src/search.rs"), "fn search() { deeper() }\n").unwrap();
    git(dir, &["commit", "-q", "-am", &format!("Deeper\n\nBench: {BENCH}")]);
    git(dir, &["checkout", "-q", "main"]);
}

/// The whole app, served on a free loopback port.
pub struct TestApp {
    pub app: Arc<App>,
    pub mock: MockLichess,
    pub url: String,
    pub token: String,
    pub http: reqwest::Client,
    pub data: PathBuf,
    _dir: tempfile::TempDir,
}

impl TestApp {
    pub async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let engine = dir.path().join("reckless");
        repo(&engine);
        let mock = MockLichess::start().await;
        let data = dir.path().join("data");
        let config = Config::new(&data, "127.0.0.1:0".parse().unwrap(), &engine, 4);
        let options =
            Options { toolchain: Some(Arc::new(FakeToolchain)), lichess: Some(mock.config()), timing: Timing::fast() };
        let app = App::start(config, options).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let router = app.router();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let token = app.hub.token.clone();
        Self { app, mock, url, token, http: reqwest::Client::new(), data, _dir: dir }
    }

    /// Stops the app: without it the test runtime waits for engines left warm.
    pub async fn stop(self) {
        self.app.stop().await;
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.url)
    }

    /// A request with the token.
    pub fn req(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http.request(method, self.url(path)).bearer_auth(&self.token)
    }

    /// Sends and reads the answer: its status and JSON body (null when empty or not JSON).
    pub async fn send(&self, req: reqwest::RequestBuilder) -> (u16, Value) {
        let res = req.send().await.unwrap();
        let status = res.status().as_u16();
        let text = res.text().await.unwrap();
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub async fn get(&self, path: &str) -> (u16, Value) {
        self.send(self.req(reqwest::Method::GET, path)).await
    }

    pub async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.send(self.req(reqwest::Method::POST, path).json(&body)).await
    }

    pub async fn put(&self, path: &str, body: Value) -> (u16, Value) {
        self.send(self.req(reqwest::Method::PUT, path).json(&body)).await
    }

    pub async fn delete(&self, path: &str) -> (u16, Value) {
        self.send(self.req(reqwest::Method::DELETE, path)).await
    }

    /// Polls `path` until `ok` holds (up to thirty seconds).
    pub async fn until(&self, what: &str, path: &str, ok: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..600 {
            let (status, v) = self.get(path).await;
            if status == 200 && ok(&v) {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("timed out waiting for {what} at {path}");
    }
}

/// Reads server-sent events from an opened stream until `stop` says so or `wait` passes; returns the envelopes.
pub async fn events(res: reqwest::Response, wait: Duration, stop: impl Fn(&[Value]) -> bool) -> Vec<Value> {
    use futures::StreamExt;
    let mut out = Vec::new();
    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let next = tokio::time::timeout_at(deadline, stream.next()).await;
        let Ok(Some(Ok(chunk))) = next else { break };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buf.find("\n\n") {
            let block: String = buf.drain(..end + 2).collect();
            let data: String = block.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim_start).collect();
            if let Ok(v) = serde_json::from_str::<Value>(&data) {
                out.push(v);
            }
        }
        if stop(&out) {
            break;
        }
    }
    out
}
