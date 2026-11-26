//! Cryptographic helpers: AES-256-GCM node secret encryption, token hashing,
//! PKCE challenges, and X25519 Reality keypair generation.

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};

/// Parses a 32-byte (64-hex-char) AES-256-GCM master key.
///
/// # Errors
/// Returns an error if the hex string is malformed or does not decode to 32 bytes.
pub fn parse_master_key(master_key_hex: &str) -> Result<[u8; 32]> {
    let raw = hex::decode(master_key_hex.trim()).context("master_key_hex must be valid hex")?;
    let key: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow!("master_key_hex must decode to exactly 32 bytes"))?;
    Ok(key)
}

/// Encrypts plaintext bytes using AES-256-GCM, returning `nonce(12) || ciphertext`.
///
/// # Errors
/// Returns an error if AEAD encryption fails.
pub fn encrypt_secret(master_key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| anyhow!("AES-256-GCM encryption failed"))?;

    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts `nonce(12) || ciphertext` encrypted by [`encrypt_secret`].
///
/// # Errors
/// Returns an error if the payload is truncated or authentication fails.
pub fn decrypt_secret(master_key: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < 13 {
        return Err(anyhow!("encrypted secret payload is too short"));
    }
    let (nonce_bytes, ciphertext) = sealed.split_at(12);
    let key = Key::<Aes256Gcm>::from_slice(master_key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow!("AES-256-GCM decryption failed"))
}

/// Generates a URL-safe random token with `byte_len` bytes of entropy.
#[must_use]
pub fn generate_random_token(byte_len: usize) -> String {
    let mut buf = vec![0_u8; byte_len];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Generates a 32-byte hex-encoded HMAC secret for a newly created node.
#[must_use]
pub fn generate_node_hmac_secret() -> String {
    let mut buf = [0_u8; 32];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Computes the lowercase hex SHA-256 digest of a bearer/session token.
#[must_use]
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

/// Computes the S256 PKCE code challenge for a verifier string.
#[must_use]
pub fn pkce_s256_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// Generates an X25519 `(private_key_b64url, public_key_b64url)` pair for VLESS Reality.
#[must_use]
pub fn generate_reality_keypair() -> (String, String) {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (
        URL_SAFE_NO_PAD.encode(secret.to_bytes()),
        URL_SAFE_NO_PAD.encode(public.to_bytes()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_gcm_roundtrip_and_tamper_detection() {
        let key = [0x42_u8; 32];
        let secret = b"super-secret-node-hmac-key";
        let sealed = encrypt_secret(&key, secret);
        assert!(sealed.is_ok());
        let Ok(mut blob) = sealed else {
            return;
        };

        let opened = decrypt_secret(&key, &blob);
        assert!(opened.is_ok());
        if let Ok(plain) = opened {
            assert_eq!(plain.as_slice(), secret);
        }

        if let Some(last) = blob.last_mut() {
            *last ^= 0xFF;
        }
        assert!(decrypt_secret(&key, &blob).is_err());
    }

    #[test]
    fn reality_keypair_produces_valid_urlsafe_keys() {
        let (priv_k, pub_k) = generate_reality_keypair();
        assert_eq!(priv_k.len(), 43);
        assert_eq!(pub_k.len(), 43);
        assert_ne!(priv_k, pub_k);
    }
}
