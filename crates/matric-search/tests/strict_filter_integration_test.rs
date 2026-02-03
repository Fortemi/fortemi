//! Integration tests for StrictTagFilter in hybrid search.
//!
//! These tests verify that StrictTagFilter is properly integrated into the
//! hybrid search system and works correctly with both FTS and semantic search.

use matric_core::StrictTagFilter;
use matric_search::{HybridSearchConfig, SearchRequest};
use uuid::Uuid;

#[test]
fn test_hybrid_search_config_with_strict_filter() {
    let filter = StrictTagFilter::new()
        .require_concept(Uuid::new_v4())
        .any_concept(Uuid::new_v4())
        .exclude_concept(Uuid::new_v4());

    let config = HybridSearchConfig::default().with_strict_filter(filter.clone());

    assert!(config.strict_filter.is_some());
    let stored_filter = config.strict_filter.unwrap();
    assert_eq!(stored_filter.required_concepts.len(), 1);
    assert_eq!(stored_filter.any_concepts.len(), 1);
    assert_eq!(stored_filter.excluded_concepts.len(), 1);
}

#[test]
fn test_hybrid_search_config_default_no_filter() {
    let config = HybridSearchConfig::default();
    assert!(config.strict_filter.is_none());
}

#[test]
fn test_search_request_with_strict_filter() {
    let filter = StrictTagFilter::new()
        .require_concept(Uuid::new_v4())
        .with_min_tag_count(2)
        .with_include_untagged(false);

    let request = SearchRequest::new("test query").with_strict_filter(filter.clone());

    // Verify filter is stored in the request's config
    assert!(request.config.strict_filter.is_some());
    let stored_filter = request.config.strict_filter.unwrap();
    assert_eq!(stored_filter.required_concepts.len(), 1);
    assert_eq!(stored_filter.min_tag_count, Some(2));
    assert!(!stored_filter.include_untagged);
}

#[test]
fn test_search_request_builder_chaining_with_strict_filter() {
    let filter = StrictTagFilter::new()
        .require_concept(Uuid::new_v4())
        .exclude_concept(Uuid::new_v4());

    let set_id = Uuid::new_v4();
    let request = SearchRequest::new("rust programming")
        .with_limit(50)
        .with_filters("tag:tutorial")
        .with_strict_filter(filter.clone())
        .with_embedding_set(set_id);

    // Verify all settings are stored correctly
    assert!(request.config.strict_filter.is_some());
    assert_eq!(request.config.embedding_set_id, Some(set_id));
}

#[test]
fn test_config_builder_chaining_with_strict_filter() {
    let filter = StrictTagFilter::new()
        .require_concept(Uuid::new_v4())
        .any_concept(Uuid::new_v4());

    let set_id = Uuid::new_v4();
    let config = HybridSearchConfig::default()
        .with_min_score(0.5)
        .with_exclude_archived(false)
        .with_embedding_set(set_id)
        .with_strict_filter(filter.clone());

    assert_eq!(config.min_score, 0.5);
    assert!(!config.exclude_archived);
    assert_eq!(config.embedding_set_id, Some(set_id));
    assert!(config.strict_filter.is_some());
}

#[test]
fn test_strict_filter_empty_by_default() {
    let config = HybridSearchConfig::default();
    assert!(config.strict_filter.is_none());
}

#[test]
fn test_strict_filter_complex_scenario() {
    // Simulate a complex filter: notes tagged with "rust" AND ("tutorial" OR "guide")
    // but NOT "archive", only from "topics" scheme, minimum 2 tags
    let rust_id = Uuid::new_v4();
    let tutorial_id = Uuid::new_v4();
    let guide_id = Uuid::new_v4();
    let archive_id = Uuid::new_v4();
    let topics_scheme_id = Uuid::new_v4();

    let filter = StrictTagFilter::new()
        .require_concept(rust_id)
        .any_concept(tutorial_id)
        .any_concept(guide_id)
        .exclude_concept(archive_id)
        .require_scheme(topics_scheme_id)
        .with_min_tag_count(2)
        .with_include_untagged(false);

    let config = HybridSearchConfig::with_weights(0.6, 0.4).with_strict_filter(filter.clone());

    assert!(config.strict_filter.is_some());
    let stored_filter = config.strict_filter.unwrap();
    assert_eq!(stored_filter.required_concepts.len(), 1);
    assert_eq!(stored_filter.any_concepts.len(), 2);
    assert_eq!(stored_filter.excluded_concepts.len(), 1);
    assert_eq!(stored_filter.required_schemes.len(), 1);
    assert_eq!(stored_filter.min_tag_count, Some(2));
    assert!(!stored_filter.include_untagged);
    assert_eq!(config.fts_weight, 0.6);
    assert_eq!(config.semantic_weight, 0.4);
}

