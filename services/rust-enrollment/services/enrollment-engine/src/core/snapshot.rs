use crate::core::models::PreloadDataset;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use unieee_domain::TermId;
use uuid::Uuid;

pub const DISK_SNAPSHOT_FORMAT_VERSION: u16 = 1;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("snapshot file not found for term {0}")]
    NotFound(TermId),
    #[error("snapshot I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("snapshot JSON codec error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported snapshot format version {0}")]
    UnsupportedVersion(u16),
    #[error("snapshot term_id mismatch: expected {expected}, got {actual}")]
    TermMismatch { expected: TermId, actual: TermId },
    #[error("snapshot SHA-256 checksum mismatch: expected {expected}, actual {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiskSnapshotEnvelope {
    pub format_version: u16,
    pub term_id: TermId,
    pub checkpoint_seq: i64,
    pub engine_epoch: u64,
    pub created_at: DateTime<Utc>,
    pub kafka_replay_from_ms: i64,
    pub checksum_sha256: String,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskSnapshotMeta {
    pub path: PathBuf,
    pub term_id: TermId,
    pub checkpoint_seq: i64,
    pub engine_epoch: u64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedDiskSnapshot {
    pub path: PathBuf,
    pub dataset: PreloadDataset,
    pub checkpoint_seq: i64,
    pub engine_epoch: u64,
    pub kafka_replay_from_ms: i64,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone)]
pub struct SnapshotStore {
    dir: PathBuf,
}

impl SnapshotStore {
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub fn snapshot_path(&self, term_id: TermId) -> PathBuf {
        self.dir.join(format!("preload-snapshot-{term_id}.json"))
    }

    pub fn write_atomic(
        &self,
        dataset: &PreloadDataset,
        kafka_replay_from_ms: i64,
    ) -> Result<DiskSnapshotMeta, SnapshotError> {
        fs::create_dir_all(&self.dir)?;

        let payload_json = serde_json::to_string(dataset)?;
        let checksum_sha256 = sha256_hex(payload_json.as_bytes());
        let envelope = DiskSnapshotEnvelope {
            format_version: DISK_SNAPSHOT_FORMAT_VERSION,
            term_id: dataset.term_id,
            checkpoint_seq: dataset.checkpoint_seq,
            engine_epoch: dataset.engine_epoch,
            created_at: dataset.created_at,
            kafka_replay_from_ms,
            checksum_sha256: checksum_sha256.clone(),
            payload_json,
        };
        let encoded = serde_json::to_vec(&envelope)?;

        let tmp_path = self.dir.join(format!(
            ".snapshot-{}-{}.tmp",
            dataset.term_id,
            Uuid::new_v4()
        ));
        let final_path = self.snapshot_path(dataset.term_id);

        let write_result = (|| -> Result<(), SnapshotError> {
            let mut file = File::create(&tmp_path)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            fs::rename(&tmp_path, &final_path)?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(err);
        }

        Ok(DiskSnapshotMeta {
            path: final_path,
            term_id: dataset.term_id,
            checkpoint_seq: dataset.checkpoint_seq,
            engine_epoch: dataset.engine_epoch,
            checksum_sha256,
        })
    }

    pub fn load_verified(&self, term_id: TermId) -> Result<LoadedDiskSnapshot, SnapshotError> {
        let path = self.snapshot_path(term_id);
        if !path.exists() {
            return Err(SnapshotError::NotFound(term_id));
        }
        Self::load_verified_path(&path, Some(term_id))
    }

    pub fn find_latest_verified(&self) -> Result<Option<LoadedDiskSnapshot>, SnapshotError> {
        if !self.dir.exists() {
            return Ok(None);
        }
        let mut candidates: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("preload-snapshot-") && name.ends_with(".json") {
                    candidates.push(path);
                }
            }
        }

        let mut loaded = Vec::new();
        for path in candidates {
            if let Ok(snap) = Self::load_verified_path(&path, None) {
                loaded.push(snap);
            }
        }
        loaded.sort_by_key(|s| s.dataset.created_at);
        Ok(loaded.pop())
    }

    fn load_verified_path(
        path: &Path,
        expected_term: Option<TermId>,
    ) -> Result<LoadedDiskSnapshot, SnapshotError> {
        let raw = fs::read(path)?;
        let envelope: DiskSnapshotEnvelope = serde_json::from_slice(&raw)?;

        if envelope.format_version != DISK_SNAPSHOT_FORMAT_VERSION {
            return Err(SnapshotError::UnsupportedVersion(envelope.format_version));
        }
        if let Some(expected) = expected_term {
            if envelope.term_id != expected {
                return Err(SnapshotError::TermMismatch {
                    expected,
                    actual: envelope.term_id,
                });
            }
        }

        let actual_checksum = sha256_hex(envelope.payload_json.as_bytes());
        if actual_checksum != envelope.checksum_sha256 {
            return Err(SnapshotError::ChecksumMismatch {
                expected: envelope.checksum_sha256,
                actual: actual_checksum,
            });
        }

        let dataset: PreloadDataset = serde_json::from_str(&envelope.payload_json)?;
        Ok(LoadedDiskSnapshot {
            path: path.to_path_buf(),
            dataset,
            checkpoint_seq: envelope.checkpoint_seq,
            engine_epoch: envelope.engine_epoch,
            kafka_replay_from_ms: envelope.kafka_replay_from_ms,
            checksum_sha256: envelope.checksum_sha256,
        })
    }
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
