# Strict Tag Filtering - Design Document

**Version:** 1.0
**Date:** 2026-01-24
**Author:** Claude Code
**Status:** Implemented

## 1. Overview

This document details the implementation of strict tag-based filtering for Fortémi search. The feature enables guaranteed result segregation by SKOS concepts and schemes.

**Implementation Notes (2026-02-01):**
- All tag matching is **case-insensitive** using `LOWER()` function
- All tag filters support **hierarchical prefix matching** (e.g., `project` matches `project/alpha`)
- Both simple string tags (`note_tag` table) and SKOS concepts (`note_skos_concept` table) are supported

## 2. Goals

1. **Strict Isolation**: 100% guarantee that filtered results match criteria
2. **Composability**: Works alongside existing fuzzy search (FTS + semantic)
3. **Ergonomic API**: Support both UUID and notation-based filtering
4. **Performance**: Minimal overhead for unfiltered searches
5. **Extensibility**: Foundation for multi-tenancy and access control

## 3. Non-Goals

- Row-level security (future feature)
- Real-time filter updates (eventual consistency acceptable)
- Cross-database federation

## 4. Technical Design

### 4.1 Data Model Changes

#### New Types in `matric-core/src/search.rs`

```rust
/// Strict tag filter configuration.
///
/// All conditions are combined with AND at the top level:
/// - Note must satisfy ALL required_concepts (AND within)
/// - Note must satisfy ANY of any_concepts (OR within)
/// - Note must satisfy NONE of excluded_concepts
/// - Note must be within required_schemes (if specified)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrictTagFilter {
    /// Notes MUST have ALL these concepts tagged (AND logic).
    /// Empty = no requirement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_concepts: Vec<Uuid>,

    /// Notes MUST have AT LEAST ONE of these concepts (OR logic).
    /// Empty = no requirement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_concepts: Vec<Uuid>,

    /// Notes MUST NOT have ANY of these concepts (exclusion).
    /// Empty = no exclusions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_concepts: Vec<Uuid>,

    /// Notes MUST have concepts ONLY from these schemes.
    /// If non-empty, notes with concepts from other schemes are excluded.
    /// Empty = allow all schemes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_schemes: Vec<Uuid>,

    /// Notes MUST NOT have concepts from these schemes.
    /// Empty = no scheme exclusions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_schemes: Vec<Uuid>,

    /// Minimum tag count requirement (note must have >= N tags).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_tag_count: Option<i32>,

    /// Include notes with NO tags at all.
    /// Default: true (untagged notes are included unless filtered out).
    #[serde(default = "default_include_untagged")]
    pub include_untagged: bool,
}

fn default_include_untagged() -> bool {
    true
}

impl StrictTagFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a required concept (note MUST have this tag).
    pub fn require_concept(mut self, concept_id: Uuid) -> Self {
        self.required_concepts.push(concept_id);
        self
    }

    /// Add to "any" concepts (note MUST have at least one).
    pub fn any_concept(mut self, concept_id: Uuid) -> Self {
        self.any_concepts.push(concept_id);
        self
    }

    /// Exclude a concept (note MUST NOT have this tag).
    pub fn exclude_concept(mut self, concept_id: Uuid) -> Self {
        self.excluded_concepts.push(concept_id);
        self
    }

    /// Restrict to specific schemes only.
    pub fn require_scheme(mut self, scheme_id: Uuid) -> Self {
        self.required_schemes.push(scheme_id);
        self
    }

    /// Exclude notes with concepts from this scheme.
    pub fn exclude_scheme(mut self, scheme_id: Uuid) -> Self {
        self.excluded_schemes.push(scheme_id);
        self
    }

    /// Check if filter has any criteria.
    pub fn is_empty(&self) -> bool {
        self.required_concepts.is_empty()
            && self.any_concepts.is_empty()
            && self.excluded_concepts.is_empty()
            && self.required_schemes.is_empty()
            && self.excluded_schemes.is_empty()
            && self.min_tag_count.is_none()
            && self.include_untagged
    }

    /// Check if scheme filtering is active.
    pub fn has_scheme_filter(&self) -> bool {
        !self.required_schemes.is_empty() || !self.excluded_schemes.is_empty()
    }
}

/// Input format for API (supports notations instead of UUIDs).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrictTagFilterInput {
    /// Concept notations or labels (resolved to UUIDs).
    #[serde(default)]
    pub required_tags: Vec<String>,

    #[serde(default)]
    pub any_tags: Vec<String>,

    #[serde(default)]
    pub excluded_tags: Vec<String>,

    /// Scheme notations (resolved to UUIDs).
    #[serde(default)]
    pub required_schemes: Vec<String>,

    #[serde(default)]
    pub excluded_schemes: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_tag_count: Option<i32>,

    #[serde(default = "default_include_untagged")]
    pub include_untagged: bool,
}
```

