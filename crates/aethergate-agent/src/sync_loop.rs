//! Main node synchronization loop: 2s gRPC sampling, Eco/Realtime telemetry flush,
//! WAL replay, SSE push handling, and pre-reload atomic stats preservation.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use sysinfo::System;
use tokio::sync::mpsc;
use tokio::time::interval;
use uuid::Uuid;

use aethergate_protocol::{
    AuthorizedUserCredential, InboundTelemetrySlice, LocalAgentOverrides, NodeSseEvent,
    NodeStatusReport, NodeTemplate, NodeUsersSyncResponse, ProtocolType, ReportAckAction,
    ReportCadenceMode, TelemetryBatchRequest, TelemetrySample, TlsProfile, UserDeltaOpKind,
    UserSyncMode,
};

use crate::clash_client::ClashClient;
use crate::client::OpenApiClient;
use crate::config::AgentConfig;
use crate::stager::stage_and_validate_config;
use crate::stats_grpc::{ParsedStatsSnapshot, V2RayStatsClient};
use crate::supervisor::SingboxSupervisor;
use crate::tls_helper::ensure_self_signed_cert;
use crate::wal::WalQueue;

/// In-memory accumulator between reporting flushes (5s in Realtime, 30s in Eco).
#[derive(Debug)]
struct TelemetryBuffer {
    /// Window start timestamp.
    window_start: DateTime<Utc>,
    /// Accumulated per-user `(uplink_bytes, downlink_bytes)`.
    user_bytes: HashMap<String, (u64, u64)>,
    /// Accumulated per-inbound `(uplink_bytes, downlink_bytes)`.
    inbound_bytes: HashMap<String, (u64, u64)>,
    /// Latest per-user active connection count.
    user_conns: HashMap<String, u32>,
}

impl TelemetryBuffer {
    /// Creates an empty telemetry buffer starting at `now`.
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            window_start: now,
            user_bytes: HashMap::new(),
            inbound_bytes: HashMap::new(),
            user_conns: HashMap::new(),
        }
    }

    /// Merges a 2s gRPC snapshot into the reporting window buffer.
    fn merge_snapshot(&mut self, snap: &ParsedStatsSnapshot, conns: &HashMap<String, u32>) {
        for (user, (up, down)) in &snap.users {
            let entry = self.user_bytes.entry(user.clone()).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(*up);
            entry.1 = entry.1.saturating_add(*down);
        }
        for (tag, (up, down)) in &snap.inbounds {
            let entry = self.inbound_bytes.entry(tag.clone()).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(*up);
            entry.1 = entry.1.saturating_add(*down);
        }
        for (user, count) in conns {
            self.user_conns.insert(user.clone(), *count);
        }
    }

    /// Returns `true` if no traffic was observed in this window (enables zero-traffic silence).
    fn is_zero_traffic(&self) -> bool {
        self.user_bytes.is_empty() && self.inbound_bytes.is_empty()
    }

    /// Drains the accumulated buffer into a [`TelemetryBatchRequest`] and resets `window_start`.
    fn drain_batch(
        &mut self,
        now: DateTime<Utc>,
        config_revision: i64,
        user_revision: i64,
        inbound_protocols: &HashMap<String, ProtocolType>,
    ) -> TelemetryBatchRequest {
        let elapsed_ms =
            u32::try_from((now - self.window_start).num_milliseconds().max(1)).unwrap_or(1000);
        let secs_f64 = f64::from(elapsed_ms) / 1000.0;

        let user_samples = self
            .user_bytes
            .drain()
            .filter(|(_, (up, down))| *up > 0 || *down > 0)
            .map(|(proxy_username, (uplink_bytes, downlink_bytes))| {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss
                )]
                let uplink_bps = ((uplink_bytes as f64) / secs_f64) as u64;
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss
                )]
                let downlink_bps = ((downlink_bytes as f64) / secs_f64) as u64;
                let active_connections = self.user_conns.get(&proxy_username).copied().unwrap_or(0);
                TelemetrySample {
                    proxy_username,
                    uplink_bytes,
                    downlink_bytes,
                    uplink_bps,
                    downlink_bps,
                    active_connections,
                }
            })
            .collect();

        let inbound_slices = self
            .inbound_bytes
            .drain()
            .filter(|(_, (up, down))| *up > 0 || *down > 0)
            .map(|(inbound_tag, (uplink_bytes, downlink_bytes))| {
                let protocol = inbound_protocols
                    .get(&inbound_tag)
                    .copied()
                    .unwrap_or(ProtocolType::VlessReality);
                InboundTelemetrySlice {
                    inbound_tag,
                    protocol,
                    uplink_bytes,
                    downlink_bytes,
                }
            })
            .collect();

        let batch = TelemetryBatchRequest {
            batch_id: Uuid::new_v4(),
            window_start: self.window_start,
            window_end: now,
            window_ms: elapsed_ms,
            config_revision,
            user_revision,
            user_samples,
            inbound_slices,
        };
        self.window_start = now;
        self.user_conns.clear();
        batch
    }
}

