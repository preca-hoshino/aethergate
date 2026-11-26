//! Agent configuration loading (TOML + CLI args).

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use uuid::Uuid;

/// CLI arguments for `aethergate-agent`.
#[derive(Debug, Parser)]
#[command(name = "aethergate-agent", version, about = "Æthergate Node Daemon")]
pub struct CliArgs {
    /// Path to the agent TOML configuration file.
    #[arg(short, long, default_value = "config.agent.toml")]
    pub config: PathBuf,
}

/// Top-level node agent configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// Assigned node UUID from the control plane.
    pub node_id: Uuid,
    /// Shared HMAC-SHA256 secret (hex or UTF-8 string).
    pub hmac_secret: String,
    /// Control plane base URL (e.g. `https://panel.example.com`).
    pub server_url: String,
    /// Path to the `sing-box` binary.
    #[serde(default = "default_singbox_bin")]
    pub singbox_bin: PathBuf,
    /// Active `sing-box` JSON configuration output path.
    #[serde(default = "default_singbox_config_path")]
    pub singbox_config_path: PathBuf,
    /// Directory storing auto-generated or external TLS certificates.
    #[serde(default = "default_cert_dir")]
    pub cert_dir: PathBuf,
    /// Directory storing local WAL telemetry batches during outages.
    #[serde(default = "default_wal_dir")]
    pub wal_dir: PathBuf,
    /// Local `sing-box` `V2Ray` gRPC stats address.
    #[serde(default = "default_v2ray_api_addr")]
    pub v2ray_api_addr: String,
    /// Local `sing-box` Clash REST API base URL.
    #[serde(default = "default_clash_api_addr")]
    pub clash_api_addr: String,
    /// Local `sing-box` Clash API bearer secret.
    #[serde(default = "default_clash_secret")]
    pub clash_secret: String,
    /// Internal gRPC sampling interval in seconds (default 2s).
    #[serde(default = "default_sample_interval_secs")]
    pub sample_interval_secs: u64,
}

/// Default `sing-box` binary path.
fn default_singbox_bin() -> PathBuf {
    PathBuf::from("sing-box")
}

/// Default active `sing-box` config path.
fn default_singbox_config_path() -> PathBuf {
    PathBuf::from("/etc/aethergate/sing-box.json")
}

/// Default certificate directory.
fn default_cert_dir() -> PathBuf {
    PathBuf::from("/etc/aethergate/certs")
}

/// Default WAL directory.
fn default_wal_dir() -> PathBuf {
    PathBuf::from("/var/lib/aethergate/wal")
}

/// Default `V2Ray` gRPC listen address.
fn default_v2ray_api_addr() -> String {
    "127.0.0.1:10085".to_owned()
}

/// Default Clash REST API URL.
fn default_clash_api_addr() -> String {
    "http://127.0.0.1:9090".to_owned()
}

/// Default Clash API secret.
fn default_clash_secret() -> String {
    "aethergate-local-clash".to_owned()
}

/// Default gRPC sample interval.
const fn default_sample_interval_secs() -> u64 {
    2
}

impl AgentConfig {
    /// Loads configuration from `path`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed as TOML.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read agent config at {}", path.display()))?;
        let mut cfg: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse agent TOML at {}", path.display()))?;

        if let Ok(url) = std::env::var("AETHERGATE_SERVER_URL") {
            cfg.server_url = url;
        }
        if let Ok(secret) = std::env::var("AETHERGATE_HMAC_SECRET") {
            cfg.hmac_secret = secret;
        }
        Ok(cfg)
    }
}
