//! Panel REST API (`/api/v1/*`) for admin management, 4-protocol node builder,
//! 3D monthly traffic analytics, audit logs, and user self-service portal.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use aethergate_protocol::{
    NodeInboundSpec, NodeSseEvent, ProtocolType, TlsProfile, TransportLayer, validate_inbound_ports,
};

use crate::auth::{AdminUser, AuthenticatedUser};
use crate::crypto::{
    decrypt_secret, encrypt_secret, generate_node_hmac_secret, generate_random_token,
    generate_reality_keypair,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::subscription::{
    RenderedSubscriptionLink, load_authorized_inbounds_for_user, render_inbound_uri,
};
use crate::telemetry::parse_stat_month;

/// Common `?month=YYYY-MM` query parameter for monthly statistics endpoints.
#[derive(Debug, Deserialize)]
pub struct MonthQuery {
    /// Optional calendar month (`YYYY-MM`); defaults to current month in `[stats].timezone`.
    pub month: Option<String>,
}

/// Inserts an audit log entry.
async fn record_audit_log(
    state: &AppState,
    actor_user_id: Uuid,
    action: &str,
    target_type: &str,
    target_id: Option<&str>,
    detail: Value,
) -> AppResult<()> {
    sqlx::query(
        r"
        INSERT INTO audit_logs (id, actor_user_id, action, target_type, target_id, detail)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(Uuid::new_v4())
    .bind(actor_user_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Resolves a logical or physical node ID to its underlying physical host node UUID.
async fn resolve_physical_node_id(state: &AppState, node_id: Uuid) -> AppResult<Uuid> {
    let target: Option<Uuid> =
        sqlx::query_scalar("SELECT COALESCE(parent_node_id, id) FROM nodes WHERE id = $1")
            .bind(node_id)
            .fetch_optional(&state.db)
            .await?;
    target.ok_or_else(|| AppError::NotFound("node not found".to_owned()))
}

/// Maps a list of logical or physical node IDs to distinct physical host node UUIDs.
async fn resolve_physical_node_ids(state: &AppState, node_ids: &[Uuid]) -> AppResult<Vec<Uuid>> {
    let mut visited = HashSet::new();
    let mut out = Vec::new();
    for &nid in node_ids {
        let target = resolve_physical_node_id(state, nid).await?;
        if visited.insert(target) {
            out.push(target);
        }
    }
    Ok(out)
}

/// Records an incremental user delta (`upsert` or `revoke`) on a set of nodes and pushes SSE.
async fn emit_user_deltas(
    state: &AppState,
    node_ids: &[Uuid],
    op: &str,
    user_id: Uuid,
    proxy_username: &str,
) -> AppResult<()> {
    let mut visited = HashSet::new();
    for &node_id in node_ids {
        let (target_id, is_logical): (Uuid, bool) = sqlx::query_as(
            "SELECT COALESCE(parent_node_id, id), (parent_node_id IS NOT NULL) FROM nodes WHERE id = $1",
        )
        .bind(node_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or((node_id, false));

        if !visited.insert(target_id) {
            continue;
        }

        let rev: i64 = sqlx::query_scalar(
            "UPDATE nodes SET user_revision = user_revision + 1, updated_at = NOW() WHERE id = $1 RETURNING user_revision",
        )
        .bind(target_id)
        .fetch_one(&state.db)
        .await?;

        sqlx::query(
            r"
            INSERT INTO node_user_deltas (node_id, rev, op, user_id, proxy_username)
            VALUES ($1, $2, $3, $4, $5)
            ",
        )
        .bind(target_id)
        .bind(rev)
        .bind(op)
        .bind(user_id)
        .bind(proxy_username)
        .execute(&state.db)
        .await?;

        state.push_node_event(
            target_id,
            NodeSseEvent::UserDeltaAvailable { user_revision: rev },
        );
        if is_logical {
            let cfg_rev: i64 = sqlx::query_scalar(
                "UPDATE nodes SET config_revision = config_revision + 1 WHERE id = $1 RETURNING config_revision",
            )
            .bind(target_id)
            .fetch_one(&state.db)
            .await?;
            state.push_node_event(
                target_id,
                NodeSseEvent::ConfigChanged {
                    config_revision: cfg_rev,
                },
            );
        }
    }
    Ok(())
}

/// Resolves all distinct node IDs currently reachable by a user (direct + via node groups).
async fn list_user_granted_node_ids(state: &AppState, user_id: Uuid) -> AppResult<Vec<Uuid>> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        r"
        SELECT DISTINCT node_id FROM (
            SELECT node_id FROM user_node_grants WHERE user_id = $1
            UNION
            SELECT ngm.node_id
            FROM user_node_group_grants ungg
            JOIN node_group_members ngm ON ngm.group_id = ungg.group_id
            WHERE ungg.user_id = $1
        ) AS sub
        ",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(ids)
}

// ---------------------------------------------------------------------------
// Nodes & 4-Protocol Builder
// ---------------------------------------------------------------------------

/// Summary node row returned by `GET /api/v1/nodes`.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NodeSummaryDto {
    /// Node UUID.
    pub id: Uuid,
    /// Optional parent physical node UUID when this is a logical egress node.
    pub parent_node_id: Option<Uuid>,
    /// Node name.
    pub name: String,
    /// Public host/IP.
    pub host: String,
    /// Region label.
    pub region: String,
    /// Status (`online`, `offline`, `degraded`).
    pub status: String,
    /// Tags array JSON.
    pub tags: Value,
    /// Config revision.
    pub config_revision: i64,
    /// User sync revision.
    pub user_revision: i64,
    /// Current reporting cadence.
    pub cadence_mode: String,
    /// CPU utilization percentage.
    pub cpu_pct: f32,
    /// Memory utilization percentage.
    pub mem_pct: f32,
    /// Active connections.
    pub active_connections: i32,
    /// Active users.
    pub active_users: i32,
    /// Uptime in seconds.
    pub uptime_secs: i64,
    /// Last heartbeat timestamp.
    pub last_seen_at: Option<DateTime<Utc>>,
    /// Number of enabled inbounds.
    pub inbound_count: i64,
    /// Current month uplink bytes.
    pub month_uplink_bytes: i64,
    /// Current month downlink bytes.
    pub month_downlink_bytes: i64,
}

/// `GET /api/v1/nodes`
pub async fn list_nodes(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<Vec<NodeSummaryDto>>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;
    let rows = sqlx::query_as::<_, NodeSummaryDto>(
        r"
        SELECT
            n.id, n.parent_node_id, n.name, n.host, n.region, n.status, n.tags,
            n.config_revision, n.user_revision, n.cadence_mode,
            n.cpu_pct, n.mem_pct, n.active_connections, n.active_users,
            n.uptime_secs, n.last_seen_at,
            COALESCE(ic.cnt, 0) AS inbound_count,
            COALESCE(mt.uplink_bytes, 0) AS month_uplink_bytes,
            COALESCE(mt.downlink_bytes, 0) AS month_downlink_bytes
        FROM nodes n
        LEFT JOIN (
            SELECT node_id, COUNT(*)::bigint AS cnt
            FROM node_inbounds
            WHERE enabled = TRUE
            GROUP BY node_id
        ) ic ON ic.node_id = n.id
        LEFT JOIN node_monthly_traffic mt
            ON mt.node_id = n.id AND mt.stat_month = $1
        ORDER BY n.name ASC
        ",
    )
    .bind(stat_month)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

/// Request payload for `POST /api/v1/nodes`.
#[derive(Debug, Deserialize)]
pub struct CreateNodeRequest {
    /// Display name.
    pub name: String,
    /// Public IP or domain.
    pub host: String,
    /// Region tag.
    #[serde(default = "default_region")]
    pub region: String,
    /// Optional metadata tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Shared TLS profile (auto-initialized if omitted).
    pub tls_profile: Option<TlsProfile>,
    /// Optional parent physical node UUID for logical egress nodes.
    pub parent_node_id: Option<Uuid>,
    /// Optional custom outbounds JSON array.
    pub outbounds: Option<Value>,
    /// Optional custom DNS/route JSON object.
    pub dns_route: Option<Value>,
}

/// Default region label.
fn default_region() -> String {
    "default".to_owned()
}

/// Response returned on node creation or secret rotation.
#[derive(Debug, Serialize)]
pub struct NodeBootstrapResponse {
    /// Node UUID.
    pub node_id: Uuid,
    /// Node name.
    pub name: String,
    /// Plaintext HMAC secret for `config.agent.toml`.
    pub hmac_secret: String,
}

/// `POST /api/v1/nodes`
pub async fn create_node(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(req): Json<CreateNodeRequest>,
) -> AppResult<(StatusCode, Json<NodeBootstrapResponse>)> {
    if req.name.trim().is_empty() || req.host.trim().is_empty() {
        return Err(AppError::BadRequest(
            "name and host are required".to_owned(),
        ));
    }

    let node_id = Uuid::new_v4();
    let hmac_secret = generate_node_hmac_secret();
    let encrypted = encrypt_secret(&state.master_key, hmac_secret.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("secret encryption failed: {e}")))?;

    let tls = req.tls_profile.unwrap_or_else(|| TlsProfile {
        mode: "self_signed".to_owned(),
        server_name: req.host.clone(),
        cert_path: "/etc/aethergate/certs/fullchain.pem".to_owned(),
        key_path: "/etc/aethergate/certs/privkey.pem".to_owned(),
    });
    let tls_json = serde_json::to_value(&tls)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("tls serialization failed: {e}")))?;
    let tags_json = serde_json::to_value(&req.tags)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("tags serialization failed: {e}")))?;

    let outbounds_json = req.outbounds.unwrap_or_else(|| json!([]));
    let dns_route_json = req.dns_route.unwrap_or_else(|| json!({}));

    sqlx::query(
        r"
        INSERT INTO nodes (
            id, parent_node_id, name, host, region, tags,
            hmac_secret_enc, tls_profile, outbounds, dns_route
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ",
    )
    .bind(node_id)
    .bind(req.parent_node_id)
    .bind(req.name.trim())
    .bind(req.host.trim())
    .bind(req.region.trim())
    .bind(tags_json)
    .bind(encrypted)
    .bind(tls_json)
    .bind(outbounds_json)
    .bind(dns_route_json)
    .execute(&state.db)
    .await?;

    record_audit_log(
        &state,
        admin.id,
        "node.create",
        "node",
        Some(&node_id.to_string()),
        json!({ "name": req.name, "host": req.host }),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(NodeBootstrapResponse {
            node_id,
            name: req.name,
            hmac_secret,
        }),
    ))
}

