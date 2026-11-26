//! Hysteria 2-inspired Node Open API (`/api/open/v1/*`) secured by per-node HMAC-SHA256.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use futures::Stream;
use futures::stream::unfold;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use aethergate_protocol::{
    AuthorizedUserCredential, ClientAuthRequest, ClientAuthResponse, CommandAckRequest,
    ConfigFormat, DEFAULT_MAX_TIMESTAMP_SKEW_SECS, HEADER_NODE_ID, HEADER_NONCE, HEADER_SIGNATURE,
    HEADER_TIMESTAMP, InboundProtocolConfig, NodeAuthVerifyResponse, NodeConfigResponse,
    NodeInboundSpec, NodeStatusReport, NodeTemplate, NodeUsersSyncResponse, ReportAckAction,
    ReportAckResponse, ReportCadenceMode, TelemetryBatchRequest, TlsProfile, UserDeltaEntry,
    UserDeltaOpKind, UserSyncMode, VerifyRequestParams, compile_singbox_config,
    compute_deterministic_etag, verify_request,
};

use crate::crypto::decrypt_secret;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::telemetry::ingest_telemetry_batch;

/// Node row loaded during Open API HMAC authentication.
#[derive(Debug, sqlx::FromRow)]
struct VerifiedNodeRow {
    /// Node UUID.
    id: Uuid,
    /// Node display name.
    name: String,
    /// Encrypted HMAC secret.
    hmac_secret_enc: Vec<u8>,
    /// Shared TLS profile JSON.
    tls_profile: Value,
    /// Current config revision.
    config_revision: i64,
    /// Current user revision.
    user_revision: i64,
}

/// Verifies HMAC-SHA256 headers and returns the authenticated node record.
async fn authenticate_node(
    state: &AppState,
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> AppResult<VerifiedNodeRow> {
    let node_id_raw = headers
        .get(HEADER_NODE_ID)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing X-Node-Id header".to_owned()))?;
    let node_id = Uuid::parse_str(node_id_raw.trim())
        .map_err(|_| AppError::Unauthorized("invalid X-Node-Id UUID".to_owned()))?;

    let timestamp_raw = headers
        .get(HEADER_TIMESTAMP)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing X-Timestamp header".to_owned()))?;
    let timestamp: i64 = timestamp_raw
        .trim()
        .parse()
        .map_err(|_| AppError::Unauthorized("invalid X-Timestamp integer".to_owned()))?;

    let nonce = headers
        .get(HEADER_NONCE)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Unauthorized("missing X-Nonce header".to_owned()))?;

    let signature_hex = headers
        .get(HEADER_SIGNATURE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing X-Signature header".to_owned()))?;

    let node = sqlx::query_as::<_, VerifiedNodeRow>(
        r"
        SELECT id, name, hmac_secret_enc, tls_profile, config_revision, user_revision
        FROM nodes
        WHERE id = $1
        ",
    )
    .bind(node_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("unknown node_id".to_owned()))?;

    let secret = decrypt_secret(&state.master_key, &node.hmac_secret_enc)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to decrypt node secret: {e}")))?;

    let path_and_query = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_owned(), |pq| pq.as_str().to_owned());

    let skew = state
        .config
        .security
        .hmac_max_skew_secs
        .max(DEFAULT_MAX_TIMESTAMP_SKEW_SECS);
    verify_request(&VerifyRequestParams {
        secret: &secret,
        method: method.as_str(),
        path_and_query: &path_and_query,
        timestamp,
        now_timestamp: Utc::now().timestamp(),
        max_skew_secs: skew,
        nonce,
        body,
        signature_hex,
    })
    .map_err(|e| AppError::Unauthorized(e.to_string()))?;

    Ok(node)
}

