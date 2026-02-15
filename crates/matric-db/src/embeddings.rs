//! Embedding repository implementation.

use async_trait::async_trait;
use chrono::Utc;
use pgvector::Vector;
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{new_v7, Embedding, EmbeddingRepository, Error, Result, SearchHit};

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
    async fn store(&self, note_id: Uuid, chunks: Vec<(String, Vector)>, model: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        self.store_tx(&mut tx, note_id, chunks, model).await?;
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
            SELECT DISTINCT ON (e.note_id)
                   e.note_id AS note_id,
                   1.0 - (e.vector <=> $1::vector) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = e.note_id
            WHERE TRUE {}
            ORDER BY e.note_id, e.vector <=> $1::vector
            "#,
            archive_clause
        );

        // Wrap to re-order by score after deduplication
        let wrapped_query = format!(
            "SELECT note_id, score, snippet, title, tags FROM ({}) sub ORDER BY score DESC LIMIT $2",
            query
        );

        let rows = sqlx::query(&wrapped_query)
            .bind(query_vec)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let results = rows
            .into_iter()
            .map(|row| {
                let tags_str: String = row.get("tags");
                let tags = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(String::from).collect()
                };
                SearchHit {
                    note_id: row.get("note_id"),
                    score: row.get::<f64, _>("score") as f32,
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                    embedding_status: None,
                }
            })
            .collect();

        Ok(results)
    }

    async fn find_similar_with_vectors(
        &self,
        query_vec: &Vector,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<(SearchHit, Vector)>> {
        let archive_clause = if exclude_archived {
            "AND (n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "AND n.deleted_at IS NULL"
        };

        let query = format!(
            r#"
            SELECT DISTINCT ON (e.note_id)
                   e.note_id AS note_id,
                   1.0 - (e.vector <=> $1::vector) AS score,
                   e.vector AS vector,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = e.note_id
            WHERE TRUE {}
            ORDER BY e.note_id, e.vector <=> $1::vector
            "#,
            archive_clause
        );

        let wrapped_query = format!(
            "SELECT note_id, score, vector, snippet, title, tags FROM ({}) sub ORDER BY score DESC LIMIT $2",
            query
        );

        let rows = sqlx::query(&wrapped_query)
            .bind(query_vec)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let results = rows
            .into_iter()
            .map(|row| {
                let tags_str: String = row.get("tags");
                let tags = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(String::from).collect()
                };
                let hit = SearchHit {
                    note_id: row.get("note_id"),
                    score: row.get::<f64, _>("score") as f32,
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                    embedding_status: None,
                };
                let vector: Vector = row.get("vector");
                (hit, vector)
            })
            .collect();

        Ok(results)
    }
}