/// Detailed node row returned by `GET /api/v1/nodes/{id}`.
#[derive(Debug, Serialize)]
pub struct NodeDetailDto {
    /// Node UUID.
    pub id: Uuid,
    /// Optional parent physical node UUID.
    pub parent_node_id: Option<Uuid>,
    /// Display name.
    pub name: String,
    /// Public host.
    pub host: String,
    /// Region label.
    pub region: String,
    /// Node status.
    pub status: String,
    /// Plaintext HMAC secret for `config.agent.toml`.
    pub hmac_secret: String,
    /// Tags list.
    pub tags: Vec<String>,
    /// Config revision.
    pub config_revision: i64,
    /// User sync revision.
    pub user_revision: i64,
    /// Shared TLS profile.
    pub tls_profile: TlsProfile,
    /// Custom outbounds JSON array.
    pub outbounds: Value,
    /// Custom DNS/route JSON object.
    pub dns_route: Value,
    /// Configured 4-protocol inbounds.
    pub inbounds: Vec<NodeInboundSpec>,
}

/// Internal DB row for `GET /api/v1/nodes/{id}`.
#[derive(Debug, sqlx::FromRow)]
struct NodeRecordRow {
    /// Node UUID.
    id: Uuid,
    /// Optional parent physical node UUID.
    parent_node_id: Option<Uuid>,
    /// Display name.
    name: String,
    /// Public host.
    host: String,
    /// Region.
    region: String,
    /// Status.
    status: String,
    /// Encrypted HMAC secret.
    hmac_secret_enc: Vec<u8>,
    /// Tags JSON.
    tags: Value,
    /// Config rev.
    config_revision: i64,
    /// User rev.
    user_revision: i64,
    /// Shared TLS JSON.
    tls_profile: Value,
    /// Custom outbounds JSON.
    outbounds: Value,
    /// Custom DNS/route JSON.
    dns_route: Value,
}

/// Stored inbound row.
#[derive(Debug, sqlx::FromRow)]
struct InboundRecordRow {
    /// Inbound UUID.
    id: Uuid,
    /// Tag.
    tag: String,
    /// Listen address.
    listen: String,
    /// Listen port.
    listen_port: i32,
    /// Optional port range.
    port_range: Option<String>,
    /// Optional outbound tag.
    outbound_tag: Option<String>,
    /// Protocol config JSON.
    protocol_config: Value,
    /// Enabled flag.
    enabled: bool,
}

