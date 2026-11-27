//! OIDC BFF authentication, `HttpOnly` session management, CSRF protection, and role extractors.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::{
    decrypt_secret, encrypt_secret, generate_random_token, hash_token, pkce_s256_challenge,
};
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
    /// Optional refresh token for background profile/avatar sync.
    #[serde(default)]
    refresh_token: Option<String>,
    /// Optional access token lifetime in seconds.
    #[serde(default)]
    expires_in: Option<i64>,
    /// Optional signed ID token JWT containing fallback profile claims.
    #[serde(default)]
    id_token: Option<String>,
}

/// OIDC userinfo / ID token claims.
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

/// Optional partial claims decoded from an ID token JWT payload.
#[derive(Debug, Deserialize)]
struct OidcIdTokenClaims {
    /// Subject identifier.
    sub: Option<String>,
    /// Optional email.
    email: Option<String>,
    /// Optional display name.
    name: Option<String>,
    /// Optional preferred username.
    preferred_username: Option<String>,
    /// Optional avatar URL.
    picture: Option<String>,
}

/// Decodes the unverified JSON payload from a JWT `id_token` returned directly by the token endpoint.
fn decode_id_token_claims(id_token: &str) -> Option<OidcIdTokenClaims> {
    let payload_b64 = id_token.split('.').nth(1)?.trim_end_matches('=');
    let raw = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    serde_json::from_slice::<OidcIdTokenClaims>(&raw).ok()
}

/// Returns the first non-empty trimmed string from `primary` then `fallback`.
fn first_non_empty(primary: Option<String>, fallback: Option<String>) -> Option<String> {
    primary
        .filter(|s| !s.trim().is_empty())
        .or_else(|| fallback.filter(|s| !s.trim().is_empty()))
}

/// Merges `userinfo` and `id_token` claims so missing `picture`/`email`/`name` fall back cleanly.
fn merge_oidc_claims(
    userinfo: Option<OidcUserInfo>,
    id_claims: Option<OidcIdTokenClaims>,
) -> AppResult<OidcUserInfo> {
    match (userinfo, id_claims) {
        (Some(ui), Some(id)) => Ok(OidcUserInfo {
            sub: ui.sub,
            email: first_non_empty(ui.email, id.email),
            name: first_non_empty(ui.name, id.name),
            preferred_username: first_non_empty(ui.preferred_username, id.preferred_username),
            picture: first_non_empty(ui.picture, id.picture),
        }),
        (Some(ui), None) => Ok(ui),
        (None, Some(id)) => {
            let sub = id
                .sub
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| AppError::Unauthorized("OIDC id_token missing sub".to_owned()))?;
            Ok(OidcUserInfo {
                sub,
                email: id.email,
                name: id.name,
                preferred_username: id.preferred_username,
                picture: id.picture,
            })
        }
        (None, None) => Err(AppError::BadRequest(
            "OIDC userinfo_endpoint and id_token both unavailable".to_owned(),
        )),
    }
}

/// Fetches OIDC `userinfo` with a bearer access token, returning `Ok(None)` if unauthorized (`401`).
async fn fetch_userinfo(
    client: &reqwest::Client,
    userinfo_url: &str,
    access_token: &str,
) -> AppResult<Option<OidcUserInfo>> {
    let resp = client
        .get(userinfo_url)
        .timeout(std::time::Duration::from_secs(5))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC userinfo failed: {e}")))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(None);
    }

    let claims = resp
        .error_for_status()
        .map_err(|e| AppError::Unauthorized(format!("OIDC userinfo rejected: {e}")))?
        .json::<OidcUserInfo>()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC userinfo JSON error: {e}")))?;
    Ok(Some(claims))
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

    let userinfo = if let Some(userinfo_url) = doc.userinfo_url.as_deref() {
        Some(
            fetch_userinfo(&state.http_client, userinfo_url, &token_resp.access_token)
                .await?
                .ok_or_else(|| AppError::Unauthorized("OIDC userinfo rejected token".to_owned()))?,
        )
    } else {
        None
    };
    let id_claims = token_resp
        .id_token
        .as_deref()
        .and_then(decode_id_token_claims);
    let claims = merge_oidc_claims(userinfo, id_claims)?;

    let user_id = upsert_jit_user(&state, &claims, &token_resp).await?;
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

