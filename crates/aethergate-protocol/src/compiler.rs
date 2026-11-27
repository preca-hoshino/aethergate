//! Shared `sing-box` 1.12+/1.13+ 4-protocol configuration compiler and port conflict validator.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::protocol::{
    AuthorizedUserCredential, DnsRouteProfile, InboundProtocolConfig, NodeInboundSpec,
    NodeTemplate, OutboundSpec, TlsProfile, TransportLayer,
};

/// Errors produced during port validation or `sing-box` JSON compilation.
#[derive(Debug, Error)]
pub enum CompileError {
    /// Two enabled inbounds on the same transport layer conflict on a port or range.
    #[error("port conflict on {layer:?} port {port}: '{first_tag}' overlaps with '{second_tag}'")]
    PortConflict {
        /// Conflicting transport layer.
        layer: TransportLayer,
        /// Conflicting port number.
        port: u16,
        /// First inbound tag.
        first_tag: String,
        /// Second inbound tag.
        second_tag: String,
    },
    /// Invalid port hopping range syntax.
    #[error("invalid port hopping range '{range}' on inbound '{tag}'")]
    InvalidPortRange {
        /// Inbound tag.
        tag: String,
        /// Invalid range string.
        range: String,
    },
    /// Mismatch between `NodeInboundSpec.protocol` and `InboundProtocolConfig` variant.
    #[error("protocol mismatch on inbound '{tag}'")]
    ProtocolMismatch {
        /// Inbound tag.
        tag: String,
    },
    /// JSON serialization error.
    #[error("serialization failure: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Node-local overrides applied by `aethergate-agent` before running `sing-box check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAgentOverrides {
    /// Local loopback gRPC listen address (`127.0.0.1:10085`).
    pub v2ray_api_listen: String,
    /// Local loopback Clash API listen address (`127.0.0.1:19090`).
    pub clash_api_listen: String,
    /// Ephemeral high-entropy Clash API bearer secret.
    pub clash_api_secret: String,
    /// Optional local override for TLS certificate path.
    pub cert_path_override: Option<String>,
    /// Optional local override for TLS private key path.
    pub key_path_override: Option<String>,
}

impl Default for LocalAgentOverrides {
    fn default() -> Self {
        Self {
            v2ray_api_listen: "127.0.0.1:10085".to_string(),
            clash_api_listen: "127.0.0.1:19090".to_string(),
            clash_api_secret: "ag_local_clash_preview_secret".to_string(),
            cert_path_override: None,
            key_path_override: None,
        }
    }
}

/// Parses a `start-end` port hopping range such as `20000-30000`.
fn parse_port_range(tag: &str, range: &str) -> Result<(u16, u16), CompileError> {
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() != 2 {
        return Err(CompileError::InvalidPortRange {
            tag: tag.to_string(),
            range: range.to_string(),
        });
    }
    let start: u16 = parts[0]
        .trim()
        .parse()
        .map_err(|_| CompileError::InvalidPortRange {
            tag: tag.to_string(),
            range: range.to_string(),
        })?;
    let end: u16 = parts[1]
        .trim()
        .parse()
        .map_err(|_| CompileError::InvalidPortRange {
            tag: tag.to_string(),
            range: range.to_string(),
        })?;
    if start == 0 || start > end {
        return Err(CompileError::InvalidPortRange {
            tag: tag.to_string(),
            range: range.to_string(),
        });
    }
    Ok((start, end))
}