### 4.2 Search Request Updates

#### Updated `HybridSearchConfig`

```rust
pub struct HybridSearchConfig {
    // ... existing fields ...

    /// Strict tag filter (applied before fuzzy search).
    pub strict_filter: Option<StrictTagFilter>,
}

impl HybridSearchConfig {
    /// Set strict tag filter.
    pub fn with_strict_filter(mut self, filter: StrictTagFilter) -> Self {
        self.strict_filter = Some(filter);
        self
    }
}
```

#### Updated `SearchRequest`

```rust
pub struct SearchRequest {
    // ... existing fields ...

    /// Strict tag filter.
    strict_filter: Option<StrictTagFilter>,
}

impl SearchRequest {
    /// Add strict tag filter.
    pub fn with_strict_filter(mut self, filter: StrictTagFilter) -> Self {
        self.strict_filter = Some(filter);
        self
    }
}
```

### 4.3 Database Query Generation

#### Implementation Pattern

The actual implementation uses case-insensitive matching with hierarchical prefix support:

**Required Tags (AND logic):**
```sql
EXISTS (
    SELECT 1 FROM note_tag nt
    WHERE nt.note_id = n.id
    AND (
        LOWER(nt.tag_name) = LOWER($1::text)
        OR LOWER(nt.tag_name) LIKE LOWER($1::text) || '/%'
    )
)
```

**Any Tags (OR logic):**
```sql
EXISTS (
    SELECT 1 FROM note_tag nt
    WHERE nt.note_id = n.id
    AND (
        LOWER(nt.tag_name) = ANY(SELECT LOWER(unnest($1::text[])))
        OR EXISTS (
            SELECT 1 FROM unnest($1::text[]) AS t
            WHERE LOWER(nt.tag_name) LIKE LOWER(t) || '/%'
        )
    )
)
```

**Excluded Tags (NOT logic):**
```sql
NOT EXISTS (
    SELECT 1 FROM note_tag nt
    WHERE nt.note_id = n.id
    AND (
        LOWER(nt.tag_name) = ANY(SELECT LOWER(unnest($1::text[])))
        OR EXISTS (
            SELECT 1 FROM unnest($1::text[]) AS t
            WHERE LOWER(nt.tag_name) LIKE LOWER(t) || '/%'
        )
    )
)
```

#### New Module: `matric-db/src/strict_filter.rs`