/// Applies full or incremental delta user updates to the local active user list.
pub fn apply_user_sync(
    users: &mut Vec<AuthorizedUserCredential>,
    sync: NodeUsersSyncResponse,
) -> i64 {
    match sync.mode {
        UserSyncMode::Full => {
            *users = sync.users;
        }
        UserSyncMode::Delta => {
            for delta in sync.deltas {
                match delta.op {
                    UserDeltaOpKind::Revoke => {
                        users.retain(|u| u.user_id != delta.user_id);
                    }
                    UserDeltaOpKind::Upsert => {
                        if let Some(cred) = delta.credential {
                            if let Some(existing) =
                                users.iter_mut().find(|u| u.user_id == delta.user_id)
                            {
                                *existing = cred;
                            } else {
                                users.push(cred);
                            }
                        }
                    }
                }
            }
        }
    }
    sync.latest_rev
}

/// Runtime state held by the agent loop.
struct AgentRuntime {
    /// Loaded agent configuration.
    cfg: AgentConfig,
    /// Signed Open API client.
    client: OpenApiClient,
    /// Persistent WAL queue.
    wal: WalQueue,
    /// `sing-box` child supervisor.
    supervisor: SingboxSupervisor,
    /// Local `V2Ray` gRPC stats client.
    stats_client: V2RayStatsClient,
    /// Local Clash REST client.
    clash_client: ClashClient,
    /// System metrics collector.
    sys: System,
    /// Current node template.
    template: NodeTemplate,
    /// Current shared TLS profile.
    tls_profile: TlsProfile,
    /// Currently authorized users.
    users: Vec<AuthorizedUserCredential>,
    /// Applied config revision.
    config_revision: i64,
    /// Applied user revision.
    user_revision: i64,
    /// Current cadence mode.
    cadence_mode: ReportCadenceMode,
    /// Accumulated telemetry buffer.
    buffer: TelemetryBuffer,
    /// Last active connections count.
    last_active_conns: u32,
}