/// `POST /api/open/v1/auth/verify`
pub async fn open_auth_verify(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<NodeAuthVerifyResponse>> {
    let node = authenticate_node(&state, &method, &uri, &headers, &body).await?;
    Ok(Json(NodeAuthVerifyResponse {
        node_id: node.id,
        node_name: node.name,
        config_revision: node.config_revision,
        user_revision: node.user_revision,
        cadence_mode: state.desired_cadence_mode(),
        server_time: Utc::now(),
    }))
}

/// `POST /api/open/v1/auth/client` - Hysteria 2 External HTTP Auth compatible check.
pub async fn open_auth_client(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<ClientAuthResponse>> {
    let node = authenticate_node(&state, &method, &uri, &headers, &body).await?;
    let payload: ClientAuthRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("invalid client auth payload: {e}")))?;

    let (username_hint, password) = payload
        .auth
        .split_once(':')
        .map_or((None, payload.auth.as_str()), |(u, p)| (Some(u), p));

    let matched = sqlx::query_as::<_, (String,)>(
        r"
        SELECT u.proxy_username
        FROM users u
        WHERE u.panel_status = 'active'
          AND u.proxy_enabled = TRUE
          AND u.proxy_password = $1
          AND ($2::text IS NULL OR u.proxy_username = $2)
          AND (
              EXISTS (SELECT 1 FROM user_node_grants ung WHERE ung.user_id = u.id AND ung.node_id = $3)
              OR EXISTS (
                  SELECT 1
                  FROM user_node_group_grants ungg
                  JOIN node_group_members ngm ON ngm.group_id = ungg.group_id
                  WHERE ungg.user_id = u.id AND ngm.node_id = $3
              )
          )
        LIMIT 1
        ",
    )
    .bind(password)
    .bind(username_hint)
    .bind(node.id)
    .fetch_optional(&state.db)
    .await?;

    match matched {
        Some((proxy_username,)) => Ok(Json(ClientAuthResponse {
            ok: true,
            id: proxy_username,
        })),
        None => Ok(Json(ClientAuthResponse {
            ok: false,
            id: String::new(),
        })),
    }
}

/// Query params for `GET /api/open/v1/config`.
#[derive(Debug, Deserialize)]
pub struct OpenConfigQuery {
    /// `compact` (default) or `singbox`.
    #[serde(default)]
    pub format: ConfigFormat,
}

/// Stored inbound row for node compilation.
#[derive(Debug, sqlx::FromRow)]
struct DbInboundRow {
    /// Inbound UUID.
    id: Uuid,
    /// Inbound tag.
    tag: String,
    /// Listen IP.
    listen: String,
    /// Listen port.
    listen_port: i32,
    /// Optional port hopping range.
    port_range: Option<String>,
    /// Protocol config JSON.
    protocol_config: Value,
    /// Enabled flag.
    enabled: bool,
}

