//! Service for resolving tag notations to SKOS concept UUIDs.
//!
//! This service provides caching and resolution of tag notation strings
//! (e.g., "programming/rust") to SKOS concept UUIDs for use in filtering
//! and search operations.
//!
//! ## Resolution Order
//!
//! For concept notations, the resolver tries:
//! 1. Cache lookup
//! 2. Exact notation match on `skos_concept.notation`
//! 3. Case-insensitive ILIKE match on `pref_label`
//! 4. Case-insensitive ILIKE match on `alt_label`
//! 5. Cache successful result
//!
//! ## Error Handling
//!
//! - Required tags: Error if not found
//! - Any tags: Silently skip if not found
//! - Excluded tags: Silently skip if not found
//! - Required schemes: Error if not found
//! - Excluded schemes: Silently skip if not found

use lru::LruCache;
use matric_core::{Error, Result, StrictTagFilter, StrictTagFilterInput};
use matric_db::Database;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Service for resolving tag notations to SKOS concept UUIDs.
///
/// Provides caching and database lookups for efficient tag resolution.
#[derive(Clone)]
pub struct TagResolver {
    db: Database,
    cache: Arc<Mutex<LruCache<String, Uuid>>>,
}

impl TagResolver {
    /// Create a new TagResolver with a 1000-entry LRU cache.
    pub fn new(db: Database) -> Self {
        let cache_size = NonZeroUsize::new(1000).expect("Cache size must be non-zero");
        Self {
            db,
            cache: Arc::new(Mutex::new(LruCache::new(cache_size))),
        }
    }

    /// Resolve a concept notation to a UUID.
    ///
    /// Returns `Ok(None)` if the concept is not found.
    ///
    /// # Resolution Order
    ///
    /// 1. Check cache
    /// 2. Query DB: exact notation match
    /// 3. Query DB: ILIKE match on pref_label
    /// 4. Query DB: ILIKE match on alt_label
    /// 5. Cache successful result
    pub async fn resolve_concept(&self, notation: &str) -> Result<Option<Uuid>> {
        // Check cache first
        {
            let mut cache = self.cache.lock().await;
            if let Some(&uuid) = cache.get(notation) {
                return Ok(Some(uuid));
            }
        }

        // Try exact notation match
        let concept_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM skos_concept
            WHERE notation = $1
            LIMIT 1
            "#,
        )
        .bind(notation)
        .fetch_optional(&self.db.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(id) = concept_id {
            // Cache and return
            let mut cache = self.cache.lock().await;
            cache.put(notation.to_string(), id);
            return Ok(Some(id));
        }

        // Try ILIKE match on pref_label
        let concept_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT c.id
            FROM skos_concept c
            JOIN skos_concept_label l ON l.concept_id = c.id
            WHERE l.label_type = 'pref_label'
              AND l.value ILIKE $1
            LIMIT 1
            "#,
        )
        .bind(notation)
        .fetch_optional(&self.db.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(id) = concept_id {
            // Cache and return
            let mut cache = self.cache.lock().await;
            cache.put(notation.to_string(), id);
            return Ok(Some(id));
        }

        // Try ILIKE match on alt_label
        let concept_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT c.id
            FROM skos_concept c
            JOIN skos_concept_label l ON l.concept_id = c.id
            WHERE l.label_type = 'alt_label'
              AND l.value ILIKE $1
            LIMIT 1
            "#,
        )
        .bind(notation)
        .fetch_optional(&self.db.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(id) = concept_id {
            // Cache and return
            let mut cache = self.cache.lock().await;
            cache.put(notation.to_string(), id);
            return Ok(Some(id));
        }

        // Not found
        Ok(None)
    }

