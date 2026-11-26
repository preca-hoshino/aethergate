//! Server configuration loading (TOML + environment overrides + CLI args).

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;

/// CLI arguments for `aethergate-server`.
#[derive(Debug, Parser)]
#[command(name = "aethergate-server", version, about = "Æthergate Control Plane")]
pub struct CliArgs {
    /// Path to the TOML configuration file.
    #[arg(short, long, default_value = "config.server.toml")]
    pub config: PathBuf,
}

/// Top-level control plane configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// HTTP server settings.
    #[serde(default)]
    pub server: HttpServerConfig,
    /// PostgreSQL / `TimescaleDB` settings.
    pub database: DatabaseConfig,
    /// Generic OIDC / Logto settings.
    #[serde(default)]
    pub oidc: OidcConfig,
    /// Master encryption & session security settings.
    pub security: SecurityConfig,
    /// Monthly statistics & timezone settings.
    #[serde(default)]
    pub stats: StatsConfig,
}

/// HTTP listener configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpServerConfig {
    /// Socket address to bind (`0.0.0.0:8080`).
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,
    /// Public base URL used for OIDC callbacks and subscription links.
    #[serde(default = "default_public_base_url")]
    pub public_base_url: String,
    /// Whether cookies should set the `Secure` attribute.
    #[serde(default)]
    pub cookie_secure: bool,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            public_base_url: default_public_base_url(),
            cookie_secure: false,
        }
    }
}

/// Default HTTP bind address.
fn default_listen_addr() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8080))
}

/// Default public base URL.
fn default_public_base_url() -> String {
    "http://localhost:8080".to_owned()
}

/// Database connection pool configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection string (`postgres://...`).
    pub url: String,
    /// Maximum pool connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

/// Default DB pool size.
const fn default_max_connections() -> u32 {
    20
}

/// OIDC / Logto BFF configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OidcConfig {
    /// Whether OIDC login is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Display name for the OIDC provider button (e.g., `"Logto"`).
    #[serde(default = "default_provider_name")]
    pub provider_name: String,
    /// OIDC issuer URL (without trailing slash).
    #[serde(default)]
    pub issuer: String,
    /// `OAuth2` client ID.
    #[serde(default)]
    pub client_id: String,
    /// `OAuth2` client secret.
    #[serde(default)]
    pub client_secret: String,
    /// Redirect URI registered at the `IdP` (defaults to `{public_base_url}/api/v1/auth/oidc/callback`).
    #[serde(default)]
    pub redirect_uri: Option<String>,
    /// Comma/space-separated scopes (`openid profile email`).
    #[serde(default = "default_scopes")]
    pub scopes: String,
    /// Optional OIDC `sub` values automatically promoted to `admin` on first login.
    #[serde(default)]
    pub bootstrap_admin_subjects: Vec<String>,
    /// Optional email addresses automatically promoted to `admin` on first login.
    #[serde(default)]
    pub bootstrap_admin_emails: Vec<String>,
    /// Optional explicit authorization endpoint override (skips discovery when set).
    #[serde(default)]
    pub authorization_endpoint: Option<String>,
    /// Optional explicit token endpoint override.
    #[serde(default)]
    pub token_endpoint: Option<String>,
    /// Optional explicit userinfo endpoint override.
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    /// Optional explicit end-session endpoint override.
    #[serde(default)]
    pub end_session_endpoint: Option<String>,
}

/// Default OIDC provider label.
fn default_provider_name() -> String {
    "Logto / OIDC".to_owned()
}

/// Default OIDC scopes.
fn default_scopes() -> String {
    "openid profile email".to_owned()
}

/// Cryptographic and session configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    /// 32-byte hex-encoded master key used to encrypt node HMAC secrets with AES-256-GCM.
    pub master_key_hex: String,
    /// Session cookie lifetime in hours.
    #[serde(default = "default_session_ttl_hours")]
    pub session_ttl_hours: i64,
    /// Maximum allowed clock skew for node HMAC signatures in seconds.
    #[serde(default = "default_hmac_skew_secs")]
    pub hmac_max_skew_secs: i64,
}

