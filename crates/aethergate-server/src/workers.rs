//! Background Tokio workers: node liveness sweep, session/dedup cleanup,
//! and dirty-driven WebSocket live metric broadcasting.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::time::interval;

use crate::state::{AppState, LiveNodeMetric, PanelWsEvent};

/// Spawns background maintenance and live-push tasks for the control plane.
pub fn spawn_background_workers(state: AppState) {
    let liveness_state = state.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            if let Err(err) = run_liveness_and_cleanup_tick(&liveness_state).await {
                tracing::warn!(error = %err, "liveness/cleanup tick failed");
            }
        }
    });

    let ws_state = state;
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(2));
        loop {
            ticker.tick().await;
            if ws_state.active_admin_ws.load(Ordering::Acquire) == 0 {
                continue;
            }
            if !ws_state.ws_dirty.swap(false, Ordering::AcqRel) {
                continue;
            }
            if let Err(err) = push_live_ws_tick(&ws_state).await {
                tracing::warn!(error = %err, "ws live push failed");
            }
        }
    });
}

/// Marks silent nodes offline and purges expired sessions, OIDC states, and old dedup keys.
async fn run_liveness_and_cleanup_tick(state: &AppState) -> Result<(), sqlx::Error> {
    let offline_secs = state.config.stats.node_offline_secs;
    let res = sqlx::query(
        r"
        UPDATE nodes
        SET status = 'offline', active_connections = 0, active_users = 0, updated_at = NOW()
        WHERE status <> 'offline'
          AND last_seen_at IS NOT NULL
          AND last_seen_at < NOW() - ($1 * INTERVAL '1 second')
        ",
    )
    .bind(offline_secs)
    .execute(&state.db)
    .await?;

    if res.rows_affected() > 0 {
        state.mark_ws_dirty();
    }

    sqlx::query("DELETE FROM user_sessions WHERE expires_at <= NOW()")
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM oidc_states WHERE expires_at <= NOW()")
        .execute(&state.db)
        .await?;
    sqlx::query("DELETE FROM telemetry_batch_dedup WHERE received_at < NOW() - INTERVAL '7 days'")
        .execute(&state.db)
        .await?;

    Ok(())
}

/// Queries compact node metrics and broadcasts a `LiveTick` to connected admin `WebSockets`.
async fn push_live_ws_tick(state: &AppState) -> Result<(), sqlx::Error> {
    let nodes = fetch_live_node_metrics(state).await?;
    let _ = state.ws_tx.send(PanelWsEvent::LiveTick {
        cadence_mode: state.desired_cadence_mode(),
        nodes,
    });
    Ok(())
}

/// Loads compact live status rows for all nodes.
///
/// # Errors
/// Returns a database error if querying `nodes` fails.
pub async fn fetch_live_node_metrics(state: &AppState) -> Result<Vec<LiveNodeMetric>, sqlx::Error> {
    sqlx::query_as::<_, LiveNodeMetric>(
        r"
        SELECT
            id, name, status, cpu_pct, mem_pct,
            active_connections, active_users, uptime_secs
        FROM nodes
        ORDER BY name ASC
        ",
    )
    .fetch_all(&state.db)
    .await
}
