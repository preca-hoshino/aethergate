//! OIDC BFF authentication, `HttpOnly` session management, CSRF protection, and role extractors.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::{generate_random_token, hash_token, pkce_s256_challenge};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Cookie name storing the opaque session token.
pub const SESSION_COOKIE_NAME: &str = "ag_session";

/// Header name required on state-changing panel API calls (`POST`, `PUT`, `PATCH`, `DELETE`).
pub const CSRF_HEADER_NAME: &str = "x-csrf-token";

/// Authenticated user profile loaded from `user_sessions` + `users`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuthUser {
    /// User UUID.
    pub id: Uuid,
    /// OIDC subject identifier.
    pub oidc_subject: String,
    /// Optional email address.
    pub email: Option<String>,
    /// Optional OIDC profile picture URL.
    pub avatar_url: Option<String>,
    /// Display name shown in the panel.
    pub display_name: String,
    /// Role (`"admin"` or `"user"`).
    pub role: String,
    /// Panel account status (`"active"` or `"disabled"`).
    pub panel_status: String,
    /// Whether proxy access is enabled by an admin.
    pub proxy_enabled: bool,
    /// Immutable proxy username.
    pub proxy_username: String,
    /// Proxy UUID for VLESS/TUIC.
    pub proxy_uuid: Uuid,
    /// Globally unique proxy password for Hy2/TUIC/`AnyTLS`.
    pub proxy_password: String,
    /// Subscription token for `/sub/{token}`.
    pub sub_token: String,
    /// Session CSRF token.
    pub csrf_token: String,
    /// Account creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Extractor requiring any active authenticated user (plus CSRF check on mutating methods).
#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub AuthUser);

/// Extractor requiring an authenticated `admin` user.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_session_cookie(&parts.headers)
            .ok_or_else(|| AppError::Unauthorized("missing session cookie".to_owned()))?;
        let token_hash = hash_token(&token);

        let user = sqlx::query_as::<_, AuthUser>(
            r"
            SELECT
                u.id, u.oidc_subject, u.email, u.avatar_url, u.display_name, u.role,
                u.panel_status, u.proxy_enabled, u.proxy_username,
                u.proxy_uuid, u.proxy_password, u.sub_token,
                s.csrf_token, u.created_at
            FROM user_sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.session_token_hash = $1 AND s.expires_at > NOW()
            ",
        )
        .bind(&token_hash)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("invalid or expired session".to_owned()))?;

        if user.panel_status != "active" {
            return Err(AppError::Forbidden("account is disabled".to_owned()));
        }

        if is_mutating_method(&parts.method) {
            let provided_csrf = parts
                .headers
                .get(CSRF_HEADER_NAME)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if provided_csrf != user.csrf_token {
                return Err(AppError::Forbidden("invalid CSRF token".to_owned()));
            }
        }

        Ok(Self(user))
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let AuthenticatedUser(user) = AuthenticatedUser::from_request_parts(parts, state).await?;
        if user.role != "admin" {
            return Err(AppError::Forbidden(
                "administrator role required".to_owned(),
            ));
        }
        Ok(Self(user))
    }
}

/// Returns `true` for HTTP methods that mutate state and require CSRF verification.
fn is_mutating_method(method: &axum::http::Method) -> bool {
    matches!(
        *method,
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::PATCH
            | axum::http::Method::DELETE
    )
}

/// Extracts the `ag_session` cookie value from request headers.
#[must_use]
pub fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        let name = parts.next()?.trim();
        let value = parts.next()?.trim();
        if name == SESSION_COOKIE_NAME && !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

/// Public auth provider metadata returned by `GET /api/v1/auth/providers`.
#[derive(Debug, Serialize)]
pub struct AuthProvidersResponse {
    /// Whether OIDC login is configured and enabled.
    pub oidc_enabled: bool,
    /// Display name for the OIDC button (e.g., `"Logto"`).
    pub provider_name: String,
    /// Login URL to redirect the browser to.
    pub login_url: &'static str,
}

/// `GET /api/v1/auth/providers`
pub async fn get_auth_providers(State(state): State<AppState>) -> Json<AuthProvidersResponse> {
    Json(AuthProvidersResponse {
        oidc_enabled: state.config.oidc.enabled,
        provider_name: state.config.oidc.provider_name.clone(),
        login_url: "/api/v1/auth/oidc/login",
    })
}