/// Normalizes an OIDC `picture` claim to a safe HTTP(S) URL or raster `data:image/*;base64` URI.
#[must_use]
pub fn normalize_avatar_url(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    if (trimmed.starts_with("https://") || trimmed.starts_with("http://")) && trimmed.len() <= 4096
    {
        return Some(trimmed.to_owned());
    }
    let lower = trimmed.to_ascii_lowercase();
    let is_safe_raster_data = [
        "data:image/png;base64,",
        "data:image/jpeg;base64,",
        "data:image/jpg;base64,",
        "data:image/webp;base64,",
        "data:image/gif;base64,",
        "data:image/avif;base64,",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix));
    if is_safe_raster_data && trimmed.len() <= 65_536 {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

/// Encrypted OIDC token fields stored on `users`.
#[derive(Debug)]
struct SealedOidcTokens {
    /// AES-256-GCM encrypted access token.
    access_enc: Vec<u8>,
    /// Optional AES-256-GCM encrypted refresh token.
    refresh_enc: Option<Vec<u8>>,
    /// Optional access token expiration timestamp.
    expires_at: Option<DateTime<Utc>>,
}

/// Encrypts OIDC access/refresh tokens for storage on `users`.
fn seal_oidc_tokens(
    state: &AppState,
    token_resp: &OidcTokenResponse,
) -> AppResult<SealedOidcTokens> {
    let access_enc = encrypt_secret(&state.master_key, token_resp.access_token.as_bytes())?;
    let refresh_enc = token_resp
        .refresh_token
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .map(|rt| encrypt_secret(&state.master_key, rt.as_bytes()))
        .transpose()?;
    let expires_at = token_resp
        .expires_in
        .filter(|&secs| secs > 0)
        .map(|secs| Utc::now() + Duration::seconds(secs));
    Ok(SealedOidcTokens {
        access_enc,
        refresh_enc,
        expires_at,
    })
}

/// Inserts or updates a JIT user from OIDC claims; new self-registered users start with
/// `role='user'`, `proxy_enabled=false` unless listed in bootstrap admin config.
async fn upsert_jit_user(
    state: &AppState,
    claims: &OidcUserInfo,
    token_resp: &OidcTokenResponse,
) -> AppResult<Uuid> {
    let avatar_url = normalize_avatar_url(claims.picture.as_deref());
    let email = claims
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_owned);
    let tokens = seal_oidc_tokens(state, token_resp)?;

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
                oidc_access_token_enc = $4,
                oidc_refresh_token_enc = COALESCE($5, oidc_refresh_token_enc),
                oidc_token_expires_at = $6,
                oidc_profile_synced_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(existing_id)
        .bind(&avatar_url)
        .bind(&email)
        .bind(&tokens.access_enc)
        .bind(&tokens.refresh_enc)
        .bind(tokens.expires_at)
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
            proxy_enabled, proxy_username, proxy_uuid, proxy_password, sub_token,
            oidc_access_token_enc, oidc_refresh_token_enc, oidc_token_expires_at,
            oidc_profile_synced_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8, $9, $10, $11, $12, $13, $14, NOW())
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
    .bind(&tokens.access_enc)
    .bind(&tokens.refresh_enc)
    .bind(tokens.expires_at)
    .execute(&state.db)
    .await?;

    Ok(user_id)
}

/// Stored OIDC token credentials for an existing user.
#[derive(Debug, sqlx::FromRow)]
struct UserOidcSyncRow {
    /// User UUID.
    id: Uuid,
    /// OIDC subject identifier.
    oidc_subject: String,
    /// Encrypted OIDC access token.
    oidc_access_token_enc: Option<Vec<u8>>,
    /// Encrypted OIDC refresh token.
    oidc_refresh_token_enc: Option<Vec<u8>>,
    /// Access token expiry timestamp.
    oidc_token_expires_at: Option<DateTime<Utc>>,
}

/// Outcome of attempting to sync a user's profile/avatar from OIDC.
#[derive(Debug, Clone, Copy)]
struct ProfileSyncOutcome {
    /// Whether fresh claims were fetched from OIDC and saved.
    synced: bool,
}

/// Summary of a batch background sync of OIDC user avatars.
#[derive(Debug)]
pub struct BatchAvatarSyncSummary {
    /// Number of users successfully refreshed from OIDC.
    pub synced_count: usize,
}

/// Exchanges a stored OIDC refresh token for fresh tokens.
async fn refresh_oidc_tokens(
    state: &AppState,
    doc: &OidcDiscoveryDoc,
    refresh_token: &str,
) -> AppResult<Option<OidcTokenResponse>> {
    let mut form: HashMap<&str, String> = HashMap::new();
    form.insert("grant_type", "refresh_token".to_owned());
    form.insert("refresh_token", refresh_token.to_owned());
    form.insert("client_id", state.config.oidc.client_id.clone());
    if !state.config.oidc.client_secret.is_empty() {
        form.insert("client_secret", state.config.oidc.client_secret.clone());
    }

    let resp = state
        .http_client
        .post(&doc.token_url)
        .timeout(std::time::Duration::from_secs(5))
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC token refresh failed: {e}")))?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let token_resp = resp
        .json::<OidcTokenResponse>()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("OIDC refresh JSON error: {e}")))?;
    Ok(Some(token_resp))
}