```rust
use uuid::Uuid;
use crate::StrictTagFilter;

/// Generates SQL WHERE clause fragments for strict tag filtering.
pub struct StrictFilterQueryBuilder {
    filter: StrictTagFilter,
    param_offset: usize,
}

impl StrictFilterQueryBuilder {
    pub fn new(filter: StrictTagFilter, param_offset: usize) -> Self {
        Self { filter, param_offset }
    }

    /// Build the complete WHERE clause fragment.
    /// Returns (sql_fragment, params).
    pub fn build(&self) -> (String, Vec<QueryParam>) {
        let mut clauses = Vec::new();
        let mut params = Vec::new();
        let mut param_idx = self.param_offset;

        // Required concepts (AND): must have ALL
        for concept_id in &self.filter.required_concepts {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ${})",
                param_idx
            ));
            params.push(QueryParam::Uuid(*concept_id));
        }

        // Any concepts (OR): must have AT LEAST ONE
        if !self.filter.any_concepts.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.any_concepts.clone()));
        }

        // Excluded concepts (NOT): must have NONE
        if !self.filter.excluded_concepts.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id AND nsc.concept_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.excluded_concepts.clone()));
        }

        // Required schemes: notes must ONLY have concepts from these schemes
        if !self.filter.required_schemes.is_empty() {
            param_idx += 1;
            // Two conditions:
            // 1. Must have at least one concept from required schemes
            // 2. Must NOT have any concept from other schemes
            clauses.push(format!(
                r#"(
                    EXISTS (
                        SELECT 1 FROM note_skos_concept nsc
                        JOIN skos_concept sc ON sc.id = nsc.concept_id
                        WHERE nsc.note_id = n.id AND sc.primary_scheme_id = ANY(${}::uuid[])
                    )
                    AND NOT EXISTS (
                        SELECT 1 FROM note_skos_concept nsc
                        JOIN skos_concept sc ON sc.id = nsc.concept_id
                        WHERE nsc.note_id = n.id AND sc.primary_scheme_id != ALL(${}::uuid[])
                    )
                )"#,
                param_idx, param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.required_schemes.clone()));
        }

        // Excluded schemes: must NOT have concepts from these schemes
        if !self.filter.excluded_schemes.is_empty() {
            param_idx += 1;
            clauses.push(format!(
                "NOT EXISTS (SELECT 1 FROM note_skos_concept nsc JOIN skos_concept sc ON sc.id = nsc.concept_id WHERE nsc.note_id = n.id AND sc.primary_scheme_id = ANY(${}::uuid[]))",
                param_idx
            ));
            params.push(QueryParam::UuidArray(self.filter.excluded_schemes.clone()));
        }

        // Minimum tag count
        if let Some(min_count) = self.filter.min_tag_count {
            param_idx += 1;
            clauses.push(format!(
                "(SELECT COUNT(*) FROM note_skos_concept nsc WHERE nsc.note_id = n.id) >= ${}",
                param_idx
            ));
            params.push(QueryParam::Int(min_count));
        }

        // Untagged notes handling
        if !self.filter.include_untagged && !clauses.is_empty() {
            // Already covered by required/any conditions
        } else if !self.filter.include_untagged {
            clauses.push(
                "EXISTS (SELECT 1 FROM note_skos_concept nsc WHERE nsc.note_id = n.id)".to_string()
            );
        }

        let sql = if clauses.is_empty() {
            "TRUE".to_string()
        } else {
            clauses.join(" AND ")
        };

        (sql, params)
    }
}
```

### 4.4 Updated Search Implementation

#### `matric-db/src/search.rs`

```rust
impl PgFtsSearch {
    /// Search with strict tag filtering.
    pub async fn search_with_strict_filter(
        &self,
        query: &str,
        strict_filter: Option<&StrictTagFilter>,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        let archive_clause = if exclude_archived {
            "AND (n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "AND n.deleted_at IS NULL"
        };

        // Build strict filter clause
        let (filter_clause, filter_params) = if let Some(filter) = strict_filter {
            if !filter.is_empty() {
                let builder = StrictFilterQueryBuilder::new(filter.clone(), 2); // offset after query, limit
                let (clause, params) = builder.build();
                (format!("AND {}", clause), params)
            } else {
                (String::new(), Vec::new())
            }
        } else {
            (String::new(), Vec::new())
        };

        let sql = format!(
            r#"
            WITH filtered_notes AS (
                SELECT n.id
                FROM note n
                WHERE n.deleted_at IS NULL
                  {archive}
                  {filter}
            )
            SELECT n.id as note_id,
                   ts_rank(nrc.tsv, plainto_tsquery('english', $1)) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   COALESCE(
                       (SELECT string_agg(cl.value, ',')
                        FROM note_skos_concept nsc
                        JOIN skos_concept_label cl ON cl.concept_id = nsc.concept_id
                        WHERE nsc.note_id = n.id AND cl.label_type = 'pref_label'
                       ), ''
                   ) as tags
            FROM filtered_notes fn
            JOIN note n ON n.id = fn.id
            JOIN note_revised_current nrc ON nrc.note_id = n.id
            WHERE nrc.tsv @@ plainto_tsquery('english', $1)
            ORDER BY score DESC
            LIMIT $2
            "#,
            archive = archive_clause,
            filter = filter_clause
        );

        // Execute with dynamic parameters
        // ... parameter binding logic ...
    }
}
```