/// Runs the complete node agent lifecycle until Ctrl-C.
///
/// # Errors
/// Returns an error if initial bootstrap or fatal I/O fails.
pub async fn run_agent_loop(cfg: AgentConfig) -> Result<()> {
    let wal = WalQueue::open(&cfg.wal_dir)?;
    let mut client = OpenApiClient::new(cfg.node_id, &cfg.hmac_secret, &cfg.server_url)?;
    let mut supervisor = SingboxSupervisor::new(&cfg.singbox_bin, &cfg.singbox_config_path);
    let _ = supervisor.detect_version().await;

    let verify = client.verify_auth().await?;
    let initial_cfg = client.fetch_compact_config().await?;
    let users_sync = client.fetch_users(None).await?;

    let mut template = NodeTemplate {
        inbounds: Vec::new(),
        dns_route: serde_json::Value::Null,
    };
    let mut tls_profile = TlsProfile::default();
    let mut config_revision = verify.config_revision;
    if let Some(cfg_resp) = initial_cfg {
        config_revision = cfg_resp.config_revision;
        if let Some(t) = cfg_resp.template {
            template = t;
        }
        if let Some(tls) = cfg_resp.tls_profile {
            tls_profile = tls;
        }
    }

    let mut users = Vec::new();
    let user_revision = apply_user_sync(&mut users, users_sync);

    let mut runtime = AgentRuntime {
        stats_client: V2RayStatsClient::new(&cfg.v2ray_api_addr),
        clash_client: ClashClient::new(&cfg.clash_api_addr, &cfg.clash_secret),
        cfg,
        client,
        wal,
        supervisor,
        sys: System::new_all()?,
        template,
        tls_profile,
        users,
        config_revision,
        user_revision,
        cadence_mode: verify.cadence_mode,
        buffer: TelemetryBuffer::new(Utc::now()),
        last_active_conns: 0,
    };

    runtime.apply_and_reload_singbox().await?;

    let (sse_tx, mut sse_rx) = mpsc::channel::<NodeSseEvent>(32);
    let sse_client = runtime.client.clone();
    tokio::spawn(async move {
        loop {
            if let Err(err) = sse_client.listen_sse(sse_tx.clone()).await {
                tracing::warn!(error = %err, "SSE stream disconnected; reconnecting in 5s");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });

    let mut sample_tick = interval(Duration::from_secs(runtime.cfg.sample_interval_secs.max(1)));
    let mut report_tick = interval(Duration::from_secs(1));
    let mut last_telemetry_flush = Utc::now();
    let mut last_status_flush = Utc::now();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("agent shutting down");
                let _ = runtime.flush_telemetry(true).await;
                runtime.supervisor.stop().await;
                break;
            }
            Some(ev) = sse_rx.recv() => {
                runtime.handle_sse_event(ev).await;
            }
            _ = sample_tick.tick() => {
                let _ = runtime.supervisor.ensure_running();
                runtime.sample_once().await;
            }
            _ = report_tick.tick() => {
                let cadence_secs = match runtime.cadence_mode {
                    ReportCadenceMode::Eco => 30,
                    ReportCadenceMode::Realtime => 5,
                };
                let now = Utc::now();
                if (now - last_telemetry_flush).num_seconds() >= cadence_secs {
                    last_telemetry_flush = now;
                    let _ = runtime.flush_telemetry(false).await;
                }
                if (now - last_status_flush).num_seconds() >= cadence_secs {
                    last_status_flush = now;
                    runtime.flush_status().await;
                }
            }
        }
    }

    Ok(())
}

impl AgentRuntime {
    /// Builds local overrides (self-signed cert if enabled) and reloads `sing-box`.
    async fn apply_and_reload_singbox(&mut self) -> Result<()> {
        let cert_path = self.cfg.cert_dir.join("fullchain.pem");
        let key_path = self.cfg.cert_dir.join("privkey.pem");
        if self.tls_profile.mode == "self_signed" {
            let _ = ensure_self_signed_cert(&cert_path, &key_path, &self.tls_profile.server_name)?;
        }

        let overrides = LocalAgentOverrides {
            cert_path_override: Some(cert_path.display().to_string()),
            key_path_override: Some(key_path.display().to_string()),
            v2ray_api_listen: self.cfg.v2ray_api_addr.clone(),
            clash_api_listen: self
                .cfg
                .clash_api_addr
                .trim_start_matches("http://")
                .to_owned(),
            clash_api_secret: self.cfg.clash_secret.clone(),
        };

        // Pre-reload atomic flush so gRPC counters are never lost on restart.
        self.sample_once().await;
        let _ = self.flush_telemetry_only(true).await;

        stage_and_validate_config(
            &self.cfg.singbox_bin,
            &self.cfg.singbox_config_path,
            &self.template,
            &self.tls_profile,
            &self.users,
            &overrides,
        )
        .await?;

        self.supervisor.reload().await?;
        Ok(())
    }

