//! Telemetry reporting DTOs shared between `aethergate-agent` and `aethergate-server`.
//!
//! Supports adaptive `Eco` (30s) / `Realtime` (5s) reporting cadences, idempotent batch
//! deduplication via `batch_id`, and `204 No Content` / `200 OK` control-plane responses.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::protocol::ProtocolType;

/// Reporting cadence requested by the control plane or selected by the node agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReportCadenceMode {
    /// Low-bandwidth mode (default 30s intervals, zero-traffic silence).
    #[default]
    Eco,
    /// High-frequency mode (5s intervals while an admin is watching live metrics).
    Realtime,
}

/// Periodic node health and resource status payload sent to `POST /api/open/v1/report/status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeStatusReport {
    /// Report creation timestamp in UTC.
    pub reported_at: DateTime<Utc>,
    /// Agent binary version string.
    pub agent_version: String,
    /// Managed `sing-box` binary version string.
    pub singbox_version: String,
    /// Current `sing-box` child process uptime in seconds.
    pub uptime_secs: u64,
    /// CPU utilization percentage (`0.0..=100.0`).
    pub cpu_pct: f32,
    /// Memory utilization percentage (`0.0..=100.0`).
    pub mem_pct: f32,
    /// Resident memory used in MiB.
    pub mem_used_mib: u32,
    /// Current active TCP/UDP proxy connections count from Clash API.
    pub active_connections: u32,
    /// Currently active user count on the node.
    pub active_users: u32,
    /// Currently applied config revision on the node.
    pub config_revision: i64,
    /// Currently applied user sync revision on the node.
    pub user_revision: i64,
    /// Current agent reporting cadence mode.
    pub cadence_mode: ReportCadenceMode,
}

/// Per-user traffic and instantaneous bandwidth delta sample for a reporting window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetrySample {
    /// Immutable proxy username (`users.proxy_username`), matching `sing-box` user identity.
    pub proxy_username: String,
    /// Uplink bytes delta during this window (client -> node).
    pub uplink_bytes: u64,
    /// Downlink bytes delta during this window (node -> client).
    pub downlink_bytes: u64,
    /// Average uplink rate in bytes/sec during this window.
    pub uplink_bps: u64,
    /// Average downlink rate in bytes/sec during this window.
    pub downlink_bps: u64,
    /// Active connections observed for this user at window end.
    pub active_connections: u32,
}

/// Per-inbound protocol traffic delta slice for protocol analytics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundTelemetrySlice {
    /// Inbound tag (e.g., `"in-vless-443"`).
    pub inbound_tag: String,
    /// Protocol family for this inbound.
    pub protocol: ProtocolType,
    /// Uplink bytes delta across all users on this inbound.
    pub uplink_bytes: u64,
    /// Downlink bytes delta across all users on this inbound.
    pub downlink_bytes: u64,
}

/// Idempotent telemetry batch sent to `POST /api/open/v1/report/telemetry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryBatchRequest {
    /// Unique idempotency key for WAL replay deduplication.
    pub batch_id: Uuid,
    /// Reporting interval start timestamp (UTC).
    pub window_start: DateTime<Utc>,
    /// Reporting interval end timestamp (UTC).
    pub window_end: DateTime<Utc>,
    /// Elapsed window duration in milliseconds.
    pub window_ms: u32,
    /// Currently applied config revision.
    pub config_revision: i64,
    /// Currently applied user revision.
    pub user_revision: i64,
    /// Per-user traffic samples (zero-byte users are omitted).
    pub user_samples: Vec<TelemetrySample>,
    /// Per-inbound traffic slices (zero-byte inbounds are omitted).
    #[serde(default)]
    pub inbound_slices: Vec<InboundTelemetrySlice>,
}

/// Control-plane action piggybacked on `200 OK` report acknowledgements when work is pending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReportAckAction {
    /// Switch the agent's telemetry cadence mode (`eco` or `realtime`).
    SwitchCadence {
        /// Target cadence mode.
        mode: ReportCadenceMode,
    },
    /// Fetch updated node configuration (`GET /api/open/v1/config`).
    SyncConfig {
        /// Latest server config revision.
        target_revision: i64,
    },
    /// Fetch incremental user delta (`GET /api/open/v1/users?since_rev=...`).
    SyncUsers {
        /// Latest server user revision.
        target_revision: i64,
    },
    /// Immediately kick a user's active connections via local Clash API.
    KickUser {
        /// Command identifier to ack on `/commands/{id}/ack`.
        command_id: Uuid,
        /// Immutable proxy username to disconnect.
        proxy_username: String,
    },
    /// Trigger an immediate `sing-box` reload.
    ReloadSingbox {
        /// Command identifier to ack on `/commands/{id}/ack`.
        command_id: Uuid,
    },
}

/// Optional `200 OK` JSON response body returned by report endpoints when actions are queued.
/// When no actions are needed, the server returns `204 No Content` (0 bytes) to save egress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportAckResponse {
    /// Server-side latest config revision.
    pub latest_config_revision: i64,
    /// Server-side latest user revision.
    pub latest_user_revision: i64,
    /// Pending actions for the agent to execute.
    #[serde(default)]
    pub actions: Vec<ReportAckAction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_batch_serializes_cleanly() {
        let now = Utc::now();
        let batch = TelemetryBatchRequest {
            batch_id: Uuid::new_v4(),
            window_start: now,
            window_end: now,
            window_ms: 5000,
            config_revision: 3,
            user_revision: 12,
            user_samples: vec![TelemetrySample {
                proxy_username: "ag_u01".to_owned(),
                uplink_bytes: 1024,
                downlink_bytes: 4096,
                uplink_bps: 204,
                downlink_bps: 819,
                active_connections: 2,
            }],
            inbound_slices: vec![InboundTelemetrySlice {
                inbound_tag: "in-hy2-8443".to_owned(),
                protocol: ProtocolType::Hysteria2,
                uplink_bytes: 1024,
                downlink_bytes: 4096,
            }],
        };

        let json = serde_json::to_string(&batch);
        assert!(json.is_ok());
        let Ok(encoded) = json else {
            return;
        };
        let decoded: Result<TelemetryBatchRequest, _> = serde_json::from_str(&encoded);
        assert!(decoded.is_ok());
        if let Ok(value) = decoded {
            assert_eq!(value, batch);
        }
    }
}
