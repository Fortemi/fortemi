//! W3C PROV provenance tracking repository.
//!
//! Implements provenance tracking for AI revision operations following
//! the W3C PROV Data Model (PROV-DM). Tracks:
//! - **Entities**: Note revisions (provenance_edge)
//! - **Activities**: AI processing operations (provenance_activity)
//! - **Relations**: Derivation chains between source notes and revisions

use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{
    Error, ProvRelation, ProvenanceActivity, ProvenanceChain, ProvenanceEdge, Result,
};

/// PostgreSQL provenance repository.
pub struct PgProvenanceRepository {
    pool: Pool<Postgres>,
}

impl PgProvenanceRepository {
    /// Create a new provenance repository.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Record a provenance edge (entity relationship).
    ///
    /// Creates a W3C PROV relationship between a revision and its source.
    pub async fn record_edge(
        &self,
        revision_id: Uuid,
        source_note_id: Option<Uuid>,
        source_url: Option<&str>,
        relation: &ProvRelation,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO provenance_edge (revision_id, source_note_id, source_url, relation)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(revision_id)
        .bind(source_note_id)
        .bind(source_url)
        .bind(relation.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Record multiple provenance edges for a revision.
    ///
    /// Used when an AI revision incorporates context from multiple source notes.
    pub async fn record_edges_batch(
        &self,
        revision_id: Uuid,
        source_note_ids: &[Uuid],
        relation: &ProvRelation,
    ) -> Result<usize> {
        let mut count = 0;
        for source_id in source_note_ids {
            sqlx::query(
                r#"
                INSERT INTO provenance_edge (revision_id, source_note_id, relation)
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(revision_id)
            .bind(source_id)
            .bind(relation.as_str())
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
            count += 1;
        }
        Ok(count)
    }

    /// Start a provenance activity (AI processing operation).
    ///
    /// Returns the activity ID for later completion with `complete_activity`.
    pub async fn start_activity(
        &self,
        note_id: Uuid,
        activity_type: &str,
        model_name: Option<&str>,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO provenance_activity (note_id, activity_type, model_name)
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(note_id)
        .bind(activity_type)
        .bind(model_name)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Complete a provenance activity with final metadata.
    pub async fn complete_activity(
        &self,
        activity_id: Uuid,
        revision_id: Option<Uuid>,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE provenance_activity
            SET ended_at = NOW(),
                revision_id = COALESCE($2, revision_id),
                metadata = COALESCE($3, metadata)
            WHERE id = $1
            "#,
        )
        .bind(activity_id)
        .bind(revision_id)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    /// Get provenance edges for a specific revision.
    pub async fn get_edges_for_revision(&self, revision_id: Uuid) -> Result<Vec<ProvenanceEdge>> {
        let rows = sqlx::query(
            r#"
            SELECT id, revision_id, source_note_id, source_url, relation, created_at_utc
            FROM provenance_edge
            WHERE revision_id = $1
            ORDER BY created_at_utc
            "#,
        )
        .bind(revision_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| ProvenanceEdge {
                id: row.get("id"),
                revision_id: row.get("revision_id"),
                source_note_id: row.get("source_note_id"),
                source_url: row.get("source_url"),
                relation: row.get("relation"),
                created_at_utc: row.get("created_at_utc"),
            })
            .collect())
    }

    /// Get provenance edges for all revisions of a note.
    pub async fn get_edges_for_note(&self, note_id: Uuid) -> Result<Vec<ProvenanceEdge>> {
        let rows = sqlx::query(
            r#"
            SELECT pe.id, pe.revision_id, pe.source_note_id, pe.source_url,
                   pe.relation, pe.created_at_utc
            FROM provenance_edge pe
            JOIN note_revision nr ON nr.id = pe.revision_id
            WHERE nr.note_id = $1
            ORDER BY pe.created_at_utc
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| ProvenanceEdge {
                id: row.get("id"),
                revision_id: row.get("revision_id"),
                source_note_id: row.get("source_note_id"),
                source_url: row.get("source_url"),
                relation: row.get("relation"),
                created_at_utc: row.get("created_at_utc"),
            })
            .collect())
    }

    /// Get activities for a note.
    pub async fn get_activities_for_note(&self, note_id: Uuid) -> Result<Vec<ProvenanceActivity>> {
        let rows = sqlx::query(
            r#"
            SELECT id, note_id, revision_id, activity_type, model_name,
                   started_at, ended_at, metadata
            FROM provenance_activity
            WHERE note_id = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| ProvenanceActivity {
                id: row.get("id"),
                note_id: row.get("note_id"),
                revision_id: row.get("revision_id"),
                activity_type: row.get("activity_type"),
                model_name: row.get("model_name"),
                started_at: row.get("started_at"),
                ended_at: row.get("ended_at"),
                metadata: row.get("metadata"),
            })
            .collect())
    }

    /// Get the full provenance chain for a note's current revision.
    pub async fn get_chain(&self, note_id: Uuid) -> Result<Option<ProvenanceChain>> {
        // Get the current revision ID
        let revision_row = sqlx::query(
            r#"
            SELECT id FROM note_revision
            WHERE note_id = $1
            ORDER BY created_at_utc DESC
            LIMIT 1
            "#,
        )
        .bind(note_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        let revision_id = match revision_row {
            Some(row) => row.get::<Uuid, _>("id"),
            None => return Ok(None),
        };

        let edges = self.get_edges_for_revision(revision_id).await?;

        // Get the latest activity for this revision
        let activity_row = sqlx::query(
            r#"
            SELECT id, note_id, revision_id, activity_type, model_name,
                   started_at, ended_at, metadata
            FROM provenance_activity
            WHERE note_id = $1 AND revision_id = $2
            ORDER BY started_at DESC
            LIMIT 1
            "#,
        )
        .bind(note_id)
        .bind(revision_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        let activity = activity_row.map(|row| ProvenanceActivity {
            id: row.get("id"),
            note_id: row.get("note_id"),
            revision_id: row.get("revision_id"),
            activity_type: row.get("activity_type"),
            model_name: row.get("model_name"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            metadata: row.get("metadata"),
        });

        Ok(Some(ProvenanceChain {
            note_id,
            revision_id,
            activity,
            edges,
        }))
    }

    /// Get notes that cite/derive from a specific source note.
    /// Useful for impact analysis: "what notes were influenced by this note?"
    pub async fn get_derived_notes(&self, source_note_id: Uuid) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT nr.note_id
            FROM provenance_edge pe
            JOIN note_revision nr ON nr.id = pe.revision_id
            WHERE pe.source_note_id = $1
            "#,
        )
        .bind(source_note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("note_id")).collect())
    }
}

/// Transaction-aware variants for provenance operations.
impl PgProvenanceRepository {
    /// Get the full provenance chain for a note's current revision within an existing transaction.
    pub async fn get_chain_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Option<ProvenanceChain>> {
        // Get the current revision ID
        let revision_row = sqlx::query(
            r#"
            SELECT id FROM note_revision
            WHERE note_id = $1
            ORDER BY created_at_utc DESC
            LIMIT 1
            "#,
        )
        .bind(note_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let revision_id = match revision_row {
            Some(row) => row.get::<Uuid, _>("id"),
            None => return Ok(None),
        };

        // Get edges for revision (inline from get_edges_for_revision)
        let edge_rows = sqlx::query(
            r#"
            SELECT id, revision_id, source_note_id, source_url, relation, created_at_utc
            FROM provenance_edge
            WHERE revision_id = $1
            ORDER BY created_at_utc
            "#,
        )
        .bind(revision_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let edges = edge_rows
            .into_iter()
            .map(|row| ProvenanceEdge {
                id: row.get("id"),
                revision_id: row.get("revision_id"),
                source_note_id: row.get("source_note_id"),
                source_url: row.get("source_url"),
                relation: row.get("relation"),
                created_at_utc: row.get("created_at_utc"),
            })
            .collect();

        // Get the latest activity for this revision
        let activity_row = sqlx::query(
            r#"
            SELECT id, note_id, revision_id, activity_type, model_name,
                   started_at, ended_at, metadata
            FROM provenance_activity
            WHERE note_id = $1 AND revision_id = $2
            ORDER BY started_at DESC
            LIMIT 1
            "#,
        )
        .bind(note_id)
        .bind(revision_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        let activity = activity_row.map(|row| ProvenanceActivity {
            id: row.get("id"),
            note_id: row.get("note_id"),
            revision_id: row.get("revision_id"),
            activity_type: row.get("activity_type"),
            model_name: row.get("model_name"),
            started_at: row.get("started_at"),
            ended_at: row.get("ended_at"),
            metadata: row.get("metadata"),
        });

        Ok(Some(ProvenanceChain {
            note_id,
            revision_id,
            activity,
            edges,
        }))
    }

    /// Get activities for a note within an existing transaction.
    pub async fn get_activities_for_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Vec<ProvenanceActivity>> {
        let rows = sqlx::query(
            r#"
            SELECT id, note_id, revision_id, activity_type, model_name,
                   started_at, ended_at, metadata
            FROM provenance_activity
            WHERE note_id = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| ProvenanceActivity {
                id: row.get("id"),
                note_id: row.get("note_id"),
                revision_id: row.get("revision_id"),
                activity_type: row.get("activity_type"),
                model_name: row.get("model_name"),
                started_at: row.get("started_at"),
                ended_at: row.get("ended_at"),
                metadata: row.get("metadata"),
            })
            .collect())
    }

    /// Get provenance edges for all revisions of a note within an existing transaction.
    pub async fn get_edges_for_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Vec<ProvenanceEdge>> {
        let rows = sqlx::query(
            r#"
            SELECT pe.id, pe.revision_id, pe.source_note_id, pe.source_url,
                   pe.relation, pe.created_at_utc
            FROM provenance_edge pe
            JOIN note_revision nr ON nr.id = pe.revision_id
            WHERE nr.note_id = $1
            ORDER BY pe.created_at_utc
            "#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| ProvenanceEdge {
                id: row.get("id"),
                revision_id: row.get("revision_id"),
                source_note_id: row.get("source_note_id"),
                source_url: row.get("source_url"),
                relation: row.get("relation"),
                created_at_utc: row.get("created_at_utc"),
            })
            .collect())
    }

    /// Get notes that cite/derive from a specific source note within an existing transaction.
    pub async fn get_derived_notes_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        source_note_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT nr.note_id
            FROM provenance_edge pe
            JOIN note_revision nr ON nr.id = pe.revision_id
            WHERE pe.source_note_id = $1
            "#,
        )
        .bind(source_note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("note_id")).collect())
    }
}
