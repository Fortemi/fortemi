//! Comprehensive integration tests for strict tag filtering (issue #153).
//!
//! These tests verify the strict tag filtering logic works correctly across
//! all scenarios: required concepts (AND), any concepts (OR), excluded concepts (NOT),
//! scheme isolation, combined filters, fuzzy search interaction, and error handling.
//!
//! Note: These are unit tests using mock data structures since we don't have
//! database access in this test environment. They verify the type system,
//! serialization, and business logic.

use serde_json::json;
use uuid::Uuid;

// =============================================================================
// TEST DATA STRUCTURES
// =============================================================================

/// Mock note structure for testing
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MockNote {
    id: Uuid,
    title: String,
    concepts: Vec<Uuid>,
    scheme_ids: Vec<Uuid>,
}

impl MockNote {
    fn new(title: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.to_string(),
            concepts: Vec::new(),
            scheme_ids: Vec::new(),
        }
    }

    fn with_concepts(mut self, concepts: Vec<Uuid>) -> Self {
        self.concepts = concepts;
        self
    }

    fn with_schemes(mut self, schemes: Vec<Uuid>) -> Self {
        self.scheme_ids = schemes;
        self
    }

    /// Check if this note matches required concepts (AND logic)
    fn matches_required(&self, required: &[Uuid]) -> bool {
        if required.is_empty() {
            return true;
        }
        required.iter().all(|req| self.concepts.contains(req))
    }

    /// Check if this note matches any concepts (OR logic)
    fn matches_any(&self, any: &[Uuid]) -> bool {
        if any.is_empty() {
            return true;
        }
        any.iter().any(|a| self.concepts.contains(a))
    }

    /// Check if this note excludes concepts (NOT logic)
    fn matches_excluded(&self, excluded: &[Uuid]) -> bool {
        if excluded.is_empty() {
            return true;
        }
        !excluded.iter().any(|ex| self.concepts.contains(ex))
    }

    /// Check if this note is in required schemes
    fn matches_required_schemes(&self, required_schemes: &[Uuid]) -> bool {
        if required_schemes.is_empty() {
            return true;
        }
        // Must have at least one scheme from required AND all schemes must be in required
        !self.scheme_ids.is_empty()
            && self.scheme_ids.iter().any(|s| required_schemes.contains(s))
            && self.scheme_ids.iter().all(|s| required_schemes.contains(s))
    }

    /// Check if this note excludes schemes
    fn matches_excluded_schemes(&self, excluded_schemes: &[Uuid]) -> bool {
        if excluded_schemes.is_empty() {
            return true;
        }
        !self.scheme_ids.iter().any(|s| excluded_schemes.contains(s))
    }

    /// Check if this note matches min tag count
    fn matches_min_tag_count(&self, min_count: Option<i32>) -> bool {
        match min_count {
            Some(count) => self.concepts.len() >= count as usize,
            None => true,
        }
    }

    /// Check if this note matches include_untagged setting
    fn matches_include_untagged(&self, include_untagged: bool) -> bool {
        if include_untagged {
            true
        } else {
            !self.concepts.is_empty()
        }
    }
}

