//! What the app's tests share: the whole app on a temporary data directory, with the fake UCI engine on its engines
//! list as `fake` (the analysis engine), a mock Lichess, and an HTTP client with the token.

#![allow(dead_code)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use dameless::{App, Options};
use dameless_hub::Config;
use dameless_lichess::testing::MockLichess;
use dameless_play::Timing;
use dameless_work::NewEngineRequest;
use serde_json::Value;

/// The fake UCI engine (analysis's, built as this crate's `dameless-fake-engine`).
pub fn fake_engine() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dameless-fake-engine"))
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
        let mock = MockLichess::start().await;
        let data = dir.path().join("data");
        let config = Config::new(&data, "127.0.0.1:0".parse().unwrap(), 4);
        let options = Options { engines: None, lichess: Some(mock.config()), timing: Timing::fast() };
        let app = App::start(config, options).await.unwrap();
        let fake =
            NewEngineRequest { name: "Fake".into(), path: fake_engine().display().to_string(), ..Default::default() };
        app.engines.add(fake).await.unwrap();
        app.engines.set_analysis("fake").await.unwrap();
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
