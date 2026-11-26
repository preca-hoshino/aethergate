//! Staging `sing-box` config compiler, `sing-box check` validator, and atomic file swapper.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use tokio::process::Command;

use aethergate_protocol::{
    AuthorizedUserCredential, LocalAgentOverrides, NodeTemplate, TlsProfile, compile_singbox_config,
};

/// Compiles, validates with `sing-box check`, and atomically writes `target_path`.
///
/// # Errors
/// Returns an error if compilation, `sing-box check`, or atomic rename fails.
pub async fn stage_and_validate_config(
    singbox_bin: &Path,
    target_path: &Path,
    template: &NodeTemplate,
    tls_profile: &TlsProfile,
    users: &[AuthorizedUserCredential],
    overrides: &LocalAgentOverrides,
) -> Result<()> {
    let compiled = compile_singbox_config(template, tls_profile, users, overrides)
        .map_err(|e| anyhow!("sing-box config compile failed: {e}"))?;
    let json_bytes = serde_json::to_vec_pretty(&compiled)?;

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let staging_path = target_path.with_extension("json.staging");
    std::fs::write(&staging_path, &json_bytes)
        .with_context(|| format!("failed to write staging config {}", staging_path.display()))?;

    let check_out = Command::new(singbox_bin)
        .arg("check")
        .arg("-c")
        .arg(&staging_path)
        .output()
        .await
        .with_context(|| format!("failed to execute {} check", singbox_bin.display()))?;

    if !check_out.status.success() {
        let stderr = String::from_utf8_lossy(&check_out.stderr);
        let _ = std::fs::remove_file(&staging_path);
        return Err(anyhow!("sing-box check failed: {stderr}"));
    }

    std::fs::rename(&staging_path, target_path)
        .with_context(|| format!("failed to atomically replace {}", target_path.display()))?;

    Ok(())
}
