//! Local `sing-box` Clash REST API client for connection counting and instant user kicks.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Minimal Clash `/connections` response schema.
#[derive(Debug, Deserialize)]
struct ClashConnectionsResponse {
    /// Active connections list.
    #[serde(default)]
    connections: Vec<ClashConnectionEntry>,
}

/// Single active connection entry from Clash API.
#[derive(Debug, Deserialize)]
struct ClashConnectionEntry {
    /// Connection UUID/identifier.
    id: String,
    /// Connection metadata.
    metadata: ClashConnectionMetadata,
}

/// Metadata attached to a Clash connection.
#[derive(Debug, Deserialize)]
struct ClashConnectionMetadata {
    /// Authenticated proxy user (`users[].name`) if available.
    #[serde(default)]
    user: Option<String>,
}

/// HTTP client talking to `sing-box`'s local Clash API (`127.0.0.1:9090`).
#[derive(Debug, Clone)]
pub struct ClashClient {
    /// Base URL (e.g. `http://127.0.0.1:9090`).
    base_url: String,
    /// Optional bearer secret.
    secret: String,
    /// Shared HTTP client.
    http: reqwest::Client,
}

impl ClashClient {
    /// Creates a new [`ClashClient`].
    #[must_use]
    pub fn new(base_url: &str, secret: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            secret: secret.to_owned(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// Counts total active connections and per-user active connection counts.
    ///
    /// # Errors
    /// Returns an error if the Clash API request fails.
    pub async fn snapshot_connections(&self) -> Result<(u32, HashMap<String, u32>)> {
        let url = format!("{}/connections", self.base_url);
        let mut req = self.http.get(&url);
        if !self.secret.is_empty() {
            req = req.bearer_auth(&self.secret);
        }

        let resp = req
            .send()
            .await
            .context("Clash GET /connections failed")?
            .error_for_status()?
            .json::<ClashConnectionsResponse>()
            .await?;

        let total = u32::try_from(resp.connections.len()).unwrap_or(u32::MAX);
        let mut per_user = HashMap::new();
        for conn in resp.connections {
            if let Some(user) = conn.metadata.user.filter(|s| !s.is_empty()) {
                let entry = per_user.entry(user).or_insert(0_u32);
                *entry = entry.saturating_add(1);
            }
        }
        Ok((total, per_user))
    }

    /// Terminates all active connections belonging to `proxy_username`.
    ///
    /// # Errors
    /// Returns an error if listing connections fails.
    pub async fn kick_user(&self, proxy_username: &str) -> Result<u32> {
        let url = format!("{}/connections", self.base_url);
        let mut req = self.http.get(&url);
        if !self.secret.is_empty() {
            req = req.bearer_auth(&self.secret);
        }

        let resp = req
            .send()
            .await?
            .error_for_status()?
            .json::<ClashConnectionsResponse>()
            .await?;

        let mut closed = 0_u32;
        for conn in resp.connections {
            if conn.metadata.user.as_deref() == Some(proxy_username) {
                let del_url = format!("{}/connections/{}", self.base_url, conn.id);
                let mut del_req = self.http.delete(&del_url);
                if !self.secret.is_empty() {
                    del_req = del_req.bearer_auth(&self.secret);
                }
                if del_req.send().await.is_ok() {
                    closed = closed.saturating_add(1);
                }
            }
        }
        Ok(closed)
    }
}
