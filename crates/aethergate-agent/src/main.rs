//! Æthergate Node Daemon (`aethergate-agent`) entrypoint.

mod clash_client;
mod client;
mod config;
mod stager;
mod stats_grpc;
mod supervisor;
mod sync_loop;
mod tls_helper;
mod wal;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::{AgentConfig, CliArgs};
use crate::sync_loop::run_agent_loop;

#[tokio::main]
async fn main() -> Result<()> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .map_err(|_| anyhow::anyhow!("failed to install aws-lc-rs crypto provider"))?;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("aethergate_agent=info")),
        )
        .init();

    let args = CliArgs::parse();
    let cfg = AgentConfig::load(&args.config)?;
    run_agent_loop(cfg).await
}