/// Validates that enabled inbounds do not overlap on the same transport layer (TCP vs UDP).
///
/// TCP and UDP are allowed to share the same port (e.g., VLESS-REALITY on TCP 443 and
/// Hysteria 2 on UDP 443).
///
/// # Errors
/// Returns [`CompileError::PortConflict`] or [`CompileError::InvalidPortRange`].
pub fn validate_inbound_ports(inbounds: &[NodeInboundSpec]) -> Result<(), CompileError> {
    let mut tcp_ports: BTreeMap<u16, String> = BTreeMap::new();
    let mut udp_ranges: Vec<(u16, u16, String)> = Vec::new();

    for inbound in inbounds {
        if !inbound.enabled {
            continue;
        }
        if inbound.protocol != inbound.protocol_config.protocol_type() {
            return Err(CompileError::ProtocolMismatch {
                tag: inbound.tag.clone(),
            });
        }
        match inbound.protocol.transport_layer() {
            TransportLayer::Tcp => {
                if let Some(existing) = tcp_ports.insert(inbound.listen_port, inbound.tag.clone()) {
                    return Err(CompileError::PortConflict {
                        layer: TransportLayer::Tcp,
                        port: inbound.listen_port,
                        first_tag: existing,
                        second_tag: inbound.tag.clone(),
                    });
                }
            }
            TransportLayer::Udp => {
                let current_ranges = match inbound.port_hopping_range.as_deref() {
                    Some(raw) if !raw.trim().is_empty() => {
                        let (start, end) = parse_port_range(&inbound.tag, raw)?;
                        vec![(inbound.listen_port, inbound.listen_port), (start, end)]
                    }
                    _ => vec![(inbound.listen_port, inbound.listen_port)],
                };

                for (s1, e1) in current_ranges {
                    for (s2, e2, existing_tag) in &udp_ranges {
                        if s1 <= *e2 && *s2 <= e1 {
                            return Err(CompileError::PortConflict {
                                layer: TransportLayer::Udp,
                                port: s1.max(*s2),
                                first_tag: existing_tag.clone(),
                                second_tag: inbound.tag.clone(),
                            });
                        }
                    }
                    udp_ranges.push((s1, e1, inbound.tag.clone()));
                }
            }
        }
    }
    Ok(())
}

/// Compiles a complete `sing-box` 1.12+/1.13+ JSON configuration from node template,
/// shared TLS profile, deduplicated user list, and node-local loopback overrides.
///
/// # Errors
/// Returns [`CompileError`] if port validation fails or JSON serialization fails.
pub fn compile_singbox_config(
    template: &NodeTemplate,
    tls: &TlsProfile,
    users: &[AuthorizedUserCredential],
    overrides: &LocalAgentOverrides,
) -> Result<Value, CompileError> {
    validate_inbound_ports(&template.inbounds)?;

    let mut sorted_inbounds: Vec<&NodeInboundSpec> =
        template.inbounds.iter().filter(|i| i.enabled).collect();
    sorted_inbounds.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then_with(|| a.tag.cmp(&b.tag))
    });

    let mut sorted_users: Vec<&AuthorizedUserCredential> = users.iter().collect();
    sorted_users.sort_by(|a, b| a.proxy_username.cmp(&b.proxy_username));

    let cert_path = overrides
        .cert_path_override
        .as_deref()
        .unwrap_or(&tls.cert_path);
    let key_path = overrides
        .key_path_override
        .as_deref()
        .unwrap_or(&tls.key_path);

    let dns_route: DnsRouteProfile = if template.dns_route.is_null() {
        DnsRouteProfile::default()
    } else {
        serde_json::from_value(template.dns_route.clone()).unwrap_or_default()
    };

    let mut compiled_inbounds = Vec::with_capacity(sorted_inbounds.len());
    let mut stat_inbound_tags = Vec::with_capacity(sorted_inbounds.len());
    let mut inbound_route_rules: Vec<Value> = Vec::new();

    for inbound in sorted_inbounds {
        stat_inbound_tags.push(inbound.tag.clone());
        let inbound_users: Vec<&AuthorizedUserCredential> =
            if let Some(allowed) = &inbound.allowed_usernames {
                sorted_users
                    .iter()
                    .copied()
                    .filter(|u| allowed.iter().any(|name| name == &u.proxy_username))
                    .collect()
            } else {
                sorted_users.clone()
            };

        compiled_inbounds.push(compile_single_inbound(
            inbound,
            tls,
            cert_path,
            key_path,
            &inbound_users,
        ));

        if let Some(out_tag) = inbound.outbound_tag.as_deref().filter(|t| !t.is_empty())
            && out_tag != "direct"
        {
            inbound_route_rules.push(json!({
                "inbound": [inbound.tag],
                "outbound": out_tag
            }));
        }
    }

    let stat_users: Vec<&str> = sorted_users
        .iter()
        .map(|u| u.proxy_username.as_str())
        .collect();

    let (compiled_outbounds, stat_outbound_tags) = compile_outbounds(&template.outbounds);
    let dns_block = compile_dns_block(&dns_route);
    let route_block = compile_route_block(&dns_route, inbound_route_rules);

    Ok(json!({
        "log": {
            "disabled": false,
            "level": "info",
            "timestamp": true
        },
        "dns": dns_block,
        "experimental": {
            "v2ray_api": {
                "listen": overrides.v2ray_api_listen,
                "stats": {
                    "enabled": true,
                    "inbounds": stat_inbound_tags,
                    "outbounds": stat_outbound_tags,
                    "users": stat_users
                }
            },
            "clash_api": {
                "external_controller": overrides.clash_api_listen,
                "secret": overrides.clash_api_secret
            }
        },
        "inbounds": compiled_inbounds,
        "outbounds": compiled_outbounds,
        "route": route_block
    }))
}