/// Discovered OIDC endpoints.
#[derive(Debug, Deserialize)]
#[expect(clippy::struct_field_names)]
struct OidcDiscoveryDoc {
    /// Authorization endpoint URL.
    #[serde(rename = "authorization_endpoint")]
    auth_url: String,
    /// Token endpoint URL.
    #[serde(rename = "token_endpoint")]
    token_url: String,
    /// Userinfo endpoint URL.
    #[serde(rename = "userinfo_endpoint")]
    userinfo_url: Option<String>,
}

/// Resolves OIDC endpoints from explicit overrides or `.well-known/openid-configuration`.
async fn resolve_oidc_endpoints(state: &AppState) -> AppResult<OidcDiscoveryDoc> {
    let oidc = &state.config.oidc;
    if !oidc.enabled {
        return Err(AppError::BadRequest(
            "OIDC authentication is disabled".to_owned(),
        ));
    }

    if let (Some(auth), Some(token)) = (&oidc.authorization_endpoint, &oidc.token_endpoint) {
        return Ok(OidcDiscoveryDoc {
            auth_url: auth.clone(),
            token_url: token.clone(),
            userinfo_url: oidc.userinfo_endpoint.clone(),
        });
    }

    let issuer = oidc.issuer.trim_end_matches('/');
    if issuer.is_empty() {
        return Err(AppError::BadRequest(
            "OIDC issuer is not configured".to_owned(),
        ));
    }
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let doc = state
        .http_client
        .get(&discovery_url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC discovery failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC discovery HTTP error: {e}")))?
        .json::<OidcDiscoveryDoc>()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC discovery JSON error: {e}")))?;

    Ok(doc)
}

/// Query params for `GET /api/v1/auth/oidc/login`.
#[derive(Debug, Deserialize)]
pub struct OidcLoginQuery {
    /// Optional post-login path within the SPA.
    pub redirect: Option<String>,
}

/// `GET /api/v1/auth/oidc/login` - starts Authorization Code + PKCE flow.
pub async fn oidc_login(
    State(state): State<AppState>,
    Query(query): Query<OidcLoginQuery>,
) -> AppResult<Redirect> {
    let doc = resolve_oidc_endpoints(&state).await?;
    let oidc_state = generate_random_token(24);
    let pkce_verifier = generate_random_token(32);
    let nonce = generate_random_token(24);
    let code_challenge = pkce_s256_challenge(&pkce_verifier);
    let redirect_to = sanitize_redirect_path(query.redirect.as_deref());
    let expires_at = Utc::now() + Duration::minutes(10);

    sqlx::query(
        "INSERT INTO oidc_states (state, pkce_verifier, nonce, redirect_to, expires_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&oidc_state)
    .bind(&pkce_verifier)
    .bind(&nonce)
    .bind(&redirect_to)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    let redirect_uri = state.config.effective_oidc_redirect_uri();
    let sep = if doc.auth_url.contains('?') { '&' } else { '?' };
    let auth_url = format!(
        "{}{}response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
        doc.auth_url,
        sep,
        urlencoding::encode(&state.config.oidc.client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&state.config.oidc.scopes),
        urlencoding::encode(&oidc_state),
        urlencoding::encode(&nonce),
        urlencoding::encode(&code_challenge),
    );

    Ok(Redirect::temporary(&auth_url))
}

/// Sanitizes post-login redirect paths to prevent open redirects.
#[must_use]
pub fn sanitize_redirect_path(raw: Option<&str>) -> String {
    match raw {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_owned(),
        _ => "/".to_owned(),
    }
}

/// Query params for `GET /api/v1/auth/oidc/callback`.
#[derive(Debug, Deserialize)]
pub struct OidcCallbackQuery {
    /// Authorization code from `IdP`.
    pub code: Option<String>,
    /// Anti-CSRF state token.
    pub state: Option<String>,
    /// Optional OAuth error code returned by `IdP`.
    pub error: Option<String>,
    /// Optional human-readable error description returned by `IdP`.
    pub error_description: Option<String>,
}

/// Stored PKCE state row.
#[derive(Debug, sqlx::FromRow)]
struct OidcStateRow {
    /// PKCE code verifier.
    pkce_verifier: String,
    /// Post-login SPA path.
    redirect_to: String,
}

/// OIDC token endpoint response.
#[derive(Debug, Deserialize)]
struct OidcTokenResponse {
    /// Access token used to query `userinfo`.
    access_token: String,
}

/// OIDC userinfo claims.
#[derive(Debug, Deserialize)]
struct OidcUserInfo {
    /// Subject identifier.
    sub: String,
    /// Optional email.
    email: Option<String>,
    /// Optional display name.
    name: Option<String>,
    /// Optional preferred username.
    preferred_username: Option<String>,
    /// Optional avatar URL.
    picture: Option<String>,
}

/// `GET /api/v1/auth/oidc/callback` - exchanges code, runs JIT user provisioning, issues `HttpOnly` session.
pub async fn oidc_callback(
    State(state): State<AppState>,
    Query(query): Query<OidcCallbackQuery>,
) -> AppResult<Response> {
    if let Some(err) = query.error {
        let detail = query
            .error_description
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| err.clone());
        return Err(AppError::Unauthorized(format!(
            "OIDC provider error ({err}): {detail}"
        )));
    }
    let code = query
        .code
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("missing OIDC authorization code".to_owned()))?;
    let state_token = query
        .state
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("missing OIDC state".to_owned()))?;

    let doc = resolve_oidc_endpoints(&state).await?;

    let state_row = sqlx::query_as::<_, OidcStateRow>(
        "DELETE FROM oidc_states WHERE state = $1 AND expires_at > NOW() RETURNING pkce_verifier, redirect_to",
    )
    .bind(&state_token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("invalid or expired OIDC state".to_owned()))?;

    let redirect_uri = state.config.effective_oidc_redirect_uri();
    let mut form: HashMap<&str, String> = HashMap::new();
    form.insert("grant_type", "authorization_code".to_owned());
    form.insert("code", code);
    form.insert("redirect_uri", redirect_uri);
    form.insert("client_id", state.config.oidc.client_id.clone());
    if !state.config.oidc.client_secret.is_empty() {
        form.insert("client_secret", state.config.oidc.client_secret.clone());
    }
    form.insert("code_verifier", state_row.pkce_verifier);

    let token_resp = state
        .http_client
        .post(&doc.token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC token exchange failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Unauthorized(format!("OIDC token exchange rejected: {e}")))?
        .json::<OidcTokenResponse>()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC token JSON error: {e}")))?;

    let userinfo_url = doc
        .userinfo_url
        .ok_or_else(|| AppError::BadRequest("OIDC userinfo_endpoint missing".to_owned()))?;

    let claims = state
        .http_client
        .get(&userinfo_url)
        .bearer_auth(&token_resp.access_token)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC userinfo failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Unauthorized(format!("OIDC userinfo rejected: {e}")))?
        .json::<OidcUserInfo>()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC userinfo JSON error: {e}")))?;

    let user_id = upsert_jit_user(&state, &claims).await?;
    let raw_session = generate_random_token(32);
    let session_hash = hash_token(&raw_session);
    let csrf_token = generate_random_token(24);
    let expires_at = Utc::now() + Duration::hours(state.config.security.session_ttl_hours);

    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, session_token_hash, csrf_token, expires_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&session_hash)
    .bind(&csrf_token)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    let cookie_val = build_session_cookie(
        &raw_session,
        state.config.server.cookie_secure,
        state.config.security.session_ttl_hours * 3600,
    );
    let mut response = Redirect::to(&state_row.redirect_to).into_response();
    if let Ok(hv) = HeaderValue::from_str(&cookie_val) {
        response.headers_mut().append(SET_COOKIE, hv);
    }
    Ok(response)
}

