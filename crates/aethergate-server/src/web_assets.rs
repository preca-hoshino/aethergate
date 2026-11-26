//! Embedded frontend asset server (`rust-embed`) with pre-compressed `.br`/`.gz`
//! negotiation and SPA route whitelisting to return 0-byte `404` for scanner probes.

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

/// Embedded production frontend bundle built from `frontend/dist`.
#[derive(Debug, Embed)]
#[folder = "../../frontend/dist"]
struct FrontendAssets;

/// Known client-side SPA route prefixes that should fall back to `index.html`.
const SPA_ROUTE_PREFIXES: &[&str] = &[
    "",
    "sign-in",
    "nodes",
    "node-groups",
    "users",
    "analytics",
    "audit-logs",
    "settings",
    "portal",
];

/// Returns `true` if `path` (without leading `/`) is a legitimate SPA route.
#[must_use]
pub fn is_allowed_spa_route(path: &str) -> bool {
    let trimmed = path.trim_matches('/');
    let first_segment = trimmed.split('/').next().unwrap_or("");
    SPA_ROUTE_PREFIXES.contains(&first_segment)
}

/// Serves embedded static assets or whitelisted SPA `index.html`, returning an empty
/// `404 Not Found` for unknown paths (e.g. `.env`, `wp-login.php`, scanner probes).
pub async fn serve_embedded_asset(uri: Uri, headers: HeaderMap) -> Response {
    let raw_path = uri.path().trim_start_matches('/');
    let accept_encoding = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !raw_path.is_empty() && FrontendAssets::get(raw_path).is_some() {
        return serve_file_with_encoding(
            raw_path,
            accept_encoding,
            raw_path.starts_with("assets/"),
        );
    }

    // Do not serve SPA fallback for API paths, hidden files, or file-like scanner probes.
    if raw_path.starts_with("api/") || raw_path.starts_with("sub/") || raw_path.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }

    if is_allowed_spa_route(raw_path) {
        return serve_file_with_encoding("index.html", accept_encoding, false);
    }

    StatusCode::NOT_FOUND.into_response()
}

/// Serves a specific embedded file, preferring `.br` then `.gz` when accepted by the client.
fn serve_file_with_encoding(path: &str, accept_encoding: &str, immutable: bool) -> Response {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned();

    let (asset, encoding_header) = if accept_encoding.contains("br")
        && let Some(br_asset) = FrontendAssets::get(&format!("{path}.br"))
    {
        (Some(br_asset), Some("br"))
    } else if accept_encoding.contains("gzip")
        && let Some(gz_asset) = FrontendAssets::get(&format!("{path}.gz"))
    {
        (Some(gz_asset), Some("gzip"))
    } else {
        (FrontendAssets::get(path), None)
    };

    let Some(file) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(
            header::CACHE_CONTROL,
            if immutable {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            },
        );

    if let Some(enc) = encoding_header {
        builder = builder
            .header(header::CONTENT_ENCODING, enc)
            .header(header::VARY, "Accept-Encoding");
    }

    let body = Body::from(file.data.into_owned());
    builder.body(body).unwrap_or_else(|_| {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spa_whitelist_rejects_scanner_probes() {
        assert!(is_allowed_spa_route(""));
        assert!(is_allowed_spa_route("/nodes/123"));
        assert!(is_allowed_spa_route("analytics"));
        assert!(is_allowed_spa_route("sign-in"));
        assert!(!is_allowed_spa_route("wp-login.php"));
        assert!(!is_allowed_spa_route(".env"));
        assert!(!is_allowed_spa_route("admin/secret"));
    }
}