/// `GET /api/v1/nodes/{id}`
pub async fn get_node_detail(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(node_id): Path<Uuid>,
) -> AppResult<Json<NodeDetailDto>> {
    let node = sqlx::query_as::<_, NodeRecordRow>(
        r"
        SELECT id, parent_node_id, name, host, region, status, hmac_secret_enc, tags, config_revision, user_revision, tls_profile, outbounds, dns_route
        FROM nodes WHERE id = $1
        ",
    )
    .bind(node_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("node not found".to_owned()))?;

    let db_inbounds = sqlx::query_as::<_, InboundRecordRow>(
        r"
        SELECT id, tag, listen, listen_port, port_range, outbound_tag, protocol_config, enabled
        FROM node_inbounds
        WHERE node_id = $1
        ORDER BY sort_order ASC, listen_port ASC
        ",
    )
    .bind(node_id)
    .fetch_all(&state.db)
    .await?;

    let mut inbounds = Vec::with_capacity(db_inbounds.len());
    for (idx, r) in db_inbounds.into_iter().enumerate() {
        let protocol_config: aethergate_protocol::InboundProtocolConfig =
            serde_json::from_value(r.protocol_config)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid inbound JSON: {e}")))?;
        inbounds.push(NodeInboundSpec {
            id: r.id,
            protocol: protocol_config.protocol_type(),
            tag: r.tag,
            enabled: r.enabled,
            listen: r.listen,
            listen_port: u16::try_from(r.listen_port).unwrap_or(443),
            port_hopping_range: r.port_range,
            protocol_config,
            sort_order: i16::try_from(idx).unwrap_or(0),
            outbound_tag: r.outbound_tag,
            allowed_usernames: None,
        });
    }

    let tls_profile: TlsProfile = serde_json::from_value(node.tls_profile)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid tls_profile JSON: {e}")))?;
    let tags: Vec<String> = serde_json::from_value(node.tags).unwrap_or_default();
    let secret_bytes = decrypt_secret(&state.master_key, &node.hmac_secret_enc)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to decrypt node secret: {e}")))?;
    let hmac_secret = String::from_utf8(secret_bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid node secret utf8: {e}")))?;

    Ok(Json(NodeDetailDto {
        id: node.id,
        parent_node_id: node.parent_node_id,
        name: node.name,
        host: node.host,
        region: node.region,
        status: node.status,
        hmac_secret,
        tags,
        config_revision: node.config_revision,
        user_revision: node.user_revision,
        tls_profile,
        outbounds: node.outbounds,
        dns_route: node.dns_route,
        inbounds,
    }))
}

/// Request payload for `PATCH /api/v1/nodes/{id}`.
#[derive(Debug, Deserialize)]
pub struct UpdateNodeRequest {
    /// Updated name.
    pub name: Option<String>,
    /// Updated host.
    pub host: Option<String>,
    /// Updated region.
    pub region: Option<String>,
    /// Updated tags.
    pub tags: Option<Vec<String>>,
    /// Updated custom outbounds JSON array.
    pub outbounds: Option<Value>,
    /// Updated custom DNS/route JSON object.
    pub dns_route: Option<Value>,
}

/// `PATCH /api/v1/nodes/{id}`
pub async fn update_node(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(node_id): Path<Uuid>,
    Json(req): Json<UpdateNodeRequest>,
) -> AppResult<StatusCode> {
    let tags_json = req
        .tags
        .map(serde_json::to_value)
        .transpose()
        .map_err(|e| AppError::BadRequest(format!("invalid tags: {e}")))?;
    let bump_cfg = req.outbounds.is_some() || req.dns_route.is_some();

    let res = sqlx::query(
        r"
        UPDATE nodes
        SET name = COALESCE($2, name),
            host = COALESCE($3, host),
            region = COALESCE($4, region),
            tags = COALESCE($5, tags),
            outbounds = COALESCE($6, outbounds),
            dns_route = COALESCE($7, dns_route),
            config_revision = CASE WHEN $8 THEN config_revision + 1 ELSE config_revision END,
            updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(node_id)
    .bind(req.name)
    .bind(req.host)
    .bind(req.region)
    .bind(tags_json)
    .bind(req.outbounds)
    .bind(req.dns_route)
    .bind(bump_cfg)
    .execute(&state.db)
    .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("node not found".to_owned()));
    }

    if bump_cfg {
        let target_id = resolve_physical_node_id(&state, node_id).await?;
        let cfg_rev: i64 = sqlx::query_scalar(
            r"
            UPDATE nodes
            SET config_revision = CASE WHEN id = $2 THEN config_revision ELSE config_revision + 1 END,
                updated_at = NOW()
            WHERE id = $1
            RETURNING config_revision
            ",
        )
        .bind(target_id)
        .bind(node_id)
        .fetch_one(&state.db)
        .await?;
        state.push_node_event(
            target_id,
            NodeSseEvent::ConfigChanged {
                config_revision: cfg_rev,
            },
        );
    }

    record_audit_log(
        &state,
        admin.id,
        "node.update",
        "node",
        Some(&node_id.to_string()),
        json!({ "bump_cfg": bump_cfg }),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/nodes/{id}`
pub async fn delete_node(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(node_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let res = sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(node_id)
        .execute(&state.db)
        .await?;

    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("node not found".to_owned()));
    }

    record_audit_log(
        &state,
        admin.id,
        "node.delete",
        "node",
        Some(&node_id.to_string()),
        json!({}),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Request payload for `PUT /api/v1/nodes/{id}/inbounds`.
#[derive(Debug, Deserialize)]
pub struct ReplaceInboundsRequest {
    /// Complete list of 4-protocol inbound specifications for this node.
    pub inbounds: Vec<NodeInboundSpec>,
}

/// Converts a stored database inbound row into a protocol [`NodeInboundSpec`].
fn inbound_record_to_spec(idx: usize, row: InboundRecordRow) -> AppResult<NodeInboundSpec> {
    let protocol_config: aethergate_protocol::InboundProtocolConfig =
        serde_json::from_value(row.protocol_config)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid inbound JSON: {e}")))?;
    Ok(NodeInboundSpec {
        id: row.id,
        protocol: protocol_config.protocol_type(),
        tag: row.tag,
        enabled: row.enabled,
        listen: row.listen,
        listen_port: u16::try_from(row.listen_port).unwrap_or(443),
        port_hopping_range: row.port_range,
        protocol_config,
        sort_order: i16::try_from(idx).unwrap_or(0),
        outbound_tag: row.outbound_tag,
        allowed_usernames: None,
    })
}

/// Loads all sibling inbounds on the same physical host (excluding `node_id`) for port validation.
async fn load_sibling_inbounds(
    state: &AppState,
    node_id: Uuid,
    physical_node_id: Uuid,
) -> AppResult<Vec<NodeInboundSpec>> {
    let sibling_rows = sqlx::query_as::<_, InboundRecordRow>(
        r"
        SELECT ni.id, ni.tag, ni.listen, ni.listen_port, ni.port_range, ni.outbound_tag, ni.protocol_config, ni.enabled
        FROM node_inbounds ni
        JOIN nodes n ON n.id = ni.node_id
        WHERE ni.node_id != $1
          AND (n.id = $2 OR n.parent_node_id = $2)
        ORDER BY ni.sort_order ASC, ni.listen_port ASC
        ",
    )
    .bind(node_id)
    .bind(physical_node_id)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(sibling_rows.len());
    for (idx, row) in sibling_rows.into_iter().enumerate() {
        out.push(inbound_record_to_spec(idx, row)?);
    }
    Ok(out)
}

/// `PUT /api/v1/nodes/{id}/inbounds` - validates TCP/UDP port non-overlap and pushes SSE config update.
pub async fn replace_node_inbounds(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(node_id): Path<Uuid>,
    Json(req): Json<ReplaceInboundsRequest>,
) -> AppResult<Json<Value>> {
    let physical_node_id = resolve_physical_node_id(&state, node_id).await?;
    let mut host_inbounds = load_sibling_inbounds(&state, node_id, physical_node_id).await?;
    host_inbounds.extend(req.inbounds.iter().cloned());
    validate_inbound_ports(&host_inbounds).map_err(|e| AppError::Conflict(e.to_string()))?;

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM node_inbounds WHERE node_id = $1")
        .bind(node_id)
        .execute(&mut *tx)
        .await?;

    for (idx, inbound) in req.inbounds.iter().enumerate() {
        let proto = inbound.protocol_config.protocol_type();
        let proto_str = match proto {
            ProtocolType::VlessReality => "vless_reality",
            ProtocolType::Hysteria2 => "hysteria2",
            ProtocolType::Tuic => "tuic",
            ProtocolType::Anytls => "anytls",
        };
        let transport_str = match proto.transport_layer() {
            TransportLayer::Tcp => "tcp",
            TransportLayer::Udp => "udp",
        };
        let cfg_json = serde_json::to_value(&inbound.protocol_config)
            .map_err(|e| AppError::BadRequest(format!("invalid protocol config: {e}")))?;

        sqlx::query(
            r"
            INSERT INTO node_inbounds (
                id, node_id, tag, protocol, transport, listen, listen_port,
                port_range, outbound_tag, protocol_config, enabled, sort_order
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ",
        )
        .bind(Uuid::new_v4())
        .bind(node_id)
        .bind(&inbound.tag)
        .bind(proto_str)
        .bind(transport_str)
        .bind(&inbound.listen)
        .bind(i32::from(inbound.listen_port))
        .bind(&inbound.port_hopping_range)
        .bind(&inbound.outbound_tag)
        .bind(cfg_json)
        .bind(inbound.enabled)
        .bind(i32::try_from(idx).unwrap_or(0))
        .execute(&mut *tx)
        .await?;
    }

    let physical_node_id: Uuid =
        sqlx::query_scalar("SELECT COALESCE(parent_node_id, id) FROM nodes WHERE id = $1")
            .bind(node_id)
            .fetch_one(&mut *tx)
            .await?;

    let config_revision: i64 = sqlx::query_scalar(
        "UPDATE nodes SET config_revision = config_revision + 1, updated_at = NOW() WHERE id = $1 RETURNING config_revision",
    )
    .bind(physical_node_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    state.push_node_event(
        physical_node_id,
        NodeSseEvent::ConfigChanged { config_revision },
    );
    record_audit_log(
        &state,
        admin.id,
        "node.inbounds.replace",
        "node",
        Some(&node_id.to_string()),
        json!({ "config_revision": config_revision, "count": req.inbounds.len() }),
    )
    .await?;

    Ok(Json(json!({ "config_revision": config_revision })))
}

/// `PUT /api/v1/nodes/{id}/tls-profile`
pub async fn update_node_tls_profile(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(node_id): Path<Uuid>,
    Json(tls): Json<TlsProfile>,
) -> AppResult<Json<Value>> {
    let tls_json = serde_json::to_value(&tls)
        .map_err(|e| AppError::BadRequest(format!("invalid TLS profile: {e}")))?;
    let target_id = resolve_physical_node_id(&state, node_id).await?;

    let config_revision: i64 = sqlx::query_scalar(
        r"
        UPDATE nodes
        SET tls_profile = $2, config_revision = config_revision + 1, updated_at = NOW()
        WHERE id = $1
        RETURNING config_revision
        ",
    )
    .bind(target_id)
    .bind(&tls_json)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        r"
        UPDATE nodes
        SET tls_profile = $2, updated_at = NOW()
        WHERE parent_node_id = $1
        ",
    )
    .bind(target_id)
    .bind(&tls_json)
    .execute(&state.db)
    .await?;

    state.push_node_event(target_id, NodeSseEvent::ConfigChanged { config_revision });
    record_audit_log(
        &state,
        admin.id,
        "node.tls.update",
        "node",
        Some(&target_id.to_string()),
        json!({ "config_revision": config_revision, "requested_node_id": node_id }),
    )
    .await?;

    Ok(Json(json!({ "config_revision": config_revision })))
}

/// `POST /api/v1/nodes/{id}/rotate-secret`
pub async fn rotate_node_secret(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(node_id): Path<Uuid>,
) -> AppResult<Json<NodeBootstrapResponse>> {
    let new_secret = generate_node_hmac_secret();
    let encrypted = encrypt_secret(&state.master_key, new_secret.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("encryption failed: {e}")))?;

    let name: String = sqlx::query_scalar(
        "UPDATE nodes SET hmac_secret_enc = $2, updated_at = NOW() WHERE id = $1 RETURNING name",
    )
    .bind(node_id)
    .bind(encrypted)
    .fetch_one(&state.db)
    .await?;

    record_audit_log(
        &state,
        admin.id,
        "node.secret.rotate",
        "node",
        Some(&node_id.to_string()),
        json!({}),
    )
    .await?;

    Ok(Json(NodeBootstrapResponse {
        node_id,
        name,
        hmac_secret: new_secret,
    }))
}

/// Request payload for `POST /api/v1/nodes/{id}/commands`.
#[derive(Debug, Deserialize)]
pub struct IssueNodeCommandRequest {
    /// Action (`"reload_singbox"` or `"kick_user"`).
    pub action: String,
    /// Optional `proxy_username` when `action == "kick_user"`.
    pub proxy_username: Option<String>,
}

/// `POST /api/v1/nodes/{id}/commands`
pub async fn issue_node_command(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(node_id): Path<Uuid>,
    Json(req): Json<IssueNodeCommandRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    let target_id = resolve_physical_node_id(&state, node_id).await?;
    let command_id = Uuid::new_v4();
    let payload = json!({ "proxy_username": req.proxy_username });

    sqlx::query("INSERT INTO node_commands (id, node_id, action, payload) VALUES ($1, $2, $3, $4)")
        .bind(command_id)
        .bind(target_id)
        .bind(&req.action)
        .bind(payload)
        .execute(&state.db)
        .await?;

    state.push_node_event(
        target_id,
        NodeSseEvent::CommandIssued {
            command_id,
            action: req.action.clone(),
            proxy_username: req.proxy_username.clone(),
        },
    );

    record_audit_log(
        &state,
        admin.id,
        "node.command.issue",
        "node",
        Some(&target_id.to_string()),
        json!({
            "command_id": command_id,
            "action": req.action,
            "requested_node_id": node_id
        }),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "command_id": command_id })),
    ))
}

/// `POST /api/v1/nodes/reality-keypair` - generates X25519 keys for VLESS Reality.
pub async fn generate_reality_keypair_endpoint(AdminUser(_admin): AdminUser) -> Json<Value> {
    let (private_key, public_key) = generate_reality_keypair();
    Json(json!({
        "private_key": private_key,
        "public_key": public_key
    }))
}

// ---------------------------------------------------------------------------
// Node Groups
// ---------------------------------------------------------------------------

/// Node group summary DTO.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NodeGroupDto {
    /// Group UUID.
    pub id: Uuid,
    /// Group name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Member count.
    pub member_count: i64,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// `GET /api/v1/node-groups`
pub async fn list_node_groups(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<Vec<NodeGroupDto>>> {
    let rows = sqlx::query_as::<_, NodeGroupDto>(
        r"
        SELECT
            g.id, g.name, g.description, g.created_at,
            COUNT(m.node_id)::bigint AS member_count
        FROM node_groups g
        LEFT JOIN node_group_members m ON m.group_id = g.id
        GROUP BY g.id
        ORDER BY g.name ASC
        ",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Request payload for `POST /api/v1/node-groups`.
#[derive(Debug, Deserialize)]
pub struct CreateNodeGroupRequest {
    /// Group name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// Initial member node IDs.
    #[serde(default)]
    pub node_ids: Vec<Uuid>,
}

/// `POST /api/v1/node-groups`
pub async fn create_node_group(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(req): Json<CreateNodeGroupRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("group name is required".to_owned()));
    }
    let group_id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO node_groups (id, name, description) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(req.name.trim())
        .bind(req.description.trim())
        .execute(&mut *tx)
        .await?;

    for nid in &req.node_ids {
        sqlx::query("INSERT INTO node_group_members (group_id, node_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(group_id)
            .bind(nid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    record_audit_log(
        &state,
        admin.id,
        "node_group.create",
        "node_group",
        Some(&group_id.to_string()),
        json!({ "name": req.name }),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "id": group_id }))))
}

/// Request payload for `PUT /api/v1/node-groups/{id}/members`.
#[derive(Debug, Deserialize)]
pub struct ReplaceGroupMembersRequest {
    /// Complete list of member node IDs.
    pub node_ids: Vec<Uuid>,
}

/// `PUT /api/v1/node-groups/{id}/members`
pub async fn replace_node_group_members(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(group_id): Path<Uuid>,
    Json(req): Json<ReplaceGroupMembersRequest>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM node_group_members WHERE group_id = $1")
        .bind(group_id)
        .execute(&mut *tx)
        .await?;
    for nid in &req.node_ids {
        sqlx::query("INSERT INTO node_group_members (group_id, node_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(group_id)
            .bind(nid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    // Bump user_revision on all nodes so affected users sync cleanly.
    let all_nodes: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM nodes")
        .fetch_all(&state.db)
        .await?;
    for nid in all_nodes {
        let rev: i64 = sqlx::query_scalar(
            "UPDATE nodes SET user_revision = user_revision + 1 WHERE id = $1 RETURNING user_revision",
        )
        .bind(nid)
        .fetch_one(&state.db)
        .await?;
        state.push_node_event(nid, NodeSseEvent::UserDeltaAvailable { user_revision: rev });
    }

    record_audit_log(
        &state,
        admin.id,
        "node_group.members.replace",
        "node_group",
        Some(&group_id.to_string()),
        json!({ "node_count": req.node_ids.len() }),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/node-groups/{id}`
pub async fn delete_node_group(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(group_id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let res = sqlx::query("DELETE FROM node_groups WHERE id = $1")
        .bind(group_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("node group not found".to_owned()));
    }
    record_audit_log(
        &state,
        admin.id,
        "node_group.delete",
        "node_group",
        Some(&group_id.to_string()),
        json!({}),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Users & Access Grants
// ---------------------------------------------------------------------------

/// User summary row returned by `GET /api/v1/users`.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserSummaryDto {
    /// User UUID.
    pub id: Uuid,
    /// Display name.
    pub display_name: String,
    /// Email.
    pub email: Option<String>,
    /// Optional OIDC avatar URL.
    pub avatar_url: Option<String>,
    /// Role (`admin` or `user`).
    pub role: String,
    /// Account type (`user` or `relay`).
    pub account_type: String,
    /// Panel status (`active` or `disabled`).
    pub panel_status: String,
    /// Whether proxy access is approved.
    pub proxy_enabled: bool,
    /// Immutable proxy username.
    pub proxy_username: String,
    /// Direct + group granted node count.
    pub granted_node_count: i64,
    /// Current month uplink bytes.
    pub month_uplink_bytes: i64,
    /// Current month downlink bytes.
    pub month_downlink_bytes: i64,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Request payload for `POST /api/v1/users` (creates a relay service account or legacy credential bridge user).
#[derive(Debug, Deserialize)]
pub struct AdminCreateUserRequest {
    /// Display name (e.g., `"relay_cn_sha_02"` or `"legacy_jp_tuic"`).
    pub display_name: String,
    /// Account type: `"user"` (default) or `"relay"` (excluded from `user_monthly_traffic` double-counting).
    #[serde(default = "default_account_type")]
    pub account_type: String,
    /// Optional explicit proxy username (auto-generated if omitted).
    pub proxy_username: Option<String>,
    /// Optional explicit proxy UUID (used for zero-downtime legacy credential bridging).
    pub proxy_uuid: Option<Uuid>,
    /// Optional explicit proxy password (used for zero-downtime legacy credential bridging).
    pub proxy_password: Option<String>,
    /// Initial granted node IDs.
    #[serde(default)]
    pub node_ids: Vec<Uuid>,
}

/// Default account type.
fn default_account_type() -> String {
    "user".to_owned()
}

/// `POST /api/v1/users` - admin creates a relay service account or legacy migration user.
pub async fn admin_create_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(req): Json<AdminCreateUserRequest>,
) -> AppResult<(StatusCode, Json<Value>)> {
    if req.display_name.trim().is_empty() {
        return Err(AppError::BadRequest("display_name is required".to_owned()));
    }
    let account_type = if req.account_type == "relay" {
        "relay"
    } else {
        "user"
    };
    let user_id = Uuid::new_v4();
    let short_id = user_id.simple().to_string();
    let proxy_username = req
        .proxy_username
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if account_type == "relay" {
                format!("relay_{}", &short_id[..10])
            } else {
                format!("ag_{}", &short_id[..12])
            }
        });
    let proxy_uuid = req.proxy_uuid.unwrap_or_else(Uuid::new_v4);
    let proxy_password = req
        .proxy_password
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| generate_random_token(24));
    let sub_token = generate_random_token(24);
    let oidc_subject = format!("internal:{user_id}");

    sqlx::query(
        r"
        INSERT INTO users (
            id, oidc_subject, display_name, role, account_type, panel_status,
            proxy_enabled, proxy_username, proxy_uuid, proxy_password, sub_token
        )
        VALUES ($1, $2, $3, 'user', $4, 'active', TRUE, $5, $6, $7, $8)
        ",
    )
    .bind(user_id)
    .bind(&oidc_subject)
    .bind(req.display_name.trim())
    .bind(account_type)
    .bind(&proxy_username)
    .bind(proxy_uuid)
    .bind(&proxy_password)
    .bind(&sub_token)
    .execute(&state.db)
    .await?;

    for nid in &req.node_ids {
        sqlx::query(
            "INSERT INTO user_node_grants (user_id, node_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(nid)
        .execute(&state.db)
        .await?;
    }

    if !req.node_ids.is_empty() {
        emit_user_deltas(&state, &req.node_ids, "upsert", user_id, &proxy_username).await?;
    }

    record_audit_log(
        &state,
        admin.id,
        "user.admin_create",
        "user",
        Some(&user_id.to_string()),
        json!({ "account_type": account_type, "proxy_username": proxy_username }),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": user_id,
            "proxy_username": proxy_username,
            "proxy_uuid": proxy_uuid,
            "proxy_password": proxy_password,
            "account_type": account_type
        })),
    ))
}

