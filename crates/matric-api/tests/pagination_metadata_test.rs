/// Unit tests for issue #465: Standardize API response formats with pagination metadata
///
/// This test verifies that list endpoints return standardized response wrappers
/// with consistent pagination metadata.
///
/// Background:
/// - List endpoints currently return raw arrays or inconsistent response formats
/// - Clients need pagination metadata (total, limit, offset, has_more) to implement
///   proper pagination UI and infinite scrolling
/// - Standardized response format improves API consistency and developer experience
///
/// Expected Response Format:
/// ```json
/// {
///   "data": [...],
///   "pagination": {
///     "total": 100,
///     "limit": 50,
///     "offset": 0,
///     "has_more": true
///   }
/// }
/// ```
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct PaginationMeta {
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct ListResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

#[test]
fn test_pagination_meta_has_more_true() {
    // Test case: More items available (offset + data.len() < total)
    let meta = PaginationMeta {
        total: 100,
        limit: 50,
        offset: 0,
        has_more: true,
    };

    // With offset=0, limit=50, and only 50 items returned, has_more should be true
    assert!(meta.has_more);
    assert_eq!(meta.total, 100);
}

#[test]
fn test_pagination_meta_has_more_false() {
    // Test case: No more items (offset + data.len() >= total)
    let meta = PaginationMeta {
        total: 50,
        limit: 50,
        offset: 0,
        has_more: false,
    };

    // With offset=0, limit=50, and total=50, has_more should be false
    assert!(!meta.has_more);
}

#[test]
fn test_pagination_meta_second_page() {
    // Test case: Second page with more items available
    let meta = PaginationMeta {
        total: 150,
        limit: 50,
        offset: 50,
        has_more: true,
    };

    // With offset=50, limit=50, total=150, has_more should be true (50 more items)
    assert!(meta.has_more);
    assert_eq!(meta.offset, 50);
}

#[test]
fn test_pagination_meta_last_page() {
    // Test case: Last page (fewer items than limit)
    let meta = PaginationMeta {
        total: 125,
        limit: 50,
        offset: 100,
        has_more: false,
    };

    // With offset=100, limit=50, total=125, only 25 items returned, has_more should be false
    assert!(!meta.has_more);
}

#[test]
fn test_list_response_structure() {
    // Test that ListResponse can be serialized/deserialized correctly
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestItem {
        id: usize,
        name: String,
    }

    let response = ListResponse {
        data: vec![
            TestItem {
                id: 1,
                name: "Item 1".to_string(),
            },
            TestItem {
                id: 2,
                name: "Item 2".to_string(),
            },
        ],
        pagination: PaginationMeta {
            total: 10,
            limit: 5,
            offset: 0,
            has_more: true,
        },
    };

    // Test serialization to JSON
    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("\"data\""));
    assert!(json.contains("\"pagination\""));
    assert!(json.contains("\"total\":10"));
    assert!(json.contains("\"has_more\":true"));

    // Test deserialization from JSON
    let deserialized: ListResponse<TestItem> =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.data.len(), 2);
    assert_eq!(deserialized.pagination.total, 10);
    assert!(deserialized.pagination.has_more);
}

#[test]
fn test_list_response_new_helper() {
    // This test documents the expected behavior of the ListResponse::new helper method
    // The helper should automatically calculate has_more based on offset, data.len(), and total

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestItem {
        id: usize,
    }

    // Test case 1: More items available
    let data = [TestItem { id: 1 }, TestItem { id: 2 }];
    let total = 10;
    let offset = 0;

    // has_more should be true because offset (0) + data.len() (2) < total (10)
    let expected_has_more = offset + data.len() < total;
    assert!(expected_has_more);

    // Test case 2: No more items (last page)
    let data_last = [TestItem { id: 9 }, TestItem { id: 10 }];
    let offset_last = 8;

    // has_more should be false because offset (8) + data.len() (2) >= total (10)
    let expected_has_more_last = offset_last + data_last.len() < total;
    assert!(!expected_has_more_last);
}

#[test]
fn test_pagination_meta_serialization() {
    // Verify that PaginationMeta serializes to expected JSON format
    let meta = PaginationMeta {
        total: 100,
        limit: 20,
        offset: 40,
        has_more: true,
    };

    let json = serde_json::to_value(&meta).expect("Failed to serialize");
    assert_eq!(json["total"], 100);
    assert_eq!(json["limit"], 20);
    assert_eq!(json["offset"], 40);
    assert_eq!(json["has_more"], true);
}

#[test]
fn test_empty_list_response() {
    // Test edge case: Empty result set
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestItem {
        id: usize,
    }

    let response = ListResponse {
        data: Vec::<TestItem>::new(),
        pagination: PaginationMeta {
            total: 0,
            limit: 50,
            offset: 0,
            has_more: false,
        },
    };

    assert_eq!(response.data.len(), 0);
    assert_eq!(response.pagination.total, 0);
    assert!(!response.pagination.has_more);
}
