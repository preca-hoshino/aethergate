//! Hysteria 2-inspired Open API (`/api/open/v1/*`) request and response contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::protocol::{AuthorizedUserCredential, NodeTemplate, TlsProfile};
use crate::telemetry::ReportCadenceMode;

/// Response for `POST /api/open/v1/auth/verify` verifying agent credentials & server capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeAuthVerifyResponse {
    /// Verified node UUID.
    pub node_id: Uuid,
    /// Human-readable node name.
    pub node_name: String,
    /// Current control-plane config revision for this node.
    pub config_revision: i64,
    /// Current control-plane user revision for this node.
    pub user_revision: i64,
    /// Requested initial reporting cadence mode.
    pub cadence_mode: ReportCadenceMode,
    /// Control-plane server timestamp in UTC.
    pub server_time: DateTime<Utc>,
}

/// Request payload compatible with Hysteria 2 External HTTP Auth (`POST /api/open/v1/auth/client`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientAuthRequest {
    /// Client peer IP:port reported by the proxy core.
    pub addr: String,
    /// Client authentication secret (`"ag_u01:password"` or `"password"`).
    pub auth: String,
    /// Negotiated client transmit rate in bps (Hy2 `tx` field).
    #[serde(default)]
    pub tx: u64,
}

/// Response payload for `POST /api/open/v1/auth/client` matching Hysteria 2 HTTP auth schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientAuthResponse {
    /// Whether the client is authorized to connect.
    pub ok: bool,
    /// Immutable user identity (`proxy_username`) returned to the proxy core on success.
    pub id: String,
}

/// Query format selector for `GET /api/open/v1/config`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFormat {
    /// Compact node template + TLS profile + user credentials (expanded locally by agent).
    #[default]
    Compact,
    /// Pre-compiled full `sing-box` JSON configuration.
    Singbox,
}

/// Compact or compiled node configuration returned by `GET /api/open/v1/config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeConfigResponse {
    /// Node UUID.
    pub node_id: Uuid,
    /// Monotonic configuration revision.
    pub config_revision: i64,
    /// Current user sync revision matching included credentials (when `Singbox` format is used).
    pub user_revision: i64,
    /// Compact node template (present for `format=compact`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<NodeTemplate>,
    /// Shared TLS profile (present for `format=compact`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_profile: Option<TlsProfile>,
    /// Full compiled `sing-box` JSON (present for `format=singbox`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub singbox_config: Option<Value>,
}

/// Sync mode returned by `GET /api/open/v1/users`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserSyncMode {
    /// Full replacement snapshot of all authorized users on this node.
    Full,
    /// Incremental delta operations since `since_rev`.
    Delta,
}

/// Incremental operation kind in a delta user sync response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserDeltaOpKind {
    /// Insert or update an authorized user credential.
    Upsert,
    /// Revoke a user's access from this node.
    Revoke,
}

/// Single incremental user change record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserDeltaEntry {
    /// Revision at which this delta was recorded.
    pub rev: i64,
    /// Operation kind (`upsert` or `revoke`).
    pub op: UserDeltaOpKind,
    /// User UUID.
    pub user_id: Uuid,
    /// Immutable proxy username.
    pub proxy_username: String,
    /// Present for `upsert` operations; `None` for `revoke`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<AuthorizedUserCredential>,
}

/// Response for `GET /api/open/v1/users?since_rev=<rev>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeUsersSyncResponse {
    /// Sync mode (`full` or `delta`).
    pub mode: UserSyncMode,
    /// Latest user revision included in this response.
    pub latest_rev: i64,
    /// Full snapshot list when `mode == Full`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<AuthorizedUserCredential>,
    /// Incremental delta list when `mode == Delta`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deltas: Vec<UserDeltaEntry>,
}

/// Request payload for `POST /api/open/v1/commands/{id}/ack`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandAckRequest {
    /// Whether the command succeeded on the node.
    pub success: bool,
    /// Optional human-readable execution message or error detail.
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hy2_client_auth_roundtrip() {
        let req = ClientAuthRequest {
            addr: "203.0.113.10:54321".to_owned(),
            auth: "ag_alice:secret-uuid".to_owned(),
            tx: 100_000_000,
        };
        let encoded = serde_json::to_string(&req);
        assert!(encoded.is_ok());
        let resp = ClientAuthResponse {
            ok: true,
            id: "ag_alice".to_owned(),
        };
        let resp_json = serde_json::to_string(&resp);
        assert!(resp_json.is_ok());
    }
}
