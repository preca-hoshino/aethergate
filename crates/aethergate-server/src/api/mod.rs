//! Axum HTTP router assembly combining Open API, Panel REST/WS, Subscriptions, and SPA assets.

pub mod open;
pub mod panel;
pub mod ws;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

use crate::auth::{get_auth_providers, get_me, logout, oidc_callback, oidc_login};
use crate::state::AppState;
use crate::subscription::get_subscription;
use crate::web_assets::serve_embedded_asset;

/// Health check endpoint for Docker/load balancers.
async fn healthz() -> StatusCode {
    StatusCode::OK
}

/// Builds the complete Axum router with middleware layers.
pub fn build_router(state: AppState) -> Router {
    let open_api = Router::new()
        .route("/auth/verify", post(open::open_auth_verify))
        .route("/auth/client", post(open::open_auth_client))
        .route("/config", get(open::open_get_config))
        .route("/users", get(open::open_get_users))
        .route("/report/status", post(open::open_report_status))
        .route("/report/telemetry", post(open::open_report_telemetry))
        .route("/events", get(open::open_sse_events))
        .route("/commands/{id}/ack", post(open::open_ack_command));

    let panel_api = Router::new()
        .route("/auth/providers", get(get_auth_providers))
        .route("/auth/oidc/login", get(oidc_login))
        .route("/auth/oidc/callback", get(oidc_callback))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(get_me))
        .route("/ws/realtime", get(ws::ws_realtime))
        .route("/nodes", get(panel::list_nodes).post(panel::create_node))
        .route(
            "/nodes/reality-keypair",
            post(panel::generate_reality_keypair_endpoint),
        )
        .route(
            "/nodes/{id}",
            get(panel::get_node_detail)
                .patch(panel::update_node)
                .delete(panel::delete_node),
        )
        .route("/nodes/{id}/inbounds", put(panel::replace_node_inbounds))
        .route(
            "/nodes/{id}/tls-profile",
            put(panel::update_node_tls_profile),
        )
        .route("/nodes/{id}/rotate-secret", post(panel::rotate_node_secret))
        .route("/nodes/{id}/commands", post(panel::issue_node_command))
        .route(
            "/node-groups",
            get(panel::list_node_groups).post(panel::create_node_group),
        )
        .route(
            "/node-groups/{id}",
            axum::routing::delete(panel::delete_node_group),
        )
        .route(
            "/node-groups/{id}/members",
            put(panel::replace_node_group_members),
        )
        .route("/users", get(panel::list_users))
        .route(
            "/users/{id}",
            get(panel::get_user_detail).patch(panel::update_user),
        )
        .route("/users/{id}/approve", post(panel::approve_user))
        .route("/users/{id}/grants", put(panel::replace_user_grants))
        .route(
            "/users/{id}/reset-sub-token",
            post(panel::reset_user_sub_token),
        )
        .route("/users/{id}/kick", post(panel::kick_user_connections))
        .route("/analytics/overview", get(panel::get_analytics_overview))
        .route("/analytics/users", get(panel::get_analytics_users))
        .route("/analytics/nodes", get(panel::get_analytics_nodes))
        .route("/analytics/matrix", get(panel::get_analytics_matrix))
        .route("/analytics/protocols", get(panel::get_analytics_protocols))
        .route(
            "/analytics/timeseries",
            get(panel::get_analytics_timeseries),
        )
        .route("/audit-logs", get(panel::list_audit_logs))
        .route("/portal/summary", get(panel::get_portal_summary))
        .route(
            "/portal/subscriptions",
            get(panel::get_portal_subscriptions),
        );

    Router::new()
        .route("/healthz", get(healthz))
        .route("/sub/{token}", get(get_subscription))
        .nest("/api/open/v1", open_api)
        .nest("/api/v1", panel_api)
        .fallback(serve_embedded_asset)
        .with_state(state)
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}
