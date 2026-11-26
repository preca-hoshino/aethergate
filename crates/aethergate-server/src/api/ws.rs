//! Persistent WebSocket endpoint `/api/v1/ws/realtime` for admin live monitoring.
//!
//! Automatically switches connected nodes to `Realtime` (5s) telemetry when at least one
//! admin WebSocket is connected, and returns to `Eco` (30s) when all viewers disconnect.

use std::sync::atomic::Ordering;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use futures::{SinkExt, StreamExt};

use aethergate_protocol::{NodeSseEvent, ReportCadenceMode};

use crate::auth::AdminUser;
use crate::state::{AppState, PanelWsEvent};
use crate::workers::fetch_live_node_metrics;

/// `GET /api/v1/ws/realtime` - upgrades an authenticated admin connection to WebSocket.
pub async fn ws_realtime(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_ws_connection(state, socket))
}

/// Handles a single admin WebSocket lifecycle with RAII cadence switching.
async fn handle_ws_connection(state: AppState, socket: WebSocket) {
    let prev = state.active_admin_ws.fetch_add(1, Ordering::AcqRel);
    if prev == 0 {
        state.broadcast_all_nodes(&NodeSseEvent::SwitchCadence {
            mode: ReportCadenceMode::Realtime,
        });
    }

    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.ws_tx.subscribe();

    if let Ok(nodes) = fetch_live_node_metrics(&state).await {
        let online_nodes = nodes.iter().filter(|n| n.status == "online").count();
        let snapshot = PanelWsEvent::Snapshot {
            online_nodes,
            cadence_mode: state.desired_cadence_mode(),
            nodes,
        };
        if let Ok(text) = serde_json::to_string(&snapshot) {
            let _ = sender.send(Message::Text(text.into())).await;
        }
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(event) => {
                        if let Ok(text) = serde_json::to_string(&event)
                            && sender.send(Message::Text(text.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    let remaining = state.active_admin_ws.fetch_sub(1, Ordering::AcqRel);
    if remaining <= 1 {
        state.broadcast_all_nodes(&NodeSseEvent::SwitchCadence {
            mode: ReportCadenceMode::Eco,
        });
    }
}