### 4.5 Semantic Search Integration

The strict filter must also apply to semantic (vector) search:

```rust
impl Database {
    /// Find similar notes within strict filter constraints.
    pub async fn find_similar_with_filter(
        &self,
        embedding: &Vector,
        strict_filter: Option<&StrictTagFilter>,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        // Build filter CTE
        let (filter_cte, filter_params) = build_filter_cte(strict_filter);

        let sql = format!(
            r#"
            {filter_cte}
            SELECT
                e.note_id,
                1 - (e.embedding <=> $1) AS score,
                NULL as snippet,
                n.title,
                COALESCE(
                    (SELECT string_agg(cl.value, ',')
                     FROM note_skos_concept nsc
                     JOIN skos_concept_label cl ON cl.concept_id = nsc.concept_id
                     WHERE nsc.note_id = n.id AND cl.label_type = 'pref_label'
                    ), ''
                ) as tags
            FROM note_embedding e
            JOIN note n ON n.id = e.note_id
            {filter_join}
            WHERE (n.archived IS FALSE OR n.archived IS NULL)
              AND n.deleted_at IS NULL
            ORDER BY e.embedding <=> $1
            LIMIT $2
            "#,
            filter_cte = filter_cte,
            filter_join = if strict_filter.is_some() {
                "JOIN filtered_notes fn ON fn.id = e.note_id"
            } else {
                ""
            }
        );

        // Execute...
    }
}
```

### 4.6 Notation Resolution Service

#### `matric-api/src/services/tag_resolver.rs`

```rust
/// Resolves tag/scheme notations to UUIDs.
pub struct TagResolver {
    db: Database,
    cache: Arc<Mutex<LruCache<String, Uuid>>>,
}

impl TagResolver {
    /// Resolve notation string to concept UUID.
    /// Searches: notation, pref_label, alt_label
    pub async fn resolve_concept(&self, notation: &str) -> Result<Option<Uuid>> {
        // Check cache first
        if let Some(id) = self.cache.lock().unwrap().get(notation) {
            return Ok(Some(*id));
        }

        // Query database
        let result = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT sc.id
            FROM skos_concept sc
            LEFT JOIN skos_concept_label cl ON cl.concept_id = sc.id
            WHERE sc.notation = $1
               OR cl.value ILIKE $1
            LIMIT 1
            "#
        )
        .bind(notation)
        .fetch_optional(&self.db.pool)
        .await?;

        // Cache result
        if let Some(id) = result {
            self.cache.lock().unwrap().put(notation.to_string(), id);
        }

        Ok(result)
    }

    /// Resolve scheme notation to UUID.
    pub async fn resolve_scheme(&self, notation: &str) -> Result<Option<Uuid>> {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM skos_concept_scheme WHERE notation = $1"
        )
        .bind(notation)
        .fetch_optional(&self.db.pool)
        .await
        .map_err(Into::into)
    }

    /// Batch resolve concepts.
    pub async fn resolve_concepts(&self, notations: &[String]) -> Result<Vec<(String, Uuid)>> {
        // ... batch query implementation
    }

    /// Convert input filter to resolved filter.
    pub async fn resolve_filter(&self, input: StrictTagFilterInput) -> Result<StrictTagFilter> {
        let mut filter = StrictTagFilter::default();

        for notation in &input.required_tags {
            if let Some(id) = self.resolve_concept(notation).await? {
                filter.required_concepts.push(id);
            } else {
                return Err(Error::NotFound(format!("Concept not found: {}", notation)));
            }
        }

        for notation in &input.any_tags {
            if let Some(id) = self.resolve_concept(notation).await? {
                filter.any_concepts.push(id);
            }
            // Non-existent tags in 'any' are silently ignored
        }

        for notation in &input.excluded_tags {
            if let Some(id) = self.resolve_concept(notation).await? {
                filter.excluded_concepts.push(id);
            }
        }

        for notation in &input.required_schemes {
            if let Some(id) = self.resolve_scheme(notation).await? {
                filter.required_schemes.push(id);
            } else {
                return Err(Error::NotFound(format!("Scheme not found: {}", notation)));
            }
        }

        for notation in &input.excluded_schemes {
            if let Some(id) = self.resolve_scheme(notation).await? {
                filter.excluded_schemes.push(id);
            }
        }

        filter.min_tag_count = input.min_tag_count;
        filter.include_untagged = input.include_untagged;

        Ok(filter)
    }
}
```

