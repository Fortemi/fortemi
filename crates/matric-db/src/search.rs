//! Full-text search implementation.
//!
//! Supports multilingual search via:
//! - `websearch_to_tsquery()` for OR/NOT/phrase operators
//! - `matric_english` for English content (default)
//! - `matric_simple` for CJK and other scripts (no stemming)
//!
//! ## PG 18.2 Bug Workaround (#418)
//!
//! All snippet extraction uses `left(convert_from(convert_to(content, 'UTF8'), 'UTF8'), N)`
//! instead of `substring(content for N)` to work around PostgreSQL Bug #19406.
//! The `substring()`/`left()` functions fail with "invalid byte sequence for
//! encoding UTF8" on TOAST-compressed text containing multi-byte characters.
//! Fixed upstream Feb 2026 — revert to `substring()` after upgrading to PG 18.3+.

use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{Error, Result, SearchHit, StrictTagFilter};

use crate::escape_like;
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

    /// Perform full-text search on notes using BM25F field-weighted scoring.
    ///
    /// Combines weighted tsvectors from title (weight A), tags (weight B),
    /// and content (weight C) to produce field-weighted ranking.
    /// This implements BM25F-style scoring where title matches rank highest.
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

        // BM25F field-weighted scoring: title (A=1.0) > tags (B=0.4) > content (C=0.2)
        // Uses setweight() to assign different weights to different fields,
        // then ts_rank with normalization flag 32 (divides by rank + 1) for BM25-like behavior.
        let sql = format!(
            r#"
            SELECT n.id as note_id,
                   ts_rank(
                       setweight(COALESCE(to_tsvector('public.matric_english', n.title), ''::tsvector), 'A') ||
                       setweight(COALESCE((
                           SELECT to_tsvector('public.matric_english', string_agg(tag_name, ' '))
                           FROM note_tag WHERE note_id = n.id
                       ), ''::tsvector), 'B') ||
                       setweight(nrc.tsv, 'C'),
                       websearch_to_tsquery('public.matric_english', $1),
                       32
                   ) AS score,
                   left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE (nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1)
                   OR to_tsvector('public.matric_english', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_english', $1))
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
                    embedding_status: None,
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

        // If filter is unsatisfiable (e.g. any_tags requested but none resolved),
        // return empty results immediately instead of falling back to unfiltered search
        if filter.match_none {
            return Ok(Vec::new());
        }

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

        // Build the query with CTE for filtered notes, using BM25F field-weighted scoring
        let sql = format!(
            r#"
            WITH filtered_notes AS (
                SELECT n.id
                FROM note n
                WHERE {}
                  AND {}
            )
            SELECT n.id as note_id,
                   ts_rank(
                       setweight(COALESCE(to_tsvector('public.matric_english', n.title), ''::tsvector), 'A') ||
                       setweight(COALESCE((
                           SELECT to_tsvector('public.matric_english', string_agg(tag_name, ' '))
                           FROM note_tag WHERE note_id = n.id
                       ), ''::tsvector), 'B') ||
                       setweight(nrc.tsv, 'C'),
                       websearch_to_tsquery('public.matric_english', $1),
                       32
                   ) AS score,
                   left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM filtered_notes fn
            JOIN note n ON n.id = fn.id
            JOIN note_revised_current nrc ON nrc.note_id = n.id
            WHERE (nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1)
                   OR to_tsvector('public.matric_english', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_english', $1))
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
                QueryParam::StringArray(arr) => q.bind(arr),
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
                    embedding_status: None,
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

        // BM25F field-weighted scoring for filtered search
        let mut sql = format!(
            r#"
            SELECT n.id as note_id,
                   ts_rank(
                       setweight(COALESCE(to_tsvector('public.matric_english', n.title), ''::tsvector), 'A') ||
                       setweight(COALESCE((
                           SELECT to_tsvector('public.matric_english', string_agg(tag_name, ' '))
                           FROM note_tag WHERE note_id = n.id
                       ), ''::tsvector), 'B') ||
                       setweight(nrc.tsv, 'C'),
                       websearch_to_tsquery('public.matric_english', $1),
                       32
                   ) AS score,
                   left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE (nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1)
                   OR to_tsvector('public.matric_english', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_english', $1))
              {}
            "#,
            archive_clause
        );

        // Parse and apply filters
        let mut params: Vec<String> = vec![query.to_string()];
        for token in filters.split_whitespace() {
            if let Some(tag) = token.strip_prefix("tag:") {
                params.push(tag.to_string());
                let exact_idx = params.len();
                params.push(escape_like(tag));
                let like_idx = params.len();
                sql.push_str(&format!(
                    " AND n.id IN (SELECT note_id FROM note_tag WHERE LOWER(tag_name) = LOWER(${exact_idx}::text) OR LOWER(tag_name) LIKE LOWER(${like_idx}::text) || '/%' ESCAPE '\\')",
                ));
            } else if let Some(collection) = token.strip_prefix("collection:") {
                if let Ok(uuid) = Uuid::parse_str(collection) {
                    params.push(uuid.to_string());
                    sql.push_str(&format!(" AND n.collection_id = ${}::uuid", params.len()));
                }
            } else if let Some(ts) = token.strip_prefix("created_after:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.created_at_utc >= ${}::timestamptz",
                        params.len()
                    ));
                }
            } else if let Some(ts) = token.strip_prefix("created_before:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.created_at_utc <= ${}::timestamptz",
                        params.len()
                    ));
                }
            } else if let Some(ts) = token.strip_prefix("updated_after:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.updated_at_utc >= ${}::timestamptz",
                        params.len()
                    ));
                }
            } else if let Some(ts) = token.strip_prefix("updated_before:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.updated_at_utc <= ${}::timestamptz",
                        params.len()
                    ));
                }
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
                    embedding_status: None,
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
            WHERE nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1)
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

    // ========================================================================
    // Trigram Search (Phase 2) - pg_trgm based similarity search
    // ========================================================================

    /// Trigram-based search for emoji, symbols, and substring matching.
    ///
    /// Uses pg_trgm extension for:
    /// - Emoji search (🎉, 📝, etc.)
    /// - Substring matching (partial words)
    /// - Fuzzy similarity matching
    ///
    /// This is a fallback when FTS fails (e.g., for CJK, emoji, symbols).
    pub async fn search_trigram(
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

        // Trigram search using similarity() function and ILIKE for exact matches
        // The % operator uses the pg_trgm.similarity_threshold (default 0.3)
        // $1 = raw query (for similarity), $2 = escaped query (for ILIKE), $3 = limit
        let sql = format!(
            r#"
            SELECT DISTINCT ON (n.id)
                   n.id as note_id,
                   GREATEST(
                       similarity(nrc.content, $1),
                       similarity(COALESCE(n.title, ''), $1)
                   ) AS score,
                   left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE (
                nrc.content % $1
                OR nrc.content ILIKE '%' || $2 || '%' ESCAPE '\'
                OR n.title ILIKE '%' || $2 || '%' ESCAPE '\'
            )
            {}
            ORDER BY n.id, score DESC
            "#,
            archive_clause
        );

        // Re-sort by score after DISTINCT ON
        let sql_with_sort = format!("SELECT * FROM ({}) AS t ORDER BY score DESC LIMIT $3", sql);
        let escaped_query = escape_like(query);

        let rows = sqlx::query(&sql_with_sort)
            .bind(query)
            .bind(&escaped_query)
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
                    embedding_status: None,
                }
            })
            .collect();

        Ok(results)
    }

    /// Search using the simple text search configuration (no stemming).
    ///
    /// Uses `matric_simple` configuration which:
    /// - Tokenizes on whitespace and punctuation
    /// - Applies unaccent for Unicode normalization
    /// - Does NOT stem words (preserves exact tokens)
    ///
    /// Suitable for CJK content where word boundaries are explicit.
    pub async fn search_simple(
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
                   ts_rank(
                       to_tsvector('public.matric_simple', COALESCE(n.title, '')) ||
                       to_tsvector('public.matric_simple', nrc.content),
                       websearch_to_tsquery('public.matric_simple', $1)
                   ) AS score,
                   left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE (
                to_tsvector('public.matric_simple', nrc.content) @@ websearch_to_tsquery('public.matric_simple', $1)
                OR to_tsvector('public.matric_simple', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_simple', $1)
            )
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
                    embedding_status: None,
                }
            })
            .collect();

        Ok(results)
    }

    /// Check if pg_trgm extension is available.
    pub async fn has_trigram_extension(&self) -> Result<bool> {
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm') AS has_trgm",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get::<bool, _>("has_trgm"))
    }

    /// Check if pg_bigm extension is available.
    pub async fn has_bigram_extension(&self) -> Result<bool> {
        let row = sqlx::query(
            "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_bigm') AS has_bigm",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get::<bool, _>("has_bigm"))
    }

    /// Bigram-based search optimized for CJK (Chinese, Japanese, Korean).
    ///
    /// Uses pg_bigm extension for 2-gram indexing which is optimal for:
    /// - Single CJK character searches
    /// - Short CJK keyword searches (1-2 characters)
    /// - CJK compound word searches
    ///
    /// Falls back to trigram search if pg_bigm is not available.
    pub async fn search_bigram(
        &self,
        query: &str,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        // Check if bigram extension is available
        if !self.has_bigram_extension().await? {
            // Fallback to trigram search
            return self.search_trigram(query, limit, exclude_archived).await;
        }

        let archive_clause = if exclude_archived {
            "AND (n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "AND n.deleted_at IS NULL"
        };

        // pg_bigm search using likequery() and bigm_similarity()
        let sql = format!(
            r#"
            SELECT DISTINCT ON (n.id)
                   n.id as note_id,
                   GREATEST(
                       bigm_similarity(nrc.content, $1),
                       bigm_similarity(COALESCE(n.title, ''), $1)
                   ) AS score,
                   left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE (
                nrc.content LIKE likequery($1)
                OR n.title LIKE likequery($1)
            )
            {}
            ORDER BY n.id, score DESC
            "#,
            archive_clause
        );

        // Re-sort by score after DISTINCT ON
        let sql_with_sort = format!("SELECT * FROM ({}) AS t ORDER BY score DESC LIMIT $2", sql);

        let rows = sqlx::query(&sql_with_sort)
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
                    embedding_status: None,
                }
            })
            .collect();

        Ok(results)
    }

    /// Search for CJK content using the best available method.
    ///
    /// Automatically selects:
    /// - pg_bigm if available (optimal for CJK)
    /// - pg_trgm as fallback
    pub async fn search_cjk(
        &self,
        query: &str,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        if self.has_bigram_extension().await? {
            self.search_bigram(query, limit, exclude_archived).await
        } else {
            self.search_trigram(query, limit, exclude_archived).await
        }
    }
}