/// Returns the default DNS server tag (`"dns-bootstrap"` when a custom `dns_server` is configured, else `"dns-local"`).
fn default_dns_tag(dns_route: &DnsRouteProfile) -> &'static str {
    if dns_route
        .dns_server
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        "dns-bootstrap"
    } else {
        "dns-local"
    }
}

/// Splits an authority string (`host`, `host:port`, `[ipv6]`, or `[ipv6]:port`) into `(host, port)`.
fn split_authority_host_port(authority: &str) -> (&str, Option<u16>) {
    if let Some(rest) = authority.strip_prefix('[')
        && let Some((ipv6, after_bracket)) = rest.split_once(']')
    {
        let port = after_bracket
            .strip_prefix(':')
            .and_then(|p| p.parse::<u16>().ok());
        return (ipv6, port);
    }
    if let Some((host, port_raw)) = authority.rsplit_once(':')
        && !host.contains(':')
        && let Ok(port) = port_raw.parse::<u16>()
    {
        return (host, Some(port));
    }
    (authority, None)
}

/// Compiles the `dns` block using [`DnsRouteProfile`].
fn compile_dns_block(dns_route: &DnsRouteProfile) -> Value {
    let mut bootstrap = serde_json::Map::new();
    bootstrap.insert("tag".to_string(), json!("dns-bootstrap"));
    let server = dns_route.dns_server.as_deref().unwrap_or("1.1.1.1").trim();

    let host_for_ip_check = if let Some(host_port_path) = server.strip_prefix("https://") {
        let (authority, path_rest) = match host_port_path.split_once('/') {
            Some((a, p)) if !p.is_empty() => (a, Some(format!("/{p}"))),
            Some((a, _)) => (a, None),
            None => (host_port_path, None),
        };
        let (parsed_host, port) = split_authority_host_port(authority);
        let host = if parsed_host.is_empty() {
            "223.5.5.5"
        } else {
            parsed_host
        };
        bootstrap.insert("type".to_string(), json!("https"));
        bootstrap.insert("server".to_string(), json!(host));
        if let Some(port) = port {
            bootstrap.insert("server_port".to_string(), json!(port));
        }
        if let Some(path) = path_rest {
            bootstrap.insert("path".to_string(), json!(path));
        }
        if let Some(sni) = dns_route
            .dns_server_name
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            bootstrap.insert(
                "tls".to_string(),
                json!({ "enabled": true, "server_name": sni }),
            );
        }
        host
    } else if let Some(host_port) = server.strip_prefix("tls://") {
        let (authority, _) = host_port.split_once('/').unwrap_or((host_port, ""));
        let (parsed_host, port) = split_authority_host_port(authority);
        let host = if parsed_host.is_empty() {
            "223.5.5.5"
        } else {
            parsed_host
        };
        bootstrap.insert("type".to_string(), json!("tls"));
        bootstrap.insert("server".to_string(), json!(host));
        if let Some(port) = port {
            bootstrap.insert("server_port".to_string(), json!(port));
        }
        if let Some(sni) = dns_route
            .dns_server_name
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            bootstrap.insert(
                "tls".to_string(),
                json!({ "enabled": true, "server_name": sni }),
            );
        }
        host
    } else {
        let bare = server.strip_prefix("udp://").unwrap_or(server);
        let (parsed_host, port) = split_authority_host_port(bare);
        let host = if parsed_host.is_empty() {
            "1.1.1.1"
        } else {
            parsed_host
        };
        bootstrap.insert("type".to_string(), json!("udp"));
        bootstrap.insert("server".to_string(), json!(host));
        if let Some(port) = port {
            bootstrap.insert("server_port".to_string(), json!(port));
        }
        host
    };

    if host_for_ip_check.parse::<std::net::IpAddr>().is_err() {
        bootstrap.insert("domain_resolver".to_string(), json!("dns-local"));
    }

    let mut dns_obj = serde_json::Map::new();
    dns_obj.insert(
        "servers".to_string(),
        json!([
            {
                "type": "local",
                "tag": "dns-local"
            },
            Value::Object(bootstrap)
        ]),
    );
    dns_obj.insert("final".to_string(), json!(default_dns_tag(dns_route)));
    if let Some(strategy) = dns_route.dns_strategy.as_deref().filter(|s| !s.is_empty()) {
        dns_obj.insert("strategy".to_string(), json!(strategy));
    }
    Value::Object(dns_obj)
}