    /// Check if a simple string tag exists in the tag table.
    ///
    /// This checks the legacy tag system (tag table), not SKOS concepts.
    /// Returns `Ok(true)` if the tag exists.
    pub async fn simple_tag_exists(&self, tag_name: &str) -> Result<bool> {
        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM tag WHERE name = $1)")
                .bind(tag_name)
                .fetch_one(&self.db.pool)
                .await
                .map_err(Error::Database)?;
        Ok(exists)
    }

    /// Resolve a scheme notation to a UUID.
    ///
    /// Returns `Ok(None)` if the scheme is not found.
    pub async fn resolve_scheme(&self, notation: &str) -> Result<Option<Uuid>> {
        let scheme_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM skos_concept_scheme
            WHERE notation = $1
            LIMIT 1
            "#,
        )
        .bind(notation)
        .fetch_optional(&self.db.pool)
        .await
        .map_err(Error::Database)?;

        Ok(scheme_id)
    }

    /// Resolve multiple concept notations to UUIDs.
    ///
    /// Returns a vector of (notation, uuid) pairs for successfully resolved concepts.
    /// Silently skips notations that cannot be resolved.
    pub async fn resolve_concepts(&self, notations: &[String]) -> Result<Vec<(String, Uuid)>> {
        let mut results = Vec::new();

        for notation in notations {
            if let Some(uuid) = self.resolve_concept(notation).await? {
                results.push((notation.clone(), uuid));
            }
        }

        Ok(results)
    }

    /// Resolve a StrictTagFilterInput to a StrictTagFilter with UUIDs.
    ///
    /// # Error Handling
    ///
    /// - Required tags: Try SKOS concept first, fall back to simple string tag
    /// - Any tags: Silently skip if not found in either system
    /// - Excluded tags: Silently skip if not found in either system
    /// - Required schemes: Error if not found
    /// - Excluded schemes: Silently skip if not found
    pub async fn resolve_filter(&self, input: StrictTagFilterInput) -> Result<StrictTagFilter> {
        let mut filter = StrictTagFilter::new();

        // Resolve required tags: try SKOS concept first, fall back to simple tag
        for notation in &input.required_tags {
            match self.resolve_concept(notation).await? {
                Some(uuid) => filter.required_concepts.push(uuid),
                None => {
                    // Check if it exists as a simple string tag
                    if self.simple_tag_exists(notation).await? {
                        filter.required_string_tags.push(notation.clone());
                    } else {
                        return Err(Error::NotFound(format!(
                            "Required tag '{}' not found (checked SKOS concepts and simple tags)",
                            notation
                        )));
                    }
                }
            }
        }

        // Resolve any tags: try SKOS concept first, fall back to simple tag
        for notation in &input.any_tags {
            match self.resolve_concept(notation).await? {
                Some(uuid) => filter.any_concepts.push(uuid),
                None => {
                    // Check if it exists as a simple string tag
                    if self.simple_tag_exists(notation).await? {
                        filter.any_string_tags.push(notation.clone());
                    }
                    // Skip if not found — but we track below whether nothing resolved
                }
            }
        }

        // If user requested any_tags but NONE resolved, the filter is unsatisfiable:
        // "give me notes with at least one of these tags" when none of them exist
        // means no notes can match. Without this check, the filter becomes empty
        // and falls back to returning ALL results (issue #182/#184).
        if !input.any_tags.is_empty()
            && filter.any_concepts.is_empty()
            && filter.any_string_tags.is_empty()
        {
            filter.match_none = true;
        }

        // Resolve excluded tags: try SKOS concept first, fall back to simple tag
        for notation in &input.excluded_tags {
            match self.resolve_concept(notation).await? {
                Some(uuid) => filter.excluded_concepts.push(uuid),
                None => {
                    // Check if it exists as a simple string tag
                    if self.simple_tag_exists(notation).await? {
                        filter.excluded_string_tags.push(notation.clone());
                    }
                    // Silently skip if not found in either system
                }
            }
        }

        // Resolve required schemes (must all be found)
        for notation in &input.required_schemes {
            match self.resolve_scheme(notation).await? {
                Some(uuid) => filter.required_schemes.push(uuid),
                None => {
                    return Err(Error::NotFound(format!(
                        "Required scheme '{}' not found",
                        notation
                    )))
                }
            }
        }

        // Resolve excluded schemes (skip if not found)
        for notation in &input.excluded_schemes {
            if let Some(uuid) = self.resolve_scheme(notation).await? {
                filter.excluded_schemes.push(uuid);
            }
        }

        // Copy over non-notation fields
        filter.min_tag_count = input.min_tag_count;
        filter.include_untagged = input.include_untagged;

        Ok(filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::{
        new_v7, AddLabelRequest, CreateConceptRequest, CreateConceptSchemeRequest, SkosLabelType,
        TagStatus,
    };
    use matric_db::skos_tags::{
        SkosConceptRepository, SkosConceptSchemeRepository, SkosLabelRepository,
    };

    async fn setup_test_db() -> Database {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
        Database::connect(&database_url)
            .await
            .expect("Failed to connect to test database")
    }

    /// Generate a unique test ID suffix to avoid collisions between test runs.
    /// Uses 16 hex chars from UUIDv7 (including random bits) to ensure uniqueness
    /// even for tests running within the same millisecond.
    fn unique_suffix() -> String {
        // Use full UUID (without hyphens) to guarantee uniqueness even for parallel tests
        // UUIDv7's first 12 hex chars are timestamp (ms resolution), so parallel tests
        // in same millisecond need the full 32 chars for the random portion
        new_v7().to_string().replace('-', "")
    }

    async fn create_test_scheme(db: &Database, base_notation: &str) -> (Uuid, String) {
        let notation = format!("{}-{}", base_notation, unique_suffix());
        let id = db
            .skos
            .create_scheme(CreateConceptSchemeRequest {
                notation: notation.clone(),
                title: format!("{} Scheme", notation),
                uri: None,
                description: None,
                creator: None,
                publisher: None,
                rights: None,
                version: None,
            })
            .await
            .expect("Failed to create test scheme");
        (id, notation)
    }

    async fn create_test_concept(
        db: &Database,
        scheme_id: Uuid,
        base_notation: &str,
        pref_label: &str,
    ) -> (Uuid, String) {
        let notation = format!("{}-{}", base_notation, unique_suffix());
        let id = db
            .skos
            .create_concept(CreateConceptRequest {
                scheme_id,
                notation: Some(notation.clone()),
                pref_label: pref_label.to_string(),
                language: "en".to_string(),
                status: TagStatus::Approved,
                facet_type: None,
                facet_source: None,
                facet_domain: None,
                facet_scope: None,
                definition: None,
                scope_note: None,
                broader_ids: vec![],
                related_ids: vec![],
                alt_labels: vec![],
            })
            .await
            .expect("Failed to create test concept");
        (id, notation)
    }

    #[tokio::test]
    async fn test_resolve_concept_by_notation() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and concept
        let (scheme_id, _) = create_test_scheme(&db, "test-resolve-notation").await;
        let (concept_id, notation) =
            create_test_concept(&db, scheme_id, "rust", "Rust Programming").await;

        // Resolve by notation
        let resolved = resolver
            .resolve_concept(&notation)
            .await
            .expect("Failed to resolve concept");

        assert_eq!(resolved, Some(concept_id));

        // Verify it's cached (second call should hit cache)
        let resolved_cached = resolver
            .resolve_concept(&notation)
            .await
            .expect("Failed to resolve cached concept");

        assert_eq!(resolved_cached, Some(concept_id));
    }

    #[tokio::test]
    async fn test_resolve_concept_by_pref_label() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and concept with unique pref_label
        let (scheme_id, _) = create_test_scheme(&db, "test-resolve-pref-label").await;
        let unique_label = format!("Machine Learning {}", unique_suffix());
        let (concept_id, _) =
            create_test_concept(&db, scheme_id, "ml-concept", &unique_label).await;

        // Resolve by pref_label (case-insensitive)
        let resolved = resolver
            .resolve_concept(&unique_label)
            .await
            .expect("Failed to resolve concept");

        assert_eq!(resolved, Some(concept_id));

        // Try different case
        let resolved_upper = resolver
            .resolve_concept(&unique_label.to_uppercase())
            .await
            .expect("Failed to resolve concept with different case");

        assert_eq!(resolved_upper, Some(concept_id));
    }

    #[tokio::test]
    async fn test_resolve_concept_by_alt_label() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and concept
        let (scheme_id, _) = create_test_scheme(&db, "test-resolve-alt-label").await;
        let (concept_id, _) = create_test_concept(&db, scheme_id, "ml", "Machine Learning").await;

        // Add alt label with unique value
        let alt_label = format!("ML-ALT-{}", unique_suffix());
        db.skos
            .add_label(AddLabelRequest {
                concept_id,
                label_type: SkosLabelType::AltLabel,
                value: alt_label.clone(),
                language: "en".to_string(),
            })
            .await
            .expect("Failed to add alt label");

        // Resolve by alt_label (case-insensitive)
        let resolved = resolver
            .resolve_concept(&alt_label)
            .await
            .expect("Failed to resolve concept");

        assert_eq!(resolved, Some(concept_id));
    }

    #[tokio::test]
    async fn test_resolve_concept_not_found() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        let resolved = resolver
            .resolve_concept("nonexistent-tag-xyz")
            .await
            .expect("Failed to resolve concept");

        assert_eq!(resolved, None);
    }

    #[tokio::test]
    async fn test_resolve_scheme() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme
        let (scheme_id, notation) = create_test_scheme(&db, "test-scheme-resolve").await;

        // Resolve by notation
        let resolved = resolver
            .resolve_scheme(&notation)
            .await
            .expect("Failed to resolve scheme");

        assert_eq!(resolved, Some(scheme_id));
    }

    #[tokio::test]
    async fn test_resolve_scheme_not_found() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        let resolved = resolver
            .resolve_scheme("nonexistent-scheme")
            .await
            .expect("Failed to resolve scheme");

        assert_eq!(resolved, None);
    }

    #[tokio::test]
    async fn test_resolve_concepts_batch() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and multiple concepts
        let (scheme_id, _) = create_test_scheme(&db, "test-batch-resolve").await;
        let (rust_id, rust_notation) = create_test_concept(&db, scheme_id, "rust", "Rust").await;
        let (python_id, python_notation) =
            create_test_concept(&db, scheme_id, "python", "Python").await;

        // Resolve multiple concepts
        let notations = vec![
            rust_notation.clone(),
            python_notation.clone(),
            "go-nonexistent".to_string(),
        ];
        let resolved = resolver
            .resolve_concepts(&notations)
            .await
            .expect("Failed to resolve concepts");

        // Should resolve rust and python, skip go
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains(&(rust_notation, rust_id)));
        assert!(resolved.contains(&(python_notation, python_id)));
    }

    #[tokio::test]
    async fn test_resolve_filter_required_tags_success() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and concept
        let (scheme_id, _) = create_test_scheme(&db, "test-filter-required").await;
        let (rust_id, rust_notation) = create_test_concept(&db, scheme_id, "rust", "Rust").await;

        let input = StrictTagFilterInput {
            required_tags: vec![rust_notation],
            any_tags: vec![],
            excluded_tags: vec![],
            required_schemes: vec![],
            excluded_schemes: vec![],
            min_tag_count: None,
            include_untagged: true,
        };

        let filter = resolver
            .resolve_filter(input)
            .await
            .expect("Failed to resolve filter");

        assert_eq!(filter.required_concepts, vec![rust_id]);
    }

    #[tokio::test]
    async fn test_resolve_filter_required_tags_not_found() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        let input = StrictTagFilterInput {
            required_tags: vec!["nonexistent-tag".to_string()],
            any_tags: vec![],
            excluded_tags: vec![],
            required_schemes: vec![],
            excluded_schemes: vec![],
            min_tag_count: None,
            include_untagged: true,
        };

        let result = resolver.resolve_filter(input).await;

        assert!(result.is_err());
        match result {
            Err(Error::NotFound(msg)) => {
                assert!(msg.contains("Required tag 'nonexistent-tag' not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_resolve_filter_any_tags_skip_not_found() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and concept
        let (scheme_id, _) = create_test_scheme(&db, "test-filter-any").await;
        let (rust_id, rust_notation) = create_test_concept(&db, scheme_id, "rust", "Rust").await;

        let input = StrictTagFilterInput {
            required_tags: vec![],
            any_tags: vec![
                rust_notation,
                "python-nonexistent".to_string(),
                "go-nonexistent".to_string(),
            ],
            excluded_tags: vec![],
            required_schemes: vec![],
            excluded_schemes: vec![],
            min_tag_count: None,
            include_untagged: true,
        };

        let filter = resolver
            .resolve_filter(input)
            .await
            .expect("Failed to resolve filter");

        // Only rust should be resolved, python and go are skipped
        assert_eq!(filter.any_concepts.len(), 1);
        assert!(filter.any_concepts.contains(&rust_id));
    }

    #[tokio::test]
    async fn test_resolve_filter_excluded_tags_skip_not_found() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and concept
        let (scheme_id, _) = create_test_scheme(&db, "test-filter-excluded").await;
        let (archive_id, archive_notation) =
            create_test_concept(&db, scheme_id, "archive", "Archive").await;

        let input = StrictTagFilterInput {
            required_tags: vec![],
            any_tags: vec![],
            excluded_tags: vec![archive_notation, "draft-nonexistent".to_string()],
            required_schemes: vec![],
            excluded_schemes: vec![],
            min_tag_count: None,
            include_untagged: true,
        };

        let filter = resolver
            .resolve_filter(input)
            .await
            .expect("Failed to resolve filter");

        // Only archive should be resolved, draft is skipped
        assert_eq!(filter.excluded_concepts.len(), 1);
        assert!(filter.excluded_concepts.contains(&archive_id));
    }

    #[tokio::test]
    async fn test_resolve_filter_required_schemes_success() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme
        let (scheme_id, scheme_notation) = create_test_scheme(&db, "topics").await;

        let input = StrictTagFilterInput {
            required_tags: vec![],
            any_tags: vec![],
            excluded_tags: vec![],
            required_schemes: vec![scheme_notation],
            excluded_schemes: vec![],
            min_tag_count: None,
            include_untagged: true,
        };

        let filter = resolver
            .resolve_filter(input)
            .await
            .expect("Failed to resolve filter");

        assert_eq!(filter.required_schemes, vec![scheme_id]);
    }

    #[tokio::test]
    async fn test_resolve_filter_required_schemes_not_found() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        let input = StrictTagFilterInput {
            required_tags: vec![],
            any_tags: vec![],
            excluded_tags: vec![],
            required_schemes: vec!["nonexistent-scheme".to_string()],
            excluded_schemes: vec![],
            min_tag_count: None,
            include_untagged: true,
        };

        let result = resolver.resolve_filter(input).await;

        assert!(result.is_err());
        match result {
            Err(Error::NotFound(msg)) => {
                assert!(msg.contains("Required scheme 'nonexistent-scheme' not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_resolve_filter_excluded_schemes_skip_not_found() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme
        let (scheme_id, scheme_notation) = create_test_scheme(&db, "test-excluded-scheme").await;

        let input = StrictTagFilterInput {
            required_tags: vec![],
            any_tags: vec![],
            excluded_tags: vec![],
            required_schemes: vec![],
            excluded_schemes: vec![scheme_notation, "nonexistent-scheme".to_string()],
            min_tag_count: None,
            include_untagged: true,
        };

        let filter = resolver
            .resolve_filter(input)
            .await
            .expect("Failed to resolve filter");

        // Only test-excluded-scheme should be resolved, nonexistent is skipped
        assert_eq!(filter.excluded_schemes.len(), 1);
        assert!(filter.excluded_schemes.contains(&scheme_id));
    }

    #[tokio::test]
    async fn test_resolve_filter_preserves_other_fields() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        let input = StrictTagFilterInput {
            required_tags: vec![],
            any_tags: vec![],
            excluded_tags: vec![],
            required_schemes: vec![],
            excluded_schemes: vec![],
            min_tag_count: Some(3),
            include_untagged: false,
        };

        let filter = resolver
            .resolve_filter(input)
            .await
            .expect("Failed to resolve filter");

        assert_eq!(filter.min_tag_count, Some(3));
        assert!(!filter.include_untagged);
    }

    #[tokio::test]
    async fn test_cache_behavior() {
        let db = setup_test_db().await;
        let resolver = TagResolver::new(db.clone());

        // Create a test scheme and concept
        let (scheme_id, _) = create_test_scheme(&db, "test-cache").await;
        let (concept_id, notation) =
            create_test_concept(&db, scheme_id, "cached-tag", "Cached Tag").await;

        // First resolution - should hit DB and cache
        let resolved1 = resolver
            .resolve_concept(&notation)
            .await
            .expect("Failed to resolve concept");
        assert_eq!(resolved1, Some(concept_id));

        // Second resolution - should hit cache
        let resolved2 = resolver
            .resolve_concept(&notation)
            .await
            .expect("Failed to resolve cached concept");
        assert_eq!(resolved2, Some(concept_id));

        // Both should return the same result
        assert_eq!(resolved1, resolved2);
    }
}
