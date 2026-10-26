//! The CLI's side of the API: HTTP to a running runner, with the token from `--token`, `CHESS_TOKEN`, or the local
//! data directory.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

pub struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl Client {
    pub fn new(url: &str, token: Option<String>, data: &Path) -> Self {
        let token = token
            .or_else(|| std::fs::read_to_string(crate::config::token_path(data)).ok())
            .map(|t| t.trim().to_owned());
        Self { base: url.trim_end_matches('/').to_owned(), token, http: reqwest::Client::new() }
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        Ok(self.send(self.http.get(self.url(path))).await?.json().await?)
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
        Ok(self.send(self.http.post(self.url(path)).json(&body)).await?.json().await?)
    }

    pub async fn put(&self, path: &str, body: Value) -> Result<Value> {
        Ok(self.send(self.http.put(self.url(path)).json(&body)).await?.json().await?)
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        Ok(self.send(self.http.delete(self.url(path))).await?.json().await?)
    }

    /// A `DELETE` with a JSON body, for deletions that ask for confirmation.
    pub async fn delete_with(&self, path: &str, body: Value) -> Result<Value> {
        Ok(self.send(self.http.delete(self.url(path)).json(&body)).await?.json().await?)
    }

    pub async fn text(&self, path: &str) -> Result<String> {
        Ok(self.send(self.http.get(self.url(path))).await?.text().await?)
    }

    /// The live event stream, from the event after `since`. Read it with [`reqwest::Response::chunk`].
    pub async fn events(&self, since: u64) -> Result<reqwest::Response> {
        self.send(self.http.get(self.url(&format!("/api/events?since={since}")))).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn send(&self, mut req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res =
            req.send().await.with_context(|| format!("no runner at {} (start one with `chess serve`)", self.base))?;
        let status = res.status();
        if !status.is_success() {
            // The runner answers errors as { "error": "..." }; show just the message.
            let text = res.text().await.unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_owned))
                .unwrap_or(text);
            bail!("{status}: {message}");
        }
        Ok(res)
    }
}

/// JSON with `--json`, otherwise the one-line summary.
pub fn print(value: &serde_json::Value, json: bool, text: impl Fn(&serde_json::Value) -> String) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
    } else {
        println!("{}", text(value));
    }
}