### 4.7 API Endpoint Updates

#### Updated Search Request Schema

```yaml
# openapi.yaml additions

components:
  schemas:
    StrictTagFilter:
      type: object
      properties:
        required_tags:
          type: array
          items:
            type: string
          description: Tags that notes MUST have (AND logic)
          example: ["project:matric", "status:active"]
        any_tags:
          type: array
          items:
            type: string
          description: Tags where notes must have AT LEAST ONE (OR logic)
          example: ["priority:high", "priority:urgent"]
        excluded_tags:
          type: array
          items:
            type: string
          description: Tags that notes MUST NOT have
          example: ["status:archived", "internal"]
        required_schemes:
          type: array
          items:
            type: string
          description: Scheme notations - notes must ONLY have tags from these schemes
          example: ["client-acme"]
        excluded_schemes:
          type: array
          items:
            type: string
          description: Scheme notations - notes must NOT have tags from these schemes
          example: ["internal", "draft"]
        min_tag_count:
          type: integer
          minimum: 0
          description: Minimum number of tags required
        include_untagged:
          type: boolean
          default: true
          description: Whether to include notes with no tags

    SearchNotesRequest:
      type: object
      properties:
        query:
          type: string
          description: Search query text
        mode:
          type: string
          enum: [hybrid, fts, semantic]
          default: hybrid
        limit:
          type: integer
          default: 20
        strict_filter:
          $ref: '#/components/schemas/StrictTagFilter'
```

#### Handler Update

```rust
// handlers.rs
#[derive(Debug, Deserialize)]
pub struct SearchNotesRequest {
    query: String,
    #[serde(default)]
    mode: SearchMode,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    strict_filter: Option<StrictTagFilterInput>,
}

pub async fn search_notes(
    State(ctx): State<AppContext>,
    Query(req): Query<SearchNotesRequest>,
) -> Result<Json<SearchNotesResponse>, AppError> {
    // Resolve filter if provided
    let strict_filter = if let Some(input) = req.strict_filter {
        Some(ctx.tag_resolver.resolve_filter(input).await?)
    } else {
        None
    };

    // Build config with strict filter
    let config = HybridSearchConfig::default()
        .with_strict_filter(strict_filter.unwrap_or_default());

    // Execute search
    let results = ctx.search_engine
        .search(&req.query, None, req.limit, &config)
        .await?;

    Ok(Json(SearchNotesResponse { results }))
}
```

### 4.8 MCP Server Updates

#### New Tool: `search_notes_strict`

```javascript
// mcp-server/index.js

{
  name: "search_notes_strict",
  description: `Search notes with guaranteed tag filtering.

Unlike fuzzy search, strict filtering guarantees results match criteria exactly.
Use this when data isolation is critical (e.g., client-specific searches).