#[test]
fn test_strict_filter_scheme_isolation() {
    let scheme1 = Uuid::new_v4();
    let scheme2 = Uuid::new_v4();

    let filter = StrictTagFilter::new()
        .require_scheme(scheme1)
        .exclude_scheme(scheme2);

    let config = HybridSearchConfig::default().with_strict_filter(filter);

    assert!(config.strict_filter.is_some());
    assert!(config.strict_filter.as_ref().unwrap().has_scheme_filter());
}

#[test]
fn test_strict_filter_only_required_concepts() {
    let concept1 = Uuid::new_v4();
    let concept2 = Uuid::new_v4();

    let filter = StrictTagFilter::new()
        .require_concept(concept1)
        .require_concept(concept2);

    let config = HybridSearchConfig::default().with_strict_filter(filter.clone());

    assert!(config.strict_filter.is_some());
    let stored_filter = config.strict_filter.unwrap();
    assert_eq!(stored_filter.required_concepts.len(), 2);
    assert!(stored_filter.any_concepts.is_empty());
    assert!(stored_filter.excluded_concepts.is_empty());
}

#[test]
fn test_strict_filter_only_any_concepts() {
    let concept1 = Uuid::new_v4();
    let concept2 = Uuid::new_v4();
    let concept3 = Uuid::new_v4();

    let filter = StrictTagFilter::new()
        .any_concept(concept1)
        .any_concept(concept2)
        .any_concept(concept3);

    let config = HybridSearchConfig::default().with_strict_filter(filter.clone());

    assert!(config.strict_filter.is_some());
    let stored_filter = config.strict_filter.unwrap();
    assert_eq!(stored_filter.any_concepts.len(), 3);
    assert!(stored_filter.required_concepts.is_empty());
    assert!(stored_filter.excluded_concepts.is_empty());
}

#[test]
fn test_strict_filter_only_exclusions() {
    let concept1 = Uuid::new_v4();
    let concept2 = Uuid::new_v4();

    let filter = StrictTagFilter::new()
        .exclude_concept(concept1)
        .exclude_concept(concept2);

    let config = HybridSearchConfig::default().with_strict_filter(filter.clone());

    assert!(config.strict_filter.is_some());
    let stored_filter = config.strict_filter.unwrap();
    assert_eq!(stored_filter.excluded_concepts.len(), 2);
    assert!(stored_filter.required_concepts.is_empty());
    assert!(stored_filter.any_concepts.is_empty());
}

#[test]
fn test_strict_filter_min_tag_count() {
    let filter = StrictTagFilter::new().with_min_tag_count(5);

    let config = HybridSearchConfig::default().with_strict_filter(filter.clone());

    assert!(config.strict_filter.is_some());
    let stored_filter = config.strict_filter.unwrap();
    assert_eq!(stored_filter.min_tag_count, Some(5));
}

#[test]
fn test_strict_filter_exclude_untagged() {
    let filter = StrictTagFilter::new().with_include_untagged(false);

    let config = HybridSearchConfig::default().with_strict_filter(filter.clone());

    assert!(config.strict_filter.is_some());
    let stored_filter = config.strict_filter.unwrap();
    assert!(!stored_filter.include_untagged);
}

#[test]
fn test_request_chaining_all_strict_filter_options() {
    let required = Uuid::new_v4();
    let any = Uuid::new_v4();
    let excluded = Uuid::new_v4();
    let scheme = Uuid::new_v4();

    let filter = StrictTagFilter::new()
        .require_concept(required)
        .any_concept(any)
        .exclude_concept(excluded)
        .require_scheme(scheme)
        .with_min_tag_count(3)
        .with_include_untagged(false);

    let request = SearchRequest::new("test query")
        .fts_only()
        .with_strict_filter(filter.clone())
        .with_limit(100);

    assert!(request.config.strict_filter.is_some());
    let stored_filter = request.config.strict_filter.unwrap();
    assert_eq!(stored_filter.required_concepts.len(), 1);
    assert_eq!(stored_filter.any_concepts.len(), 1);
    assert_eq!(stored_filter.excluded_concepts.len(), 1);
    assert_eq!(stored_filter.required_schemes.len(), 1);
    assert_eq!(stored_filter.min_tag_count, Some(3));
    assert!(!stored_filter.include_untagged);
    assert_eq!(request.config.fts_weight, 1.0);
    assert_eq!(request.config.semantic_weight, 0.0);
}