/// Transaction-aware variants for archive-scoped operations (Issue #108).
impl PgFtsSearch {
    /// Perform full-text search within an existing transaction.
    ///
    /// Uses BM25F field-weighted scoring with title/tags/content weights.
    pub async fn search_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
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
                   ts_rank(
                       setweight(COALESCE(to_tsvector('public.matric_english', n.title), ''::tsvector), 'A') ||
                       setweight(COALESCE((
                           SELECT to_tsvector('public.matric_english', string_agg(tag_name, ' '))
                           FROM note_tag WHERE note_id = n.id
                       ), ''::tsvector), 'B') ||
                       setweight(nrc.tsv, 'C'),
                       websearch_to_tsquery('public.matric_english', $1),
                       32
                   ) AS score,
                   left(convert_from(convert_to(nrc.content, 'UTF8'), 'UTF8'), 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(tag_name, ',') FROM note_tag WHERE note_id = n.id),
                       ''
                   ) as tags
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE (nrc.tsv @@ websearch_to_tsquery('public.matric_english', $1)
                   OR to_tsvector('public.matric_english', COALESCE(n.title, '')) @@ websearch_to_tsquery('public.matric_english', $1))
              {}
            ORDER BY score DESC
            LIMIT $2
            "#,
            archive_clause
        );

        let rows = sqlx::query(&sql)
            .bind(query)
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
                    score: row.get::<Option<f32>, _>("score").unwrap_or(0.0),
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