/// `GET /api/open/v1/config?format=compact|singbox` with `ETag` / `304 Not Modified`.
pub async fn open_get_config(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    Query(query): Query<OpenConfigQuery>,
) -> AppResult<Response> {
    let node = authenticate_node(&state, &method, &uri, &headers, &[]).await?;
    let tls_profile: TlsProfile = serde_json::from_value(node.tls_profile)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid node tls_profile JSON: {e}")))?;

    let db_inbounds = sqlx::query_as::<_, DbInboundRow>(
        r"
        SELECT id, tag, listen, listen_port, port_range, protocol_config, enabled
        FROM node_inbounds
        WHERE node_id = $1 AND enabled = TRUE
        ORDER BY sort_order ASC, listen_port ASC
        ",
    )
    .bind(node.id)
    .fetch_all(&state.db)
    .await?;

    let mut inbounds = Vec::with_capacity(db_inbounds.len());
    for (idx, row) in db_inbounds.into_iter().enumerate() {
        let protocol_config: InboundProtocolConfig = serde_json::from_value(row.protocol_config)
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!("invalid inbound protocol_config JSON: {e}"))
            })?;
        inbounds.push(NodeInboundSpec {
            id: row.id,
            protocol: protocol_config.protocol_type(),
            tag: row.tag,
            enabled: row.enabled,
            listen: row.listen,
            listen_port: u16::try_from(row.listen_port).unwrap_or(443),
            port_hopping_range: row.port_range,
            protocol_config,
            sort_order: i16::try_from(idx).unwrap_or(0),
        });
    }

    let template = NodeTemplate {
        inbounds,
        dns_route: Value::Null,
    };

    let payload = match query.format {
        ConfigFormat::Compact => NodeConfigResponse {
            node_id: node.id,
            config_revision: node.config_revision,
            user_revision: node.user_revision,
            template: Some(template),
            tls_profile: Some(tls_profile),
            singbox_config: None,
        },
        ConfigFormat::Singbox => {
            let users = load_authorized_credentials_for_node(&state, node.id).await?;
            let compiled = compile_singbox_config(
                &template,
                &tls_profile,
                &users,
                &aethergate_protocol::LocalAgentOverrides::default(),
            )
            .map_err(|e| AppError::Internal(anyhow::anyhow!("sing-box compile error: {e}")))?;
            NodeConfigResponse {
                node_id: node.id,
                config_revision: node.config_revision,
                user_revision: node.user_revision,
                template: None,
                tls_profile: None,
                singbox_config: Some(compiled),
            }
        }
    };

    let etag = compute_deterministic_etag(&payload)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ETag computation error: {e}")))?;
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim().trim_matches('"') == etag)
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    let mut resp = Json(payload).into_response();
    if let Ok(hv) = HeaderValue::from_str(&format!("\"{etag}\"")) {
        resp.headers_mut().insert(header::ETAG, hv);
    }
    Ok(resp)
}

/// Query params for `GET /api/open/v1/users`.
#[derive(Debug, Deserialize)]
pub struct OpenUsersQuery {
    /// Client's last applied `user_revision`.
    pub since_rev: Option<i64>,
}

/// Authorized user row loaded from PostgreSQL.
#[derive(Debug, sqlx::FromRow)]
struct DbAuthorizedUserRow {
    /// User UUID.
    id: Uuid,
    /// Immutable proxy username.
    proxy_username: String,
    /// Proxy UUID.
    proxy_uuid: Uuid,
    /// Proxy password.
    proxy_password: String,
}

/// Delta row loaded from `node_user_deltas`.
#[derive(Debug, sqlx::FromRow)]
struct DbUserDeltaRow {
    /// Revision number.
    rev: i64,
    /// Operation (`upsert` or `revoke`).
    op: String,
    /// User UUID.
    user_id: Uuid,
    /// Immutable proxy username.
    proxy_username: String,
}

/// Loads all currently authorized user credentials for `node_id`.
async fn load_authorized_credentials_for_node(
    state: &AppState,
    node_id: Uuid,
) -> AppResult<Vec<AuthorizedUserCredential>> {
    let rows = sqlx::query_as::<_, DbAuthorizedUserRow>(
        r"
        SELECT DISTINCT u.id, u.proxy_username, u.proxy_uuid, u.proxy_password
        FROM users u
        WHERE u.panel_status = 'active'
          AND u.proxy_enabled = TRUE
          AND (
              EXISTS (SELECT 1 FROM user_node_grants ung WHERE ung.user_id = u.id AND ung.node_id = $1)
              OR EXISTS (
                  SELECT 1
                  FROM user_node_group_grants ungg
                  JOIN node_group_members ngm ON ngm.group_id = ungg.group_id
                  WHERE ungg.user_id = u.id AND ngm.node_id = $1
              )
          )
        ORDER BY u.proxy_username ASC
        ",
    )
    .bind(node_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| AuthorizedUserCredential {
            user_id: r.id,
            proxy_username: r.proxy_username,
            proxy_uuid: r.proxy_uuid,
            proxy_password: r.proxy_password,
        })
        .collect())
}

