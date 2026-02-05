//! ColBERT token embeddings repository.
//!
//! Provides database operations for storing and retrieving token-level embeddings
//! used in ColBERT late interaction re-ranking.

use pgvector::Vector;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{Error, Result};

/// Token embedding data structure.
#[derive(Debug, Clone)]
pub struct TokenEmbedding {
    pub id: Uuid,
    pub note_id: Uuid,
    pub chunk_id: Option<Uuid>,
    pub token_position: i32,
    pub token_text: String,
    pub embedding: Vector,
    pub model: String,
}

/// ColBERT repository for token embeddings.
pub struct ColBERTRepository {
    pool: Pool<Postgres>,
}

impl ColBERTRepository {
    /// Create a new ColBERT repository.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Store token embeddings for a note.
    ///
    /// # Arguments
    /// * `note_id` - The note these tokens belong to
    /// * `tokens` - List of (position, text, embedding) tuples
    /// * `model` - Model name used to generate embeddings
    /// * `chunk_id` - Optional chunk reference for multi-chunk documents
    pub async fn store_token_embeddings(
        &self,
        note_id: Uuid,
        tokens: Vec<(i32, String, Vector)>,
        model: &str,
        chunk_id: Option<Uuid>,
    ) -> Result<()> {
        // Delete existing token embeddings for this note/chunk
        if let Some(chunk_id) = chunk_id {
            sqlx::query("DELETE FROM note_token_embeddings WHERE note_id = $1 AND chunk_id = $2")
                .bind(note_id)
                .bind(chunk_id)
                .execute(&self.pool)
                .await
                .map_err(Error::Database)?;
        } else {
            sqlx::query(
                "DELETE FROM note_token_embeddings WHERE note_id = $1 AND chunk_id IS NULL",
            )
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        }

        // Insert new token embeddings in batch
        if tokens.is_empty() {
            return Ok(());
        }

        // Use a transaction for atomicity
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        for (position, text, embedding) in tokens {
            sqlx::query(
                r#"
                INSERT INTO note_token_embeddings
                    (note_id, chunk_id, token_position, token_text, embedding, model)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(note_id)
            .bind(chunk_id)
            .bind(position)
            .bind(&text)
            .bind(&embedding)
            .bind(model)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;

        Ok(())
    }

    /// Retrieve token embeddings for a note.
    ///
    /// Returns tokens ordered by position.
    pub async fn get_token_embeddings(&self, note_id: Uuid) -> Result<Vec<TokenEmbedding>> {
        let rows = sqlx::query(
            r#"
            SELECT id, note_id, chunk_id, token_position, token_text, embedding, model
            FROM note_token_embeddings
            WHERE note_id = $1
            ORDER BY token_position
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let tokens = rows
            .into_iter()
            .map(|row| TokenEmbedding {
                id: row.get("id"),
                note_id: row.get("note_id"),
                chunk_id: row.get("chunk_id"),
                token_position: row.get("token_position"),
                token_text: row.get("token_text"),
                embedding: row.get("embedding"),
                model: row.get("model"),
            })
            .collect();

        Ok(tokens)
    }

    /// Retrieve token embeddings for a specific chunk.
    pub async fn get_chunk_token_embeddings(&self, chunk_id: Uuid) -> Result<Vec<TokenEmbedding>> {
        let rows = sqlx::query(
            r#"
            SELECT id, note_id, chunk_id, token_position, token_text, embedding, model
            FROM note_token_embeddings
            WHERE chunk_id = $1
            ORDER BY token_position
            "#,
        )
        .bind(chunk_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let tokens = rows
            .into_iter()
            .map(|row| TokenEmbedding {
                id: row.get("id"),
                note_id: row.get("note_id"),
                chunk_id: row.get("chunk_id"),
                token_position: row.get("token_position"),
                token_text: row.get("token_text"),
                embedding: row.get("embedding"),
                model: row.get("model"),
            })
            .collect();

        Ok(tokens)
    }

    /// Check if a note has token embeddings.
    pub async fn has_embeddings(&self, note_id: Uuid) -> Result<bool> {
        let row = sqlx::query("SELECT has_colbert_embeddings($1) as has_embeddings")
            .bind(note_id)
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(row.get("has_embeddings"))
    }

    /// Get token count for a note.
    pub async fn get_token_count(&self, note_id: Uuid) -> Result<i32> {
        let row = sqlx::query("SELECT get_token_count($1) as count")
            .bind(note_id)
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(row.get("count"))
    }

    /// Delete token embeddings for a note.
    pub async fn delete_token_embeddings(&self, note_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note_token_embeddings WHERE note_id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    /// Get notes that have ColBERT embeddings.
    pub async fn get_notes_with_embeddings(&self, limit: i64) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT note_id
            FROM note_token_embeddings
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|row| row.get("note_id")).collect())
    }

    /// Get statistics about ColBERT embeddings.
    pub async fn get_stats(&self) -> Result<ColBERTStats> {
        let row = sqlx::query("SELECT * FROM colbert_embedding_stats")
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(ColBERTStats {
            notes_with_tokens: row.get::<Option<i64>, _>("notes_with_tokens").unwrap_or(0) as i32,
            total_tokens: row.get::<Option<i64>, _>("total_tokens").unwrap_or(0) as i64,
            avg_tokens_per_note: row
                .get::<Option<f64>, _>("avg_tokens_per_note")
                .unwrap_or(0.0) as f32,
            max_tokens_per_note: row
                .get::<Option<i64>, _>("max_tokens_per_note")
                .unwrap_or(0) as i32,
            model_count: row.get::<Option<i64>, _>("model_count").unwrap_or(0) as i32,
            total_size: row
                .get::<Option<String>, _>("total_size")
                .unwrap_or_else(|| "0 bytes".to_string()),
        })
    }
}

/// Statistics about ColBERT embeddings.
#[derive(Debug, Clone)]
pub struct ColBERTStats {
    pub notes_with_tokens: i32,
    pub total_tokens: i64,
    pub avg_tokens_per_note: f32,
    pub max_tokens_per_note: i32,
    pub model_count: i32,
    pub total_size: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_embedding_structure() {
        let embedding = TokenEmbedding {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            chunk_id: Some(Uuid::new_v4()),
            token_position: 0,
            token_text: "test".to_string(),
            embedding: Vector::from(vec![0.1; 128]),
            model: "colbert-v2".to_string(),
        };

        assert_eq!(embedding.token_position, 0);
        assert_eq!(embedding.token_text, "test");
        assert_eq!(embedding.model, "colbert-v2");
        assert!(embedding.chunk_id.is_some());
    }

    #[test]
    fn test_colbert_stats_structure() {
        let stats = ColBERTStats {
            notes_with_tokens: 100,
            total_tokens: 50000,
            avg_tokens_per_note: 500.0,
            max_tokens_per_note: 2000,
            model_count: 1,
            total_size: "10 MB".to_string(),
        };

        assert_eq!(stats.notes_with_tokens, 100);
        assert_eq!(stats.total_tokens, 50000);
        assert_eq!(stats.avg_tokens_per_note, 500.0);
        assert_eq!(stats.max_tokens_per_note, 2000);
    }

    // Integration tests would go here testing actual database operations
    // These require a test database setup and are typically in tests/ directory
}
