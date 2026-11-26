//! Self-signed TLS certificate generator and `pinSHA256` fingerprint helper.

use std::path::Path;

use anyhow::{Context, Result};
use rcgen::generate_simple_self_signed;
use sha2::{Digest, Sha256};

/// Ensures a self-signed certificate and private key exist at `cert_path` / `key_path`.
///
/// Returns the lowercase hex SHA-256 fingerprint of the DER certificate (`pinSHA256`).
///
/// # Errors
/// Returns an error if certificate generation or filesystem I/O fails.
pub fn ensure_self_signed_cert(
    cert_path: &Path,
    key_path: &Path,
    server_name: &str,
) -> Result<String> {
    if cert_path.exists() && key_path.exists() {
        let pem = std::fs::read_to_string(cert_path)
            .with_context(|| format!("failed to read existing cert {}", cert_path.display()))?;
        return Ok(hex::encode(Sha256::digest(pem.as_bytes())));
    }

    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let san = if server_name.trim().is_empty() {
        "localhost".to_owned()
    } else {
        server_name.trim().to_owned()
    };

    let cert = generate_simple_self_signed(vec![san])
        .context("failed to generate self-signed TLS certificate")?;
    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();
    let der_fingerprint = hex::encode(Sha256::digest(cert.cert.der().as_ref()));

    std::fs::write(cert_path, cert_pem)
        .with_context(|| format!("failed to write cert {}", cert_path.display()))?;
    std::fs::write(key_path, key_pem)
        .with_context(|| format!("failed to write key {}", key_path.display()))?;

    Ok(der_fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_reuses_self_signed_cert() {
        let dir = std::env::temp_dir().join(format!("ag-tls-test-{}", uuid::Uuid::new_v4()));
        let cert = dir.join("fullchain.pem");
        let key = dir.join("privkey.pem");

        let fp = ensure_self_signed_cert(&cert, &key, "edge.example.com");
        assert!(fp.is_ok());
        assert!(cert.exists());
        assert!(key.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
