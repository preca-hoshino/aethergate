//! 4-protocol subscription generator (`GET /sub/{token}`) and portal URI builder.

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use aethergate_protocol::{InboundProtocolConfig, ProtocolType, TlsProfile};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Query parameters for `GET /sub/{token}`.
#[derive(Debug, Deserialize)]
pub struct SubscriptionQuery {
    /// Output format: `"uri"` (base64 lines, default) or `"singbox"` (JSON outbounds).
    #[serde(default = "default_sub_format")]
    pub format: String,
}

/// Default subscription format.
fn default_sub_format() -> String {
    "uri".to_owned()
}

/// User credential row loaded for subscription rendering.
#[derive(Debug, sqlx::FromRow)]
struct SubUserRow {
    /// User UUID.
    id: Uuid,
    /// Whether proxy access is active.
    proxy_enabled: bool,
    /// Panel account status.
    panel_status: String,
    /// Proxy UUID.
    proxy_uuid: Uuid,
    /// Proxy password.
    proxy_password: String,
}

/// Authorized node inbound row loaded for a user.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuthorizedInboundRow {
    /// Node UUID.
    pub node_id: Uuid,
    /// Node display name.
    pub node_name: String,
    /// Node public host/IP.
    pub host: String,
    /// Node region label.
    pub region: String,
    /// Node shared TLS profile JSON.
    pub tls_profile_json: Value,
    /// Inbound tag.
    pub tag: String,
    /// Listen port.
    pub listen_port: i32,
    /// Optional port hopping range.
    pub port_range: Option<String>,
    /// Inbound protocol configuration JSON.
    pub protocol_config_json: Value,
}

/// Rendered protocol URI item returned to the user portal UI.
#[derive(Debug, Clone, Serialize)]
pub struct RenderedSubscriptionLink {
    /// Node UUID.
    pub node_id: Uuid,
    /// Node display name.
    pub node_name: String,
    /// Node region.
    pub region: String,
    /// Inbound tag.
    pub inbound_tag: String,
    /// Protocol identifier (`vless_reality`, `hysteria2`, `tuic`, `anytls`).
    pub protocol: ProtocolType,
    /// Shareable client URI.
    pub uri: String,
}

