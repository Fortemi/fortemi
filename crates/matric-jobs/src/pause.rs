//! Job processing pause state manager (Issue #466).
//!
//! Provides global and per-archive pause/resume control for the job worker.
//! State is held in-memory for fast hot-path checks and persisted to the
//! `system_config` table for durability across container restarts.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use matric_core::{Error, JobPauseQueueStats, JobPauseState, Result};

/// Persisted pause state shape (stored as JSON in `system_config`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedPauseState {
    global_paused: bool,
    paused_archives: Vec<String>,
}

const CONFIG_KEY: &str = "job_pause_state";

/// Thread-safe pause state manager.
///
/// The hot path (`is_globally_paused`, `is_archive_paused`) uses lock-free
/// atomics and a read-biased `RwLock`. The cold path (pause/resume/persist)
/// acquires a write lock and flushes to the database.
#[derive(Clone)]
pub struct PauseState {
    global_paused: Arc<AtomicBool>,
    paused_archives: Arc<RwLock<HashSet<String>>>,
    pool: Pool<Postgres>,
}

impl PauseState {
    /// Create a new PauseState and load persisted state from the database.
    ///
    /// If no persisted state exists, defaults to all-running.
    pub async fn load(pool: Pool<Postgres>) -> Result<Self> {
        let state = Self {
            global_paused: Arc::new(AtomicBool::new(false)),
            paused_archives: Arc::new(RwLock::new(HashSet::new())),
            pool,
        };

        // Load persisted state
        let persisted: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
                .bind(CONFIG_KEY)
                .fetch_optional(&state.pool)
                .await
                .map_err(Error::Database)?;

        if let Some((value,)) = persisted {
            match serde_json::from_value::<PersistedPauseState>(value) {
                Ok(ps) => {
                    state
                        .global_paused
                        .store(ps.global_paused, Ordering::SeqCst);
                    let mut archives = state.paused_archives.write().await;
                    for a in ps.paused_archives {
                        archives.insert(a);
                    }
                    drop(archives);
                    if ps.global_paused {
                        info!("Job processing loaded as GLOBALLY PAUSED from persisted state");
                    }
                    let count = state.paused_archives.read().await.len();
                    if count > 0 {
                        info!(count, "Per-archive pauses loaded from persisted state");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse persisted pause state, defaulting to running");
                }
            }
        } else {
            debug!("No persisted pause state found, defaulting to all-running");
        }

        Ok(state)
    }

    /// Check if job processing is globally paused (hot path — lock-free).
    pub fn is_globally_paused(&self) -> bool {
        self.global_paused.load(Ordering::Relaxed)
    }

    /// Check if a specific archive is paused.
    pub async fn is_archive_paused(&self, archive: &str) -> bool {
        self.paused_archives.read().await.contains(archive)
    }

    /// Get the set of all paused archive names.
    pub async fn paused_archives(&self) -> HashSet<String> {
        self.paused_archives.read().await.clone()
    }

    /// Pause job processing globally.
    pub async fn pause_global(&self) -> Result<()> {
        self.global_paused.store(true, Ordering::SeqCst);
        info!("Job processing PAUSED globally");
        self.persist().await
    }

    /// Resume job processing globally.
    pub async fn resume_global(&self) -> Result<()> {
        self.global_paused.store(false, Ordering::SeqCst);
        info!("Job processing RESUMED globally");
        self.persist().await
    }

    /// Pause job processing for a specific archive.
    pub async fn pause_archive(&self, archive: &str) -> Result<()> {
        {
            let mut archives = self.paused_archives.write().await;
            archives.insert(archive.to_string());
        }
        info!(archive, "Job processing PAUSED for archive");
        self.persist().await
    }

    /// Resume job processing for a specific archive.
    pub async fn resume_archive(&self, archive: &str) -> Result<()> {
        {
            let mut archives = self.paused_archives.write().await;
            archives.remove(archive);
        }
        info!(archive, "Job processing RESUMED for archive");
        self.persist().await
    }

    /// Build the full pause state response with optional queue stats.
    pub async fn state(&self, queue_stats: Option<JobPauseQueueStats>) -> JobPauseState {
        let global = if self.is_globally_paused() {
            "paused"
        } else {
            "running"
        };

        let paused = self.paused_archives.read().await;
        let mut archives = HashMap::new();
        for a in paused.iter() {
            archives.insert(a.clone(), "paused".to_string());
        }

        JobPauseState {
            global: global.to_string(),
            archives,
            queue: queue_stats,
        }
    }

    /// Get the list of paused archive names for SQL filtering.
    pub async fn paused_archive_names(&self) -> Vec<String> {
        self.paused_archives.read().await.iter().cloned().collect()
    }

    /// Persist current state to the database.
    async fn persist(&self) -> Result<()> {
        let state = PersistedPauseState {
            global_paused: self.global_paused.load(Ordering::SeqCst),
            paused_archives: self.paused_archives.read().await.iter().cloned().collect(),
        };

        let value = serde_json::to_value(&state)
            .map_err(|e| Error::Internal(format!("Failed to serialize pause state: {e}")))?;

        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, NOW())
             ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()",
        )
        .bind(CONFIG_KEY)
        .bind(&value)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        debug!("Pause state persisted to database");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persisted_state_serialization() {
        let state = PersistedPauseState {
            global_paused: true,
            paused_archives: vec!["research".to_string(), "personal".to_string()],
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"global_paused\":true"));
        assert!(json.contains("research"));
        assert!(json.contains("personal"));

        let recovered: PersistedPauseState = serde_json::from_str(&json).unwrap();
        assert!(recovered.global_paused);
        assert_eq!(recovered.paused_archives.len(), 2);
    }

    #[test]
    fn test_persisted_state_default() {
        let state = PersistedPauseState::default();
        assert!(!state.global_paused);
        assert!(state.paused_archives.is_empty());
    }

    #[test]
    fn test_job_pause_state_response() {
        let state = JobPauseState {
            global: "running".to_string(),
            archives: {
                let mut m = HashMap::new();
                m.insert("research".to_string(), "paused".to_string());
                m
            },
            queue: Some(JobPauseQueueStats {
                pending: 42,
                running: 3,
            }),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"global\":\"running\""));
        assert!(json.contains("\"research\":\"paused\""));
        assert!(json.contains("\"pending\":42"));
    }
}