/// Default session lifetime (7 days).
const fn default_session_ttl_hours() -> i64 {
    168
}

/// Default HMAC timestamp skew tolerance.
const fn default_hmac_skew_secs() -> i64 {
    60
}

/// Monthly statistics configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StatsConfig {
    /// IANA timezone used to compute natural calendar month boundaries (`stat_month`).
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// Seconds without heartbeat before a node is marked offline.
    #[serde(default = "default_node_offline_secs")]
    pub node_offline_secs: i64,
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self {
            timezone: default_timezone(),
            node_offline_secs: default_node_offline_secs(),
        }
    }
}

/// Default stats timezone.
fn default_timezone() -> String {
    "Asia/Shanghai".to_owned()
}

/// Default offline threshold.
const fn default_node_offline_secs() -> i64 {
    95
}

impl ServerConfig {
    /// Loads configuration from `path` (if present) and applies `AETHERGATE_*` env overrides.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read/parsed or required env vars are missing.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let mut cfg: Self = if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read server config at {}", path.display()))?;
            toml::from_str(&content)
                .with_context(|| format!("failed to parse TOML config at {}", path.display()))?
        } else {
            let db_url = std::env::var("AETHERGATE_DATABASE_URL")
                .context("config file missing and AETHERGATE_DATABASE_URL is not set")?;
            let master_key_hex = std::env::var("AETHERGATE_MASTER_KEY_HEX")
                .context("config file missing and AETHERGATE_MASTER_KEY_HEX is not set")?;
            Self {
                server: HttpServerConfig::default(),
                database: DatabaseConfig {
                    url: db_url,
                    max_connections: default_max_connections(),
                },
                oidc: OidcConfig::default(),
                security: SecurityConfig {
                    master_key_hex,
                    session_ttl_hours: default_session_ttl_hours(),
                    hmac_max_skew_secs: default_hmac_skew_secs(),
                },
                stats: StatsConfig::default(),
            }
        };

        if let Ok(url) = std::env::var("AETHERGATE_DATABASE_URL") {
            cfg.database.url = url;
        }
        if let Ok(key) = std::env::var("AETHERGATE_MASTER_KEY_HEX") {
            cfg.security.master_key_hex = key;
        }
        if let Ok(base) = std::env::var("AETHERGATE_PUBLIC_BASE_URL") {
            cfg.server.public_base_url = base;
        }
        if let Ok(listen) = std::env::var("AETHERGATE_LISTEN_ADDR") {
            cfg.server.listen_addr = listen
                .parse()
                .context("invalid AETHERGATE_LISTEN_ADDR socket address")?;
        }
        if let Ok(tz) = std::env::var("AETHERGATE_STATS_TIMEZONE") {
            cfg.stats.timezone = tz;
        }
        if let Ok(enabled) = std::env::var("AETHERGATE_OIDC_ENABLED") {
            cfg.oidc.enabled = enabled == "1" || enabled.eq_ignore_ascii_case("true");
        }
        if let Ok(issuer) = std::env::var("AETHERGATE_OIDC_ISSUER") {
            cfg.oidc.issuer = issuer;
        }
        if let Ok(cid) = std::env::var("AETHERGATE_OIDC_CLIENT_ID") {
            cfg.oidc.client_id = cid;
        }
        if let Ok(csec) = std::env::var("AETHERGATE_OIDC_CLIENT_SECRET") {
            cfg.oidc.client_secret = csec;
        }
        Ok(cfg)
    }

    /// Resolves the effective OIDC callback URL.
    #[must_use]
    pub fn effective_oidc_redirect_uri(&self) -> String {
        self.oidc.redirect_uri.clone().unwrap_or_else(|| {
            format!(
                "{}/api/v1/auth/oidc/callback",
                self.server.public_base_url.trim_end_matches('/')
            )
        })
    }
}