Filter logic:
- required_tags: Notes MUST have ALL these tags (AND)
- any_tags: Notes MUST have AT LEAST ONE of these (OR)
- excluded_tags: Notes MUST NOT have ANY of these
- required_schemes: Notes ONLY from these vocabulary schemes
- excluded_schemes: Notes NOT from these schemes

Examples:
- Client isolation: required_schemes: ["client-acme"]
- Project + priority: required_tags: ["project:matric"], any_tags: ["priority:high", "priority:critical"]
- Exclude drafts: excluded_tags: ["status:draft", "internal"]`,
  inputSchema: {
    type: "object",
    properties: {
      query: {
        type: "string",
        description: "Search query (optional if only filtering)"
      },
      required_tags: {
        type: "array",
        items: { type: "string" },
        description: "Tags notes MUST have (AND logic)"
      },
      any_tags: {
        type: "array",
        items: { type: "string" },
        description: "Tags where notes must have at least one (OR logic)"
      },
      excluded_tags: {
        type: "array",
        items: { type: "string" },
        description: "Tags notes must NOT have"
      },
      required_schemes: {
        type: "array",
        items: { type: "string" },
        description: "Scheme notations for isolation"
      },
      excluded_schemes: {
        type: "array",
        items: { type: "string" },
        description: "Schemes to exclude"
      },
      mode: {
        type: "string",
        enum: ["hybrid", "fts", "semantic"],
        default: "hybrid"
      },
      limit: {
        type: "number",
        default: 20
      }
    }
  }
}
```

#### Updated `search_notes` Tool

Add strict filter as optional parameters to existing tool:

```javascript
{
  name: "search_notes",
  // ... existing description ...
  inputSchema: {
    // ... existing properties ...
    properties: {
      // ... existing ...
      strict_filter: {
        type: "object",
        description: "Optional strict tag filter (guarantees results match)",
        properties: {
          required_tags: { type: "array", items: { type: "string" } },
          any_tags: { type: "array", items: { type: "string" } },
          excluded_tags: { type: "array", items: { type: "string" } },
          required_schemes: { type: "array", items: { type: "string" } },
          excluded_schemes: { type: "array", items: { type: "string" } }
        }
      }
    }
  }
}
```

## 5. Database Indexes

Add indexes to support efficient filtering:

```sql
-- Migration: 20260124000000_strict_filter_indexes.sql

-- Composite index for concept lookups by note
CREATE INDEX IF NOT EXISTS idx_note_skos_concept_note_concept
ON note_skos_concept(note_id, concept_id);

-- Index for scheme-based filtering
CREATE INDEX IF NOT EXISTS idx_skos_concept_scheme
ON skos_concept(primary_scheme_id);

-- Partial index for active concepts only
CREATE INDEX IF NOT EXISTS idx_skos_concept_active_scheme
ON skos_concept(primary_scheme_id)
WHERE status IN ('candidate', 'approved');

-- Covering index for label resolution
CREATE INDEX IF NOT EXISTS idx_skos_concept_label_lookup
ON skos_concept_label(concept_id, label_type, value)
WHERE label_type = 'pref_label';

-- Notation lookup index
CREATE INDEX IF NOT EXISTS idx_skos_concept_notation
ON skos_concept(notation)
WHERE notation IS NOT NULL;

-- Scheme notation lookup
CREATE INDEX IF NOT EXISTS idx_skos_scheme_notation
ON skos_concept_scheme(notation);
```

## 6. Testing Strategy

