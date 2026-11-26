//! `sing-box` child process supervisor with graceful reload and uptime tracking.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::process::{Child, Command};

/// Supervises the `sing-box run -c <config>` child process.
#[derive(Debug)]
pub struct SingboxSupervisor {
    /// Path to `sing-box` binary.
    bin_path: PathBuf,
    /// Path to active `sing-box` JSON config.
    config_path: PathBuf,
    /// Running child process handle.
    child: Option<Child>,
    /// Start timestamp for uptime calculation.
    started_at: Option<Instant>,
    /// Cached `sing-box version` string.
    version: String,
}

impl SingboxSupervisor {
    /// Creates a new supervisor without starting the child process yet.
    #[must_use]
    pub fn new(bin_path: impl AsRef<Path>, config_path: impl AsRef<Path>) -> Self {
        Self {
            bin_path: bin_path.as_ref().to_path_buf(),
            config_path: config_path.as_ref().to_path_buf(),
            child: None,
            started_at: None,
            version: "unknown".to_owned(),
        }
    }

    /// Queries `sing-box version` once and caches the result.
    pub async fn detect_version(&mut self) -> &str {
        if let Ok(out) = Command::new(&self.bin_path).arg("version").output().await
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(first_line) = text.lines().next() {
                first_line.trim().clone_into(&mut self.version);
            }
        }
        &self.version
    }

    /// Returns the cached `sing-box` version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns current child process uptime in seconds (0 if not running).
    #[must_use]
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.map_or(0, |t| t.elapsed().as_secs())
    }

    /// Ensures the `sing-box` child process is running, restarting if it exited.
    ///
    /// # Errors
    /// Returns an error if spawning `sing-box` fails.
    pub fn ensure_running(&mut self) -> Result<()> {
        if let Some(child) = self.child.as_mut() {
            if let Ok(None) = child.try_wait() {
                return Ok(());
            }
            tracing::warn!("sing-box exited unexpectedly; restarting");
        }
        self.start()
    }

    /// Stops any existing child process and starts a fresh `sing-box` instance.
    ///
    /// # Errors
    /// Returns an error if spawning `sing-box` fails.
    pub async fn reload(&mut self) -> Result<()> {
        self.stop().await;
        self.start()
    }

    /// Starts `sing-box run -c <config_path>`.
    fn start(&mut self) -> Result<()> {
        let child = Command::new(&self.bin_path)
            .arg("run")
            .arg("-c")
            .arg(&self.config_path)
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start sing-box ({} -c {})",
                    self.bin_path.display(),
                    self.config_path.display()
                )
            })?;

        self.child = Some(child);
        self.started_at = Some(Instant::now());
        tracing::info!("sing-box child process started");
        Ok(())
    }

    /// Gracefully terminates the running child process if present.
    pub async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        self.started_at = None;
    }
}
