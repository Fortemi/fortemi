//! Embedding repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use pgvector::Vector;
use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{Embedding, EmbeddingRepository, Error, Result, SearchHit};

/// PostgreSQL implementation of EmbeddingRepository.
pub struct PgEmbeddingRepository {
    pool: Pool<Postgres>,
}

impl PgEmbeddingRepository {
    /// Create a new PgEmbeddingRepository with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EmbeddingRepository for PgEmbeddingRepository {
    async fn store(
        &self,
        note_id: Uuid,
        chunks: Vec<(String, Vector)>,
        model: &str,
    ) -> Result<()> {
        // Delete existing embeddings
        sqlx::query("DELETE FROM embedding WHERE note_id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        if chunks.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        let now = Utc::now();

        for (i, (text, vector)) in chunks.into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO embedding (id, note_id, chunk_index, text, vector, model, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(Uuid::new_v4())
            .bind(note_id)
            .bind(i as i32)
            .bind(&text)
            .bind(&vector)
            .bind(model)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    async fn get_for_note(&self, note_id: Uuid) -> Result<Vec<Embedding>> {
        let rows = sqlx::query(
            "SELECT id, note_id, chunk_index, text, vector, model
             FROM embedding
             WHERE note_id = $1
             ORDER BY chunk_index",
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        let embeddings = rows
            .into_iter()
            .map(|row| Embedding {
                id: row.get("id"),
                note_id: row.get("note_id"),
                chunk_index: row.get("chunk_index"),
                text: row.get("text"),
                vector: row.get("vector"),
                model: row.get("model"),
            })
            .collect();

        Ok(embeddings)
    }

    async fn delete_for_note(&self, note_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM embedding WHERE note_id = $1")
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(())
    }

    async fn find_similar(
        &self,
        query_vec: &Vector,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        let archive_clause = if exclude_archived {
            "AND (n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "AND n.deleted_at IS NULL"
        };

        let query = format!(
            r#"
            SELECT e.note_id AS note_id,
                   1.0 - (e.vector <=> $1::vector) AS score
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            WHERE TRUE {}
            ORDER BY e.vector <=> $1::vector
            LIMIT $2
            "#,
            archive_clause
        );

        let rows = sqlx::query(&query)
            .bind(query_vec)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let results = rows
            .into_iter()
            .map(|row| SearchHit {
                note_id: row.get("note_id"),
                score: row.get::<f64, _>("score") as f32,
                snippet: None,
            })
            .collect();

        Ok(results)
    }
}

/// Utility functions for embedding operations.
pub mod utils {
    /// Chunk text into pieces of at most `max_chars` characters.
    pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
        if text.is_empty() {
            return vec![];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            // Find end position, respecting character boundaries
            let end = if start + max_chars >= text.len() {
                text.len()
            } else {
                // Try to break at a natural boundary (space, newline)
                let raw_end = start + max_chars;
                let slice = &text[start..raw_end];

                // Look for last newline or space
                if let Some(pos) = slice.rfind('\n') {
                    start + pos + 1
                } else if let Some(pos) = slice.rfind(' ') {
                    start + pos + 1
                } else {
                    // Fall back to raw boundary, but ensure valid UTF-8
                    let mut end = raw_end;
                    while !text.is_char_boundary(end) && end > start {
                        end -= 1;
                    }
                    end
                }
            };

            if end > start {
                chunks.push(text[start..end].trim().to_string());
            }
            start = end;
        }

        // Filter out empty chunks
        chunks.into_iter().filter(|s| !s.is_empty()).collect()
    }

    /// Chunk text with overlap between chunks.
    pub fn chunk_text_with_overlap(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
        if text.is_empty() {
            return vec![];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            let end = (start + chunk_size).min(text.len());

            // Ensure we don't break in the middle of a UTF-8 character
            let mut end = end;
            while !text.is_char_boundary(end) && end > start {
                end -= 1;
            }

            if end > start {
                chunks.push(text[start..end].trim().to_string());
            }

            // Move start forward, accounting for overlap
            let step = chunk_size.saturating_sub(overlap);
            if step == 0 {
                break; // Prevent infinite loop
            }
            start += step;

            // Ensure start is at a character boundary
            while !text.is_char_boundary(start) && start < text.len() {
                start += 1;
            }
        }

        chunks.into_iter().filter(|s| !s.is_empty()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::utils::*;

    #[test]
    fn test_chunk_text() {
        let text = "Hello world. This is a test.";
        let chunks = chunk_text(text, 15);
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.len() <= 15 || !chunk.contains(' '));
        }
    }

    #[test]
    fn test_chunk_empty() {
        let chunks = chunk_text("", 100);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_with_overlap() {
        let text = "ABCDEFGHIJ";
        let chunks = chunk_text_with_overlap(text, 5, 2);
        assert!(!chunks.is_empty());
    }
}