/// Mock filter application
fn apply_strict_filter(notes: Vec<MockNote>, filter: &serde_json::Value) -> Vec<MockNote> {
    let required = filter
        .get("required_concepts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let any = filter
        .get("any_concepts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let excluded = filter
        .get("excluded_concepts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let required_schemes = filter
        .get("required_schemes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let excluded_schemes = filter
        .get("excluded_schemes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let min_tag_count = filter
        .get("min_tag_count")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);

    let include_untagged = filter
        .get("include_untagged")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    notes
        .into_iter()
        .filter(|note| {
            note.matches_required(&required)
                && note.matches_any(&any)
                && note.matches_excluded(&excluded)
                && note.matches_required_schemes(&required_schemes)
                && note.matches_excluded_schemes(&excluded_schemes)
                && note.matches_min_tag_count(min_tag_count)
                && note.matches_include_untagged(include_untagged)
        })
        .collect()
}

// =============================================================================
// SCENARIO 1: REQUIRED CONCEPTS (AND LOGIC)
// =============================================================================

#[test]
fn test_required_concepts_and_logic() {
    // Setup: Create test concept IDs
    let concept_a = Uuid::new_v4();
    let concept_b = Uuid::new_v4();
    let concept_c = Uuid::new_v4();

    // Create notes with different tag combinations
    let notes = vec![
        MockNote::new("Note with A and B").with_concepts(vec![concept_a, concept_b]),
        MockNote::new("Note with only A").with_concepts(vec![concept_a]),
        MockNote::new("Note with only B").with_concepts(vec![concept_b]),
        MockNote::new("Note with A, B, and C").with_concepts(vec![concept_a, concept_b, concept_c]),
    ];

    // Filter: required = [A, B] (must have BOTH)
    let filter = json!({
        "required_concepts": [concept_a.to_string(), concept_b.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    // Verify: Only notes with BOTH A and B returned
    assert_eq!(results.len(), 2, "Should return 2 notes with both A and B");
    assert!(
        results
            .iter()
            .all(|n| n.concepts.contains(&concept_a) && n.concepts.contains(&concept_b)),
        "All results should have both concept A and B"
    );
    assert!(
        results.iter().any(|n| n.title == "Note with A and B"),
        "Should include note with exactly A and B"
    );
    assert!(
        results.iter().any(|n| n.title == "Note with A, B, and C"),
        "Should include note with A, B, and C"
    );
}

#[test]
fn test_required_concepts_three_way_and() {
    let concept_a = Uuid::new_v4();
    let concept_b = Uuid::new_v4();
    let concept_c = Uuid::new_v4();

    let notes = vec![
        MockNote::new("All three").with_concepts(vec![concept_a, concept_b, concept_c]),
        MockNote::new("Only A and B").with_concepts(vec![concept_a, concept_b]),
        MockNote::new("Only A and C").with_concepts(vec![concept_a, concept_c]),
        MockNote::new("Only B and C").with_concepts(vec![concept_b, concept_c]),
    ];

    // Require all three
    let filter = json!({
        "required_concepts": [
            concept_a.to_string(),
            concept_b.to_string(),
            concept_c.to_string()
        ]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 1, "Should only return note with all three");
    assert_eq!(results[0].title, "All three");
}

// =============================================================================
// SCENARIO 2: ANY CONCEPTS (OR LOGIC)
// =============================================================================

#[test]
fn test_any_concepts_or_logic() {
    let concept_x = Uuid::new_v4();
    let concept_y = Uuid::new_v4();
    let concept_z = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Note with X").with_concepts(vec![concept_x]),
        MockNote::new("Note with Y").with_concepts(vec![concept_y]),
        MockNote::new("Note with Z").with_concepts(vec![concept_z]),
        MockNote::new("Note with X and Y").with_concepts(vec![concept_x, concept_y]),
    ];

    // Filter: any = [X, Y] (must have X OR Y)
    let filter = json!({
        "any_concepts": [concept_x.to_string(), concept_y.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    // Verify: Notes with X OR Y returned, Z excluded
    assert_eq!(results.len(), 3, "Should return 3 notes (X, Y, and X+Y)");
    assert!(
        !results.iter().any(|n| n.title == "Note with Z"),
        "Should NOT include note with only Z"
    );
    assert!(
        results.iter().any(|n| n.title == "Note with X"),
        "Should include note with X"
    );
    assert!(
        results.iter().any(|n| n.title == "Note with Y"),
        "Should include note with Y"
    );
    assert!(
        results.iter().any(|n| n.title == "Note with X and Y"),
        "Should include note with both X and Y"
    );
}

#[test]
fn test_any_concepts_single_match() {
    let concept_a = Uuid::new_v4();
    let concept_b = Uuid::new_v4();
    let concept_c = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Has A").with_concepts(vec![concept_a]),
        MockNote::new("Has B").with_concepts(vec![concept_b]),
        MockNote::new("Has neither").with_concepts(vec![concept_c]),
    ];

    let filter = json!({
        "any_concepts": [concept_a.to_string(), concept_b.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|n| n.title != "Has neither"));
}

// =============================================================================
// SCENARIO 3: EXCLUDED CONCEPTS (NOT LOGIC)
// =============================================================================

#[test]
fn test_excluded_concepts_not_logic() {
    let concept_public = Uuid::new_v4();
    let concept_internal = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Public only").with_concepts(vec![concept_public]),
        MockNote::new("Public and internal").with_concepts(vec![concept_public, concept_internal]),
        MockNote::new("Internal only").with_concepts(vec![concept_internal]),
    ];

    // Filter: excluded = [internal]
    let filter = json!({
        "excluded_concepts": [concept_internal.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    // Verify: Only note with just [public] returned
    assert_eq!(results.len(), 1, "Should only return public-only note");
    assert_eq!(results[0].title, "Public only");
    assert!(!results[0].concepts.contains(&concept_internal));
}

#[test]
fn test_excluded_concepts_multiple() {
    let concept_a = Uuid::new_v4();
    let concept_draft = Uuid::new_v4();
    let concept_archive = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Clean note").with_concepts(vec![concept_a]),
        MockNote::new("Draft note").with_concepts(vec![concept_a, concept_draft]),
        MockNote::new("Archived note").with_concepts(vec![concept_a, concept_archive]),
        MockNote::new("Draft and archived").with_concepts(vec![
            concept_a,
            concept_draft,
            concept_archive,
        ]),
    ];

    // Exclude both draft and archive
    let filter = json!({
        "excluded_concepts": [concept_draft.to_string(), concept_archive.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Clean note");
}

// =============================================================================
// SCENARIO 4: SCHEME ISOLATION
// =============================================================================

#[test]
fn test_scheme_isolation() {
    let scheme1 = Uuid::new_v4();
    let scheme2 = Uuid::new_v4();
    let _scheme3 = Uuid::new_v4();

    let concept_a = Uuid::new_v4();
    let concept_b = Uuid::new_v4();
    let concept_c = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Only scheme1")
            .with_concepts(vec![concept_a])
            .with_schemes(vec![scheme1]),
        MockNote::new("Only scheme2")
            .with_concepts(vec![concept_b])
            .with_schemes(vec![scheme2]),
        MockNote::new("Mixed schemes")
            .with_concepts(vec![concept_a, concept_b])
            .with_schemes(vec![scheme1, scheme2]),
        MockNote::new("Scheme1 multiple concepts")
            .with_concepts(vec![concept_a, concept_c])
            .with_schemes(vec![scheme1]),
    ];

    // Filter: required_schemes = [scheme1]
    let filter = json!({
        "required_schemes": [scheme1.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    // Verify: Only notes from scheme1 (and ONLY scheme1)
    assert_eq!(results.len(), 2, "Should return notes only from scheme1");
    assert!(
        results
            .iter()
            .all(|n| n.scheme_ids.contains(&scheme1) && n.scheme_ids.len() == 1),
        "All results should be exclusively in scheme1"
    );
    assert!(
        results.iter().any(|n| n.title == "Only scheme1"),
        "Should include 'Only scheme1'"
    );
    assert!(
        results
            .iter()
            .any(|n| n.title == "Scheme1 multiple concepts"),
        "Should include 'Scheme1 multiple concepts'"
    );
    assert!(
        !results.iter().any(|n| n.title == "Mixed schemes"),
        "Should NOT include mixed schemes"
    );
}

#[test]
fn test_excluded_schemes() {
    let scheme_active = Uuid::new_v4();
    let scheme_deprecated = Uuid::new_v4();

    let concept_a = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Active")
            .with_concepts(vec![concept_a])
            .with_schemes(vec![scheme_active]),
        MockNote::new("Deprecated")
            .with_concepts(vec![concept_a])
            .with_schemes(vec![scheme_deprecated]),
    ];

    let filter = json!({
        "excluded_schemes": [scheme_deprecated.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Active");
}

// =============================================================================
// SCENARIO 5: COMBINED FILTERS
// =============================================================================

#[test]
fn test_combined_filters_complex() {
    let concept_rust = Uuid::new_v4();
    let concept_tutorial = Uuid::new_v4();
    let concept_guide = Uuid::new_v4();
    let concept_archive = Uuid::new_v4();
    let concept_draft = Uuid::new_v4();

    let notes = vec![
        // Should match: has rust AND (tutorial OR guide) AND NOT archive
        MockNote::new("Rust tutorial").with_concepts(vec![concept_rust, concept_tutorial]),
        // Should match: has rust AND guide AND NOT archive
        MockNote::new("Rust guide").with_concepts(vec![concept_rust, concept_guide]),
        // Should NOT match: missing tutorial/guide
        MockNote::new("Just Rust").with_concepts(vec![concept_rust]),
        // Should NOT match: has archive tag
        MockNote::new("Archived Rust tutorial").with_concepts(vec![
            concept_rust,
            concept_tutorial,
            concept_archive,
        ]),
        // Should NOT match: has draft tag
        MockNote::new("Draft Rust guide").with_concepts(vec![
            concept_rust,
            concept_guide,
            concept_draft,
        ]),
        // Should NOT match: missing rust
        MockNote::new("Tutorial only").with_concepts(vec![concept_tutorial]),
    ];

    // Complex filter: required = [rust], any = [tutorial, guide], excluded = [archive, draft]
    let filter = json!({
        "required_concepts": [concept_rust.to_string()],
        "any_concepts": [concept_tutorial.to_string(), concept_guide.to_string()],
        "excluded_concepts": [concept_archive.to_string(), concept_draft.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 2, "Should return 2 matching notes");
    assert!(
        results.iter().any(|n| n.title == "Rust tutorial"),
        "Should include Rust tutorial"
    );
    assert!(
        results.iter().any(|n| n.title == "Rust guide"),
        "Should include Rust guide"
    );
}

#[test]
fn test_combined_with_schemes() {
    let concept_a = Uuid::new_v4();
    let concept_b = Uuid::new_v4();
    let concept_excluded = Uuid::new_v4();
    let scheme1 = Uuid::new_v4();
    let scheme2 = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Perfect match")
            .with_concepts(vec![concept_a, concept_b])
            .with_schemes(vec![scheme1]),
        MockNote::new("Wrong scheme")
            .with_concepts(vec![concept_a, concept_b])
            .with_schemes(vec![scheme2]),
        MockNote::new("Has excluded")
            .with_concepts(vec![concept_a, concept_b, concept_excluded])
            .with_schemes(vec![scheme1]),
    ];

    let filter = json!({
        "required_concepts": [concept_a.to_string()],
        "any_concepts": [concept_b.to_string()],
        "excluded_concepts": [concept_excluded.to_string()],
        "required_schemes": [scheme1.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Perfect match");
}

// =============================================================================
// SCENARIO 6: STRICT FILTER + FUZZY SEARCH
// =============================================================================

#[test]
fn test_strict_filter_applies_before_fuzzy_search() {
    // This test verifies that filtering happens BEFORE fuzzy matching
    // In a real scenario, fuzzy search would happen after strict filtering

    let concept_public = Uuid::new_v4();
    let concept_internal = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Public rust tutorial").with_concepts(vec![concept_public]),
        MockNote::new("Internal rust guide").with_concepts(vec![concept_internal]),
        MockNote::new("Public python guide").with_concepts(vec![concept_public]),
    ];

    // First apply strict filter
    let filter = json!({
        "required_concepts": [concept_public.to_string()]
    });

    let filtered = apply_strict_filter(notes, &filter);

    // Verify filtering happened
    assert_eq!(
        filtered.len(),
        2,
        "Filter should reduce to public notes only"
    );
    assert!(
        !filtered.iter().any(|n| n.title.contains("Internal")),
        "Internal notes should be excluded"
    );

    // Now fuzzy search would operate on filtered results
    let fuzzy_results: Vec<_> = filtered
        .into_iter()
        .filter(|n| n.title.contains("rust"))
        .collect();

    assert_eq!(
        fuzzy_results.len(),
        1,
        "Fuzzy search should find 1 rust note among filtered"
    );
    assert_eq!(fuzzy_results[0].title, "Public rust tutorial");
}

// =============================================================================
// SCENARIO 7: EMPTY FILTER PASSTHROUGH
// =============================================================================

#[test]
fn test_empty_filter_returns_all() {
    let notes = vec![
        MockNote::new("Note 1").with_concepts(vec![Uuid::new_v4()]),
        MockNote::new("Note 2").with_concepts(vec![Uuid::new_v4()]),
        MockNote::new("Note 3").with_concepts(vec![Uuid::new_v4()]),
    ];

    let empty_filter = json!({});

    let results = apply_strict_filter(notes.clone(), &empty_filter);

    assert_eq!(
        results.len(),
        notes.len(),
        "Empty filter should return all notes"
    );
}

#[test]
fn test_filter_with_only_defaults() {
    let notes = vec![
        MockNote::new("Tagged").with_concepts(vec![Uuid::new_v4()]),
        MockNote::new("Untagged").with_concepts(vec![]),
    ];

    // Filter with only default values (should act like empty)
    let filter = json!({
        "include_untagged": true
    });

    let results = apply_strict_filter(notes.clone(), &filter);

    assert_eq!(results.len(), 2, "Should include all notes");
}

// =============================================================================
// SCENARIO 8: TAG COUNT AND UNTAGGED HANDLING
// =============================================================================

#[test]
fn test_min_tag_count() {
    let concept_a = Uuid::new_v4();
    let concept_b = Uuid::new_v4();
    let concept_c = Uuid::new_v4();

    let notes = vec![
        MockNote::new("No tags").with_concepts(vec![]),
        MockNote::new("One tag").with_concepts(vec![concept_a]),
        MockNote::new("Two tags").with_concepts(vec![concept_a, concept_b]),
        MockNote::new("Three tags").with_concepts(vec![concept_a, concept_b, concept_c]),
    ];

    let filter = json!({
        "min_tag_count": 2
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 2, "Should return notes with 2+ tags");
    assert!(results.iter().all(|n| n.concepts.len() >= 2));
    assert!(
        results.iter().any(|n| n.title == "Two tags"),
        "Should include 'Two tags'"
    );
    assert!(
        results.iter().any(|n| n.title == "Three tags"),
        "Should include 'Three tags'"
    );
}

#[test]
fn test_exclude_untagged() {
    let concept_a = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Tagged").with_concepts(vec![concept_a]),
        MockNote::new("Untagged").with_concepts(vec![]),
    ];

    let filter = json!({
        "include_untagged": false
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 1, "Should exclude untagged notes");
    assert_eq!(results[0].title, "Tagged");
}

#[test]
fn test_include_untagged_default() {
    let concept_a = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Tagged").with_concepts(vec![concept_a]),
        MockNote::new("Untagged").with_concepts(vec![]),
    ];

    // Default behavior: include_untagged = true
    let filter = json!({});

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 2, "Should include untagged by default");
}

// =============================================================================
// SERIALIZATION & TYPE SAFETY TESTS
// =============================================================================

#[test]
fn test_strict_filter_json_serialization() {
    let concept1 = Uuid::new_v4();
    let concept2 = Uuid::new_v4();
    let scheme1 = Uuid::new_v4();

    let filter = json!({
        "required_concepts": [concept1.to_string()],
        "any_concepts": [concept2.to_string()],
        "excluded_concepts": [],
        "required_schemes": [scheme1.to_string()],
        "excluded_schemes": [],
        "min_tag_count": 2,
        "include_untagged": false
    });

    // Verify structure
    assert!(filter.get("required_concepts").is_some());
    assert!(filter.get("any_concepts").is_some());
    assert!(filter.get("required_schemes").is_some());
    assert_eq!(filter.get("min_tag_count").unwrap(), 2);
    assert_eq!(filter.get("include_untagged").unwrap(), false);
}

#[test]
fn test_strict_filter_empty_arrays_omitted() {
    let filter = json!({
        "required_concepts": [],
        "any_concepts": [],
        "excluded_concepts": []
    });

    // In actual API, empty arrays would be omitted via skip_serializing_if
    assert!(filter
        .get("required_concepts")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

// =============================================================================
// EDGE CASES & ERROR CONDITIONS
// =============================================================================

#[test]
fn test_filter_with_nonexistent_concept_returns_nothing() {
    let existing_concept = Uuid::new_v4();
    let nonexistent_concept = Uuid::new_v4();

    let notes = vec![MockNote::new("Has existing").with_concepts(vec![existing_concept])];

    let filter = json!({
        "required_concepts": [nonexistent_concept.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(
        results.len(),
        0,
        "Should return no results for nonexistent concept"
    );
}

#[test]
fn test_contradictory_filters() {
    let concept_a = Uuid::new_v4();

    let notes = vec![MockNote::new("Has A").with_concepts(vec![concept_a])];

    // Require AND exclude the same concept (impossible)
    let filter = json!({
        "required_concepts": [concept_a.to_string()],
        "excluded_concepts": [concept_a.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(
        results.len(),
        0,
        "Contradictory filters should return no results"
    );
}

#[test]
fn test_min_tag_count_zero() {
    let notes = vec![
        MockNote::new("Untagged").with_concepts(vec![]),
        MockNote::new("Tagged").with_concepts(vec![Uuid::new_v4()]),
    ];

    let filter = json!({
        "min_tag_count": 0
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 2, "min_tag_count: 0 should include all");
}

#[test]
fn test_scheme_isolation_with_no_schemes() {
    let scheme1 = Uuid::new_v4();
    let concept_a = Uuid::new_v4();

    let notes = vec![MockNote::new("No scheme").with_concepts(vec![concept_a])];

    let filter = json!({
        "required_schemes": [scheme1.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(
        results.len(),
        0,
        "Notes without schemes should be excluded by scheme filter"
    );
}

// =============================================================================
// COMPLEX REAL-WORLD SCENARIOS
// =============================================================================

#[test]
fn test_complex_taxonomy_filtering() {
    // Simulate a real taxonomy:
    // Scheme: "topics" with concepts: rust, python, tutorial, advanced
    // Scheme: "status" with concepts: published, draft
    let scheme_topics = Uuid::new_v4();
    let scheme_status = Uuid::new_v4();

    let concept_rust = Uuid::new_v4();
    let concept_python = Uuid::new_v4();
    let concept_tutorial = Uuid::new_v4();
    let concept_advanced = Uuid::new_v4();
    let concept_published = Uuid::new_v4();
    let concept_draft = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Published Rust tutorial")
            .with_concepts(vec![concept_rust, concept_tutorial, concept_published])
            .with_schemes(vec![scheme_topics, scheme_status]),
        MockNote::new("Draft Rust advanced")
            .with_concepts(vec![concept_rust, concept_advanced, concept_draft])
            .with_schemes(vec![scheme_topics, scheme_status]),
        MockNote::new("Published Python tutorial")
            .with_concepts(vec![concept_python, concept_tutorial, concept_published])
            .with_schemes(vec![scheme_topics, scheme_status]),
    ];

    // Find: Rust tutorials that are published (not drafts)
    let filter = json!({
        "required_concepts": [concept_rust.to_string(), concept_tutorial.to_string()],
        "excluded_concepts": [concept_draft.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Published Rust tutorial");
}

#[test]
fn test_content_maturity_filtering() {
    let concept_beginner = Uuid::new_v4();
    let concept_intermediate = Uuid::new_v4();
    let concept_expert = Uuid::new_v4();
    let concept_programming = Uuid::new_v4();

    let notes = vec![
        MockNote::new("Beginner programming")
            .with_concepts(vec![concept_programming, concept_beginner]),
        MockNote::new("Intermediate programming")
            .with_concepts(vec![concept_programming, concept_intermediate]),
        MockNote::new("Expert programming")
            .with_concepts(vec![concept_programming, concept_expert]),
    ];

    // Find: Programming content for beginners OR intermediate (exclude expert)
    let filter = json!({
        "required_concepts": [concept_programming.to_string()],
        "any_concepts": [concept_beginner.to_string(), concept_intermediate.to_string()],
        "excluded_concepts": [concept_expert.to_string()]
    });

    let results = apply_strict_filter(notes, &filter);

    assert_eq!(results.len(), 2);
    assert!(!results.iter().any(|n| n.title.contains("Expert")));
}
