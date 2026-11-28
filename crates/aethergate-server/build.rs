//! Build script embedding the Git commit SHA at compile time for the About API.

use std::process::Command;

/// Resolves the Git commit SHA from environment variables or local `.git` metadata.
fn resolve_git_commit() -> String {
    if let Ok(sha) = std::env::var("AETHERGATE_GIT_COMMIT") {
        let trimmed = sha.trim();
        if !trimmed.is_empty() && trimmed != "unknown" {
            return trimmed.to_owned();
        }
    }

    if let Ok(sha) = std::env::var("GITHUB_SHA") {
        let trimmed = sha.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    if let Ok(output) = Command::new("git").args(["rev-parse", "HEAD"]).output()
        && output.status.success()
        && let Ok(stdout) = String::from_utf8(output.stdout)
    {
        let trimmed = stdout.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    "unknown".to_owned()
}

/// Registers Cargo rerun triggers for `.git/HEAD` and its referenced branch file.
fn register_git_rerun_hints() {
    println!("cargo:rerun-if-env-changed=AETHERGATE_GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");

    let head_path = std::path::Path::new("../../.git/HEAD");
    if !head_path.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", head_path.display());

    if let Ok(head_content) = std::fs::read_to_string(head_path)
        && let Some(ref_path) = head_content.trim().strip_prefix("ref: ")
    {
        let target_ref = std::path::Path::new("../../.git").join(ref_path);
        if target_ref.exists() {
            println!("cargo:rerun-if-changed={}", target_ref.display());
        }
    }
}

fn main() {
    register_git_rerun_hints();
    let commit = resolve_git_commit();
    println!("cargo:rustc-env=AETHERGATE_BUILD_COMMIT={commit}");
}