    /// Samples gRPC traffic counters and Clash connection counts once.
    async fn sample_once(&mut self) {
        let (total_conns, per_user_conns) = self
            .clash_client
            .snapshot_connections()
            .await
            .unwrap_or_default();
        self.last_active_conns = total_conns;

        if let Ok(snap) = self.stats_client.query_and_reset().await {
            self.buffer.merge_snapshot(&snap, &per_user_conns);
        }
    }

    /// Replays pending WAL batches, flushes the current buffer, and processes ack actions.
    async fn flush_telemetry(&mut self, force: bool) -> Result<()> {
        if let Some(actions) = self.flush_telemetry_only(force).await? {
            self.process_ack_actions(actions).await;
        }
        Ok(())
    }

    /// Flushes WAL + current buffer and returns any piggybacked ack actions without executing them.
    async fn flush_telemetry_only(&mut self, force: bool) -> Result<Option<Vec<ReportAckAction>>> {
        for wal_path in self.wal.list_pending()? {
            let batch = WalQueue::read_batch(&wal_path)?;
            if self.client.report_telemetry(&batch).await.is_ok() {
                let _ = WalQueue::remove(&wal_path);
            } else {
                break;
            }
        }

        if !force && self.buffer.is_zero_traffic() {
            return Ok(None);
        }

        let inbound_protocols: HashMap<String, ProtocolType> = self
            .template
            .inbounds
            .iter()
            .map(|i| (i.tag.clone(), i.protocol))
            .collect();

        let batch = self.buffer.drain_batch(
            Utc::now(),
            self.config_revision,
            self.user_revision,
            &inbound_protocols,
        );
        if batch.user_samples.is_empty() && batch.inbound_slices.is_empty() {
            return Ok(None);
        }

        match self.client.report_telemetry(&batch).await {
            Ok(Some(ack)) => Ok(Some(ack.actions)),
            Ok(None) => Ok(None),
            Err(err) => {
                tracing::warn!(error = %err, "telemetry upload failed; spooling to WAL");
                let _ = self.wal.enqueue(&batch);
                Ok(None)
            }
        }
    }

    /// Reports host CPU/RAM/connections heartbeat and handles piggybacked actions.
    async fn flush_status(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        let cpu_pct = self.sys.global_cpu_usage();
        let total_mem = self.sys.total_memory().max(1);
        let used_mem = self.sys.used_memory();
        #[expect(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let mem_pct = ((used_mem as f64 / total_mem as f64) * 100.0) as f32;
        let mem_used_mib = u32::try_from(used_mem / (1024 * 1024)).unwrap_or(u32::MAX);

        let report = NodeStatusReport {
            reported_at: Utc::now(),
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            singbox_version: self.supervisor.version().to_owned(),
            uptime_secs: self.supervisor.uptime_secs(),
            cpu_pct,
            mem_pct,
            mem_used_mib,
            active_connections: self.last_active_conns,
            active_users: u32::try_from(self.users.len()).unwrap_or(u32::MAX),
            config_revision: self.config_revision,
            user_revision: self.user_revision,
            cadence_mode: self.cadence_mode,
        };

        if let Ok(Some(ack)) = self.client.report_status(&report).await {
            self.process_ack_actions(ack.actions).await;
        }
    }

    /// Processes an incoming SSE event immediately.
    async fn handle_sse_event(&mut self, ev: NodeSseEvent) {
        match ev {
            NodeSseEvent::ConfigChanged { config_revision }
                if config_revision > self.config_revision =>
            {
                let _ = self.sync_config_now().await;
            }
            NodeSseEvent::UserDeltaAvailable { user_revision }
                if user_revision > self.user_revision =>
            {
                let _ = self.sync_users_now().await;
            }
            NodeSseEvent::SwitchCadence { mode } => {
                self.cadence_mode = mode;
            }
            NodeSseEvent::CommandIssued {
                command_id,
                action,
                proxy_username,
            } => {
                self.execute_command(command_id, &action, proxy_username.as_deref())
                    .await;
            }
            _ => {}
        }
    }

