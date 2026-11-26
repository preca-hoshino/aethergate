//! HMAC-SHA256 request signing and constant-time verification for `/api/open/v1/*`.
//!
//! Canonical signing string:
//! `"{METHOD}\n{PATH_AND_QUERY}\n{TIMESTAMP}\n{NONCE}\n{BODY_SHA256_HEX}"`

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Type alias for HMAC-SHA256.
type HmacSha256 = Hmac<Sha256>;

/// Maximum allowed clock skew in seconds for incoming signed requests.
pub const DEFAULT_MAX_TIMESTAMP_SKEW_SECS: i64 = 60;

/// Header carrying the node UUID.
pub const HEADER_NODE_ID: &str = "x-node-id";

/// Header carrying the UNIX epoch timestamp in seconds.
pub const HEADER_TIMESTAMP: &str = "x-timestamp";

/// Header carrying a per-request random nonce.
pub const HEADER_NONCE: &str = "x-nonce";

/// Header carrying the hex-encoded HMAC-SHA256 signature.
pub const HEADER_SIGNATURE: &str = "x-signature";

/// Errors produced during HMAC canonicalization or verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HmacAuthError {
    /// The secret key could not initialize HMAC.
    #[error("invalid HMAC key")]
    InvalidKey,
    /// The timestamp is outside the allowed replay window.
    #[error("timestamp {timestamp} is outside the allowed +/-{max_skew_secs}s window")]
    TimestampSkew {
        /// Supplied request timestamp.
        timestamp: i64,
        /// Allowed clock skew in seconds.
        max_skew_secs: i64,
    },
    /// The supplied signature is not valid hex.
    #[error("malformed hex signature")]
    MalformedSignature,
    /// The signature does not match the canonical payload.
    #[error("signature verification failed")]
    SignatureMismatch,
}

/// Computes the lowercase hex-encoded SHA-256 digest of a request body.
#[must_use]
pub fn sha256_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    hex::encode(hasher.finalize())
}

/// Builds the canonical signing message for an Open API HTTP request.
#[must_use]
pub fn build_canonical_string(
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    body_sha256_hex: &str,
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        path_and_query,
        timestamp,
        nonce,
        body_sha256_hex
    )
}

/// Signs an Open API HTTP request using the node's shared HMAC secret.
///
/// # Errors
/// Returns [`HmacAuthError::InvalidKey`] if the secret cannot initialize HMAC-SHA256.
pub fn sign_request(
    secret: &[u8],
    method: &str,
    path_and_query: &str,
    timestamp: i64,
    nonce: &str,
    body: &[u8],
) -> Result<String, HmacAuthError> {
    let body_hash = sha256_hex(body);
    let canonical = build_canonical_string(method, path_and_query, timestamp, nonce, &body_hash);
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| HmacAuthError::InvalidKey)?;
    mac.update(canonical.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Borrowed metadata and payload required to verify a signed Open API request.
#[derive(Debug, Clone, Copy)]
pub struct VerifyRequestParams<'a> {
    /// Node shared HMAC secret.
    pub secret: &'a [u8],
    /// HTTP method (e.g. `"GET"`, `"POST"`).
    pub method: &'a str,
    /// Path and query string (e.g. `"/api/open/v1/config"`).
    pub path_and_query: &'a str,
    /// Supplied `X-Timestamp` header value.
    pub timestamp: i64,
    /// Current server UTC timestamp in seconds.
    pub now_timestamp: i64,
    /// Maximum allowed clock skew in seconds.
    pub max_skew_secs: i64,
    /// Supplied `X-Nonce` header value.
    pub nonce: &'a str,
    /// Raw request body bytes.
    pub body: &'a [u8],
    /// Supplied hex-encoded `X-Signature` header value.
    pub signature_hex: &'a str,
}

/// Verifies an incoming Open API request's timestamp window and HMAC-SHA256 signature.
///
/// Uses constant-time tag comparison via `hmac::Mac::verify_slice`.
///
/// # Errors
/// Returns [`HmacAuthError`] if timestamp skew exceeds `max_skew_secs`, the hex signature
/// is malformed, or the cryptographic tag check fails.
pub fn verify_request(params: &VerifyRequestParams<'_>) -> Result<(), HmacAuthError> {
    if (params.now_timestamp - params.timestamp).abs() > params.max_skew_secs {
        return Err(HmacAuthError::TimestampSkew {
            timestamp: params.timestamp,
            max_skew_secs: params.max_skew_secs,
        });
    }

    let provided_tag =
        hex::decode(params.signature_hex.trim()).map_err(|_| HmacAuthError::MalformedSignature)?;
    let body_hash = sha256_hex(params.body);
    let canonical = build_canonical_string(
        params.method,
        params.path_and_query,
        params.timestamp,
        params.nonce,
        &body_hash,
    );

    let mut mac =
        HmacSha256::new_from_slice(params.secret).map_err(|_| HmacAuthError::InvalidKey)?;
    mac.update(canonical.as_bytes());
    mac.verify_slice(&provided_tag)
        .map_err(|_| HmacAuthError::SignatureMismatch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let secret = b"node-secret-key-0123456789abcdef";
        let body = br#"{"batch_id":"123"}"#;
        let ts = 1_760_000_000;
        let sig = sign_request(
            secret,
            "POST",
            "/api/open/v1/report/telemetry",
            ts,
            "nonce-1",
            body,
        );
        assert!(sig.is_ok());
        let Ok(signature) = sig else {
            return;
        };

        let verified = verify_request(&VerifyRequestParams {
            secret,
            method: "post",
            path_and_query: "/api/open/v1/report/telemetry",
            timestamp: ts,
            now_timestamp: ts + 10,
            max_skew_secs: DEFAULT_MAX_TIMESTAMP_SKEW_SECS,
            nonce: "nonce-1",
            body,
            signature_hex: &signature,
        });
        assert_eq!(verified, Ok(()));
    }

    #[test]
    fn rejects_stale_timestamp_and_tampered_body() {
        let secret = b"node-secret-key";
        let ts = 1_760_000_000;
        let Ok(signature) = sign_request(secret, "GET", "/api/open/v1/config", ts, "n1", b"")
        else {
            return;
        };

        let stale = verify_request(&VerifyRequestParams {
            secret,
            method: "GET",
            path_and_query: "/api/open/v1/config",
            timestamp: ts,
            now_timestamp: ts + 120,
            max_skew_secs: DEFAULT_MAX_TIMESTAMP_SKEW_SECS,
            nonce: "n1",
            body: b"",
            signature_hex: &signature,
        });
        assert_eq!(
            stale,
            Err(HmacAuthError::TimestampSkew {
                timestamp: ts,
                max_skew_secs: DEFAULT_MAX_TIMESTAMP_SKEW_SECS,
            })
        );

        let tampered = verify_request(&VerifyRequestParams {
            secret,
            method: "GET",
            path_and_query: "/api/open/v1/config",
            timestamp: ts,
            now_timestamp: ts + 5,
            max_skew_secs: DEFAULT_MAX_TIMESTAMP_SKEW_SECS,
            nonce: "n1",
            body: b"tampered",
            signature_hex: &signature,
        });
        assert_eq!(tampered, Err(HmacAuthError::SignatureMismatch));
    }
}
