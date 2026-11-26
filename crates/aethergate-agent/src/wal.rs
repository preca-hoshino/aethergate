//! Local Write-Ahead Log (WAL) queue for lossless telemetry delivery during network outages.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use aethergate_protocol::TelemetryBatchRequest;

/// Maximum number of queued telemetry batches retained on disk.
const MAX_WAL_FILES: usize = 2000;

/// Persistent JSON-file WAL queue stored under `wal_dir`.
#[derive(Debug, Clone)]
pub struct WalQueue {
    /// Directory containing queued `.json` batch files.
    dir: PathBuf,
}

impl WalQueue {
    /// Initializes a WAL queue at `dir`, creating the directory if needed.
    ///
    /// # Errors
    /// Returns an error if the directory cannot be created.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create WAL directory {}", dir.display()))?;
        Ok(Self { dir })
    }

    /// Appends a failed telemetry batch to disk for later FIFO replay.
    ///
    /// # Errors
    /// Returns an error if serialization or file write fails.
    pub fn enqueue(&self, batch: &TelemetryBatchRequest) -> Result<()> {
        self.evict_oldest_if_full()?;
        let filename = format!(
            "{:016}_{}.json",
            batch.window_end.timestamp_millis(),
            batch.batch_id
        );
        let path = self.dir.join(filename);
        let bytes = serde_json::to_vec(batch)?;
        std::fs::write(&path, bytes)
            .with_context(|| format!("failed to write WAL file {}", path.display()))?;
        Ok(())
    }

    /// Lists queued batch file paths in chronological FIFO order.
    ///
    /// # Errors
    /// Returns an error if the WAL directory cannot be read.
    pub fn list_pending(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }

    /// Reads and deserializes a queued batch from disk.
    ///
    /// # Errors
    /// Returns an error if reading or JSON parsing fails.
    pub fn read_batch(path: &Path) -> Result<TelemetryBatchRequest> {
        let bytes = std::fs::read(path)?;
        let batch = serde_json::from_slice(&bytes)?;
        Ok(batch)
    }

    /// Removes a successfully acknowledged batch file from the WAL.
    ///
    /// # Errors
    /// Returns an error if file deletion fails.
    pub fn remove(path: &Path) -> Result<()> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Evicts oldest WAL files when the queue reaches [`MAX_WAL_FILES`].
    fn evict_oldest_if_full(&self) -> Result<()> {
        let pending = self.list_pending()?;
        if pending.len() >= MAX_WAL_FILES {
            let overflow = pending.len() - MAX_WAL_FILES + 1;
            for path in pending.into_iter().take(overflow) {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn wal_enqueue_fifo_and_remove() {
        let dir = std::env::temp_dir().join(format!("ag-wal-test-{}", Uuid::new_v4()));
        let Ok(wal) = WalQueue::open(&dir) else {
            return;
        };

        let b1 = TelemetryBatchRequest {
            batch_id: Uuid::new_v4(),
            window_start: Utc::now(),
            window_end: Utc::now(),
            window_ms: 2000,
            config_revision: 1,
            user_revision: 1,
            user_samples: vec![],
            inbound_slices: vec![],
        };
        assert!(wal.enqueue(&b1).is_ok());

        let pending = wal.list_pending().unwrap_or_default();
        assert_eq!(pending.len(), 1);

        if let Some(first) = pending.first() {
            let loaded = WalQueue::read_batch(first);
            assert!(loaded.is_ok());
            assert!(WalQueue::remove(first).is_ok());
        }
        assert_eq!(wal.list_pending().unwrap_or_default().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