// Additional methods not part of the trait
impl PgEmbeddingRepository {
    /// List all embeddings with pagination (for export).
    pub async fn list_all(&self, limit: i64, offset: i64) -> Result<Vec<Embedding>> {
        let rows = sqlx::query(
            "SELECT id, note_id, chunk_index, text, vector, model
             FROM embedding
             ORDER BY note_id, chunk_index
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
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

    /// Count total embeddings.
    pub async fn count(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM embedding")
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        Ok(row.get("count"))
    }

    /// Find similar embeddings within a specific embedding set.
    pub async fn find_similar_in_set(
        &self,
        query_vec: &Vector,
        embedding_set_id: Uuid,
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
            SELECT DISTINCT ON (e.note_id)
                   e.note_id AS note_id,
                   1.0 - (e.vector <=> $1::vector) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = e.note_id
            WHERE e.embedding_set_id = $3 {}
            ORDER BY e.note_id, e.vector <=> $1::vector
            "#,
            archive_clause
        );

        // Wrap to re-order by score after deduplication
        let wrapped_query = format!(
            "SELECT note_id, score, snippet, title, tags FROM ({}) sub ORDER BY score DESC LIMIT $2",
            query
        );

        let rows = sqlx::query(&wrapped_query)
            .bind(query_vec)
            .bind(limit)
            .bind(embedding_set_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let results = rows
            .into_iter()
            .map(|row| {
                let tags_str: String = row.get("tags");
                let tags = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(String::from).collect()
                };
                SearchHit {
                    note_id: row.get("note_id"),
                    score: row.get::<f64, _>("score") as f32,
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                    embedding_status: None,
                }
            })
            .collect();

        Ok(results)
    }

    /// Find similar embeddings with strict SKOS concept filtering.
    ///
    /// This applies strict tag filtering to ensure data isolation in multi-tenant scenarios.
    /// Only notes that match the strict filter criteria will be included in results.
    pub async fn find_similar_with_strict_filter(
        &self,
        query_vec: &Vector,
        strict_filter: &matric_core::StrictTagFilter,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        use crate::strict_filter::StrictFilterQueryBuilder;

        // If filter is unsatisfiable, return empty results immediately
        if strict_filter.match_none {
            return Ok(Vec::new());
        }

        // If filter is empty, fall back to regular search
        if strict_filter.is_empty() {
            return self.find_similar(query_vec, limit, exclude_archived).await;
        }

        let archive_clause = if exclude_archived {
            "(n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "n.deleted_at IS NULL"
        };

        // Build strict filter SQL using the query builder
        // param $1 is query_vec, $2 is limit, strict filter starts at $3
        let builder = StrictFilterQueryBuilder::new(strict_filter.clone(), 2);
        let (strict_filter_clause, filter_params) = builder.build();

        // Build the query with CTE for filtered notes
        let query = format!(
            r#"
            WITH filtered_notes AS (
                SELECT n.id
                FROM note n
                WHERE {}
                  AND {}
            )
            SELECT DISTINCT ON (e.note_id)
                   e.note_id AS note_id,
                   1.0 - (e.vector <=> $1::vector) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM embedding e
            JOIN filtered_notes fn ON fn.id = e.note_id
            JOIN note n ON n.id = e.note_id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = e.note_id
            ORDER BY e.note_id, e.vector <=> $1::vector
            "#,
            archive_clause, strict_filter_clause
        );

        // Wrap to re-order by score after deduplication
        let wrapped_query = format!(
            "SELECT note_id, score, snippet, title, tags FROM ({}) sub ORDER BY score DESC LIMIT $2",
            query
        );

        // Build the query with dynamic parameters
        let mut query_builder = sqlx::query(&wrapped_query).bind(query_vec).bind(limit);

        // Bind all strict filter parameters
        for param in filter_params {
            query_builder = match param {
                crate::strict_filter::QueryParam::Uuid(v) => query_builder.bind(v),
                crate::strict_filter::QueryParam::UuidArray(v) => query_builder.bind(v),
                crate::strict_filter::QueryParam::Int(v) => query_builder.bind(v),
                crate::strict_filter::QueryParam::Timestamp(v) => query_builder.bind(v),
                crate::strict_filter::QueryParam::Bool(v) => query_builder.bind(v),
                crate::strict_filter::QueryParam::String(v) => query_builder.bind(v),
                crate::strict_filter::QueryParam::StringArray(v) => query_builder.bind(v),
            };
        }

        let rows = query_builder
            .fetch_all(&self.pool)
            .await
            .map_err(Error::Database)?;

        let results = rows
            .into_iter()
            .map(|row| {
                let tags_str: String = row.get("tags");
                let tags = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(String::from).collect()
                };
                SearchHit {
                    note_id: row.get("note_id"),
                    score: row.get::<f64, _>("score") as f32,
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                    embedding_status: None,
                }
            })
            .collect();

        Ok(results)
    }
}

