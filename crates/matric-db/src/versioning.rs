//! Note versioning repository for dual-track version history.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use std::fmt;
use uuid::Uuid;

use crate::error::{Error, Result};

/// A version entry in the original content history.
#[derive(Clone, sqlx::FromRow)]
pub struct OriginalVersion {
    pub id: Uuid,
    pub note_id: Uuid,
    pub version_number: i32,
    pub content: String,
    pub hash: String,
    pub created_at_utc: DateTime<Utc>,
    pub created_by: String,
}

impl fmt::Debug for OriginalVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OriginalVersion")
            .field("id_set", &(!self.id.is_nil()))
            .field("note_id_set", &true)
            .field("version_number", &self.version_number)
            .field("content_len", &self.content.len())
            .field("hash_len", &self.hash.len())
            .field("created_at_utc", &self.created_at_utc)
            .field("created_by_len", &self.created_by.len())
            .finish()
    }
}

/// Summary of a version (without full content).
#[derive(Clone)]
pub struct VersionSummary {
    pub version_number: i32,
    pub created_at_utc: DateTime<Utc>,
    pub created_by: String,
    pub is_current: bool,
}

impl fmt::Debug for VersionSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VersionSummary")
            .field("version_number", &self.version_number)
            .field("created_at_utc", &self.created_at_utc)
            .field("created_by_len", &self.created_by.len())
            .field("is_current", &self.is_current)
            .finish()
    }
}

/// A revision version summary from note_revision table.
#[derive(Clone, sqlx::FromRow)]
pub struct RevisionVersionSummary {
    pub id: Uuid,
    pub revision_number: i32,
    pub created_at_utc: DateTime<Utc>,
    pub model: Option<String>,
    pub is_user_edited: bool,
}

impl fmt::Debug for RevisionVersionSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RevisionVersionSummary")
            .field("id_set", &(!self.id.is_nil()))
            .field("revision_number", &self.revision_number)
            .field("created_at_utc", &self.created_at_utc)
            .field("model_len", &self.model.as_ref().map(String::len))
            .field("is_user_edited", &self.is_user_edited)
            .finish()
    }
}

/// Combined version listing for both tracks.
#[derive(Clone)]
pub struct NoteVersions {
    pub note_id: Uuid,
    pub current_original_version: i32,
    pub current_revision_number: Option<i32>,
    pub original_versions: Vec<VersionSummary>,
    pub revised_versions: Vec<RevisionVersionSummary>,
}

impl fmt::Debug for NoteVersions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoteVersions")
            .field("note_id_set", &true)
            .field("current_original_version", &self.current_original_version)
            .field("current_revision_number", &self.current_revision_number)
            .field("original_versions_count", &self.original_versions.len())
            .field("revised_versions_count", &self.revised_versions.len())
            .finish()
    }
}

/// Repository for note version history.
pub struct VersioningRepository {
    pool: PgPool,
}