/// Compiles `outbounds` (always includes `"direct"`, plus any custom [`OutboundSpec`]s).
fn compile_outbounds(custom: &[OutboundSpec]) -> (Vec<Value>, Vec<String>) {
    let mut outbounds = vec![json!({
        "type": "direct",
        "tag": "direct"
    })];
    let mut tags = vec!["direct".to_string()];

    for spec in custom {
        match spec {
            OutboundSpec::Direct {
                tag,
                bind_interface,
                routing_mark,
            } => {
                if tag == "direct" {
                    continue;
                }
                let mut obj = serde_json::Map::new();
                obj.insert("type".to_string(), json!("direct"));
                obj.insert("tag".to_string(), json!(tag));
                if let Some(iface) = bind_interface.as_deref().filter(|s| !s.is_empty()) {
                    obj.insert("bind_interface".to_string(), json!(iface));
                }
                if let Some(mark) = routing_mark {
                    obj.insert("routing_mark".to_string(), json!(mark));
                }
                tags.push(tag.clone());
                outbounds.push(Value::Object(obj));
            }
            OutboundSpec::Hysteria2 {
                tag,
                server,
                server_port,
                password,
                server_name,
                insecure,
                alpn,
                bind_interface,
                routing_mark,
            } => {
                let mut tls_obj = serde_json::Map::new();
                tls_obj.insert("enabled".to_string(), json!(true));
                tls_obj.insert("insecure".to_string(), json!(insecure));
                if let Some(sni) = server_name.as_deref().filter(|s| !s.is_empty()) {
                    tls_obj.insert("server_name".to_string(), json!(sni));
                }
                if !alpn.is_empty() {
                    tls_obj.insert("alpn".to_string(), json!(alpn));
                }

                let mut obj = serde_json::Map::new();
                obj.insert("type".to_string(), json!("hysteria2"));
                obj.insert("tag".to_string(), json!(tag));
                obj.insert("server".to_string(), json!(server));
                obj.insert("server_port".to_string(), json!(server_port));
                obj.insert("password".to_string(), json!(password));
                obj.insert("tls".to_string(), Value::Object(tls_obj));
                if let Some(iface) = bind_interface.as_deref().filter(|s| !s.is_empty()) {
                    obj.insert("bind_interface".to_string(), json!(iface));
                }
                if let Some(mark) = routing_mark {
                    obj.insert("routing_mark".to_string(), json!(mark));
                }
                tags.push(tag.clone());
                outbounds.push(Value::Object(obj));
            }
        }
    }

    (outbounds, tags)
}