/// `GET /api/v1/users`
pub async fn list_users(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<Vec<UserSummaryDto>>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;
    let rows = sqlx::query_as::<_, UserSummaryDto>(
        r"
        SELECT
            u.id, u.display_name, u.email, u.avatar_url, u.role, u.account_type, u.panel_status,
            u.proxy_enabled, u.proxy_username, u.created_at,
            COALESCE((
                SELECT COUNT(DISTINCT node_id)::bigint FROM (
                    SELECT node_id FROM user_node_grants ung WHERE ung.user_id = u.id
                    UNION
                    SELECT ngm.node_id
                    FROM user_node_group_grants ungg
                    JOIN node_group_members ngm ON ngm.group_id = ungg.group_id
                    WHERE ungg.user_id = u.id
                ) g
            ), 0) AS granted_node_count,
            COALESCE(umt.uplink_bytes, 0) AS month_uplink_bytes,
            COALESCE(umt.downlink_bytes, 0) AS month_downlink_bytes
        FROM users u
        LEFT JOIN user_monthly_traffic umt
            ON umt.user_id = u.id AND umt.stat_month = $1
        ORDER BY u.created_at DESC
        ",
    )
    .bind(stat_month)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

/// Detailed user view with explicit node and group grant IDs.
#[derive(Debug, Serialize)]
pub struct UserDetailDto {
    /// User UUID.
    pub id: Uuid,
    /// Display name.
    pub display_name: String,
    /// Email.
    pub email: Option<String>,
    /// Optional OIDC avatar URL.
    pub avatar_url: Option<String>,
    /// Role.
    pub role: String,
    /// Panel status.
    pub panel_status: String,
    /// Proxy enabled flag.
    pub proxy_enabled: bool,
    /// Immutable proxy username.
    pub proxy_username: String,
    /// Proxy UUID.
    pub proxy_uuid: Uuid,
    /// Subscription token.
    pub sub_token: String,
    /// Directly granted node IDs.
    pub node_ids: Vec<Uuid>,
    /// Granted node group IDs.
    pub group_ids: Vec<Uuid>,
}

/// Internal DB row for `GET /api/v1/users/{id}`.
#[derive(Debug, sqlx::FromRow)]
struct UserRecordRow {
    /// User UUID.
    id: Uuid,
    /// Display name.
    display_name: String,
    /// Email.
    email: Option<String>,
    /// Optional OIDC avatar URL.
    avatar_url: Option<String>,
    /// Role.
    role: String,
    /// Panel status.
    panel_status: String,
    /// Proxy enabled.
    proxy_enabled: bool,
    /// Proxy username.
    proxy_username: String,
    /// Proxy UUID.
    proxy_uuid: Uuid,
    /// Subscription token.
    sub_token: String,
}

/// `GET /api/v1/users/{id}`
pub async fn get_user_detail(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<UserDetailDto>> {
    let u = sqlx::query_as::<_, UserRecordRow>(
        r"
        SELECT id, display_name, email, avatar_url, role, panel_status, proxy_enabled,
               proxy_username, proxy_uuid, sub_token
        FROM users WHERE id = $1
        ",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("user not found".to_owned()))?;

    let node_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT node_id FROM user_node_grants WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;
    let group_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT group_id FROM user_node_group_grants WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;

    Ok(Json(UserDetailDto {
        id: u.id,
        display_name: u.display_name,
        email: u.email,
        avatar_url: u.avatar_url,
        role: u.role,
        panel_status: u.panel_status,
        proxy_enabled: u.proxy_enabled,
        proxy_username: u.proxy_username,
        proxy_uuid: u.proxy_uuid,
        sub_token: u.sub_token,
        node_ids,
        group_ids,
    }))
}

/// Request payload for `PATCH /api/v1/users/{id}`.
#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    /// Updated display name.
    pub display_name: Option<String>,
    /// Updated role (`admin` or `user`).
    pub role: Option<String>,
    /// Updated panel status (`active` or `disabled`).
    pub panel_status: Option<String>,
    /// Updated proxy access flag.
    pub proxy_enabled: Option<bool>,
}

/// `PATCH /api/v1/users/{id}`
pub async fn update_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(user_id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> AppResult<StatusCode> {
    let row = sqlx::query_as::<_, (String, bool, String)>(
        r"
        UPDATE users
        SET display_name = COALESCE($2, display_name),
            role = COALESCE($3, role),
            panel_status = COALESCE($4, panel_status),
            proxy_enabled = COALESCE($5, proxy_enabled),
            updated_at = NOW()
        WHERE id = $1
        RETURNING proxy_username, (proxy_enabled AND panel_status = 'active'), panel_status
        ",
    )
    .bind(user_id)
    .bind(req.display_name)
    .bind(req.role)
    .bind(req.panel_status)
    .bind(req.proxy_enabled)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("user not found".to_owned()))?;

    let (proxy_username, is_active_proxy, _) = row;
    let granted_nodes = list_user_granted_node_ids(&state, user_id).await?;
    let op = if is_active_proxy { "upsert" } else { "revoke" };
    emit_user_deltas(&state, &granted_nodes, op, user_id, &proxy_username).await?;

    record_audit_log(
        &state,
        admin.id,
        "user.update",
        "user",
        Some(&user_id.to_string()),
        json!({ "op": op }),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Request payload for `POST /api/v1/users/{id}/approve`.
#[derive(Debug, Deserialize)]
pub struct ApproveUserRequest {
    /// Initial direct node grants.
    #[serde(default)]
    pub node_ids: Vec<Uuid>,
    /// Initial node group grants.
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
}

/// `POST /api/v1/users/{id}/approve`
pub async fn approve_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(user_id): Path<Uuid>,
    Json(req): Json<ApproveUserRequest>,
) -> AppResult<StatusCode> {
    let proxy_username: String = sqlx::query_scalar(
        "UPDATE users SET proxy_enabled = TRUE, panel_status = 'active', updated_at = NOW() WHERE id = $1 RETURNING proxy_username",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("user not found".to_owned()))?;

    for nid in &req.node_ids {
        sqlx::query("INSERT INTO user_node_grants (user_id, node_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(nid)
            .execute(&state.db)
            .await?;
    }
    for gid in &req.group_ids {
        sqlx::query("INSERT INTO user_node_group_grants (user_id, group_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(gid)
            .execute(&state.db)
            .await?;
    }

    let granted_nodes = list_user_granted_node_ids(&state, user_id).await?;
    emit_user_deltas(&state, &granted_nodes, "upsert", user_id, &proxy_username).await?;

    record_audit_log(
        &state,
        admin.id,
        "user.approve",
        "user",
        Some(&user_id.to_string()),
        json!({ "node_ids": req.node_ids, "group_ids": req.group_ids }),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Request payload for `PUT /api/v1/users/{id}/grants`.
#[derive(Debug, Deserialize)]
pub struct ReplaceUserGrantsRequest {
    /// New direct node IDs.
    #[serde(default)]
    pub node_ids: Vec<Uuid>,
    /// New node group IDs.
    #[serde(default)]
    pub group_ids: Vec<Uuid>,
}

/// Bumps `config_revision` and pushes `ConfigChanged` once per parent when child logical grants change.
async fn bump_parent_configs_for_changed_logical_nodes(
    state: &AppState,
    changed_node_ids: &[Uuid],
) -> AppResult<()> {
    if changed_node_ids.is_empty() {
        return Ok(());
    }
    let parent_ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT DISTINCT parent_node_id FROM nodes WHERE id = ANY($1) AND parent_node_id IS NOT NULL",
    )
    .bind(changed_node_ids)
    .fetch_all(&state.db)
    .await?;

    for parent_id in parent_ids {
        let cfg_rev: i64 = sqlx::query_scalar(
            "UPDATE nodes SET config_revision = config_revision + 1, updated_at = NOW() WHERE id = $1 RETURNING config_revision",
        )
        .bind(parent_id)
        .fetch_one(&state.db)
        .await?;
        state.push_node_event(
            parent_id,
            NodeSseEvent::ConfigChanged {
                config_revision: cfg_rev,
            },
        );
    }
    Ok(())
}

/// `PUT /api/v1/users/{id}/grants`
pub async fn replace_user_grants(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(user_id): Path<Uuid>,
    Json(req): Json<ReplaceUserGrantsRequest>,
) -> AppResult<StatusCode> {
    let (proxy_username, proxy_active) = sqlx::query_as::<_, (String, bool)>(
        "SELECT proxy_username, (proxy_enabled AND panel_status = 'active') FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("user not found".to_owned()))?;

    let old_node_list = list_user_granted_node_ids(&state, user_id).await?;
    let old_nodes: HashSet<Uuid> = old_node_list.iter().copied().collect();
    let old_physical: HashSet<Uuid> = resolve_physical_node_ids(&state, &old_node_list)
        .await?
        .into_iter()
        .collect();

    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM user_node_grants WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_node_group_grants WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    for nid in &req.node_ids {
        sqlx::query("INSERT INTO user_node_grants (user_id, node_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(nid)
            .execute(&mut *tx)
            .await?;
    }
    for gid in &req.group_ids {
        sqlx::query("INSERT INTO user_node_group_grants (user_id, group_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let new_node_list = list_user_granted_node_ids(&state, user_id).await?;
    let new_nodes: HashSet<Uuid> = new_node_list.iter().copied().collect();
    let new_physical: HashSet<Uuid> = resolve_physical_node_ids(&state, &new_node_list)
        .await?
        .into_iter()
        .collect();

    let removed_physical: Vec<Uuid> = old_physical.difference(&new_physical).copied().collect();
    emit_user_deltas(
        &state,
        &removed_physical,
        "revoke",
        user_id,
        &proxy_username,
    )
    .await?;

    if proxy_active {
        let active_physical: Vec<Uuid> = new_physical.iter().copied().collect();
        emit_user_deltas(&state, &active_physical, "upsert", user_id, &proxy_username).await?;
    }

    let changed_nodes: Vec<Uuid> = old_nodes
        .symmetric_difference(&new_nodes)
        .copied()
        .collect();
    bump_parent_configs_for_changed_logical_nodes(&state, &changed_nodes).await?;

    record_audit_log(
        &state,
        admin.id,
        "user.grants.replace",
        "user",
        Some(&user_id.to_string()),
        json!({ "node_ids": req.node_ids, "group_ids": req.group_ids }),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/users/{id}/reset-sub-token`
pub async fn reset_user_sub_token(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let new_token = generate_random_token(24);
    sqlx::query("UPDATE users SET sub_token = $2, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .bind(&new_token)
        .execute(&state.db)
        .await?;

    record_audit_log(
        &state,
        admin.id,
        "user.sub_token.reset",
        "user",
        Some(&user_id.to_string()),
        json!({}),
    )
    .await?;

    Ok(Json(json!({ "sub_token": new_token })))
}

/// `POST /api/v1/users/{id}/kick` - kicks user's live proxy connections across all granted nodes.
pub async fn kick_user_connections(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(user_id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let proxy_username: String =
        sqlx::query_scalar("SELECT proxy_username FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("user not found".to_owned()))?;

    let granted_ids = list_user_granted_node_ids(&state, user_id).await?;
    let node_ids = resolve_physical_node_ids(&state, &granted_ids).await?;
    for &nid in &node_ids {
        let cmd_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO node_commands (id, node_id, action, payload) VALUES ($1, $2, 'kick_user', $3)",
        )
        .bind(cmd_id)
        .bind(nid)
        .bind(json!({ "proxy_username": proxy_username }))
        .execute(&state.db)
        .await?;

        state.push_node_event(
            nid,
            NodeSseEvent::CommandIssued {
                command_id: cmd_id,
                action: "kick_user".to_owned(),
                proxy_username: Some(proxy_username.clone()),
            },
        );
    }

    record_audit_log(
        &state,
        admin.id,
        "user.kick",
        "user",
        Some(&user_id.to_string()),
        json!({ "node_count": node_ids.len() }),
    )
    .await?;

    Ok(Json(json!({ "dispatched_nodes": node_ids.len() })))
}

// ---------------------------------------------------------------------------
// 3D Monthly Analytics & Audit Logs
// ---------------------------------------------------------------------------

/// Overview KPIs for `GET /api/v1/analytics/overview`.
#[derive(Debug, Serialize)]
pub struct AnalyticsOverviewDto {
    /// Selected calendar month (`YYYY-MM-01`).
    pub stat_month: NaiveDate,
    /// Total monthly uplink bytes across all nodes.
    pub total_uplink_bytes: i64,
    /// Total monthly downlink bytes across all nodes.
    pub total_downlink_bytes: i64,
    /// Online node count.
    pub online_nodes: i64,
    /// Total node count.
    pub total_nodes: i64,
    /// Total active proxy-enabled users.
    pub active_proxy_users: i64,
    /// Pending approval users (`proxy_enabled = FALSE`).
    pub pending_approval_users: i64,
    /// Current aggregate active connections across online nodes.
    pub live_connections: i64,
}

/// `GET /api/v1/analytics/overview`
pub async fn get_analytics_overview(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<AnalyticsOverviewDto>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;

    let (total_uplink_bytes, total_downlink_bytes) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT COALESCE(SUM(uplink_bytes), 0)::bigint, COALESCE(SUM(downlink_bytes), 0)::bigint FROM node_monthly_traffic WHERE stat_month = $1",
    )
    .bind(stat_month)
    .fetch_one(&state.db)
    .await?;

    let (online_nodes, total_nodes, live_connections) = sqlx::query_as::<_, (i64, i64, i64)>(
        r"
        SELECT
            COUNT(*) FILTER (WHERE status = 'online')::bigint,
            COUNT(*)::bigint,
            COALESCE(SUM(active_connections) FILTER (WHERE status = 'online'), 0)::bigint
        FROM nodes
        ",
    )
    .fetch_one(&state.db)
    .await?;

    let (active_proxy_users, pending_approval_users) = sqlx::query_as::<_, (i64, i64)>(
        r"
        SELECT
            COUNT(*) FILTER (WHERE proxy_enabled = TRUE AND panel_status = 'active' AND account_type = 'user')::bigint,
            COUNT(*) FILTER (WHERE proxy_enabled = FALSE AND panel_status = 'active' AND role = 'user' AND account_type = 'user')::bigint
        FROM users
        ",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(AnalyticsOverviewDto {
        stat_month,
        total_uplink_bytes,
        total_downlink_bytes,
        online_nodes,
        total_nodes,
        active_proxy_users,
        pending_approval_users,
        live_connections,
    }))
}

/// Monthly user traffic row.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserMonthlyTrafficDto {
    /// User UUID.
    pub user_id: Uuid,
    /// Display name.
    pub display_name: String,
    /// Optional OIDC email address.
    pub email: Option<String>,
    /// Optional OIDC avatar URL.
    pub avatar_url: Option<String>,
    /// Immutable proxy username.
    pub proxy_username: String,
    /// Monthly uplink bytes.
    pub uplink_bytes: i64,
    /// Monthly downlink bytes.
    pub downlink_bytes: i64,
    /// Peak uplink bps.
    pub peak_uplink_bps: i64,
    /// Peak downlink bps.
    pub peak_downlink_bps: i64,
}

/// `GET /api/v1/analytics/users`
pub async fn get_analytics_users(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<Vec<UserMonthlyTrafficDto>>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;
    let rows = sqlx::query_as::<_, UserMonthlyTrafficDto>(
        r"
        SELECT
            u.id AS user_id, u.display_name, u.email, u.avatar_url, u.proxy_username,
            COALESCE(t.uplink_bytes, 0) AS uplink_bytes,
            COALESCE(t.downlink_bytes, 0) AS downlink_bytes,
            COALESCE(t.peak_uplink_bps, 0) AS peak_uplink_bps,
            COALESCE(t.peak_downlink_bps, 0) AS peak_downlink_bps
        FROM users u
        LEFT JOIN user_monthly_traffic t ON t.user_id = u.id AND t.stat_month = $1
        WHERE u.account_type = 'user'
        ORDER BY (COALESCE(t.uplink_bytes, 0) + COALESCE(t.downlink_bytes, 0)) DESC
        ",
    )
    .bind(stat_month)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Monthly node traffic row.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NodeMonthlyTrafficDto {
    /// Node UUID.
    pub node_id: Uuid,
    /// Node name.
    pub node_name: String,
    /// Region.
    pub region: String,
    /// Status.
    pub status: String,
    /// Monthly uplink bytes.
    pub uplink_bytes: i64,
    /// Monthly downlink bytes.
    pub downlink_bytes: i64,
    /// Peak uplink bps.
    pub peak_uplink_bps: i64,
    /// Peak downlink bps.
    pub peak_downlink_bps: i64,
}

/// `GET /api/v1/analytics/nodes`
pub async fn get_analytics_nodes(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<Vec<NodeMonthlyTrafficDto>>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;
    let rows = sqlx::query_as::<_, NodeMonthlyTrafficDto>(
        r"
        SELECT
            n.id AS node_id, n.name AS node_name, n.region, n.status,
            COALESCE(t.uplink_bytes, 0) AS uplink_bytes,
            COALESCE(t.downlink_bytes, 0) AS downlink_bytes,
            COALESCE(t.peak_uplink_bps, 0) AS peak_uplink_bps,
            COALESCE(t.peak_downlink_bps, 0) AS peak_downlink_bps
        FROM nodes n
        LEFT JOIN node_monthly_traffic t ON t.node_id = n.id AND t.stat_month = $1
        ORDER BY (COALESCE(t.uplink_bytes, 0) + COALESCE(t.downlink_bytes, 0)) DESC
        ",
    )
    .bind(stat_month)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// User × Node 3D matrix cell DTO.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserNodeMatrixDto {
    /// User UUID.
    pub user_id: Uuid,
    /// User display name.
    pub display_name: String,
    /// Proxy username.
    pub proxy_username: String,
    /// Node UUID.
    pub node_id: Uuid,
    /// Node name.
    pub node_name: String,
    /// Monthly uplink bytes.
    pub uplink_bytes: i64,
    /// Monthly downlink bytes.
    pub downlink_bytes: i64,
    /// Peak uplink bps.
    pub peak_uplink_bps: i64,
    /// Peak downlink bps.
    pub peak_downlink_bps: i64,
}

/// `GET /api/v1/analytics/matrix`
pub async fn get_analytics_matrix(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<Vec<UserNodeMatrixDto>>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;
    let rows = sqlx::query_as::<_, UserNodeMatrixDto>(
        r"
        SELECT
            m.user_id, u.display_name, u.proxy_username,
            m.node_id, n.name AS node_name,
            m.uplink_bytes, m.downlink_bytes,
            m.peak_uplink_bps, m.peak_downlink_bps
        FROM user_node_monthly_traffic m
        JOIN users u ON u.id = m.user_id
        JOIN nodes n ON n.id = m.node_id
        WHERE m.stat_month = $1
        ORDER BY (m.uplink_bytes + m.downlink_bytes) DESC
        ",
    )
    .bind(stat_month)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Protocol breakdown DTO.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProtocolMonthlyDto {
    /// Protocol name (`vless_reality`, `hysteria2`, `tuic`, `anytls`).
    pub protocol: String,
    /// Monthly uplink bytes.
    pub uplink_bytes: i64,
    /// Monthly downlink bytes.
    pub downlink_bytes: i64,
}

/// `GET /api/v1/analytics/protocols`
pub async fn get_analytics_protocols(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<Vec<ProtocolMonthlyDto>>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;
    let rows = sqlx::query_as::<_, ProtocolMonthlyDto>(
        r"
        SELECT
            protocol,
            COALESCE(SUM(uplink_bytes), 0)::bigint AS uplink_bytes,
            COALESCE(SUM(downlink_bytes), 0)::bigint AS downlink_bytes
        FROM node_inbound_monthly_traffic
        WHERE stat_month = $1
        GROUP BY protocol
        ORDER BY (SUM(uplink_bytes) + SUM(downlink_bytes)) DESC
        ",
    )
    .bind(stat_month)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Query params for `GET /api/v1/analytics/timeseries`.
#[derive(Debug, Deserialize)]
pub struct TimeSeriesQuery {
    /// Lookback window in hours (default 24, max 720).
    #[serde(default = "default_hours")]
    pub hours: i64,
}

/// Default lookback window.
const fn default_hours() -> i64 {
    24
}

/// Hourly time-series bucket DTO.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TimeSeriesBucketDto {
    /// Bucket timestamp (UTC).
    pub bucket: DateTime<Utc>,
    /// Total uplink bytes in this bucket.
    pub uplink_bytes: i64,
    /// Total downlink bytes in this bucket.
    pub downlink_bytes: i64,
}

/// `GET /api/v1/analytics/timeseries`
pub async fn get_analytics_timeseries(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<TimeSeriesQuery>,
) -> AppResult<Json<Vec<TimeSeriesBucketDto>>> {
    let hours = query.hours.clamp(1, 720);
    let rows = sqlx::query_as::<_, TimeSeriesBucketDto>(
        r"
        SELECT
            date_trunc('hour', time) AS bucket,
            COALESCE(SUM(uplink_bytes), 0)::bigint AS uplink_bytes,
            COALESCE(SUM(downlink_bytes), 0)::bigint AS downlink_bytes
        FROM traffic_samples_raw
        WHERE time >= NOW() - ($1 * INTERVAL '1 hour')
        GROUP BY bucket
        ORDER BY bucket ASC
        ",
    )
    .bind(hours)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

/// Audit log row DTO.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditLogDto {
    /// Log UUID.
    pub id: Uuid,
    /// Actor user UUID.
    pub actor_user_id: Option<Uuid>,
    /// Actor display name.
    pub actor_name: Option<String>,
    /// Action name.
    pub action: String,
    /// Target type.
    pub target_type: String,
    /// Target ID string.
    pub target_id: Option<String>,
    /// Detail JSON.
    pub detail: Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// `GET /api/v1/audit-logs`
pub async fn list_audit_logs(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<Vec<AuditLogDto>>> {
    let rows = sqlx::query_as::<_, AuditLogDto>(
        r"
        SELECT
            a.id, a.actor_user_id, u.display_name AS actor_name,
            a.action, a.target_type, a.target_id, a.detail, a.created_at
        FROM audit_logs a
        LEFT JOIN users u ON u.id = a.actor_user_id
        ORDER BY a.created_at DESC
        LIMIT 200
        ",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// User Self-Service Portal
// ---------------------------------------------------------------------------

/// Portal summary DTO for authenticated end users.
#[derive(Debug, Serialize)]
pub struct PortalSummaryDto {
    /// Selected calendar month.
    pub stat_month: NaiveDate,
    /// Whether proxy access is approved by admin.
    pub proxy_enabled: bool,
    /// Immutable proxy username.
    pub proxy_username: String,
    /// Monthly uplink bytes for this user.
    pub uplink_bytes: i64,
    /// Monthly downlink bytes for this user.
    pub downlink_bytes: i64,
    /// Peak uplink bps.
    pub peak_uplink_bps: i64,
    /// Peak downlink bps.
    pub peak_downlink_bps: i64,
    /// Full subscription URL (`/sub/{token}`).
    pub subscription_url: String,
    /// Per-node monthly breakdown for this user.
    pub node_usage: Vec<PortalNodeUsageDto>,
}

/// Per-node usage row for a user.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PortalNodeUsageDto {
    /// Node UUID.
    pub node_id: Uuid,
    /// Node name.
    pub node_name: String,
    /// Node region.
    pub region: String,
    /// Node status.
    pub status: String,
    /// Monthly uplink bytes on this node.
    pub uplink_bytes: i64,
    /// Monthly downlink bytes on this node.
    pub downlink_bytes: i64,
}

/// `GET /api/v1/portal/summary`
pub async fn get_portal_summary(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
    Query(query): Query<MonthQuery>,
) -> AppResult<Json<PortalSummaryDto>> {
    let stat_month = parse_stat_month(query.month.as_deref(), state.stats_tz)?;

    let (uplink_bytes, downlink_bytes, peak_uplink_bps, peak_downlink_bps) =
        sqlx::query_as::<_, (i64, i64, i64, i64)>(
            r"
            SELECT
                COALESCE(uplink_bytes, 0),
                COALESCE(downlink_bytes, 0),
                COALESCE(peak_uplink_bps, 0),
                COALESCE(peak_downlink_bps, 0)
            FROM user_monthly_traffic
            WHERE stat_month = $1 AND user_id = $2
            ",
        )
        .bind(stat_month)
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or((0, 0, 0, 0));

    let node_usage = sqlx::query_as::<_, PortalNodeUsageDto>(
        r"
        SELECT
            n.id AS node_id,
            n.name AS node_name,
            n.region,
            n.status,
            COALESCE(unm.uplink_bytes, 0) AS uplink_bytes,
            COALESCE(unm.downlink_bytes, 0) AS downlink_bytes
        FROM nodes n
        LEFT JOIN user_node_monthly_traffic unm
            ON unm.node_id = n.id AND unm.user_id = $1 AND unm.stat_month = $2
        WHERE n.id IN (
            SELECT node_id FROM user_node_grants WHERE user_id = $1
            UNION
            SELECT ngm.node_id
            FROM user_node_group_grants ungg
            JOIN node_group_members ngm ON ngm.group_id = ungg.group_id
            WHERE ungg.user_id = $1
        )
        ORDER BY n.name ASC
        ",
    )
    .bind(user.id)
    .bind(stat_month)
    .fetch_all(&state.db)
    .await?;

    let subscription_url = format!(
        "{}/sub/{}",
        state.config.server.public_base_url.trim_end_matches('/'),
        user.sub_token
    );

    Ok(Json(PortalSummaryDto {
        stat_month,
        proxy_enabled: user.proxy_enabled,
        proxy_username: user.proxy_username,
        uplink_bytes,
        downlink_bytes,
        peak_uplink_bps,
        peak_downlink_bps,
        subscription_url,
        node_usage,
    }))
}

/// `GET /api/v1/portal/subscriptions` - returns 4-protocol URIs for approved users.
pub async fn get_portal_subscriptions(
    State(state): State<AppState>,
    AuthenticatedUser(user): AuthenticatedUser,
) -> AppResult<Json<Vec<RenderedSubscriptionLink>>> {
    if !user.proxy_enabled {
        return Ok(Json(Vec::new()));
    }

    let inbounds = load_authorized_inbounds_for_user(&state, user.id).await?;
    let links = inbounds
        .iter()
        .filter_map(|row| {
            render_inbound_uri(row, user.proxy_uuid, &user.proxy_password).map(|(protocol, uri)| {
                RenderedSubscriptionLink {
                    node_id: row.node_id,
                    node_name: row.node_name.clone(),
                    region: row.region.clone(),
                    inbound_tag: row.tag.clone(),
                    protocol,
                    uri,
                }
            })
        })
        .collect();

    Ok(Json(links))
}

/// `GET /api/v1/settings/mihomo-template`
pub async fn get_mihomo_template(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<crate::subscription::MihomoTemplateSettings>> {
    let tpl = crate::subscription::load_mihomo_template(&state).await?;
    Ok(Json(tpl))
}

/// `PUT /api/v1/settings/mihomo-template`
pub async fn update_mihomo_template(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Json(tpl): Json<crate::subscription::MihomoTemplateSettings>,
) -> AppResult<StatusCode> {
    let val = serde_json::to_value(&tpl)
        .map_err(|e| AppError::BadRequest(format!("invalid template JSON: {e}")))?;

    sqlx::query(
        r"
        INSERT INTO panel_settings (key, value, updated_at)
        VALUES ('mihomo_template', $1, NOW())
        ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
        ",
    )
    .bind(val)
    .execute(&state.db)
    .await?;

    record_audit_log(
        &state,
        admin.id,
        "settings.mihomo_template.update",
        "panel_settings",
        Some("mihomo_template"),
        json!({}),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Control-plane build, version, license, and repository metadata.
#[derive(Debug, Serialize)]
pub struct AboutInfoDto {
    /// Semantic package version.
    pub version: &'static str,
    /// Short Git commit SHA (or `"unknown"` when unavailable).
    pub commit: String,
    /// Full Git commit SHA (or `"unknown"` when unavailable).
    pub commit_full: String,
    /// SPDX license identifier.
    pub license: &'static str,
    /// Upstream GitHub repository URL.
    pub repository_url: &'static str,
}

/// `GET /api/v1/system/about`
pub async fn get_system_about(AuthenticatedUser(_user): AuthenticatedUser) -> Json<AboutInfoDto> {
    let commit_full = env!("AETHERGATE_BUILD_COMMIT").to_owned();
    let commit = if commit_full == "unknown" {
        "unknown".to_owned()
    } else {
        commit_full.chars().take(7).collect()
    };

    Json(AboutInfoDto {
        version: env!("CARGO_PKG_VERSION"),
        commit,
        commit_full,
        license: env!("CARGO_PKG_LICENSE"),
        repository_url: env!("CARGO_PKG_REPOSITORY"),
    })
}
