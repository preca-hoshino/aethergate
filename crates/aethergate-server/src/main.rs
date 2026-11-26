//! Æthergate Control Plane binary entrypoint.

mod api;
mod auth;
mod config;
mod crypto;
mod error;
mod state;
mod subscription;
mod telemetry;
mod web_assets;
mod workers;

use anyhow::{Context, Result};
use chrono_tz::Tz;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

use crate::api::build_router;
use crate::config::{CliArgs, ServerConfig};
use crate::crypto::parse_master_key;
use crate::state::AppState;
use crate::workers::spawn_background_workers;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("aethergate_server=info,tower_http=info")),
        )
        .init();

    let args = CliArgs::parse();
    let cfg = ServerConfig::load(&args.config)?;
    let master_key = parse_master_key(&cfg.security.master_key_hex)?;
    let stats_tz: Tz = cfg
        .stats
        .timezone
        .parse()
        .context("invalid [stats].timezone IANA identifier")?;

    let db = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await
        .context("failed to connect to PostgreSQL")?;

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .context("failed to run database migrations")?;

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("failed to initialize outbound HTTP client")?;

    let listen_addr = cfg.server.listen_addr;
    let state = AppState::new(db, cfg, master_key, stats_tz, http_client);
    spawn_background_workers(state.clone());

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(listen_addr)
        .await
        .with_context(|| format!("failed to bind {listen_addr}"))?;

    tracing::info!(%listen_addr, "aethergate-server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server error")?;

    Ok(())
}

/// Waits for Ctrl-C / SIGTERM for graceful shutdown.
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