/// Normalizes an OIDC `picture` claim to a safe HTTP(S) URL.
#[must_use]
pub fn normalize_avatar_url(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return None;
    }
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

/// Inserts or updates a JIT user from OIDC claims; new self-registered users start with
/// `role='user'`, `proxy_enabled=false` unless listed in bootstrap admin config.
async fn upsert_jit_user(state: &AppState, claims: &OidcUserInfo) -> AppResult<Uuid> {
    let avatar_url = normalize_avatar_url(claims.picture.as_deref());
    let email = claims
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_owned);

    if let Some((existing_id,)) =
        sqlx::query_as::<_, (Uuid,)>("SELECT id FROM users WHERE oidc_subject = $1")
            .bind(&claims.sub)
            .fetch_optional(&state.db)
            .await?
    {
        sqlx::query(
            r"
            UPDATE users
            SET avatar_url = COALESCE($2, avatar_url),
                email = COALESCE($3, email),
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(existing_id)
        .bind(&avatar_url)
        .bind(&email)
        .execute(&state.db)
        .await?;
        return Ok(existing_id);
    }

    let is_bootstrap_admin = state
        .config
        .oidc
        .bootstrap_admin_subjects
        .iter()
        .any(|s| s == &claims.sub)
        || email.as_deref().is_some_and(|em| {
            state
                .config
                .oidc
                .bootstrap_admin_emails
                .iter()
                .any(|ae| ae.eq_ignore_ascii_case(em))
        });

    let role = if is_bootstrap_admin { "admin" } else { "user" };
    let proxy_enabled = is_bootstrap_admin;
    let user_id = Uuid::new_v4();
    let short_id = user_id.simple().to_string();
    let proxy_username = format!("ag_{}", &short_id[..12]);
    let proxy_uuid = Uuid::new_v4();
    let proxy_password = generate_random_token(24);
    let sub_token = generate_random_token(24);
    let display_name = claims
        .name
        .clone()
        .or_else(|| claims.preferred_username.clone())
        .or_else(|| email.clone())
        .unwrap_or_else(|| proxy_username.clone());

    sqlx::query(
        r"
        INSERT INTO users (
            id, oidc_subject, email, avatar_url, display_name, role, panel_status,
            proxy_enabled, proxy_username, proxy_uuid, proxy_password, sub_token
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8, $9, $10, $11)
        ",
    )
    .bind(user_id)
    .bind(&claims.sub)
    .bind(&email)
    .bind(&avatar_url)
    .bind(&display_name)
    .bind(role)
    .bind(proxy_enabled)
    .bind(&proxy_username)
    .bind(proxy_uuid)
    .bind(&proxy_password)
    .bind(&sub_token)
    .execute(&state.db)
    .await?;

    Ok(user_id)
}