/// Compiles `route` including `default_domain_resolver`, `hijack-dns`, per-inbound rules, and private-IP policy.
fn compile_route_block(dns_route: &DnsRouteProfile, inbound_rules: Vec<Value>) -> Value {
    let mut rules = vec![json!({ "action": "sniff" })];
    if dns_route.hijack_dns {
        rules.push(json!({ "protocol": "dns", "action": "hijack-dns" }));
    }
    rules.extend(inbound_rules);
    if dns_route.reject_private_ip {
        rules.push(json!({ "ip_is_private": true, "action": "reject" }));
    }

    json!({
        "rules": rules,
        "final": "direct",
        "auto_detect_interface": dns_route.auto_detect_interface,
        "default_domain_resolver": default_dns_tag(dns_route)
    })
}

/// Renders a single protocol inbound block according to `sing-box` 1.12+/1.13+ schema.
fn compile_single_inbound(
    inbound: &NodeInboundSpec,
    tls: &TlsProfile,
    cert_path: &str,
    key_path: &str,
    users: &[&AuthorizedUserCredential],
) -> Value {
    match &inbound.protocol_config {
        InboundProtocolConfig::VlessReality { .. } => compile_vless_reality(inbound, users),
        InboundProtocolConfig::Hysteria2 { .. } => {
            compile_hysteria2(inbound, tls, cert_path, key_path, users)
        }
        InboundProtocolConfig::Tuic { .. } => {
            compile_tuic(inbound, tls, cert_path, key_path, users)
        }
        InboundProtocolConfig::Anytls { .. } => {
            compile_anytls(inbound, tls, cert_path, key_path, users)
        }
    }
}

/// Compiles a VLESS + Reality inbound block.
fn compile_vless_reality(inbound: &NodeInboundSpec, users: &[&AuthorizedUserCredential]) -> Value {
    let InboundProtocolConfig::VlessReality {
        server_name,
        handshake_server,
        handshake_port,
        private_key,
        short_ids,
        flow,
        alpn,
        max_time_difference,
        ..
    } = &inbound.protocol_config
    else {
        return json!({});
    };

    let vless_users: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.proxy_username,
                "uuid": u.proxy_uuid,
                "flow": flow
            })
        })
        .collect();

    let mut reality_obj = serde_json::Map::new();
    reality_obj.insert("enabled".to_string(), json!(true));
    reality_obj.insert(
        "handshake".to_string(),
        json!({
            "server": handshake_server,
            "server_port": handshake_port
        }),
    );
    reality_obj.insert("private_key".to_string(), json!(private_key));
    reality_obj.insert("short_id".to_string(), json!(short_ids));
    if let Some(mtd) = max_time_difference.as_deref().filter(|s| !s.is_empty()) {
        reality_obj.insert("max_time_difference".to_string(), json!(mtd));
    }

    let mut tls_obj = serde_json::Map::new();
    tls_obj.insert("enabled".to_string(), json!(true));
    tls_obj.insert("server_name".to_string(), json!(server_name));
    if !alpn.is_empty() {
        tls_obj.insert("alpn".to_string(), json!(alpn));
    }
    tls_obj.insert("reality".to_string(), Value::Object(reality_obj));

    json!({
        "type": "vless",
        "tag": inbound.tag,
        "listen": inbound.listen,
        "listen_port": inbound.listen_port,
        "tcp_fast_open": true,
        "users": vless_users,
        "tls": Value::Object(tls_obj)
    })
}

/// Compiles a Hysteria 2 inbound block.
fn compile_hysteria2(
    inbound: &NodeInboundSpec,
    tls: &TlsProfile,
    cert_path: &str,
    key_path: &str,
    users: &[&AuthorizedUserCredential],
) -> Value {
    let InboundProtocolConfig::Hysteria2 {
        up_mbps,
        down_mbps,
        ignore_client_bandwidth,
        obfs,
        masquerade,
        alpn,
    } = &inbound.protocol_config
    else {
        return json!({});
    };

    let hy2_users: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.proxy_username,
                "password": u.proxy_password
            })
        })
        .collect();
    let mut obj = serde_json::Map::new();
    obj.insert("type".to_string(), json!("hysteria2"));
    obj.insert("tag".to_string(), json!(inbound.tag));
    obj.insert("listen".to_string(), json!(inbound.listen));
    obj.insert("listen_port".to_string(), json!(inbound.listen_port));
    if let Some(up) = up_mbps {
        obj.insert("up_mbps".to_string(), json!(up));
    }
    if let Some(down) = down_mbps {
        obj.insert("down_mbps".to_string(), json!(down));
    }
    if *ignore_client_bandwidth {
        obj.insert("ignore_client_bandwidth".to_string(), json!(true));
    }
    if let Some(obfs_cfg) = obfs {
        obj.insert("obfs".to_string(), json!(obfs_cfg));
    }
    if let Some(masq) = masquerade
        .as_ref()
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        obj.insert("masquerade".to_string(), masq.clone());
    }
    obj.insert("users".to_string(), json!(hy2_users));
    obj.insert(
        "tls".to_string(),
        shared_tls_block(tls, cert_path, key_path, Some(alpn)),
    );
    Value::Object(obj)
}

