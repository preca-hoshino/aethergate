//! Server-Sent Events (SSE) push payload definitions for `GET /api/open/v1/events`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::telemetry::ReportCadenceMode;

/// Typed SSE event emitted by the control plane to connected node agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum NodeSseEvent {
    /// Node configuration or TLS profile changed; agent should pull `/api/open/v1/config`.
    ConfigChanged {
        /// Latest config revision on the control plane.
        config_revision: i64,
    },
    /// One or more user grants/credentials changed; agent should pull `/api/open/v1/users`.
    UserDeltaAvailable {
        /// Latest user sync revision on the control plane.
        user_revision: i64,
    },
    /// Administrative command queued for immediate execution.
    CommandIssued {
        /// Command identifier to acknowledge after execution.
        command_id: Uuid,
        /// Command action kind (`kick_user`, `reload_singbox`, `sync_now`).
        action: String,
        /// Optional target `proxy_username` for `kick_user`.
        proxy_username: Option<String>,
    },
    /// Control plane requests switching between `eco` and `realtime` cadence.
    SwitchCadence {
        /// Target reporting cadence mode.
        mode: ReportCadenceMode,
    },
    /// Lightweight keep-alive heartbeat.
    Heartbeat {
        /// Server UTC timestamp.
        server_time: DateTime<Utc>,
    },
}

impl NodeSseEvent {
    /// Returns the SSE event name (`event:` line) matching the variant.
    #[must_use]
    pub const fn event_name(&self) -> &'static str {
        match self {
            Self::ConfigChanged { .. } => "config_changed",
            Self::UserDeltaAvailable { .. } => "user_delta_available",
            Self::CommandIssued { .. } => "command_issued",
            Self::SwitchCadence { .. } => "switch_cadence",
            Self::Heartbeat { .. } => "heartbeat",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_event_names_match_variants() {
        let ev = NodeSseEvent::SwitchCadence {
            mode: ReportCadenceMode::Realtime,
        };
        assert_eq!(ev.event_name(), "switch_cadence");
        let serialized = serde_json::to_string(&ev);
        assert!(serialized.is_ok());
    }
}