/// Logout response payload.
#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    /// Optional `IdP` end-session URL if configured.
    pub end_session_url: Option<String>,
}

/// `POST /api/v1/auth/logout`
pub async fn logout(
    State(state): State<AppState>,
    AuthenticatedUser(_user): AuthenticatedUser,
    headers: HeaderMap,
) -> AppResult<Response> {
    if let Some(token) = extract_session_cookie(&headers) {
        let token_hash = hash_token(&token);
        sqlx::query("DELETE FROM user_sessions WHERE session_token_hash = $1")
            .bind(&token_hash)
            .execute(&state.db)
            .await?;
    }

    let clear_cookie = build_session_cookie("", state.config.server.cookie_secure, 0);
    let body = Json(LogoutResponse {
        end_session_url: state.config.oidc.end_session_endpoint.clone(),
    });
    let mut resp = (StatusCode::OK, body).into_response();
    if let Ok(hv) = HeaderValue::from_str(&clear_cookie) {
        resp.headers_mut().append(SET_COOKIE, hv);
    }
    Ok(resp)
}

/// `GET /api/v1/auth/me`
pub async fn get_me(AuthenticatedUser(user): AuthenticatedUser) -> Json<AuthUser> {
    Json(user)
}

/// Formats an `HttpOnly`, `SameSite=Lax` session cookie header value.
#[must_use]
fn build_session_cookie(value: &str, secure: bool, max_age_secs: i64) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure_attr}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_sanitizer_blocks_external_urls() {
        assert_eq!(sanitize_redirect_path(Some("/nodes/1")), "/nodes/1");
        assert_eq!(sanitize_redirect_path(Some("https://evil.example")), "/");
        assert_eq!(sanitize_redirect_path(Some("//evil.example")), "/");
        assert_eq!(sanitize_redirect_path(None), "/");
    }

    #[test]
    fn avatar_url_normalizer_accepts_only_http_urls() {
        assert_eq!(
            normalize_avatar_url(Some("  https://cdn.example/a.png ")).as_deref(),
            Some("https://cdn.example/a.png")
        );
        assert_eq!(normalize_avatar_url(Some("javascript:alert(1)")), None);
        assert_eq!(normalize_avatar_url(Some("   ")), None);
        assert_eq!(normalize_avatar_url(None), None);
    }
}
