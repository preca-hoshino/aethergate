//! Shared protocol models for the 4 initial `sing-box` inbound protocols and node TLS profile.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported proxy protocols in Phase 1 (single-node 4-protocol architecture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolType {
    /// VLESS over TCP + REALITY.
    VlessReality,
    /// Hysteria 2 over QUIC/UDP.
    Hysteria2,
    /// TUIC v5 over QUIC/UDP.
    Tuic,
    /// `AnyTLS` over TCP/TLS (`sing-box` >= 1.12.0).
    Anytls,
}

impl ProtocolType {
    /// Returns the underlying transport layer (`tcp` or `udp`) for port conflict checks.
    #[must_use]
    pub const fn transport_layer(self) -> TransportLayer {
        match self {
            Self::VlessReality | Self::Anytls => TransportLayer::Tcp,
            Self::Hysteria2 | Self::Tuic => TransportLayer::Udp,
        }
    }

    /// Returns the string identifier stored in PostgreSQL `node_inbounds.protocol`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VlessReality => "vless_reality",
            Self::Hysteria2 => "hysteria2",
            Self::Tuic => "tuic",
            Self::Anytls => "anytls",
        }
    }
}

/// Transport layer used for same-node port collision validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportLayer {
    /// Transmission Control Protocol.
    Tcp,
    /// User Datagram Protocol.
    Udp,
}

/// Hysteria 2 QUIC obfuscation configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hy2ObfsConfig {
    /// Obfuscation algorithm (`salamander` or `gecko`).
    #[serde(rename = "type")]
    pub obfs_type: String,
    /// Shared obfuscation password.
    pub password: String,
    /// Minimum on-wire packet size in bytes (`gecko` only, `sing-box` >= 1.14.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_packet_size: Option<u16>,
    /// Maximum on-wire packet size in bytes (`gecko` only, `sing-box` >= 1.14.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_packet_size: Option<u16>,
}

/// Protocol-specific configuration parameters stored in `node_inbounds.protocol_config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "snake_case")]
pub enum InboundProtocolConfig {
    /// VLESS + REALITY parameters.
    VlessReality {
        /// Target SNI presented during TLS handshake.
        server_name: String,
        /// Target fallback server for unauthenticated probes.
        handshake_server: String,
        /// Target fallback port (usually 443).
        handshake_port: u16,
        /// X25519 REALITY private key (decrypted only when compiling).
        private_key: String,
        /// X25519 REALITY public key (shared in user subscriptions).
        public_key: String,
        /// Allowed hex short IDs.
        short_ids: Vec<String>,
        /// XTLS flow control (default `xtls-rprx-vision`).
        flow: String,
        /// Client uTLS fingerprint (default `chrome`).
        fingerprint: String,
        /// Optional ALPN list for REALITY TLS handshake (e.g. `["h2", "http/1.1"]`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        alpn: Vec<String>,
        /// Optional REALITY `max_time_difference` (e.g. `"30s"`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_time_difference: Option<String>,
    },
    /// Hysteria 2 parameters.
    Hysteria2 {
        /// Optional server upload cap in Mbps.
        up_mbps: Option<u32>,
        /// Optional server download cap in Mbps.
        down_mbps: Option<u32>,
        /// Whether to ignore client-declared bandwidth.
        ignore_client_bandwidth: bool,
        /// Optional QUIC packet obfuscation.
        obfs: Option<Hy2ObfsConfig>,
        /// Optional HTTP/3 masquerade target URL or structured `sing-box` masquerade object.
        masquerade: Option<serde_json::Value>,
        /// ALPN list (default `["h3"]`).
        alpn: Vec<String>,
    },
    /// TUIC v5 parameters.
    Tuic {
        /// Congestion control algorithm (`bbr`, `cubic`, `new_reno`).
        congestion_control: String,
        /// UDP relay mode (`native` or `quic`).
        udp_relay_mode: String,
        /// Enable 0-RTT QUIC handshake.
        zero_rtt_handshake: bool,
        /// Authentication timeout (default `"3s"`).
        auth_timeout: String,
        /// Keepalive heartbeat interval (default `"10s"`).
        heartbeat: String,
        /// ALPN list (default `["h3"]`).
        alpn: Vec<String>,
    },
    /// `AnyTLS` parameters (`sing-box` >= 1.12.0).
    Anytls {
        /// Optional padding scheme lines; empty uses `sing-box` server default.
        padding_scheme: Vec<String>,
    },
}

impl InboundProtocolConfig {
    /// Returns the matching [`ProtocolType`].
    #[must_use]
    pub const fn protocol_type(&self) -> ProtocolType {
        match self {
            Self::VlessReality { .. } => ProtocolType::VlessReality,
            Self::Hysteria2 { .. } => ProtocolType::Hysteria2,
            Self::Tuic { .. } => ProtocolType::Tuic,
            Self::Anytls { .. } => ProtocolType::Anytls,
        }
    }
}

/// Single inbound definition attached to a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeInboundSpec {
    /// Inbound UUID.
    pub id: Uuid,
    /// Protocol type.
    pub protocol: ProtocolType,
    /// Unique inbound tag within the node.
    pub tag: String,
    /// Whether this inbound is active.
    pub enabled: bool,
    /// Listen IP (`::` or `0.0.0.0`).
    pub listen: String,
    /// Primary listen port.
    pub listen_port: u16,
    /// Optional UDP port hopping range for Hysteria 2 (e.g. `20000-30000`).
    pub port_hopping_range: Option<String>,
    /// Protocol-specific configuration.
    pub protocol_config: InboundProtocolConfig,
    /// Deterministic ordering key.
    pub sort_order: i16,
    /// Optional outbound tag routed from this inbound (defaults to `"direct"` when omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound_tag: Option<String>,
    /// Optional per-inbound authorized username filter (used when a physical host hosts multiple logical nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_usernames: Option<Vec<String>>,
}