    /// Processes actions piggybacked on `200 OK` report acknowledgements.
    async fn process_ack_actions(&mut self, actions: Vec<ReportAckAction>) {
        for act in actions {
            match act {
                ReportAckAction::SwitchCadence { mode } => self.cadence_mode = mode,
                ReportAckAction::SyncConfig { .. } => {
                    let _ = self.sync_config_now().await;
                }
                ReportAckAction::SyncUsers { .. } => {
                    let _ = self.sync_users_now().await;
                }
                ReportAckAction::KickUser {
                    command_id,
                    proxy_username,
                } => {
                    self.execute_command(command_id, "kick_user", Some(&proxy_username))
                        .await;
                }
                ReportAckAction::ReloadSingbox { command_id } => {
                    self.execute_command(command_id, "reload_singbox", None)
                        .await;
                }
            }
        }
    }

    /// Fetches updated compact config and reloads `sing-box` if changed.
    async fn sync_config_now(&mut self) -> Result<()> {
        if let Some(cfg_resp) = self.client.fetch_compact_config().await? {
            self.config_revision = cfg_resp.config_revision;
            if let Some(t) = cfg_resp.template {
                self.template = t;
            }
            if let Some(tls) = cfg_resp.tls_profile {
                self.tls_profile = tls;
            }
            self.apply_and_reload_singbox().await?;
        }
        Ok(())
    }

    /// Fetches incremental user delta and reloads `sing-box`.
    async fn sync_users_now(&mut self) -> Result<()> {
        let sync = self.client.fetch_users(Some(self.user_revision)).await?;
        self.user_revision = apply_user_sync(&mut self.users, sync);
        self.apply_and_reload_singbox().await?;
        Ok(())
    }

    /// Executes an administrative command and sends `/commands/{id}/ack`.
    async fn execute_command(
        &mut self,
        command_id: Uuid,
        action: &str,
        proxy_username: Option<&str>,
    ) {
        let res = match action {
            "kick_user" => {
                if let Some(username) = proxy_username {
                    self.clash_client
                        .kick_user(username)
                        .await
                        .map(|n| format!("closed {n} connections"))
                } else {
                    Ok("no username provided".to_owned())
                }
            }
            "reload_singbox" => self
                .apply_and_reload_singbox()
                .await
                .map(|()| "reloaded sing-box".to_owned()),
            _ => Ok("ignored unknown command".to_owned()),
        };

        match res {
            Ok(msg) => {
                let _ = self.client.ack_command(command_id, true, Some(msg)).await;
            }
            Err(err) => {
                let _ = self
                    .client
                    .ack_command(command_id, false, Some(err.to_string()))
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use aethergate_protocol::UserDeltaEntry;

    use super::*;

    #[test]
    fn delta_sync_upserts_and_revokes_users() {
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();
        let mut users = vec![AuthorizedUserCredential {
            user_id: u1,
            proxy_username: "ag_u1".to_owned(),
            proxy_uuid: u1,
            proxy_password: "p1".to_owned(),
        }];

        let rev = apply_user_sync(
            &mut users,
            NodeUsersSyncResponse {
                mode: UserSyncMode::Delta,
                latest_rev: 5,
                users: vec![],
                deltas: vec![
                    UserDeltaEntry {
                        rev: 4,
                        op: UserDeltaOpKind::Revoke,
                        user_id: u1,
                        proxy_username: "ag_u1".to_owned(),
                        credential: None,
                    },
                    UserDeltaEntry {
                        rev: 5,
                        op: UserDeltaOpKind::Upsert,
                        user_id: u2,
                        proxy_username: "ag_u2".to_owned(),
                        credential: Some(AuthorizedUserCredential {
                            user_id: u2,
                            proxy_username: "ag_u2".to_owned(),
                            proxy_uuid: u2,
                            proxy_password: "p2".to_owned(),
                        }),
                    },
                ],
            },
        );

        assert_eq!(rev, 5);
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].proxy_username, "ag_u2");
    }
}
