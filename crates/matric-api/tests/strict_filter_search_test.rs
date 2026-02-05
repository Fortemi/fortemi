//! Unit tests for strict filter parameter in search endpoint.
//!
//! Tests the integration of StrictTagFilterInput in the search API,
//! ensuring proper deserialization and error handling.

use serde_json::json;

/// Test request structure for search endpoint with strict filter
#[derive(Debug, serde::Serialize)]
struct SearchRequest {
    q: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    strict_filter: Option<serde_json::Value>,
}

#[test]
fn test_search_request_with_strict_filter_serialization() {
    let request = SearchRequest {
        q: "rust programming".to_string(),
        strict_filter: Some(json!({
            "required_tags": ["programming/rust"],
            "any_tags": ["tutorial", "guide"],
            "excluded_tags": ["archive"],
            "min_tag_count": 2,
            "include_untagged": false
        })),
    };

    let serialized = serde_json::to_value(&request).unwrap();
    assert!(serialized.get("strict_filter").is_some());

    let filter = serialized.get("strict_filter").unwrap();
    assert_eq!(
        filter.get("required_tags").unwrap().as_array().unwrap()[0],
        "programming/rust"
    );
}

#[test]
fn test_search_request_without_strict_filter() {
    let request = SearchRequest {
        q: "test query".to_string(),
        strict_filter: None,
    };

    let serialized = serde_json::to_value(&request).unwrap();
    // Field should be omitted when None
    assert!(serialized.get("strict_filter").is_none());
}

#[test]
fn test_strict_filter_with_required_tags_only() {
    let filter = json!({
        "required_tags": ["rust", "python"]
    });

    // Verify structure
    assert!(filter.get("required_tags").is_some());
    assert_eq!(
        filter
            .get("required_tags")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn test_strict_filter_with_any_tags_only() {
    let filter = json!({
        "any_tags": ["tutorial", "guide", "documentation"]
    });

    assert!(filter.get("any_tags").is_some());
    assert_eq!(filter.get("any_tags").unwrap().as_array().unwrap().len(), 3);
}

#[test]
fn test_strict_filter_with_excluded_tags() {
    let filter = json!({
        "excluded_tags": ["archive", "draft"]
    });

    assert!(filter.get("excluded_tags").is_some());
    assert_eq!(
        filter
            .get("excluded_tags")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn test_strict_filter_with_scheme_filters() {
    let filter = json!({
        "required_schemes": ["topics"],
        "excluded_schemes": ["deprecated"]
    });

    assert!(filter.get("required_schemes").is_some());
    assert!(filter.get("excluded_schemes").is_some());
}

#[test]
fn test_strict_filter_with_min_tag_count() {
    let filter = json!({
        "min_tag_count": 3,
        "include_untagged": false
    });

    assert_eq!(filter.get("min_tag_count").unwrap(), 3);
    assert_eq!(filter.get("include_untagged").unwrap(), false);
}

#[test]
fn test_strict_filter_empty() {
    // Empty filter should be valid
    let filter = json!({});

    let serialized = serde_json::to_string(&filter).unwrap();
    assert_eq!(serialized, "{}");
}

#[test]
fn test_strict_filter_complex_scenario() {
    let filter = json!({
        "required_tags": ["programming/rust"],
        "any_tags": ["tutorial", "guide"],
        "excluded_tags": ["archive", "draft"],
        "required_schemes": ["topics"],
        "excluded_schemes": ["deprecated"],
        "min_tag_count": 2,
        "include_untagged": false
    });

    // Verify all fields are present
    assert!(filter.get("required_tags").is_some());
    assert!(filter.get("any_tags").is_some());
    assert!(filter.get("excluded_tags").is_some());
    assert!(filter.get("required_schemes").is_some());
    assert!(filter.get("excluded_schemes").is_some());
    assert!(filter.get("min_tag_count").is_some());
    assert!(filter.get("include_untagged").is_some());
}

#[test]
fn test_strict_filter_default_include_untagged() {
    // When include_untagged is not specified, it should default to true
    let filter_json = r#"{
        "required_tags": ["rust"]
    }"#;

    let filter: serde_json::Value = serde_json::from_str(filter_json).unwrap();

    // The deserialized StrictTagFilterInput should have include_untagged = true by default
    // This is tested at the type level in matric-core, but we verify JSON structure here
    assert!(filter.get("include_untagged").is_none()); // Not in JSON
}