/// Persists updated OIDC profile claims (and optional rotated tokens) for a user.
async fn persist_synced_oidc_profile(
    state: &AppState,
    row: &UserOidcSyncRow,
    claims: &OidcUserInfo,
    refreshed_tokens: Option<&OidcTokenResponse>,
) -> AppResult<()> {
    if claims.sub != row.oidc_subject {
        return Err(AppError::Unauthorized(
            "OIDC userinfo subject mismatch".to_owned(),
        ));
    }

    let avatar_url = normalize_avatar_url(claims.picture.as_deref());
    let email = claims
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_owned);

    if let Some(tokens) = refreshed_tokens {
        let sealed = seal_oidc_tokens(state, tokens)?;
        sqlx::query(
            r"
            UPDATE users
            SET avatar_url = COALESCE($2, avatar_url),
                email = COALESCE($3, email),
                oidc_access_token_enc = $4,
                oidc_refresh_token_enc = COALESCE($5, oidc_refresh_token_enc),
                oidc_token_expires_at = $6,
                oidc_profile_synced_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(row.id)
        .bind(&avatar_url)
        .bind(&email)
        .bind(&sealed.access_enc)
        .bind(&sealed.refresh_enc)
        .bind(sealed.expires_at)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query(
            r"
            UPDATE users
            SET avatar_url = COALESCE($2, avatar_url),
                email = COALESCE($3, email),
                oidc_profile_synced_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            ",
        )
        .bind(row.id)
        .bind(&avatar_url)
        .bind(&email)
        .execute(&state.db)
        .await?;
    }

    Ok(())
}

/// Syncs a single user's profile/avatar using stored OIDC access/refresh tokens.
async fn sync_user_with_discovery(
    state: &AppState,
    doc: &OidcDiscoveryDoc,
    row: &UserOidcSyncRow,
) -> AppResult<ProfileSyncOutcome> {
    if row.oidc_subject.starts_with("internal:") {
        return Ok(ProfileSyncOutcome { synced: false });
    }

    let access_not_expired = row
        .oidc_token_expires_at
        .is_none_or(|exp| exp > Utc::now() + Duration::seconds(30));

    if access_not_expired
        && let Some(sealed_access) = row.oidc_access_token_enc.as_deref()
        && let Some(userinfo_url) = doc.userinfo_url.as_deref()
        && let Ok(raw_bytes) = decrypt_secret(&state.master_key, sealed_access)
        && let Ok(access_token) = String::from_utf8(raw_bytes)
        && let Some(claims) =
            fetch_userinfo(&state.http_client, userinfo_url, &access_token).await?
    {
        persist_synced_oidc_profile(state, row, &claims, None).await?;
        return Ok(ProfileSyncOutcome { synced: true });
    }

    if let Some(sealed_refresh) = row.oidc_refresh_token_enc.as_deref()
        && let Ok(raw_bytes) = decrypt_secret(&state.master_key, sealed_refresh)
        && let Ok(refresh_token) = String::from_utf8(raw_bytes)
        && let Some(new_tokens) = refresh_oidc_tokens(state, doc, &refresh_token).await?
    {
        let userinfo = if let Some(userinfo_url) = doc.userinfo_url.as_deref() {
            fetch_userinfo(&state.http_client, userinfo_url, &new_tokens.access_token).await?
        } else {
            None
        };
        let id_claims = new_tokens
            .id_token
            .as_deref()
            .and_then(decode_id_token_claims);
        if userinfo.is_some() || id_claims.is_some() {
            let claims = merge_oidc_claims(userinfo, id_claims)?;
            persist_synced_oidc_profile(state, row, &claims, Some(&new_tokens)).await?;
            return Ok(ProfileSyncOutcome { synced: true });
        }
    }

    Ok(ProfileSyncOutcome { synced: false })
}