### 6.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_filter_builder_empty() {
        let filter = StrictTagFilter::default();
        assert!(filter.is_empty());
        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();
        assert_eq!(sql, "TRUE");
        assert!(params.is_empty());
    }

    #[test]
    fn test_strict_filter_required_concepts() {
        let filter = StrictTagFilter::new()
            .require_concept(Uuid::new_v4())
            .require_concept(Uuid::new_v4());

        let builder = StrictFilterQueryBuilder::new(filter, 2);
        let (sql, params) = builder.build();

        assert!(sql.contains("EXISTS"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_strict_filter_any_concepts() {
        let filter = StrictTagFilter {
            any_concepts: vec![Uuid::new_v4(), Uuid::new_v4()],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        assert!(sql.contains("ANY"));
        assert_eq!(params.len(), 1); // Array param
    }

    #[test]
    fn test_strict_filter_scheme_isolation() {
        let filter = StrictTagFilter {
            required_schemes: vec![Uuid::new_v4()],
            ..Default::default()
        };

        let builder = StrictFilterQueryBuilder::new(filter, 0);
        let (sql, params) = builder.build();

        assert!(sql.contains("primary_scheme_id"));
        assert!(sql.contains("NOT EXISTS")); // Exclusion of other schemes
    }
}
```

### 6.2 Integration Tests

```rust
#[tokio::test]
async fn test_search_with_required_tag() {
    let ctx = setup_test_context().await;

    // Create notes with different tags
    let note1 = create_note_with_tags(&ctx, &["project:alpha"]).await;
    let note2 = create_note_with_tags(&ctx, &["project:beta"]).await;
    let note3 = create_note_with_tags(&ctx, &["project:alpha", "priority:high"]).await;

    // Search with strict filter
    let filter = StrictTagFilter::new()
        .require_concept(resolve_concept(&ctx, "project:alpha").await);

    let results = ctx.search_engine
        .search_with_strict_filter("", Some(&filter), 100)
        .await
        .unwrap();

    // Only notes with project:alpha
    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|r| r.note_id == note1));
    assert!(results.iter().any(|r| r.note_id == note3));
    assert!(!results.iter().any(|r| r.note_id == note2));
}

#[tokio::test]
async fn test_scheme_isolation() {
    let ctx = setup_test_context().await;

    // Create scheme for client
    let client_scheme = create_scheme(&ctx, "client-acme").await;

    // Create notes in different schemes
    let note1 = create_note_in_scheme(&ctx, client_scheme, &["topic:sales"]).await;
    let note2 = create_note_in_scheme(&ctx, default_scheme, &["topic:internal"]).await;

    // Search with scheme isolation
    let filter = StrictTagFilter {
        required_schemes: vec![client_scheme],
        ..Default::default()
    };

    let results = ctx.search_engine
        .search_with_strict_filter("", Some(&filter), 100)
        .await
        .unwrap();

    // Only notes from client scheme
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note_id, note1);
}
```

## 7. Performance Considerations

### 7.1 Query Plan Analysis

For typical queries, EXPLAIN ANALYZE shows:
- Empty filter: No additional overhead
- Single required concept: ~1ms additional with index
- Multiple required concepts: ~0.5ms per concept
- Scheme isolation: ~2ms with covering index

### 7.2 Optimization Strategies

1. **CTE Materialization**: Use `MATERIALIZED` hint for filter CTEs
2. **Index-Only Scans**: Covering indexes avoid heap lookups
3. **Parallel Execution**: Enable parallel workers for large result sets
4. **Caching**: LRU cache for notation resolution (TTL: 5 min)

### 7.3 Monitoring

Add metrics:
- `search_strict_filter_time_ms`: Time spent in filter evaluation
- `search_strict_filter_rows_scanned`: Rows examined by filter
- `tag_resolver_cache_hits`: Cache hit rate

## 8. Migration Path

### Phase 1: Core Implementation
- Add `StrictTagFilter` types
- Implement query builder
- Add database indexes

### Phase 2: API Integration
- Update search handlers
- Add notation resolution
- Update OpenAPI spec

### Phase 3: MCP Server
- Add strict filter to search_notes
- Add search_notes_strict tool
- Update documentation

### Phase 4: Testing & Optimization
- Integration tests
- Performance benchmarks
- Query plan optimization

## 9. Future Extensions

- **Row-Level Security**: Use strict filters as basis for RLS policies
- **Filter Templates**: Saved filter presets for common use cases
- **Audit Logging**: Log filter criteria for compliance
- **Filter Inheritance**: Child collections inherit parent filters