// Transaction-aware variants for archive-scoped operations
impl PgEmbeddingRepository {
    /// Store embeddings scoped to a specific embedding set.
    ///
    /// Unlike `store()`, this only deletes embeddings for the given set (not all sets),
    /// making it safe for multi-set scenarios.
    pub async fn store_for_set(
        &self,
        note_id: Uuid,
        embedding_set_id: Uuid,
        chunks: Vec<(String, Vector)>,
        model: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;

        // Delete only embeddings for this specific set
        sqlx::query("DELETE FROM embedding WHERE note_id = $1 AND embedding_set_id = $2")
            .bind(note_id)
            .bind(embedding_set_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;

        if !chunks.is_empty() {
            let now = Utc::now();
            for (i, (text, vector)) in chunks.into_iter().enumerate() {
                sqlx::query(
                    "INSERT INTO embedding (id, note_id, chunk_index, text, vector, model, created_at, embedding_set_id)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .bind(new_v7())
                .bind(note_id)
                .bind(i as i32)
                .bind(&text)
                .bind(&vector)
                .bind(model)
                .bind(now)
                .bind(embedding_set_id)
                .execute(&mut *tx)
                .await
                .map_err(Error::Database)?;
            }
        }

        tx.commit().await.map_err(Error::Database)?;
        Ok(())
    }

    /// Store embeddings within an existing transaction.
    pub async fn store_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
        chunks: Vec<(String, Vector)>,
        model: &str,
    ) -> Result<()> {
        // Delete existing embeddings
        sqlx::query("DELETE FROM embedding WHERE note_id = $1")
            .bind(note_id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;

        if chunks.is_empty() {
            return Ok(());
        }

        // Get the default embedding set ID
        let default_set_id: Option<Uuid> =
            sqlx::query_scalar("SELECT get_default_embedding_set_id()")
                .fetch_optional(&mut **tx)
                .await
                .map_err(Error::Database)?;

        // If no default set exists, create one
        let embedding_set_id = match default_set_id {
            Some(id) => id,
            None => {
                return Err(Error::Internal(
                    "No default embedding set found. Run migrations to create default set."
                        .to_string(),
                ));
            }
        };

        let now = Utc::now();

        for (i, (text, vector)) in chunks.into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO embedding (id, note_id, chunk_index, text, vector, model, created_at, embedding_set_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(new_v7())
            .bind(note_id)
            .bind(i as i32)
            .bind(&text)
            .bind(&vector)
            .bind(model)
            .bind(now)
            .bind(embedding_set_id)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        }

        Ok(())
    }

    /// Get embeddings for a note within an existing transaction.
    pub async fn get_for_note_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Vec<Embedding>> {
        let rows = sqlx::query(
            "SELECT id, note_id, chunk_index, text, vector, model
             FROM embedding
             WHERE note_id = $1
             ORDER BY chunk_index",
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
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

    /// Find similar embeddings within an existing transaction.
    pub async fn find_similar_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
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
            SELECT DISTINCT ON (e.note_id)
                   e.note_id AS note_id,
                   1.0 - (e.vector <=> $1::vector) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM embedding e
            JOIN note n ON n.id = e.note_id
            LEFT JOIN note_revised_current nrc ON nrc.note_id = e.note_id
            WHERE TRUE {}
            ORDER BY e.note_id, e.vector <=> $1::vector
            "#,
            archive_clause
        );

        // Wrap to re-order by score after deduplication
        let wrapped_query = format!(
            "SELECT note_id, score, snippet, title, tags FROM ({}) sub ORDER BY score DESC LIMIT $2",
            query
        );

        let rows = sqlx::query(&wrapped_query)
            .bind(query_vec)
            .bind(limit)
            .fetch_all(&mut **tx)
            .await
            .map_err(Error::Database)?;

        let results = rows
            .into_iter()
            .map(|row| {
                let tags_str: String = row.get("tags");
                let tags = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(String::from).collect()
                };
                SearchHit {
                    note_id: row.get("note_id"),
                    score: row.get::<f64, _>("score") as f32,
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                    embedding_status: None,
                }
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