/// Custom outbound definition attached to a node (e.g., policy-routed CERNET direct or upstream relay).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutboundSpec {
    /// Direct outbound with optional interface binding and Linux `fwmark`.
    Direct {
        /// Outbound tag (e.g. `"direct-campus"`).
        tag: String,
        /// Optional network interface name (`"wan"`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bind_interface: Option<String>,
        /// Optional Linux routing mark (`262144` == `0x40000`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        routing_mark: Option<u32>,
    },
    /// Upstream Hysteria 2 relay outbound (e.g. `cn-sha-02` -> `jp-hnd-01:40001`).
    Hysteria2 {
        /// Outbound tag (e.g. `"jp-hy2"`).
        tag: String,
        /// Upstream server host/IP.
        server: String,
        /// Upstream server port.
        server_port: u16,
        /// Relay authentication password (decrypted during compilation).
        password: String,
        /// Optional TLS SNI override.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_name: Option<String>,
        /// Whether to skip TLS verification (`true` for self-signed upstream).
        #[serde(default)]
        insecure: bool,
        /// Optional ALPN list (default `["h3"]`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        alpn: Vec<String>,
        /// Optional network interface name (`"wan"`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bind_interface: Option<String>,
        /// Optional Linux routing mark (`262144` == `0x40000`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        routing_mark: Option<u32>,
    },
}

/// Optional node-level DNS and routing policy overrides stored in `NodeTemplate.dns_route`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsRouteProfile {
    /// Optional primary DNS URL (`https://223.5.5.5:443/dns-query`, `tls://8.8.8.8`, or `1.1.1.1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_server: Option<String>,
    /// Optional TLS SNI for DoH/DoT DNS server (e.g. `"dns.alidns.com"`, `"dns.google"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_server_name: Option<String>,
    /// Optional `sing-box` DNS strategy (`"ipv4_only"`, `"prefer_ipv6"`, `"prefer_ipv4"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_strategy: Option<String>,
    /// Whether to hijack `protocol: "dns"` traffic in route rules.
    #[serde(default = "default_true")]
    pub hijack_dns: bool,
    /// `route.auto_detect_interface` flag (must be `false` on multi-WAN routers like `cn-sha-02`).
    #[serde(default = "default_true")]
    pub auto_detect_interface: bool,
    /// Whether to reject private/campus/Tailnet IP destinations (`false` on CERNET/Tailnet nodes).
    #[serde(default = "default_true")]
    pub reject_private_ip: bool,
}

impl Default for DnsRouteProfile {
    fn default() -> Self {
        Self {
            dns_server: None,
            dns_server_name: None,
            dns_strategy: None,
            hijack_dns: true,
            auto_detect_interface: true,
            reject_private_ip: true,
        }
    }
}

/// Default `true` helper for serde.
const fn default_true() -> bool {
    true
}

/// Default post-quantum hybrid TLS 1.3 key exchange curve preferences for `sing-box` 1.13+.
pub const DEFAULT_PQC_CURVE_PREFERENCES: &[&str] = &["X25519MLKEM768", "X25519", "P256"];

/// Node-level shared TLS certificate profile reused across Hysteria 2, TUIC, and `AnyTLS`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsProfile {
    /// Certificate mode: `external_files`, `self_signed`, or `singbox_acme`.
    pub mode: String,
    /// Expected TLS SNI / server name.
    pub server_name: String,
    /// Path to PEM certificate chain on the node filesystem.
    pub cert_path: String,
    /// Path to PEM private key on the node filesystem.
    pub key_path: String,
    /// Enables post-quantum hybrid key exchange (`X25519MLKEM768`) in `sing-box` 1.13+.
    #[serde(default = "default_true")]
    pub pqc_enabled: bool,
    /// Optional custom TLS `curve_preferences`; defaults to [`DEFAULT_PQC_CURVE_PREFERENCES`] when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curve_preferences: Vec<String>,
}

impl TlsProfile {
    /// Returns the effective TLS `curve_preferences` list when PQC is enabled.
    #[must_use]
    pub fn resolved_curve_preferences(&self) -> Vec<String> {
        if !self.curve_preferences.is_empty() {
            return self.curve_preferences.clone();
        }
        DEFAULT_PQC_CURVE_PREFERENCES
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }
}

impl Default for TlsProfile {
    fn default() -> Self {
        Self {
            mode: "self_signed".to_string(),
            server_name: String::new(),
            cert_path: "/etc/aethergate/certs/fullchain.pem".to_string(),
            key_path: "/etc/aethergate/certs/privkey.pem".to_string(),
            pqc_enabled: true,
            curve_preferences: Vec::new(),
        }
    }
}

/// Compact node template sent via `GET /api/open/v1/config?format=compact` without user lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeTemplate {
    /// Configured protocol inbounds for the node.
    pub inbounds: Vec<NodeInboundSpec>,
    /// Optional custom DNS/route overrides (`DnsRouteProfile` JSON).
    pub dns_route: serde_json::Value,
    /// Optional additional outbounds (policy-routed direct or upstream relay).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbounds: Vec<OutboundSpec>,
}

/// Deduplicated proxy user credential synced to a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedUserCredential {
    /// Internal user UUID.
    pub user_id: Uuid,
    /// Immutable statistics key injected into `users[].name` and `v2ray_api.stats.users`.
    pub proxy_username: String,
    /// User UUID for VLESS and TUIC.
    pub proxy_uuid: Uuid,
    /// Globally unique password for Hysteria 2, TUIC, and `AnyTLS`.
    pub proxy_password: String,
}