impl VersioningRepository {
    /// Create a new versioning repository.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get all versions for a note (both tracks).
    pub async fn list_versions(&self, note_id: Uuid) -> Result<NoteVersions> {
        // Get current original version number
        let current_original: Option<(i32,)> =
            sqlx::query_as("SELECT version_number FROM note_original WHERE note_id = $1")
                .bind(note_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?;

        let current_original_version = current_original.map(|r| r.0).unwrap_or(1);

        // Get original version history
        let original_history: Vec<(i32, DateTime<Utc>, String)> = sqlx::query_as(
            r#"
            SELECT version_number, created_at_utc, created_by
            FROM note_original_history
            WHERE note_id = $1
            ORDER BY version_number DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let mut original_versions: Vec<VersionSummary> = original_history
            .into_iter()
            .map(
                |(version_number, created_at_utc, created_by)| VersionSummary {
                    version_number,
                    created_at_utc,
                    created_by,
                    is_current: false,
                },
            )
            .collect();

        // Add current version to the list
        if let Some((version,)) = current_original {
            // Get the timestamp from note_original
            let current_time: Option<(DateTime<Utc>,)> =
                sqlx::query_as("SELECT user_last_edited_at FROM note_original WHERE note_id = $1")
                    .bind(note_id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(Error::Database)?;

            original_versions.insert(
                0,
                VersionSummary {
                    version_number: version,
                    created_at_utc: current_time.map(|t| t.0).unwrap_or_else(Utc::now),
                    created_by: "user".to_string(),
                    is_current: true,
                },
            );
        }

        // Get revision versions
        let revised_versions: Vec<RevisionVersionSummary> = sqlx::query_as(
            r#"
            SELECT id, revision_number, created_at_utc, model, is_user_edited
            FROM note_revision
            WHERE note_id = $1
            ORDER BY revision_number DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Get current revision number (MAX can return NULL if no revisions exist)
        let current_revision: Option<(Option<i32>,)> = sqlx::query_as(
            r#"
            SELECT MAX(revision_number)
            FROM note_revision
            WHERE note_id = $1
            "#,
        )
        .bind(note_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(NoteVersions {
            note_id,
            current_original_version,
            current_revision_number: current_revision.and_then(|r| r.0),
            original_versions,
            revised_versions,
        })
    }

    /// Get a specific original version.
    pub async fn get_original_version(
        &self,
        note_id: Uuid,
        version: i32,
    ) -> Result<Option<OriginalVersion>> {
        // Check if this is the current version
        let current: Option<(i32, String, String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT version_number, content, hash, user_last_edited_at
            FROM note_original
            WHERE note_id = $1 AND version_number = $2
            "#,
        )
        .bind(note_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        if let Some((version_number, content, hash, created_at_utc)) = current {
            return Ok(Some(OriginalVersion {
                id: Uuid::nil(), // Current version doesn't have a history ID
                note_id,
                version_number,
                content,
                hash,
                created_at_utc,
                created_by: "user".to_string(),
            }));
        }

        // Check history
        let history: Option<OriginalVersion> = sqlx::query_as(
            r#"
            SELECT id, note_id, version_number, content, hash, created_at_utc, created_by
            FROM note_original_history
            WHERE note_id = $1 AND version_number = $2
            "#,
        )
        .bind(note_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(history)
    }

    /// Get a specific revision version.
    pub async fn get_revision_version(
        &self,
        note_id: Uuid,
        revision_number: i32,
    ) -> Result<Option<crate::RevisionVersion>> {
        let revision: Option<crate::RevisionVersion> = sqlx::query_as(
            r#"
            SELECT id, note_id, revision_number, content, type, summary, rationale,
                   created_at_utc, model, is_user_edited
            FROM note_revision
            WHERE note_id = $1 AND revision_number = $2
            "#,
        )
        .bind(note_id)
        .bind(revision_number)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(revision)
    }

    /// Restore a previous original version (creates a new version).
    pub async fn restore_original_version(
        &self,
        note_id: Uuid,
        version: i32,
        restore_tags: bool,
    ) -> Result<i32> {
        // Get the version to restore
        let version_data = self
            .get_original_version(note_id, version)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Version {} not found", version)))?;

        // Parse tags from YAML frontmatter if restore_tags is true
        let content_to_restore = if version_data.content.starts_with("---\n") {
            // Extract content after frontmatter
            if let Some(end_idx) = version_data.content[4..].find("\n---\n") {
                let frontmatter = &version_data.content[4..4 + end_idx];
                let actual_content = &version_data.content[4 + end_idx + 5..];

                // Restore tags if requested
                if restore_tags {
                    // Parse snapshot_tags from frontmatter
                    for line in frontmatter.lines() {
                        if let Some(tags_json) = line.strip_prefix("snapshot_tags: ") {
                            if let Ok(tags) = serde_json::from_str::<Vec<String>>(tags_json) {
                                // Delete current tags
                                sqlx::query("DELETE FROM note_tag WHERE note_id = $1")
                                    .bind(note_id)
                                    .execute(&self.pool)
                                    .await
                                    .map_err(Error::Database)?;

                                // Insert restored tags
                                for tag in tags {
                                    sqlx::query(
                                        "INSERT INTO note_tag (note_id, tag_name) VALUES ($1, $2)
                                         ON CONFLICT DO NOTHING",
                                    )
                                    .bind(note_id)
                                    .bind(&tag)
                                    .execute(&self.pool)
                                    .await
                                    .map_err(Error::Database)?;
                                }
                            }
                            break;
                        }
                    }
                }

                actual_content.to_string()
            } else {
                version_data.content.clone()
            }
        } else {
            version_data.content.clone()
        };

        // Update note_original (trigger will create history entry)
        let new_hash = format!("{:x}", md5::compute(&content_to_restore));

        sqlx::query(
            r#"
            UPDATE note_original
            SET content = $1, hash = $2
            WHERE note_id = $3
            "#,
        )
        .bind(&content_to_restore)
        .bind(&new_hash)
        .bind(note_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Mark the most recent history entry as a restore
        sqlx::query(
            r#"
            UPDATE note_original_history
            SET created_by = 'restore'
            WHERE id = (
                SELECT id FROM note_original_history
                WHERE note_id = $1
                ORDER BY version_number DESC
                LIMIT 1
            )
            "#,
        )
        .bind(note_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        // Get the new version number
        let new_version: (i32,) =
            sqlx::query_as("SELECT version_number FROM note_original WHERE note_id = $1")
                .bind(note_id)
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;

        Ok(new_version.0)
    }

    /// Delete a specific version from history.
    pub async fn delete_version(&self, note_id: Uuid, version: i32) -> Result<bool> {
        // Can't delete current version
        let current: Option<(i32,)> =
            sqlx::query_as("SELECT version_number FROM note_original WHERE note_id = $1")
                .bind(note_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?;

        if current.map(|c| c.0) == Some(version) {
            return Err(Error::InvalidInput(
                "Cannot delete current version".to_string(),
            ));
        }

        let result = sqlx::query(
            "DELETE FROM note_original_history WHERE note_id = $1 AND version_number = $2",
        )
        .bind(note_id)
        .bind(version)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all versions before a specific version.
    pub async fn delete_versions_before(&self, note_id: Uuid, before_version: i32) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM note_original_history WHERE note_id = $1 AND version_number < $2",
        )
        .bind(note_id)
        .bind(before_version)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected())
    }

    /// Generate a diff between two versions.
    pub async fn diff_versions(
        &self,
        note_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<String> {
        let from = self
            .get_original_version(note_id, from_version)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Version {} not found", from_version)))?;

        let to = self
            .get_original_version(note_id, to_version)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Version {} not found", to_version)))?;

        // Extract actual content (strip frontmatter if present)
        let from_content = strip_frontmatter(&from.content);
        let to_content = strip_frontmatter(&to.content);

        // Generate unified diff
        let diff = similar::TextDiff::from_lines(&from_content, &to_content);
        let mut output = String::new();

        output.push_str(&format!("--- version {}\n", from_version));
        output.push_str(&format!("+++ version {}\n", to_version));

        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                similar::ChangeTag::Delete => "-",
                similar::ChangeTag::Insert => "+",
                similar::ChangeTag::Equal => " ",
            };
            output.push_str(&format!("{}{}", sign, change));
        }

        Ok(output)
    }

    /// Check if versioning is enabled.
    pub async fn is_versioning_enabled(&self) -> Result<bool> {
        let result: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT value FROM user_config WHERE key = 'versioning_enabled'")
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?;

        Ok(result
            .map(|r| r.0.as_bool().unwrap_or(true))
            .unwrap_or(true))
    }

    /// Get max history setting.
    pub async fn get_max_history(&self) -> Result<i32> {
        let result: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT value FROM user_config WHERE key = 'versioning_max_history'")
                .fetch_optional(&self.pool)
                .await
                .map_err(Error::Database)?;

        Ok(result
            .map(|r| r.0.as_i64().unwrap_or(50) as i32)
            .unwrap_or(50))
    }

    /// Set versioning enabled/disabled.
    pub async fn set_versioning_enabled(&self, enabled: bool) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO user_config (key, value)
            VALUES ('versioning_enabled', $1::jsonb)
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value
            "#,
        )
        .bind(serde_json::json!(enabled))
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }
}

/// Transaction-aware variants for archive-scoped operations.
impl VersioningRepository {
    /// Transaction-aware variant of list_versions.
    pub async fn list_versions_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<NoteVersions> {
        // Get current original version number
        let current_original: Option<(i32,)> =
            sqlx::query_as("SELECT version_number FROM note_original WHERE note_id = $1")
                .bind(note_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(Error::Database)?;

        let current_original_version = current_original.map(|r| r.0).unwrap_or(1);

        // Get original version history
        let original_history: Vec<(i32, DateTime<Utc>, String)> = sqlx::query_as(
            r#"
            SELECT version_number, created_at_utc, created_by
            FROM note_original_history
            WHERE note_id = $1
            ORDER BY version_number DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let mut original_versions: Vec<VersionSummary> = original_history
            .into_iter()
            .map(
                |(version_number, created_at_utc, created_by)| VersionSummary {
                    version_number,
                    created_at_utc,
                    created_by,
                    is_current: false,
                },
            )
            .collect();

        // Add current version to the list
        if let Some((version,)) = current_original {
            // Get the timestamp from note_original
            let current_time: Option<(DateTime<Utc>,)> =
                sqlx::query_as("SELECT user_last_edited_at FROM note_original WHERE note_id = $1")
                    .bind(note_id)
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(Error::Database)?;

            original_versions.insert(
                0,
                VersionSummary {
                    version_number: version,
                    created_at_utc: current_time.map(|t| t.0).unwrap_or_else(Utc::now),
                    created_by: "user".to_string(),
                    is_current: true,
                },
            );
        }

        // Get revision versions
        let revised_versions: Vec<RevisionVersionSummary> = sqlx::query_as(
            r#"
            SELECT id, revision_number, created_at_utc, model, is_user_edited
            FROM note_revision
            WHERE note_id = $1
            ORDER BY revision_number DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Get current revision number (MAX can return NULL if no revisions exist)
        let current_revision: Option<(Option<i32>,)> = sqlx::query_as(
            r#"
            SELECT MAX(revision_number)
            FROM note_revision
            WHERE note_id = $1
            "#,
        )
        .bind(note_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(NoteVersions {
            note_id,
            current_original_version,
            current_revision_number: current_revision.and_then(|r| r.0),
            original_versions,
            revised_versions,
        })
    }

    /// Transaction-aware variant of get_original_version.
    pub async fn get_original_version_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        version: i32,
    ) -> Result<Option<OriginalVersion>> {
        // Check if this is the current version
        let current: Option<(i32, String, String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT version_number, content, hash, user_last_edited_at
            FROM note_original
            WHERE note_id = $1 AND version_number = $2
            "#,
        )
        .bind(note_id)
        .bind(version)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        if let Some((version_number, content, hash, created_at_utc)) = current {
            return Ok(Some(OriginalVersion {
                id: Uuid::nil(), // Current version doesn't have a history ID
                note_id,
                version_number,
                content,
                hash,
                created_at_utc,
                created_by: "user".to_string(),
            }));
        }

        // Check history
        let history: Option<OriginalVersion> = sqlx::query_as(
            r#"
            SELECT id, note_id, version_number, content, hash, created_at_utc, created_by
            FROM note_original_history
            WHERE note_id = $1 AND version_number = $2
            "#,
        )
        .bind(note_id)
        .bind(version)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(history)
    }

    /// Transaction-aware variant of get_revision_version.
    pub async fn get_revision_version_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        revision_number: i32,
    ) -> Result<Option<crate::RevisionVersion>> {
        let revision: Option<crate::RevisionVersion> = sqlx::query_as(
            r#"
            SELECT id, note_id, revision_number, content, type, summary, rationale,
                   created_at_utc, model, is_user_edited
            FROM note_revision
            WHERE note_id = $1 AND revision_number = $2
            "#,
        )
        .bind(note_id)
        .bind(revision_number)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(revision)
    }

    /// Transaction-aware variant of restore_original_version.
    pub async fn restore_original_version_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        version: i32,
        restore_tags: bool,
    ) -> Result<i32> {
        // Get the version to restore
        let version_data = self
            .get_original_version_tx(tx, note_id, version)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Version {} not found", version)))?;

        // Parse tags from YAML frontmatter if restore_tags is true
        let content_to_restore = if version_data.content.starts_with("---\n") {
            // Extract content after frontmatter
            if let Some(end_idx) = version_data.content[4..].find("\n---\n") {
                let frontmatter = &version_data.content[4..4 + end_idx];
                let actual_content = &version_data.content[4 + end_idx + 5..];

                // Restore tags if requested
                if restore_tags {
                    // Parse snapshot_tags from frontmatter
                    for line in frontmatter.lines() {
                        if let Some(tags_json) = line.strip_prefix("snapshot_tags: ") {
                            if let Ok(tags) = serde_json::from_str::<Vec<String>>(tags_json) {
                                // Delete current tags
                                sqlx::query("DELETE FROM note_tag WHERE note_id = $1")
                                    .bind(note_id)
                                    .execute(&mut **tx)
                                    .await
                                    .map_err(Error::Database)?;

                                // Insert restored tags
                                for tag in tags {
                                    sqlx::query(
                                        "INSERT INTO note_tag (note_id, tag_name) VALUES ($1, $2)
                                         ON CONFLICT DO NOTHING",
                                    )
                                    .bind(note_id)
                                    .bind(&tag)
                                    .execute(&mut **tx)
                                    .await
                                    .map_err(Error::Database)?;
                                }
                            }
                            break;
                        }
                    }
                }

                actual_content.to_string()
            } else {
                version_data.content.clone()
            }
        } else {
            version_data.content.clone()
        };

        // Update note_original (trigger will create history entry)
        let new_hash = format!("{:x}", md5::compute(&content_to_restore));

        sqlx::query(
            r#"
            UPDATE note_original
            SET content = $1, hash = $2
            WHERE note_id = $3
            "#,
        )
        .bind(&content_to_restore)
        .bind(&new_hash)
        .bind(note_id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Mark the most recent history entry as a restore
        sqlx::query(
            r#"
            UPDATE note_original_history
            SET created_by = 'restore'
            WHERE id = (
                SELECT id FROM note_original_history
                WHERE note_id = $1
                ORDER BY version_number DESC
                LIMIT 1
            )
            "#,
        )
        .bind(note_id)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        // Get the new version number
        let new_version: (i32,) =
            sqlx::query_as("SELECT version_number FROM note_original WHERE note_id = $1")
                .bind(note_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(Error::Database)?;

        Ok(new_version.0)
    }

    /// Transaction-aware variant of delete_version.
    pub async fn delete_version_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        version: i32,
    ) -> Result<bool> {
        // Can't delete current version
        let current: Option<(i32,)> =
            sqlx::query_as("SELECT version_number FROM note_original WHERE note_id = $1")
                .bind(note_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(Error::Database)?;

        if current.map(|c| c.0) == Some(version) {
            return Err(Error::InvalidInput(
                "Cannot delete current version".to_string(),
            ));
        }

        let result = sqlx::query(
            "DELETE FROM note_original_history WHERE note_id = $1 AND version_number = $2",
        )
        .bind(note_id)
        .bind(version)
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(result.rows_affected() > 0)
    }

    /// Transaction-aware variant of diff_versions.
    pub async fn diff_versions_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        from_version: i32,
        to_version: i32,
    ) -> Result<String> {
        let from = self
            .get_original_version_tx(tx, note_id, from_version)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Version {} not found", from_version)))?;

        let to = self
            .get_original_version_tx(tx, note_id, to_version)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Version {} not found", to_version)))?;

        // Extract actual content (strip frontmatter if present)
        let from_content = strip_frontmatter(&from.content);
        let to_content = strip_frontmatter(&to.content);

        // Generate unified diff
        let diff = similar::TextDiff::from_lines(&from_content, &to_content);
        let mut output = String::new();

        output.push_str(&format!("--- version {}\n", from_version));
        output.push_str(&format!("+++ version {}\n", to_version));

        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                similar::ChangeTag::Delete => "-",
                similar::ChangeTag::Insert => "+",
                similar::ChangeTag::Equal => " ",
            };
            output.push_str(&format!("{}{}", sign, change));
        }

        Ok(output)
    }
}

/// Strip YAML frontmatter from content.
fn strip_frontmatter(content: &str) -> String {
    if let Some(stripped) = content.strip_prefix("---\n") {
        if let Some(end_idx) = stripped.find("\n---\n") {
            return stripped[end_idx + 5..].to_string();
        }
    }
    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioning_debug_redacts_content_hash_ids_actor_and_model() {
        let version_id = Uuid::parse_str("018fd1a0-0000-7000-8000-000000000014").unwrap();
        let note_id = Uuid::parse_str("018fd1a0-0000-7000-8000-000000000015").unwrap();
        let revision_id = Uuid::parse_str("018fd1a0-0000-7000-8000-000000000016").unwrap();
        let timestamp = DateTime::parse_from_rfc3339("2026-06-27T08:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let original = OriginalVersion {
            id: version_id,
            note_id,
            version_number: 3,
            content: "private note customer@example.com postgres://user:secret@db.internal sk-version-token".to_string(),
            hash: "hash-secret-abcdef1234567890".to_string(),
            created_at_utc: timestamp,
            created_by: "editor@example.com".to_string(),
        };
        let summary = VersionSummary {
            version_number: 3,
            created_at_utc: timestamp,
            created_by: "editor@example.com".to_string(),
            is_current: true,
        };
        let revision = RevisionVersionSummary {
            id: revision_id,
            revision_number: 2,
            created_at_utc: timestamp,
            model: Some("tenant-private-model@example.internal".to_string()),
            is_user_edited: false,
        };
        let versions = NoteVersions {
            note_id,
            current_original_version: 3,
            current_revision_number: Some(2),
            original_versions: vec![summary.clone()],
            revised_versions: vec![revision.clone()],
        };

        let debug = format!("{original:?} {summary:?} {revision:?} {versions:?}");

        assert!(debug.contains("OriginalVersion"));
        assert!(debug.contains("content_len"));
        assert!(debug.contains("hash_len"));
        assert!(debug.contains("created_by_len"));
        assert!(debug.contains("RevisionVersionSummary"));
        assert!(debug.contains("model_len"));
        assert!(debug.contains("NoteVersions"));
        assert!(debug.contains("original_versions_count"));
        assert!(debug.contains("revised_versions_count"));
        assert!(!debug.contains(&version_id.to_string()));
        assert!(!debug.contains(&note_id.to_string()));
        assert!(!debug.contains(&revision_id.to_string()));
        assert!(!debug.contains("private note"));
        assert!(!debug.contains("customer@example.com"));
        assert!(!debug.contains("postgres://"));
        assert!(!debug.contains("db.internal"));
        assert!(!debug.contains("sk-version-token"));
        assert!(!debug.contains("hash-secret"));
        assert!(!debug.contains("editor@example.com"));
        assert!(!debug.contains("tenant-private-model"));
    }
}
