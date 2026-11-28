//! 4-protocol subscription generator (`GET /sub/{token}`) and portal URI builder.
//!
//! Supports `format=uri` (base64 lines, default), `format=singbox` (`sing-box` JSON outbounds),
//! and `format=mihomo` / `format=clash` (full Clash Meta / Mihomo YAML with 2-level node/protocol groups).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header::{self, HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::NaiveDate;
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use aethergate_protocol::{InboundProtocolConfig, ProtocolType, TlsProfile};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Customizable Mihomo / Clash Meta template blocks stored in `panel_settings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MihomoTemplateSettings {
    /// YAML block for top-level settings + `dns:` section.
    pub header_and_dns_yaml: String,
    /// YAML block for `rules:` section.
    pub rules_yaml: String,
}

impl Default for MihomoTemplateSettings {
    fn default() -> Self {
        Self {
            header_and_dns_yaml: DEFAULT_MIHOMO_HEADER_AND_DNS.to_owned(),
            rules_yaml: DEFAULT_MIHOMO_RULES.to_owned(),
        }
    }
}

/// Default Mihomo top-level controller, ports, and Fake-IP DNS configuration.
pub const DEFAULT_MIHOMO_HEADER_AND_DNS: &str = r"external-controller: 127.0.0.1:9090

port: 7890
socks-port: 7891
allow-lan: true
mode: rule
log-level: info
ipv6: true

geodata-mode: false
geo-auto-update: false

dns:
  enable: true
  listen: ':53'
  enhanced-mode: 'fake-ip'
  fake-ip-range: '198.18.0.1/16'
  fake-ip-range6: 'fdfe:dcba:9876::1/64'
  fake-ip-filter-mode: 'blacklist'
  prefer-h3: true
  respect-rules: true
  use-hosts: false
  use-system-hosts: false
  ipv6: true
  fake-ip-filter:
    - '*.lan'
    - '*.local'
    - '*.arpa'
    - 'time.*.com'
    - 'ntp.*.com'
    - '+.market.xiaomi.com'
    - 'localhost.ptlogin2.qq.com'
    - '*.msftncsi.com'
    - 'www.msftconnecttest.com'
  default-nameserver:
    - 'system'
    - '223.6.6.6'
    - '8.8.8.8'
    - '2400:3200::1'
    - '2001:4860:4860::8888'
  nameserver:
    - 'https://doh.pub/dns-query'
    - 'https://dns.alidns.com/dns-query'
  direct-nameserver-follow-policy: true
  fallback-filter:
    geoip: true
    geoip-code: 'CN'
    ipcidr:
      - '240.0.0.0/4'
      - '0.0.0.0/32'
      - '100.64.0.0/10'
      - 'fd7a:115c:a1e0::/48'
  fallback:
    - 'https://dns.google/dns-query'
    - 'https://1.1.1.1/dns-query'
  proxy-server-nameserver:
    - 'https://doh.pub/dns-query'
    - 'https://dns.alidns.com/dns-query'
    - 'tls://223.5.5.5'";

/// Default Mihomo routing rules.
pub const DEFAULT_MIHOMO_RULES: &str = r"rules:
  - DOMAIN-SUFFIX,ts.net,DIRECT
  - IP-CIDR,100.64.0.0/10,DIRECT,no-resolve
  - IP-CIDR6,fd7a:115c:a1e0::/48,DIRECT,no-resolve
  - IP-CIDR,192.168.0.0/16,DIRECT,no-resolve
  - IP-CIDR,10.0.0.0/8,DIRECT,no-resolve
  - IP-CIDR,172.16.0.0/12,DIRECT,no-resolve
  - IP-CIDR,127.0.0.0/8,DIRECT,no-resolve
  - IP-CIDR,169.254.0.0/16,DIRECT,no-resolve
  - IP-CIDR,224.0.0.0/4,DIRECT,no-resolve
  - DOMAIN-SUFFIX,localhost,DIRECT
  - DOMAIN-SUFFIX,local,DIRECT
  - DOMAIN-SUFFIX,lan,DIRECT
  - IP-CIDR6,fc00::/7,DIRECT,no-resolve
  - IP-CIDR6,fe80::/10,DIRECT,no-resolve
  - IP-CIDR6,::1/128,DIRECT,no-resolve
  - GEOSITE,cn,DIRECT
  - GEOIP,CN,DIRECT
  - MATCH,PROXY";

/// Query parameters for `GET /sub/{token}`.
#[derive(Debug, Deserialize)]
pub struct SubscriptionQuery {
    /// Output format: `"uri"` (base64 lines, default), `"singbox"`, or `"mihomo"`/`"clash"`.
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
    /// Account type (`"user"` or `"relay"`).
    account_type: String,
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
            COALESCE(p.tls_profile, n.tls_profile) AS tls_profile_json,
            ni.tag,
            ni.listen_port,
            ni.port_range,
            ni.protocol_config AS protocol_config_json
        FROM nodes n
        LEFT JOIN nodes p ON p.id = n.parent_node_id
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

/// Builds a `sing-box` client outbound `tls` object with optional PQC `curve_preferences`.
fn singbox_outbound_tls(tls: &TlsProfile, alpn: Option<&Vec<String>>) -> Value {
    let mut tls_map = serde_json::Map::new();
    tls_map.insert("enabled".to_owned(), json!(true));
    tls_map.insert("server_name".to_owned(), json!(tls.server_name));
    if let Some(a) = alpn.filter(|v| !v.is_empty()) {
        tls_map.insert("alpn".to_owned(), json!(a));
    }
    tls_map.insert("insecure".to_owned(), json!(tls.mode == "self_signed"));
    if tls.pqc_enabled {
        tls_map.insert(
            "curve_preferences".to_owned(),
            json!(tls.resolved_curve_preferences()),
        );
    }
    Value::Object(tls_map)
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
        InboundProtocolConfig::Hysteria2 { obfs, .. } => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_owned(), json!("hysteria2"));
            obj.insert("tag".to_owned(), json!(tag));
            obj.insert("server".to_owned(), json!(row.host));
            obj.insert("server_port".to_owned(), json!(row.listen_port));
            if let Some(ports) = row.port_range.as_deref().filter(|s| !s.is_empty()) {
                obj.insert("server_ports".to_owned(), json!([ports.replace('-', ":")]));
            }
            obj.insert("password".to_owned(), json!(proxy_password));
            if let Some(obfs_cfg) = obfs {
                obj.insert("obfs".to_owned(), json!(obfs_cfg));
            }
            obj.insert("tls".to_owned(), singbox_outbound_tls(&tls, None));
            Some(Value::Object(obj))
        }
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
            "tls": singbox_outbound_tls(&tls, Some(&alpn))
        })),
        InboundProtocolConfig::Anytls { .. } => Some(json!({
            "type": "anytls",
            "tag": tag,
            "server": row.host,
            "server_port": row.listen_port,
            "password": proxy_password,
            "tls": singbox_outbound_tls(&tls, None)
        })),
    }
}

/// Loads the admin-configured Mihomo template from `panel_settings`, falling back to defaults.
///
/// # Errors
/// Returns a database error if querying `panel_settings` fails.
pub async fn load_mihomo_template(state: &AppState) -> AppResult<MihomoTemplateSettings> {
    let row = sqlx::query_scalar::<_, Value>(
        "SELECT value FROM panel_settings WHERE key = 'mihomo_template'",
    )
    .fetch_optional(&state.db)
    .await?;

    if let Some(val) = row
        && let Ok(parsed) = serde_json::from_value::<MihomoTemplateSettings>(val)
    {
        return Ok(parsed);
    }
    Ok(MihomoTemplateSettings::default())
}

/// Escapes double quotes in YAML double-quoted strings.
fn yaml_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Returns the optional Mihomo `reality-opts` PQC line when enabled.
const fn mihomo_reality_pqc_line(pqc_enabled: bool) -> &'static str {
    if pqc_enabled {
        "\n      support-x25519mlkem768: true"
    } else {
        ""
    }
}

/// Appends one Mihomo proxy YAML entry and returns `(node_name, proxy_name)` when valid.
fn append_mihomo_proxy_entry(
    out: &mut String,
    row: &AuthorizedInboundRow,
    proxy_uuid: Uuid,
    proxy_password: &str,
) -> Option<String> {
    let tls = serde_json::from_value::<TlsProfile>(row.tls_profile_json.clone()).ok()?;
    let cfg =
        serde_json::from_value::<InboundProtocolConfig>(row.protocol_config_json.clone()).ok()?;
    let skip_cert = tls.mode == "self_signed";
    let sni = if tls.server_name.is_empty() {
        &row.host
    } else {
        &tls.server_name
    };

    let proxy_name = match cfg {
        InboundProtocolConfig::VlessReality { .. } => {
            format!("VLESS-Reality ({})", row.node_name)
        }
        InboundProtocolConfig::Hysteria2 { .. } => format!("Hysteria2 ({})", row.node_name),
        InboundProtocolConfig::Tuic { .. } => format!("TUIC ({})", row.node_name),
        InboundProtocolConfig::Anytls { .. } => format!("AnyTLS ({})", row.node_name),
    };

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
            let pqc_line = mihomo_reality_pqc_line(tls.pqc_enabled);
            let _ = writeln!(
                out,
                "  - name: {}\n    type: vless\n    server: {}\n    port: {}\n    uuid: {}\n    flow: {}\n    tls: true\n    udp: true\n    servername: {}\n    reality: true\n    reality-opts:\n      public-key: {}\n      short-id: {}{pqc_line}\n    client-fingerprint: {fingerprint}\n",
                yaml_quote(&proxy_name),
                row.host,
                row.listen_port,
                proxy_uuid,
                flow,
                server_name,
                public_key,
                sid,
            );
        }
        InboundProtocolConfig::Hysteria2 { obfs, .. } => {
            let _ = writeln!(
                out,
                "  - name: {}\n    type: hysteria2\n    server: {}\n    port: {}",
                yaml_quote(&proxy_name),
                row.host,
                row.listen_port,
            );
            if let Some(ports) = row.port_range.as_deref().filter(|s| !s.is_empty()) {
                let _ = writeln!(out, "    ports: {ports}");
            }
            let _ = writeln!(
                out,
                "    password: {}\n    tls: true\n    sni: {}\n    skip-cert-verify: {skip_cert}",
                yaml_quote(proxy_password),
                sni,
            );
            if let Some(obfs_cfg) = obfs {
                let _ = writeln!(
                    out,
                    "    obfs: {}\n    obfs-password: {}",
                    obfs_cfg.obfs_type,
                    yaml_quote(&obfs_cfg.password),
                );
            }
            out.push('\n');
        }
        InboundProtocolConfig::Tuic {
            congestion_control,
            udp_relay_mode,
            alpn,
            ..
        } => {
            let alpn_list = alpn.join(", ");
            let _ = writeln!(
                out,
                "  - name: {}\n    type: tuic\n    server: {}\n    port: {}\n    uuid: {}\n    password: {}\n    alpn: [{alpn_list}]\n    congestion-controller: {congestion_control}\n    udp-relay-mode: {udp_relay_mode}\n    sni: {}\n    udp: true\n    skip-cert-verify: {skip_cert}\n",
                yaml_quote(&proxy_name),
                row.host,
                row.listen_port,
                proxy_uuid,
                yaml_quote(proxy_password),
                sni,
            );
        }
        InboundProtocolConfig::Anytls { .. } => {
            let _ = writeln!(
                out,
                "  - name: {}\n    type: anytls\n    server: {}\n    port: {}\n    password: {}\n    sni: {}\n    skip-cert-verify: {skip_cert}\n    udp: true\n",
                yaml_quote(&proxy_name),
                row.host,
                row.listen_port,
                yaml_quote(proxy_password),
                sni,
            );
        }
    }

    Some(proxy_name)
}

/// Renders a complete Mihomo (Clash Meta) YAML configuration with 2-level node/protocol proxy-groups.
#[must_use]
pub fn render_mihomo_yaml(
    inbounds: &[AuthorizedInboundRow],
    proxy_uuid: Uuid,
    proxy_password: &str,
    template: &MihomoTemplateSettings,
) -> String {
    let mut out = String::new();
    out.push_str(template.header_and_dns_yaml.trim_end());
    out.push_str("\n\nproxies:\n");

    let mut node_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in inbounds {
        if let Some(proxy_name) =
            append_mihomo_proxy_entry(&mut out, row, proxy_uuid, proxy_password)
        {
            node_groups
                .entry(row.node_name.clone())
                .or_default()
                .push(proxy_name);
        }
    }

    out.push_str("proxy-groups:\n  - name: \"PROXY\"\n    type: select\n    proxies:\n");
    for node_name in node_groups.keys() {
        let _ = writeln!(out, "      - {}", yaml_quote(node_name));
    }
    out.push_str("      - \"DIRECT\"\n\n");

    for (node_name, proxy_names) in &node_groups {
        let _ = writeln!(
            out,
            "  - name: {}\n    type: select\n    proxies:",
            yaml_quote(node_name)
        );
        for pname in proxy_names {
            let _ = writeln!(out, "      - {}", yaml_quote(pname));
        }
        out.push_str("    url: \"http://www.gstatic.com/generate_204\"\n    interval: 300\n\n");
    }

    out.push_str(template.rules_yaml.trim_end());
    out.push('\n');
    out
}

/// HTTP response header used by Clash/Mihomo/v2rayN/Shadowrocket clients to display traffic usage.
const SUBSCRIPTION_USERINFO_HEADER: HeaderName = HeaderName::from_static("subscription-userinfo");

/// HTTP response header used by Clash/Mihomo clients to schedule periodic subscription updates (in hours).
const PROFILE_UPDATE_INTERVAL_HEADER: HeaderName =
    HeaderName::from_static("profile-update-interval");

/// Default monthly virtual traffic quota (1 TiB = 1,099,511,627,776 bytes).
const DEFAULT_VIRTUAL_TOTAL_BYTES: i64 = 1024 * 1024 * 1024 * 1024;

/// Computes the Unix timestamp (seconds) of the start of the next calendar month in `tz`.
fn next_month_timestamp(stat_month: NaiveDate, tz: Tz) -> i64 {
    crate::telemetry::month_utc_bounds(stat_month, tz)
        .1
        .timestamp()
}

/// Formats the `Subscription-Userinfo` header value from monthly uplink, downlink bytes and expiration timestamp.
#[must_use]
fn format_subscription_userinfo(
    uplink_bytes: i64,
    downlink_bytes: i64,
    total_bytes: i64,
    expire_ts: i64,
) -> String {
    format!(
        "upload={}; download={}; total={}; expire={}",
        uplink_bytes.max(0),
        downlink_bytes.max(0),
        total_bytes.max(1),
        expire_ts.max(0)
    )
}

/// Loads the user's current calendar-month `(uplink_bytes, downlink_bytes, expire_ts)` from `user_monthly_traffic`.
async fn load_user_monthly_traffic(state: &AppState, user_id: Uuid) -> AppResult<(i64, i64, i64)> {
    let stat_month = crate::telemetry::parse_stat_month(None, state.stats_tz)?;
    let expire_ts = next_month_timestamp(stat_month, state.stats_tz);
    let (uplink_bytes, downlink_bytes) = sqlx::query_as::<_, (i64, i64)>(
        r"
        SELECT
            COALESCE(uplink_bytes, 0),
            COALESCE(downlink_bytes, 0)
        FROM user_monthly_traffic
        WHERE stat_month = $1 AND user_id = $2
        ",
    )
    .bind(stat_month)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or((0, 0));

    Ok((uplink_bytes, downlink_bytes, expire_ts))
}

/// `GET /sub/{token}` - public subscription endpoint gated by `sub_token`, `panel_status`, and `proxy_enabled`.
pub async fn get_subscription(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<SubscriptionQuery>,
) -> AppResult<Response> {
    let user = sqlx::query_as::<_, SubUserRow>(
        "SELECT id, account_type, proxy_enabled, panel_status, proxy_uuid, proxy_password FROM users WHERE sub_token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("subscription token not found".to_owned()))?;

    if user.account_type == "relay" {
        return Err(AppError::Forbidden(
            "relay service accounts do not expose end-user subscriptions".to_owned(),
        ));
    }

    if user.panel_status != "active" || !user.proxy_enabled {
        return Err(AppError::Forbidden(
            "proxy access is not enabled for this account".to_owned(),
        ));
    }

    let inbounds = load_authorized_inbounds_for_user(&state, user.id).await?;
    let (uplink_bytes, downlink_bytes, expire_ts) =
        load_user_monthly_traffic(&state, user.id).await?;
    let userinfo = format_subscription_userinfo(
        uplink_bytes,
        downlink_bytes,
        DEFAULT_VIRTUAL_TOTAL_BYTES,
        expire_ts,
    );

    let mut response = if query.format.eq_ignore_ascii_case("mihomo")
        || query.format.eq_ignore_ascii_case("clash")
    {
        let template = load_mihomo_template(&state).await?;
        let yaml = render_mihomo_yaml(&inbounds, user.proxy_uuid, &user.proxy_password, &template);
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/yaml; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"aethergate.yaml\"",
                ),
            ],
            yaml,
        )
            .into_response()
    } else if query.format.eq_ignore_ascii_case("singbox") {
        let outbounds: Vec<Value> = inbounds
            .iter()
            .filter_map(|row| render_singbox_outbound(row, user.proxy_uuid, &user.proxy_password))
            .collect();
        let body = serde_json::to_string_pretty(&json!({ "outbounds": outbounds }))
            .map_err(|e| AppError::Internal(anyhow::anyhow!("JSON serialization error: {e}")))?;
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"aethergate.json\"",
                ),
            ],
            body,
        )
            .into_response()
    } else {
        let uris: Vec<String> = inbounds
            .iter()
            .filter_map(|row| render_inbound_uri(row, user.proxy_uuid, &user.proxy_password))
            .map(|(_, uri)| uri)
            .collect();
        let encoded = STANDARD.encode(uris.join("\n"));
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"aethergate.txt\"",
                ),
            ],
            encoded,
        )
            .into_response()
    };

    if let Ok(header_value) = HeaderValue::from_str(&userinfo) {
        response
            .headers_mut()
            .insert(SUBSCRIPTION_USERINFO_HEADER, header_value);
    }
    response.headers_mut().insert(
        PROFILE_UPDATE_INTERVAL_HEADER,
        HeaderValue::from_static("24"),
    );

    Ok(response)
}

#[cfg(test)]
mod tests {
    use aethergate_protocol::Hy2ObfsConfig;
    use chrono::{Datelike, TimeZone, Timelike};

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
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
                    alpn: vec![],
                    max_time_difference: None,
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
                        min_packet_size: None,
                        max_packet_size: None,
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

    #[test]
    fn renders_mihomo_yaml_with_node_groups() {
        let tls = json!({
            "mode": "self_signed",
            "server_name": "cn-ckg-01.example.com",
            "cert_path": "/etc/aethergate/certs/fullchain.pem",
            "key_path": "/etc/aethergate/certs/privkey.pem"
        });
        let rows = vec![
            AuthorizedInboundRow {
                node_id: Uuid::nil(),
                node_name: "CN-CKG-01".to_owned(),
                host: "cn-ckg-01.example.com".to_owned(),
                region: "CN".to_owned(),
                tls_profile_json: tls.clone(),
                tag: "in-vless".to_owned(),
                listen_port: 30000,
                port_range: None,
                protocol_config_json: serde_json::to_value(InboundProtocolConfig::VlessReality {
                    server_name: "ys.mihoyo.com".to_owned(),
                    handshake_server: "ys.mihoyo.com".to_owned(),
                    handshake_port: 443,
                    private_key: "priv".to_owned(),
                    public_key: "pub123".to_owned(),
                    short_ids: vec!["ecd269cf".to_owned()],
                    flow: "xtls-rprx-vision".to_owned(),
                    fingerprint: "chrome".to_owned(),
                    alpn: vec![],
                    max_time_difference: None,
                })
                .unwrap_or(Value::Null),
            },
            AuthorizedInboundRow {
                node_id: Uuid::nil(),
                node_name: "CN-CKG-01".to_owned(),
                host: "cn-ckg-01.example.com".to_owned(),
                region: "CN".to_owned(),
                tls_profile_json: tls,
                tag: "in-hy2".to_owned(),
                listen_port: 30001,
                port_range: Some("32000-32999".to_owned()),
                protocol_config_json: serde_json::to_value(InboundProtocolConfig::Hysteria2 {
                    up_mbps: None,
                    down_mbps: None,
                    ignore_client_bandwidth: false,
                    obfs: None,
                    masquerade: None,
                    alpn: vec!["h3".to_owned()],
                })
                .unwrap_or(Value::Null),
            },
        ];

        let yaml = render_mihomo_yaml(
            &rows,
            Uuid::nil(),
            "secret-pass",
            &MihomoTemplateSettings::default(),
        );
        assert!(yaml.contains("external-controller: 127.0.0.1:9090"));
        assert!(yaml.contains("name: \"VLESS-Reality (CN-CKG-01)\""));
        assert!(yaml.contains("support-x25519mlkem768: true"));
        assert!(yaml.contains("name: \"Hysteria2 (CN-CKG-01)\""));
        assert!(yaml.contains("ports: 32000-32999"));
        assert!(yaml.contains("proxy-groups:"));
        assert!(yaml.contains("name: \"PROXY\""));
        assert!(yaml.contains("name: \"CN-CKG-01\""));
        assert!(yaml.contains("rules:"));
    }

    #[test]
    fn formats_subscription_userinfo_header() {
        assert_eq!(
            format_subscription_userinfo(
                1_048_576,
                2_097_152,
                DEFAULT_VIRTUAL_TOTAL_BYTES,
                1_764_547_200
            ),
            "upload=1048576; download=2097152; total=1099511627776; expire=1764547200"
        );
        assert_eq!(
            format_subscription_userinfo(-10, 0, 0, -1),
            "upload=0; download=0; total=1; expire=0"
        );
    }

    #[test]
    fn next_month_timestamp_calculates_boundary() {
        let Ok(tz) = "Asia/Shanghai".parse::<Tz>() else {
            return;
        };
        let Some(stat_month) = NaiveDate::from_ymd_opt(2026, 11, 1) else {
            return;
        };
        let ts = next_month_timestamp(stat_month, tz);
        let Some(dt) = tz.timestamp_opt(ts, 0).single() else {
            return;
        };
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 12);
        assert_eq!(dt.day(), 1);
        assert_eq!(dt.hour(), 0);
        assert_eq!(dt.minute(), 0);
        assert_eq!(dt.second(), 0);

        // Year wrap-around: December -> January
        let Some(dec) = NaiveDate::from_ymd_opt(2026, 12, 1) else {
            return;
        };
        let ts_dec = next_month_timestamp(dec, tz);
        let Some(dt_dec) = tz.timestamp_opt(ts_dec, 0).single() else {
            return;
        };
        assert_eq!(dt_dec.year(), 2027);
        assert_eq!(dt_dec.month(), 1);
        assert_eq!(dt_dec.day(), 1);
    }
}