/// Loads all enabled inbounds on nodes granted directly or via node groups to `user_id`.
///
/// # Errors
/// Returns a database error if querying fails.
pub async fn load_authorized_inbounds_for_user(
    state: &AppState,
    user_id: Uuid,
) -> AppResult<Vec<AuthorizedInboundRow>> {
    let rows = sqlx::query_as::<_, AuthorizedInboundRow>(
        r"
        SELECT DISTINCT
            n.id AS node_id,
            n.name AS node_name,
            n.host,
            n.region,
            n.tls_profile AS tls_profile_json,
            ni.tag,
            ni.listen_port,
            ni.port_range,
            ni.protocol_config AS protocol_config_json
        FROM nodes n
        JOIN node_inbounds ni ON ni.node_id = n.id AND ni.enabled = TRUE
        WHERE n.id IN (
            SELECT node_id FROM user_node_grants WHERE user_id = $1
            UNION
            SELECT ngm.node_id
            FROM user_node_group_grants ungg
            JOIN node_group_members ngm ON ngm.group_id = ungg.group_id
            WHERE ungg.user_id = $1
        )
        ORDER BY n.name ASC, ni.listen_port ASC
        ",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows)
}

/// Renders a single shareable URI (`vless://`, `hysteria2://`, `tuic://`, `anytls://`).
#[must_use]
pub fn render_inbound_uri(
    row: &AuthorizedInboundRow,
    proxy_uuid: Uuid,
    proxy_password: &str,
) -> Option<(ProtocolType, String)> {
    let tls: TlsProfile = serde_json::from_value(row.tls_profile_json.clone()).ok()?;
    let cfg: InboundProtocolConfig =
        serde_json::from_value(row.protocol_config_json.clone()).ok()?;
    let label = urlencoding::encode(&format!("{} - {}", row.node_name, row.tag)).into_owned();
    let host = &row.host;
    let port = row.listen_port;

    match cfg {
        InboundProtocolConfig::VlessReality {
            server_name,
            public_key,
            short_ids,
            flow,
            fingerprint,
            ..
        } => {
            let sid = short_ids.first().map_or("", String::as_str);
            let uri = format!(
                "vless://{proxy_uuid}@{host}:{port}?encryption=none&security=reality&sni={}&fp={}&pbk={}&sid={}&type=tcp&flow={}#{label}",
                urlencoding::encode(&server_name),
                urlencoding::encode(&fingerprint),
                urlencoding::encode(&public_key),
                urlencoding::encode(sid),
                urlencoding::encode(&flow),
            );
            Some((ProtocolType::VlessReality, uri))
        }
        InboundProtocolConfig::Hysteria2 { obfs, .. } => {
            let mut params = vec![format!("sni={}", urlencoding::encode(&tls.server_name))];
            if tls.mode == "self_signed" {
                params.push("insecure=1".to_owned());
            }
            if let Some(obfs_cfg) = obfs {
                params.push(format!("obfs={}", urlencoding::encode(&obfs_cfg.obfs_type)));
                params.push(format!(
                    "obfs-password={}",
                    urlencoding::encode(&obfs_cfg.password)
                ));
            }
            if let Some(mport) = row.port_range.as_deref().filter(|s| !s.is_empty()) {
                params.push(format!("mport={}", urlencoding::encode(mport)));
            }
            let uri = format!(
                "hysteria2://{}@{host}:{port}/?{}#{label}",
                urlencoding::encode(proxy_password),
                params.join("&"),
            );
            Some((ProtocolType::Hysteria2, uri))
        }
        InboundProtocolConfig::Tuic {
            congestion_control,
            udp_relay_mode,
            alpn,
            ..
        } => {
            let alpn_joined = alpn.join(",");
            let insecure = u8::from(tls.mode == "self_signed");
            let uri = format!(
                "tuic://{proxy_uuid}:{}@{host}:{port}?congestion_control={}&alpn={}&sni={}&udp_relay_mode={}&allow_insecure={insecure}#{label}",
                urlencoding::encode(proxy_password),
                urlencoding::encode(&congestion_control),
                urlencoding::encode(&alpn_joined),
                urlencoding::encode(&tls.server_name),
                urlencoding::encode(&udp_relay_mode),
            );
            Some((ProtocolType::Tuic, uri))
        }
        InboundProtocolConfig::Anytls { .. } => {
            let insecure = u8::from(tls.mode == "self_signed");
            let uri = format!(
                "anytls://{}@{host}:{port}?sni={}&insecure={insecure}#{label}",
                urlencoding::encode(proxy_password),
                urlencoding::encode(&tls.server_name),
            );
            Some((ProtocolType::Anytls, uri))
        }
    }
}

/// Renders a `sing-box` client outbound JSON object for one inbound.
#[must_use]
fn render_singbox_outbound(
    row: &AuthorizedInboundRow,
    proxy_uuid: Uuid,
    proxy_password: &str,
) -> Option<Value> {
    let tls: TlsProfile = serde_json::from_value(row.tls_profile_json.clone()).ok()?;
    let cfg: InboundProtocolConfig =
        serde_json::from_value(row.protocol_config_json.clone()).ok()?;
    let tag = format!("{}-{}", row.node_name, row.tag);

    match cfg {
        InboundProtocolConfig::VlessReality {
            server_name,
            public_key,
            short_ids,
            flow,
            fingerprint,
            ..
        } => Some(json!({
            "type": "vless",
            "tag": tag,
            "server": row.host,
            "server_port": row.listen_port,
            "uuid": proxy_uuid,
            "flow": flow,
            "tls": {
                "enabled": true,
                "server_name": server_name,
                "utls": { "enabled": true, "fingerprint": fingerprint },
                "reality": {
                    "enabled": true,
                    "public_key": public_key,
                    "short_id": short_ids.first().cloned().unwrap_or_default()
                }
            }
        })),
        InboundProtocolConfig::Hysteria2 { obfs, .. } => Some(json!({
            "type": "hysteria2",
            "tag": tag,
            "server": row.host,
            "server_port": row.listen_port,
            "password": proxy_password,
            "obfs": obfs,
            "tls": {
                "enabled": true,
                "server_name": tls.server_name,
                "insecure": tls.mode == "self_signed"
            }
        })),
        InboundProtocolConfig::Tuic {
            congestion_control,
            udp_relay_mode,
            alpn,
            ..
        } => Some(json!({
            "type": "tuic",
            "tag": tag,
            "server": row.host,
            "server_port": row.listen_port,
            "uuid": proxy_uuid,
            "password": proxy_password,
            "congestion_control": congestion_control,
            "udp_relay_mode": udp_relay_mode,
            "tls": {
                "enabled": true,
                "server_name": tls.server_name,
                "alpn": alpn,
                "insecure": tls.mode == "self_signed"
            }
        })),
        InboundProtocolConfig::Anytls { .. } => Some(json!({
            "type": "anytls",
            "tag": tag,
            "server": row.host,
            "server_port": row.listen_port,
            "password": proxy_password,
            "tls": {
                "enabled": true,
                "server_name": tls.server_name,
                "insecure": tls.mode == "self_signed"
            }
        })),
    }
}

/// `GET /sub/{token}` - public subscription endpoint gated by `sub_token`, `panel_status`, and `proxy_enabled`.
pub async fn get_subscription(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<SubscriptionQuery>,
) -> AppResult<Response> {
    let user = sqlx::query_as::<_, SubUserRow>(
        "SELECT id, proxy_enabled, panel_status, proxy_uuid, proxy_password FROM users WHERE sub_token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("subscription token not found".to_owned()))?;

    if user.panel_status != "active" || !user.proxy_enabled {
        return Err(AppError::Forbidden(
            "proxy access is not enabled for this account".to_owned(),
        ));
    }

    let inbounds = load_authorized_inbounds_for_user(&state, user.id).await?;

    if query.format.eq_ignore_ascii_case("singbox") {
        let outbounds: Vec<Value> = inbounds
            .iter()
            .filter_map(|row| render_singbox_outbound(row, user.proxy_uuid, &user.proxy_password))
            .collect();
        let body = serde_json::to_string_pretty(&json!({ "outbounds": outbounds }))
            .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON serialization error: {e}")))?;
        return Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            body,
        )
            .into_response());
    }

    let uris: Vec<String> = inbounds
        .iter()
        .filter_map(|row| render_inbound_uri(row, user.proxy_uuid, &user.proxy_password))
        .map(|(_, uri)| uri)
        .collect();
    let encoded = STANDARD.encode(uris.join("\n"));
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        encoded,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use aethergate_protocol::Hy2ObfsConfig;

    use super::*;

    #[test]
    fn renders_all_four_protocol_uris() {
        let tls = json!({
            "mode": "self_signed",
            "server_name": "edge.example.com",
            "cert_path": "/etc/aethergate/certs/fullchain.pem",
            "key_path": "/etc/aethergate/certs/privkey.pem"
        });
        let base_row = |tag: &str, port: i32, cfg: InboundProtocolConfig| AuthorizedInboundRow {
            node_id: Uuid::nil(),
            node_name: "Tokyo-01".to_owned(),
            host: "198.51.100.1".to_owned(),
            region: "JP".to_owned(),
            tls_profile_json: tls.clone(),
            tag: tag.to_owned(),
            listen_port: port,
            port_range: None,
            protocol_config_json: serde_json::to_value(cfg).unwrap_or(Value::Null),
        };
        let uid = Uuid::nil();

        let vless = render_inbound_uri(
            &base_row(
                "in-vless",
                443,
                InboundProtocolConfig::VlessReality {
                    server_name: "www.microsoft.com".to_owned(),
                    handshake_server: "www.microsoft.com".to_owned(),
                    handshake_port: 443,
                    private_key: "priv".to_owned(),
                    public_key: "pub".to_owned(),
                    short_ids: vec!["01ab".to_owned()],
                    flow: "xtls-rprx-vision".to_owned(),
                    fingerprint: "chrome".to_owned(),
                },
            ),
            uid,
            "pw",
        );
        assert!(vless.as_ref().is_some_and(|(p, u)| *p == ProtocolType::VlessReality && u.starts_with("vless://")));

        let hy2 = render_inbound_uri(
            &base_row(
                "in-hy2",
                443,
                InboundProtocolConfig::Hysteria2 {
                    up_mbps: Some(100),
                    down_mbps: Some(200),
                    ignore_client_bandwidth: false,
                    obfs: Some(Hy2ObfsConfig {
                        obfs_type: "salamander".to_owned(),
                        password: "obfs".to_owned(),
                    }),
                    masquerade: None,
                    alpn: vec!["h3".to_owned()],
                },
            ),
            uid,
            "pw",
        );
        assert!(
            hy2.as_ref().is_some_and(
                |(p, u)| *p == ProtocolType::Hysteria2 && u.starts_with("hysteria2://")
            )
        );

        let tuic = render_inbound_uri(
            &base_row(
                "in-tuic",
                8443,
                InboundProtocolConfig::Tuic {
                    congestion_control: "bbr".to_owned(),
                    udp_relay_mode: "native".to_owned(),
                    zero_rtt_handshake: false,
                    auth_timeout: "3s".to_owned(),
                    heartbeat: "10s".to_owned(),
                    alpn: vec!["h3".to_owned()],
                },
            ),
            uid,
            "pw",
        );
        assert!(
            tuic.as_ref()
                .is_some_and(|(p, u)| *p == ProtocolType::Tuic && u.starts_with("tuic://"))
        );

        let anytls = render_inbound_uri(
            &base_row(
                "in-anytls",
                9443,
                InboundProtocolConfig::Anytls {
                    padding_scheme: vec![],
                },
            ),
            uid,
            "pw",
        );
        assert!(
            anytls
                .as_ref()
                .is_some_and(|(p, u)| *p == ProtocolType::Anytls && u.starts_with("anytls://"))
        );
    }
}