/// Compiles a TUIC v5 inbound block (note: `udp_relay_mode` is client-outbound-only in `sing-box` 1.12+).
fn compile_tuic(
    inbound: &NodeInboundSpec,
    tls: &TlsProfile,
    cert_path: &str,
    key_path: &str,
    users: &[&AuthorizedUserCredential],
) -> Value {
    let InboundProtocolConfig::Tuic {
        congestion_control,
        zero_rtt_handshake,
        auth_timeout,
        heartbeat,
        alpn,
        ..
    } = &inbound.protocol_config
    else {
        return json!({});
    };

    let tuic_users: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.proxy_username,
                "uuid": u.proxy_uuid,
                "password": u.proxy_password
            })
        })
        .collect();
    json!({
        "type": "tuic",
        "tag": inbound.tag,
        "listen": inbound.listen,
        "listen_port": inbound.listen_port,
        "congestion_control": congestion_control,
        "zero_rtt_handshake": zero_rtt_handshake,
        "auth_timeout": auth_timeout,
        "heartbeat": heartbeat,
        "users": tuic_users,
        "tls": shared_tls_block(tls, cert_path, key_path, Some(alpn))
    })
}

/// Compiles an `AnyTLS` inbound block.
fn compile_anytls(
    inbound: &NodeInboundSpec,
    tls: &TlsProfile,
    cert_path: &str,
    key_path: &str,
    users: &[&AuthorizedUserCredential],
) -> Value {
    let InboundProtocolConfig::Anytls { padding_scheme } = &inbound.protocol_config else {
        return json!({});
    };

    let anytls_users: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.proxy_username,
                "password": u.proxy_password
            })
        })
        .collect();
    let mut obj = serde_json::Map::new();
    obj.insert("type".to_string(), json!("anytls"));
    obj.insert("tag".to_string(), json!(inbound.tag));
    obj.insert("listen".to_string(), json!(inbound.listen));
    obj.insert("listen_port".to_string(), json!(inbound.listen_port));
    obj.insert("users".to_string(), json!(anytls_users));
    if !padding_scheme.is_empty() {
        obj.insert("padding_scheme".to_string(), json!(padding_scheme));
    }
    obj.insert(
        "tls".to_string(),
        shared_tls_block(tls, cert_path, key_path, None),
    );
    Value::Object(obj)
}

/// Builds the shared TLS configuration block for Hysteria 2, TUIC, and `AnyTLS`.
fn shared_tls_block(
    tls: &TlsProfile,
    cert_path: &str,
    key_path: &str,
    alpn: Option<&Vec<String>>,
) -> Value {
    let mut tls_map = serde_json::Map::new();
    tls_map.insert("enabled".to_string(), json!(true));
    if !tls.server_name.is_empty() {
        tls_map.insert("server_name".to_string(), json!(tls.server_name));
    }
    if let Some(a) = alpn.filter(|v| !v.is_empty()) {
        tls_map.insert("alpn".to_string(), json!(a));
    }
    tls_map.insert("certificate_path".to_string(), json!(cert_path));
    tls_map.insert("key_path".to_string(), json!(key_path));
    Value::Object(tls_map)
}

/// Computes a deterministic `sha256:<hex>` `ETag` over a serializable value with sorted object keys.
///
/// # Errors
/// Returns [`CompileError::Serialization`] if serialization fails.
pub fn compute_deterministic_etag<T: Serialize>(value: &T) -> Result<String, CompileError> {
    let raw = serde_json::to_value(value)?;
    let canonical = canonicalize_json(raw);
    let bytes = serde_json::to_vec(&canonical)?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("sha256:{}", hex::encode(digest)))
}

