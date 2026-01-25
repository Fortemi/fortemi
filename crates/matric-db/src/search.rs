//! Full-text search implementation.

use sqlx::{Pool, Postgres, Row};
use uuid::Uuid;

use matric_core::{Error, Result, SearchHit, StrictTagFilter};

use crate::strict_filter::{QueryParam, StrictFilterQueryBuilder};

/// Full-text search provider using PostgreSQL tsvector.
pub struct PgFtsSearch {
    pool: Pool<Postgres>,
}

impl PgFtsSearch {
    /// Create a new PgFtsSearch with the given connection pool.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Perform full-text search on notes.
    pub async fn search(
        &self,
        query: &str,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        let archive_clause = if exclude_archived {
            "AND (n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "AND n.deleted_at IS NULL"
        };

        let sql = format!(
            r#"
            SELECT n.id as note_id,
                   ts_rank(nrc.tsv, plainto_tsquery('english', $1)) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE nrc.tsv @@ plainto_tsquery('english', $1)
              {}
            ORDER BY score DESC
            LIMIT $2
            "#,
            archive_clause
        );

        let rows = sqlx::query(&sql)
            .bind(query)
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
                    score: row.get::<Option<f32>, _>("score").unwrap_or(0.0),
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                }
            })
            .collect();

        Ok(results)
    }

    /// Perform full-text search with strict tag filter.
    ///
    /// Uses a CTE approach to filter notes by SKOS concepts before applying FTS,
    /// ensuring precise taxonomy-based result segregation.
    pub async fn search_with_strict_filter(
        &self,
        query: &str,
        strict_filter: Option<&StrictTagFilter>,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        // If no filter provided, fall back to regular search
        let Some(filter) = strict_filter else {
            return self.search(query, limit, exclude_archived).await;
        };

        // If filter is empty, fall back to regular search
        if filter.is_empty() {
            return self.search(query, limit, exclude_archived).await;
        }

        let archive_clause = if exclude_archived {
            "(n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "n.deleted_at IS NULL"
        };

        // Build strict filter SQL using the query builder
        let builder = StrictFilterQueryBuilder::new(filter.clone(), 1); // param $1 is query, strict filter starts at $2
        let (strict_filter_clause, filter_params) = builder.build();

        // Build the query with CTE for filtered notes
        let sql = format!(
            r#"
            WITH filtered_notes AS (
                SELECT n.id
                FROM note n
                WHERE {}
                  AND {}
            )
            SELECT n.id as note_id,
                   ts_rank(nrc.tsv, plainto_tsquery('english', $1)) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM filtered_notes fn
            JOIN note n ON n.id = fn.id
            JOIN note_revised_current nrc ON nrc.note_id = n.id
            WHERE nrc.tsv @@ plainto_tsquery('english', $1)
            ORDER BY score DESC
            LIMIT ${}
            "#,
            archive_clause,
            strict_filter_clause,
            filter_params.len() + 2 // +1 for query, +1 for limit
        );

        // Build the query with dynamic parameters
        let mut q = sqlx::query(&sql);
        q = q.bind(query); // $1

        // Bind strict filter parameters
        for param in &filter_params {
            q = match param {
                QueryParam::Uuid(id) => q.bind(id),
                QueryParam::UuidArray(ids) => q.bind(ids),
                QueryParam::Int(val) => q.bind(val),
                QueryParam::Timestamp(ts) => q.bind(ts),
                QueryParam::Bool(b) => q.bind(b),
                QueryParam::String(s) => q.bind(s),
            };
        }

        q = q.bind(limit); // Final parameter

        let rows = q.fetch_all(&self.pool).await.map_err(Error::Database)?;

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
                    score: row.get::<Option<f32>, _>("score").unwrap_or(0.0),
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                }
            })
            .collect();

        Ok(results)
    }

    /// Perform filtered full-text search.
    ///
    /// Supports filter syntax:
    /// - `tag:tagname` - filter by tag
    /// - `collection:uuid` - filter by collection
    pub async fn search_filtered(
        &self,
        query: &str,
        filters: &str,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        let archive_clause = if exclude_archived {
            "AND (n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "AND n.deleted_at IS NULL"
        };

        let mut sql = format!(
            r#"
            SELECT n.id as note_id,
                   ts_rank(nrc.tsv, plainto_tsquery('english', $1)) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE nrc.tsv @@ plainto_tsquery('english', $1)
              {}
            "#,
            archive_clause
        );

        // Parse and apply filters
        let mut params: Vec<String> = vec![query.to_string()];
        for token in filters.split_whitespace() {
            if let Some(tag) = token.strip_prefix("tag:") {
                params.push(tag.to_string());
                sql.push_str(&format!(
                    " AND n.id IN (SELECT note_id FROM note_tag WHERE tag_name = ${})",
                    params.len()
                ));
            } else if let Some(collection) = token.strip_prefix("collection:") {
                if let Ok(uuid) = Uuid::parse_str(collection) {
                    params.push(uuid.to_string());
                    sql.push_str(&format!(" AND n.collection_id = ${}::uuid", params.len()));
                }
            } else if let Some(ts) = token.strip_prefix("created_after:") {
                // Temporal filter: notes created after this ISO 8601 timestamp
                params.push(ts.to_string());
                sql.push_str(&format!(
                    " AND n.created_at >= ${}::timestamptz",
                    params.len()
                ));
            } else if let Some(ts) = token.strip_prefix("created_before:") {
                // Temporal filter: notes created before this ISO 8601 timestamp
                params.push(ts.to_string());
                sql.push_str(&format!(
                    " AND n.created_at <= ${}::timestamptz",
                    params.len()
                ));
            } else if let Some(ts) = token.strip_prefix("updated_after:") {
                // Temporal filter: notes updated after this ISO 8601 timestamp
                params.push(ts.to_string());
                sql.push_str(&format!(
                    " AND n.updated_at >= ${}::timestamptz",
                    params.len()
                ));
            } else if let Some(ts) = token.strip_prefix("updated_before:") {
                // Temporal filter: notes updated before this ISO 8601 timestamp
                params.push(ts.to_string());
                sql.push_str(&format!(
                    " AND n.updated_at <= ${}::timestamptz",
                    params.len()
                ));
            }
        }

        sql.push_str(&format!(" ORDER BY score DESC LIMIT ${}", params.len() + 1));

        // Build query with dynamic params
        let mut q = sqlx::query(&sql);
        for param in &params {
            q = q.bind(param);
        }
        q = q.bind(limit);

        let rows = q.fetch_all(&self.pool).await.map_err(Error::Database)?;

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
                    score: row.get::<Option<f32>, _>("score").unwrap_or(0.0),
                    snippet: row.get("snippet"),
                    title: row.get("title"),
                    tags,
                }
            })
            .collect();

        Ok(results)
    }

    /// Search for notes by keyword (FTS) and return IDs.
    pub async fn search_by_keyword(&self, term: &str, limit: i64) -> Result<Vec<Uuid>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT n.id
            FROM note n
            JOIN note_revised_current nrc ON nrc.note_id = n.id
            WHERE nrc.tsv @@ plainto_tsquery('english', $1)
              AND (n.archived IS FALSE OR n.archived IS NULL)
              AND n.deleted_at IS NULL
            LIMIT $2
            "#,
        )
        .bind(term)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_filter_parsing() {
        // Basic test for filter syntax
        let filters = "tag:rust collection:123e4567-e89b-12d3-a456-426614174000";
        let tokens: Vec<&str> = filters.split_whitespace().collect();
        assert_eq!(tokens.len(), 2);
        assert!(tokens[0].starts_with("tag:"));
        assert!(tokens[1].starts_with("collection:"));
    }
}