/// `GET /api/open/v1/users?since_rev=<rev>` - full snapshot or incremental delta sync.
pub async fn open_get_users(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    Query(query): Query<OpenUsersQuery>,
) -> AppResult<Json<NodeUsersSyncResponse>> {
    let node = authenticate_node(&state, &method, &uri, &headers, &[]).await?;

    if let Some(since_rev) = query.since_rev
        && since_rev >= 0
        && since_rev <= node.user_revision
    {
        if since_rev == node.user_revision {
            return Ok(Json(NodeUsersSyncResponse {
                mode: UserSyncMode::Delta,
                latest_rev: node.user_revision,
                users: Vec::new(),
                deltas: Vec::new(),
            }));
        }

        let delta_rows = sqlx::query_as::<_, DbUserDeltaRow>(
            r"
            SELECT rev, op, user_id, proxy_username
            FROM node_user_deltas
            WHERE node_id = $1 AND rev > $2
            ORDER BY rev ASC
            ",
        )
        .bind(node.id)
        .bind(since_rev)
        .fetch_all(&state.db)
        .await?;

        if !delta_rows.is_empty() && delta_rows.len() <= 500 {
            let active_users = load_authorized_credentials_for_node(&state, node.id).await?;
            let mut entries = Vec::with_capacity(delta_rows.len());
            for row in delta_rows {
                let op = if row.op == "revoke" {
                    UserDeltaOpKind::Revoke
                } else {
                    UserDeltaOpKind::Upsert
                };
                let credential = if op == UserDeltaOpKind::Upsert {
                    active_users
                        .iter()
                        .find(|u| u.user_id == row.user_id)
                        .cloned()
                } else {
                    None
                };
                entries.push(UserDeltaEntry {
                    rev: row.rev,
                    op,
                    user_id: row.user_id,
                    proxy_username: row.proxy_username,
                    credential,
                });
            }
            return Ok(Json(NodeUsersSyncResponse {
                mode: UserSyncMode::Delta,
                latest_rev: node.user_revision,
                users: Vec::new(),
                deltas: entries,
            }));
        }
    }

    let users = load_authorized_credentials_for_node(&state, node.id).await?;
    Ok(Json(NodeUsersSyncResponse {
        mode: UserSyncMode::Full,
        latest_rev: node.user_revision,
        users,
        deltas: Vec::new(),
    }))
}

/// Pending command row from `node_commands`.
#[derive(Debug, sqlx::FromRow)]
struct PendingCommandRow {
    /// Command UUID.
    id: Uuid,
    /// Action name.
    action: String,
    /// Action JSON payload.
    payload: Value,
}

/// Collects pending actions for a node to piggyback on `200 OK` report responses.
async fn collect_pending_actions(
    state: &AppState,
    node: &VerifiedNodeRow,
    reported_config_rev: i64,
    reported_user_rev: i64,
    reported_cadence: ReportCadenceMode,
) -> AppResult<Vec<ReportAckAction>> {
    let mut actions = Vec::new();

    let desired_cadence = state.desired_cadence_mode();
    if desired_cadence != reported_cadence {
        actions.push(ReportAckAction::SwitchCadence {
            mode: desired_cadence,
        });
    }
    if reported_config_rev < node.config_revision {
        actions.push(ReportAckAction::SyncConfig {
            target_revision: node.config_revision,
        });
    }
    if reported_user_rev < node.user_revision {
        actions.push(ReportAckAction::SyncUsers {
            target_revision: node.user_revision,
        });
    }

    let cmds = sqlx::query_as::<_, PendingCommandRow>(
        "SELECT id, action, payload FROM node_commands WHERE node_id = $1 AND status = 'pending' ORDER BY created_at ASC LIMIT 16",
    )
    .bind(node.id)
    .fetch_all(&state.db)
    .await?;

    for cmd in cmds {
        match cmd.action.as_str() {
            "kick_user" => {
                if let Some(username) = cmd.payload.get("proxy_username").and_then(Value::as_str) {
                    actions.push(ReportAckAction::KickUser {
                        command_id: cmd.id,
                        proxy_username: username.to_owned(),
                    });
                }
            }
            "reload_singbox" => {
                actions.push(ReportAckAction::ReloadSingbox { command_id: cmd.id });
            }
            _ => {}
        }
    }

    Ok(actions)
}