/// Syncs all OIDC-backed users that have valid stored OIDC tokens.
///
/// # Errors
/// Returns an error if OIDC is disabled, discovery fails, or a database query fails.
pub async fn sync_all_oidc_user_profiles(state: &AppState) -> AppResult<BatchAvatarSyncSummary> {
    if !state.config.oidc.enabled {
        return Err(AppError::BadRequest(
            "OIDC authentication is disabled".to_owned(),
        ));
    }

    let doc = resolve_oidc_endpoints(state).await?;
    let rows = sqlx::query_as::<_, UserOidcSyncRow>(
        r"
        SELECT id, oidc_subject, oidc_access_token_enc, oidc_refresh_token_enc, oidc_token_expires_at
        FROM users
        WHERE oidc_subject NOT LIKE 'internal:%'
          AND (oidc_access_token_enc IS NOT NULL OR oidc_refresh_token_enc IS NOT NULL)
        ",
    )
    .fetch_all(&state.db)
    .await?;

    let mut synced_count = 0_usize;

    for row in &rows {
        match sync_user_with_discovery(state, &doc, row).await {
            Ok(outcome) if outcome.synced => synced_count += 1,
            Ok(_) => {}
            Err(err) => {
                tracing::debug!(user_id = %row.id, error = %err, "failed to sync OIDC user avatar");
            }
        }
    }

    Ok(BatchAvatarSyncSummary { synced_count })
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

/// Reloads mutable profile fields (`avatar_url`, `email`) after an OIDC sync.
async fn reload_user_profile_fields(state: &AppState, user: &mut AuthUser) -> AppResult<()> {
    if let Some((avatar_url, email)) = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT avatar_url, email FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    {
        user.avatar_url = avatar_url;
        user.email = email;
    }
    Ok(())
}

/// `GET /api/v1/auth/me`
pub async fn get_me(
    State(state): State<AppState>,
    AuthenticatedUser(mut user): AuthenticatedUser,
) -> AppResult<Json<AuthUser>> {
    if state.config.oidc.enabled && !user.oidc_subject.starts_with("internal:") {
        let sync_meta = sqlx::query_as::<
            _,
            (
                Option<DateTime<Utc>>,
                Option<Vec<u8>>,
                Option<Vec<u8>>,
                Option<DateTime<Utc>>,
            ),
        >(
            r"
            SELECT oidc_profile_synced_at, oidc_access_token_enc, oidc_refresh_token_enc, oidc_token_expires_at
            FROM users
            WHERE id = $1
            ",
        )
        .bind(user.id)
        .fetch_optional(&state.db)
        .await?;

        if let Some((synced_at, access_enc, refresh_enc, token_expires_at)) = sync_meta
            && (access_enc.is_some() || refresh_enc.is_some())
        {
            let stale_secs = if user.avatar_url.is_none() { 60 } else { 300 };
            let is_stale =
                synced_at.is_none_or(|ts| Utc::now() - ts >= Duration::seconds(stale_secs));
            if is_stale {
                let row = UserOidcSyncRow {
                    id: user.id,
                    oidc_subject: user.oidc_subject.clone(),
                    oidc_access_token_enc: access_enc,
                    oidc_refresh_token_enc: refresh_enc,
                    oidc_token_expires_at: token_expires_at,
                };
                match resolve_oidc_endpoints(&state).await {
                    Ok(doc) => match sync_user_with_discovery(&state, &doc, &row).await {
                        Ok(outcome) if outcome.synced => {
                            reload_user_profile_fields(&state, &mut user).await?;
                        }
                        Ok(_) => {}
                        Err(err) => {
                            tracing::debug!(user_id = %user.id, error = %err, "auto OIDC profile sync skipped");
                        }
                    },
                    Err(err) => {
                        tracing::debug!(user_id = %user.id, error = %err, "OIDC discovery skipped in /auth/me");
                    }
                }
            }
        }
    }

    Ok(Json(user))
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
        assert_eq!(
            normalize_avatar_url(Some("data:image/png;base64,iVBORw0KGgo=")).as_deref(),
            Some("data:image/png;base64,iVBORw0KGgo=")
        );
        assert_eq!(
            normalize_avatar_url(Some("data:image/svg+xml;base64,PHN2Zz4=")),
            None
        );
        assert_eq!(normalize_avatar_url(Some("javascript:alert(1)")), None);
        assert_eq!(normalize_avatar_url(Some("   ")), None);
        assert_eq!(normalize_avatar_url(None), None);
    }

    #[test]
    fn merge_oidc_claims_falls_back_to_id_token_picture() {
        let id_token_payload = URL_SAFE_NO_PAD
            .encode(r#"{"sub":"u1","picture":"https://cdn.example/id.png","email":"a@b.c"}"#);
        let jwt = format!("hdr.{id_token_payload}.sig");
        let id_claims = decode_id_token_claims(&jwt);
        let userinfo = Some(OidcUserInfo {
            sub: "u1".to_owned(),
            email: None,
            name: Some("Alice".to_owned()),
            preferred_username: None,
            picture: Some("   ".to_owned()),
        });
        let merged = merge_oidc_claims(userinfo, id_claims);
        assert!(merged.is_ok());
        if let Ok(claims) = merged {
            assert_eq!(
                claims.picture.as_deref(),
                Some("https://cdn.example/id.png")
            );
            assert_eq!(claims.email.as_deref(), Some("a@b.c"));
            assert_eq!(claims.name.as_deref(), Some("Alice"));
        }
    }
}