/// Recursively sorts JSON object keys into deterministic `BTreeMap` order.
fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map
                .into_iter()
                .map(|(k, v)| (k, canonicalize_json(v)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(canonicalize_json).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::protocol::ProtocolType;

    fn sample_inbounds() -> Vec<NodeInboundSpec> {
        vec![
            NodeInboundSpec {
                id: Uuid::new_v4(),
                protocol: ProtocolType::VlessReality,
                tag: "vless-reality-in".to_string(),
                enabled: true,
                listen: "::".to_string(),
                listen_port: 443,
                port_hopping_range: None,
                protocol_config: InboundProtocolConfig::VlessReality {
                    server_name: "gateway.icloud.com".to_string(),
                    handshake_server: "gateway.icloud.com".to_string(),
                    handshake_port: 443,
                    private_key: "priv_test".to_string(),
                    public_key: "pub_test".to_string(),
                    short_ids: vec!["0123456789abcdef".to_string()],
                    flow: "xtls-rprx-vision".to_string(),
                    fingerprint: "chrome".to_string(),
                    alpn: vec!["h2".to_string(), "http/1.1".to_string()],
                    max_time_difference: Some("30s".to_string()),
                },
                sort_order: 1,
                outbound_tag: None,
                allowed_usernames: None,
            },
            NodeInboundSpec {
                id: Uuid::new_v4(),
                protocol: ProtocolType::Hysteria2,
                tag: "hysteria2-in".to_string(),
                enabled: true,
                listen: "::".to_string(),
                listen_port: 443,
                port_hopping_range: Some("20000-30000".to_string()),
                protocol_config: InboundProtocolConfig::Hysteria2 {
                    up_mbps: Some(200),
                    down_mbps: Some(500),
                    ignore_client_bandwidth: false,
                    obfs: None,
                    masquerade: Some(json!("https://bing.com")),
                    alpn: vec!["h3".to_string()],
                },
                sort_order: 2,
                outbound_tag: None,
                allowed_usernames: None,
            },
            NodeInboundSpec {
                id: Uuid::new_v4(),
                protocol: ProtocolType::Tuic,
                tag: "tuic-in".to_string(),
                enabled: true,
                listen: "::".to_string(),
                listen_port: 9443,
                port_hopping_range: None,
                protocol_config: InboundProtocolConfig::Tuic {
                    congestion_control: "bbr".to_string(),
                    udp_relay_mode: "native".to_string(),
                    zero_rtt_handshake: false,
                    auth_timeout: "3s".to_string(),
                    heartbeat: "10s".to_string(),
                    alpn: vec!["h3".to_string()],
                },
                sort_order: 3,
                outbound_tag: None,
                allowed_usernames: None,
            },
            NodeInboundSpec {
                id: Uuid::new_v4(),
                protocol: ProtocolType::Anytls,
                tag: "anytls-in".to_string(),
                enabled: true,
                listen: "::".to_string(),
                listen_port: 8443,
                port_hopping_range: None,
                protocol_config: InboundProtocolConfig::Anytls {
                    padding_scheme: vec!["stop=8".to_string()],
                },
                sort_order: 4,
                outbound_tag: None,
                allowed_usernames: None,
            },
        ]
    }

    #[test]
    fn allows_tcp_443_and_udp_443_coexistence_and_compiles_all_4() {
        let template = NodeTemplate {
            inbounds: sample_inbounds(),
            dns_route: json!({}),
            outbounds: vec![],
        };
        let users = vec![AuthorizedUserCredential {
            user_id: Uuid::new_v4(),
            proxy_username: "u_alice01".to_string(),
            proxy_uuid: Uuid::new_v4(),
            proxy_password: "unique_password_alice".to_string(),
        }];

        let compiled = compile_singbox_config(
            &template,
            &TlsProfile::default(),
            &users,
            &LocalAgentOverrides::default(),
        );
        assert!(compiled.is_ok());
        if let Ok(cfg) = compiled {
            assert_eq!(cfg["inbounds"].as_array().map(Vec::len), Some(4));
            assert_eq!(cfg["dns"]["servers"][0]["type"], "local");
            assert_eq!(cfg["route"]["rules"][0]["action"], "sniff");
            assert_eq!(cfg["route"]["default_domain_resolver"], "dns-local");
            assert!(cfg["inbounds"][2].get("udp_relay_mode").is_none());
            assert_eq!(
                cfg["experimental"]["v2ray_api"]["stats"]["users"][0],
                "u_alice01"
            );
        }
    }

    #[test]
    fn rejects_udp_port_hopping_overlap() {
        let mut inbounds = sample_inbounds();
        // Set TUIC UDP port inside Hysteria 2 hopping range 20000-30000
        inbounds[2].listen_port = 25000;
        let res = validate_inbound_ports(&inbounds);
        assert!(matches!(res, Err(CompileError::PortConflict { .. })));
    }

    #[test]
    fn compiles_custom_doh_and_multi_outbound_with_per_inbound_users() {
        let mut inbounds = sample_inbounds();
        inbounds[0].outbound_tag = Some("direct-campus".to_string());
        inbounds[0].allowed_usernames = Some(vec!["u_alice01".to_string()]);
        inbounds[1].outbound_tag = Some("jp-hy2".to_string());

        let template = NodeTemplate {
            inbounds,
            dns_route: json!({
                "dns_server": "https://dns.alidns.com:443/dns-query",
                "dns_server_name": "dns.alidns.com",
                "dns_strategy": "prefer_ipv4",
                "hijack_dns": true,
                "auto_detect_interface": true,
                "reject_private_ip": true
            }),
            outbounds: vec![
                OutboundSpec::Direct {
                    tag: "direct-campus".to_string(),
                    bind_interface: Some("wan".to_string()),
                    routing_mark: Some(262_144),
                },
                OutboundSpec::Hysteria2 {
                    tag: "jp-hy2".to_string(),
                    server: "arts-jp-hnd-01.proxy.restart.org.cn".to_string(),
                    server_port: 40001,
                    password: "relay_password".to_string(),
                    server_name: Some("jp.example.com".to_string()),
                    insecure: false,
                    alpn: vec!["h3".to_string()],
                    bind_interface: Some("wan".to_string()),
                    routing_mark: Some(262_144),
                },
            ],
        };
        let users = vec![
            AuthorizedUserCredential {
                user_id: Uuid::new_v4(),
                proxy_username: "u_alice01".to_string(),
                proxy_uuid: Uuid::new_v4(),
                proxy_password: "pw1".to_string(),
            },
            AuthorizedUserCredential {
                user_id: Uuid::new_v4(),
                proxy_username: "u_bob02".to_string(),
                proxy_uuid: Uuid::new_v4(),
                proxy_password: "pw2".to_string(),
            },
        ];

        let cfg = compile_singbox_config(
            &template,
            &TlsProfile::default(),
            &users,
            &LocalAgentOverrides::default(),
        );
        assert!(cfg.is_ok());
        if let Ok(cfg) = cfg {
            assert_eq!(cfg["dns"]["final"], "dns-bootstrap");
            assert_eq!(cfg["dns"]["servers"][1]["type"], "https");
            assert_eq!(cfg["dns"]["servers"][1]["server"], "dns.alidns.com");
            assert_eq!(cfg["dns"]["servers"][1]["server_port"], 443);
            assert_eq!(cfg["dns"]["servers"][1]["path"], "/dns-query");
            assert_eq!(cfg["dns"]["servers"][1]["domain_resolver"], "dns-local");
            assert_eq!(cfg["route"]["default_domain_resolver"], "dns-bootstrap");
            assert_eq!(cfg["outbounds"].as_array().map(Vec::len), Some(3));
            assert_eq!(
                cfg["inbounds"][0]["users"].as_array().map(Vec::len),
                Some(1)
            );
            assert_eq!(cfg["inbounds"][0]["users"][0]["name"], "u_alice01");
            assert_eq!(
                cfg["inbounds"][1]["users"].as_array().map(Vec::len),
                Some(2)
            );
        }
    }
}