/// `POST /api/open/v1/report/status` - returns `204 No Content` when idle or `200 OK` with actions.
pub async fn open_report_status(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Response> {
    let node = authenticate_node(&state, &method, &uri, &headers, &body).await?;
    let report: NodeStatusReport = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("invalid status report JSON: {e}")))?;

    let cadence_str = match report.cadence_mode {
        ReportCadenceMode::Eco => "eco",
        ReportCadenceMode::Realtime => "realtime",
    };

    sqlx::query(
        r"
        UPDATE nodes
        SET status = 'online',
            cadence_mode = $2,
            agent_version = $3,
            singbox_version = $4,
            cpu_pct = $5,
            mem_pct = $6,
            mem_used_mib = $7,
            active_connections = $8,
            active_users = $9,
            uptime_secs = $10,
            last_seen_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(node.id)
    .bind(cadence_str)
    .bind(&report.agent_version)
    .bind(&report.singbox_version)
    .bind(report.cpu_pct)
    .bind(report.mem_pct)
    .bind(i32::try_from(report.mem_used_mib).unwrap_or(i32::MAX))
    .bind(i32::try_from(report.active_connections).unwrap_or(i32::MAX))
    .bind(i32::try_from(report.active_users).unwrap_or(i32::MAX))
    .bind(i64::try_from(report.uptime_secs).unwrap_or(i64::MAX))
    .execute(&state.db)
    .await?;

    state.mark_ws_dirty();

    let actions = collect_pending_actions(
        &state,
        &node,
        report.config_revision,
        report.user_revision,
        report.cadence_mode,
    )
    .await?;

    if actions.is_empty() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    Ok(Json(ReportAckResponse {
        latest_config_revision: node.config_revision,
        latest_user_revision: node.user_revision,
        actions,
    })
    .into_response())
}

/// `POST /api/open/v1/report/telemetry` - idempotent batch ingestion returning `204` or `200`.
pub async fn open_report_telemetry(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Response> {
    let node = authenticate_node(&state, &method, &uri, &headers, &body).await?;
    let batch: TelemetryBatchRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("invalid telemetry batch JSON: {e}")))?;

    let _ = ingest_telemetry_batch(&state, node.id, &batch).await?;

    sqlx::query("UPDATE nodes SET status = 'online', last_seen_at = NOW() WHERE id = $1")
        .bind(node.id)
        .execute(&state.db)
        .await?;

    let actions = collect_pending_actions(
        &state,
        &node,
        batch.config_revision,
        batch.user_revision,
        state.desired_cadence_mode(),
    )
    .await?;

    if actions.is_empty() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }

    Ok(Json(ReportAckResponse {
        latest_config_revision: node.config_revision,
        latest_user_revision: node.user_revision,
        actions,
    })
    .into_response())
}

/// `GET /api/open/v1/events` - persistent SSE stream for instant node push notifications.
pub async fn open_sse_events(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let node = authenticate_node(&state, &method, &uri, &headers, &[]).await?;
    let rx = state.node_sse_sender(node.id).subscribe();

    let stream = unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let name = ev.event_name();
                    if let Ok(data) = serde_json::to_string(&ev) {
                        return Some((Ok(Event::default().event(name).data(data)), rx));
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(25))
            .text("keep-alive"),
    ))
}

/// `POST /api/open/v1/commands/{id}/ack` - marks an executed command acknowledged (`204 No Content`).
pub async fn open_ack_command(
    State(state): State<AppState>,
    Path(command_id): Path<Uuid>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<StatusCode> {
    let node = authenticate_node(&state, &method, &uri, &headers, &body).await?;
    let payload: CommandAckRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::BadRequest(format!("invalid command ack JSON: {e}")))?;

    let status = if payload.success { "acked" } else { "failed" };
    sqlx::query(
        r"
        UPDATE node_commands
        SET status = $3, result_message = $4, acked_at = NOW()
        WHERE id = $1 AND node_id = $2
        ",
    )
    .bind(command_id)
    .bind(node.id)
    .bind(status)
    .bind(payload.message)
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
