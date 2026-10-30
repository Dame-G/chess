//! The CLI's side of the API: HTTP to a running app, with the token from `--token`, `CHESS_TOKEN` or the data
//! directory, and the event stream read as server-sent events.

use std::{path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use futures::StreamExt;
use serde_json::Value;

pub struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl Client {
    /// A client of the app at `url`; the token is `token`, else the one in the data directory `data`.
    pub fn new(url: &str, token: Option<String>, data: &Path) -> Self {
        let token = token
            .or_else(|| std::fs::read_to_string(data.join("token")).ok())
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty());
        Self { base: url.trim_end_matches('/').to_owned(), token, http: reqwest::Client::new() }
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        json(self.send(self.http.get(self.url(path))).await?).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
        json(self.send(self.http.post(self.url(path)).json(&body)).await?).await
    }

    pub async fn put(&self, path: &str, body: Value) -> Result<Value> {
        json(self.send(self.http.put(self.url(path)).json(&body)).await?).await
    }

    pub async fn send_json(&self, method: reqwest::Method, path: &str, body: Value) -> Result<Value> {
        json(self.send(self.http.request(method, self.url(path)).json(&body)).await?).await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        json(self.send(self.http.delete(self.url(path))).await?).await
    }

    pub async fn text(&self, path: &str) -> Result<String> {
        Ok(self.send(self.http.get(self.url(path))).await?.text().await?)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn send(&self, mut req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res = req
            .send()
            .await
            .with_context(|| format!("no Chess app at {} (start one with `chess serve`)", self.base))?;
        let status = res.status();
        if !status.is_success() {
            // The app answers errors as { "error": { "code", "message" } }; show the message.
            let text = res.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_owned));
            match message {
                Some(message) => bail!("{message}"),
                None => bail!("the app answered {status}: {text}"),
            }
        }
        Ok(res)
    }

    /// Follows the event stream on `topics` after event `since`, calling `on` with each event's envelope
    /// (`{ id, topic, kind, data }`) until it returns false. Returns the last id seen.
    pub async fn follow(&self, topics: &str, since: u64, on: impl FnMut(&Value) -> bool) -> Result<u64> {
        let res = self.open_stream(topics, since).await?;
        read_events(res, since, on).await
    }

    /// The stream, opened (its headers received), so nothing published after this call is missed.
    pub async fn open_stream(&self, topics: &str, since: u64) -> Result<reqwest::Response> {
        let path = format!("/api/events?since={since}&topics={topics}");
        self.send(self.http.get(self.url(&path))).await
    }

    /// Polls `path` until `done` says the answer is final, then returns it.
    pub async fn poll(&self, path: &str, done: impl Fn(&Value) -> bool) -> Result<Value> {
        loop {
            let v = self.get(path).await?;
            if done(&v) {
                return Ok(v);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

async fn json(res: reqwest::Response) -> Result<Value> {
    let text = res.text().await?;
    if text.trim().is_empty() { Ok(Value::Null) } else { Ok(serde_json::from_str(&text)?) }
}

/// One JSON document with `--json`, otherwise the summary.
pub fn print(value: &Value, json: bool, text: impl Fn(&Value) -> String) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
    } else {
        println!("{}", text(value));
    }
}

/// Reads an opened event stream, calling `on` with each event's envelope until it returns false or the stream ends.
/// Returns the last id seen.
pub async fn read_events(res: reqwest::Response, since: u64, mut on: impl FnMut(&Value) -> bool) -> Result<u64> {
    let mut last = since;
    let mut stream = res.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        buf.push_str(&String::from_utf8_lossy(&chunk.context("the event stream broke")?));
        while let Some(end) = buf.find("\n\n") {
            let block: String = buf.drain(..end + 2).collect();
            let data: String = block
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(|d| d.strip_prefix(' ').unwrap_or(d))
                .collect::<Vec<_>>()
                .join("\n");
            if data.is_empty() {
                continue;
            }
            let Ok(event) = serde_json::from_str::<Value>(&data) else { continue };
            if let Some(id) = event["id"].as_u64().filter(|id| *id > 0) {
                last = id;
            }
            if !on(&event) {
                return Ok(last);
            }
        }
    }
    Ok(last)
}
