//! The CLI's side of the API: HTTP to a running runner, with the token from `--token`, `CHESS_TOKEN`, or the local
//! data directory.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;

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

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.send(self.http.get(self.url(path))).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn send<T: DeserializeOwned>(&self, mut req: reqwest::RequestBuilder) -> Result<T> {
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let res =
            req.send().await.with_context(|| format!("no runner at {} (start one with `chess serve`)", self.base))?;
        let status = res.status();
        if !status.is_success() {
            bail!("{status}: {}", res.text().await.unwrap_or_default());
        }
        Ok(res.json().await?)
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
