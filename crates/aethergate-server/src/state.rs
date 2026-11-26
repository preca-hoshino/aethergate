//! Shared application state for `aethergate-server`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use chrono_tz::Tz;
use dashmap::DashMap;
use serde::Serialize;
use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use aethergate_protocol::{NodeSseEvent, ReportCadenceMode};

use crate::config::ServerConfig;

/// Lightweight dirty notification sent to connected panel WebSocket clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PanelWsEvent {
    /// Initial snapshot or refreshed live state.
    Snapshot {
        /// Current online node count.
        online_nodes: usize,
        /// Current cadence mode (`eco` or `realtime`).
        cadence_mode: ReportCadenceMode,
        /// Latest node telemetry rows.
        nodes: Vec<LiveNodeMetric>,
    },
    /// Incremental live update triggered by node status or telemetry ingestion.
    LiveTick {
        /// Current cadence mode (`eco` or `realtime`).
        cadence_mode: ReportCadenceMode,
        /// Updated node telemetry rows.
        nodes: Vec<LiveNodeMetric>,
    },
}

/// Compact per-node live metric row broadcast over `/api/v1/ws/realtime`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LiveNodeMetric {
    /// Node UUID.
    pub id: Uuid,
    /// Node display name.
    pub name: String,
    /// Node status (`online`, `offline`, `degraded`).
    pub status: String,
    /// CPU utilization percentage.
    pub cpu_pct: f32,
    /// Memory utilization percentage.
    pub mem_pct: f32,
    /// Active connections on the node.
    pub active_connections: i32,
    /// Active users on the node.
    pub active_users: i32,
    /// Current node uptime in seconds.
    pub uptime_secs: i64,
}

/// Shared state injected into all Axum handlers and background workers.
#[derive(Debug, Clone)]
pub struct AppState {
    /// PostgreSQL connection pool.
    pub db: PgPool,
    /// Loaded server configuration.
    pub config: Arc<ServerConfig>,
    /// Parsed 32-byte AES-256-GCM master key for node HMAC secrets.
    pub master_key: [u8; 32],
    /// Parsed IANA timezone for natural calendar month calculations.
    pub stats_tz: Tz,
    /// Reusable outbound HTTP client for OIDC requests.
    pub http_client: reqwest::Client,
    /// Per-node SSE broadcast channels (`node_id -> sender`).
    pub node_sse_channels: Arc<DashMap<Uuid, broadcast::Sender<NodeSseEvent>>>,
    /// Flag set whenever node status or telemetry changes, consumed by WS push worker.
    pub ws_dirty: Arc<AtomicBool>,
    /// Broadcast channel for connected panel WebSocket clients.
    pub ws_tx: broadcast::Sender<PanelWsEvent>,
    /// Number of currently connected admin WebSocket clients.
    pub active_admin_ws: Arc<AtomicUsize>,
}

impl AppState {
    /// Creates a new [`AppState`] instance.
    #[must_use]
    pub fn new(
        db: PgPool,
        config: ServerConfig,
        master_key: [u8; 32],
        stats_tz: Tz,
        http_client: reqwest::Client,
    ) -> Self {
        let (ws_tx, _) = broadcast::channel(64);
        Self {
            db,
            config: Arc::new(config),
            master_key,
            stats_tz,
            http_client,
            node_sse_channels: Arc::new(DashMap::new()),
            ws_dirty: Arc::new(AtomicBool::new(false)),
            ws_tx,
            active_admin_ws: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Returns or creates the SSE broadcast sender for a specific node.
    #[must_use]
    pub fn node_sse_sender(&self, node_id: Uuid) -> broadcast::Sender<NodeSseEvent> {
        if let Some(existing) = self.node_sse_channels.get(&node_id) {
            return existing.clone();
        }
        let (tx, _) = broadcast::channel(32);
        self.node_sse_channels.insert(node_id, tx.clone());
        tx
    }

    /// Emits an SSE event to a specific node if it has subscribers.
    pub fn push_node_event(&self, node_id: Uuid, event: NodeSseEvent) {
        if let Some(tx) = self.node_sse_channels.get(&node_id) {
            let _ = tx.send(event);
        }
    }

    /// Emits an SSE event to all connected nodes.
    pub fn broadcast_all_nodes(&self, event: &NodeSseEvent) {
        for entry in self.node_sse_channels.iter() {
            let _ = entry.value().send(event.clone());
        }
    }

    /// Marks live metrics dirty so the background WS dispatcher pushes an update.
    pub fn mark_ws_dirty(&self) {
        self.ws_dirty.store(true, Ordering::Release);
    }

    /// Returns the desired global reporting cadence based on active admin WS viewers.
    #[must_use]
    pub fn desired_cadence_mode(&self) -> ReportCadenceMode {
        if self.active_admin_ws.load(Ordering::Acquire) > 0 {
            ReportCadenceMode::Realtime
        } else {
            ReportCadenceMode::Eco
        }
    }
}
