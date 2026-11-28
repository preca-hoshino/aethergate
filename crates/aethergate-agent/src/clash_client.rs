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
    /// Combined `inbound_type/inbound_tag` string emitted by `sing-box` Clash API.
    #[serde(default, rename = "type")]
    conn_type: Option<String>,
    /// Explicit inbound tag field if emitted by Clash-compatible cores.
    #[serde(default, rename = "inboundName")]
    inbound_name: Option<String>,
    /// Fallback inbound tag field.
    #[serde(default)]
    inbound: Option<String>,
}

/// Aggregated active connection counts from Clash `/connections`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ConnectionSnapshot {
    /// Total active connections across all inbounds.
    pub total: u32,
    /// Active connection counts grouped by `proxy_username`.
    pub per_user: HashMap<String, u32>,
    /// Active connection counts grouped by `sing-box` inbound tag.
    pub per_inbound: HashMap<String, u32>,
}

/// Extracts the `sing-box` inbound tag from Clash connection metadata.
fn extract_inbound_tag(meta: &ClashConnectionMetadata) -> Option<&str> {
    if let Some(name) = meta.inbound_name.as_deref().filter(|s| !s.is_empty()) {
        return Some(name);
    }
    if let Some(inbound) = meta.inbound.as_deref().filter(|s| !s.is_empty()) {
        return Some(inbound);
    }
    if let Some((_, tag)) = meta.conn_type.as_deref().and_then(|t| t.split_once('/'))
        && !tag.is_empty()
    {
        return Some(tag);
    }
    None
}

/// Parses a Clash `/connections` response into [`ConnectionSnapshot`].
fn aggregate_connections(resp: ClashConnectionsResponse) -> ConnectionSnapshot {
    let total = u32::try_from(resp.connections.len()).unwrap_or(u32::MAX);
    let mut per_user = HashMap::new();
    let mut per_inbound = HashMap::new();
    for conn in resp.connections {
        if let Some(tag) = extract_inbound_tag(&conn.metadata) {
            let entry = per_inbound.entry(tag.to_owned()).or_insert(0_u32);
            *entry = entry.saturating_add(1);
        }
        if let Some(user) = conn.metadata.user.filter(|s| !s.is_empty()) {
            let entry = per_user.entry(user).or_insert(0_u32);
            *entry = entry.saturating_add(1);
        }
    }
    ConnectionSnapshot {
        total,
        per_user,
        per_inbound,
    }
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

    /// Counts total active connections, per-user connections, and per-inbound connections.
    ///
    /// # Errors
    /// Returns an error if the Clash API request fails.
    pub async fn snapshot_connections(&self) -> Result<ConnectionSnapshot> {
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

        Ok(aggregate_connections(resp))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_connections_groups_by_user_and_inbound_tag() {
        let raw = r#"{
            "connections": [
                {
                    "id": "1",
                    "metadata": {
                        "type": "vless/in-cernet-vless",
                        "user": "ag_alice"
                    }
                },
                {
                    "id": "2",
                    "metadata": {
                        "type": "hysteria2/in-cernet-hy2",
                        "user": "ag_alice"
                    }
                },
                {
                    "id": "3",
                    "metadata": {
                        "inboundName": "in-phys-tuic",
                        "type": "tuic/in-phys-tuic",
                        "user": "ag_bob"
                    }
                }
            ]
        }"#;
        let parsed: Result<ClashConnectionsResponse, _> = serde_json::from_str(raw);
        assert!(parsed.is_ok());
        if let Ok(resp) = parsed {
            let snap = aggregate_connections(resp);
            assert_eq!(snap.total, 3);
            assert_eq!(snap.per_user.get("ag_alice"), Some(&2));
            assert_eq!(snap.per_user.get("ag_bob"), Some(&1));
            assert_eq!(snap.per_inbound.get("in-cernet-vless"), Some(&1));
            assert_eq!(snap.per_inbound.get("in-cernet-hy2"), Some(&1));
            assert_eq!(snap.per_inbound.get("in-phys-tuic"), Some(&1));
        }
    }
}
