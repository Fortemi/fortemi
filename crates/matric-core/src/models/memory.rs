//! Memory search models for temporal-spatial queries.
//!
//! Supports searching memories by:
//! - Time: Timeline grouping, date ranges
//! - Location: Radius search, named places
//! - Both: "Photos from Paris last summer"

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// MEMORY SEARCH TYPES
// =============================================================================

/// A memory result with temporal and spatial context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    /// Provenance record ID
    pub provenance_id: Uuid,
    /// Attachment ID
    pub attachment_id: Uuid,
    /// Associated note ID
    pub note_id: Uuid,
    /// Filename
    pub filename: String,
    /// Content type (MIME type)
    pub content_type: Option<String>,
    /// Capture time range (start/end for video, single instant for photo)
    pub capture_time: Option<(DateTime<Utc>, Option<DateTime<Utc>>)>,
    /// Event type (photo, video, audio, etc.)
    pub event_type: Option<String>,
    /// Event title/description
    pub event_title: Option<String>,
    /// Distance from query point (for spatial queries)
    pub distance_m: Option<f64>,
    /// Named location name
    pub location_name: Option<String>,
}

/// Memory search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResponse {
    pub memories: Vec<MemoryHit>,
    pub total: usize,
}

/// Timeline grouping response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineResponse {
    pub groups: Vec<TimelineGroup>,
    pub total: usize,
}

/// A group of memories within a time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineGroup {
    /// Group period (e.g., "2024-01", "2024-W23", "2024-01-15")
    pub period: String,
    /// Start of period
    pub start: DateTime<Utc>,
    /// End of period
    pub end: DateTime<Utc>,
    /// Memories in this group
    pub memories: Vec<MemoryHit>,
    /// Count of memories in this group
    pub count: usize,
}

// =============================================================================
// CROSS-ARCHIVE SEARCH TYPES
// =============================================================================

/// Cross-archive search request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossArchiveSearchRequest {
    /// Search query
    pub query: String,
    /// Archive schemas to search (empty = all)
    #[serde(default)]
    pub archives: Vec<String>,
    /// Search mode (fts, vector, hybrid)
    #[serde(default)]
    pub mode: crate::SearchMode,
    /// Maximum results per archive
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Enable RRF fusion across archives
    #[serde(default)]
    pub enable_fusion: bool,
}

fn default_limit() -> i64 {
    20
}

/// Cross-archive search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossArchiveSearchResult {
    /// Archive name (schema)
    pub archive_name: String,
    /// Note ID
    pub note_id: Uuid,
    /// Search score (RRF score if fusion enabled)
    pub score: f32,
    /// Snippet
    pub snippet: Option<String>,
    /// Title
    pub title: Option<String>,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Cross-archive search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossArchiveSearchResponse {
    pub results: Vec<CrossArchiveSearchResult>,
    pub archives_searched: Vec<String>,
    pub total: usize,
}

// =============================================================================
// ATTACHMENT SEARCH TYPES
// =============================================================================

/// Attachment search request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentSearchRequest {
    /// Filter by note ID
    pub note_id: Option<Uuid>,
    /// Filter by content type (MIME type prefix, e.g., "image/", "video/")
    pub content_type: Option<String>,
    /// Filter by event type
    pub event_type: Option<String>,
    /// Filter by capture time range
    pub capture_after: Option<DateTime<Utc>>,
    pub capture_before: Option<DateTime<Utc>>,
    /// Filter by location (radius search)
    pub near_lat: Option<f64>,
    pub near_lon: Option<f64>,
    pub radius_m: Option<f64>,
    /// Filter by named location
    pub location_name: Option<String>,
    /// Filter by device
    pub device_id: Option<Uuid>,
    /// Maximum results
    #[serde(default = "default_limit")]
    pub limit: i64,
}

/// Attachment search response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentSearchResponse {
    pub attachments: Vec<MemoryHit>,
    pub total: usize,
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_hit_serialization() {
        let hit = MemoryHit {
            provenance_id: Uuid::new_v4(),
            attachment_id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            filename: "photo.jpg".to_string(),
            content_type: Some("image/jpeg".to_string()),
            capture_time: Some((Utc::now(), None)),
            event_type: Some("photo".to_string()),
            event_title: Some("Beach sunset".to_string()),
            distance_m: Some(150.5),
            location_name: Some("Santa Monica Beach".to_string()),
        };

        let json = serde_json::to_string(&hit).unwrap();
        let deserialized: MemoryHit = serde_json::from_str(&json).unwrap();

        assert_eq!(hit.provenance_id, deserialized.provenance_id);
        assert_eq!(hit.filename, deserialized.filename);
        assert_eq!(hit.content_type, deserialized.content_type);
    }

    #[test]
    fn test_cross_archive_request_defaults() {
        let req = CrossArchiveSearchRequest {
            query: "test".to_string(),
            archives: vec![],
            mode: Default::default(),
            limit: default_limit(),
            enable_fusion: false,
        };

        assert_eq!(req.limit, 20);
        assert!(!req.enable_fusion);
        assert!(req.archives.is_empty());
    }

    #[test]
    fn test_cross_archive_request_serialization() {
        let req = CrossArchiveSearchRequest {
            query: "rust programming".to_string(),
            archives: vec!["archive_2024".to_string(), "archive_2025".to_string()],
            mode: crate::SearchMode::Hybrid,
            limit: 50,
            enable_fusion: true,
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CrossArchiveSearchRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(req.query, deserialized.query);
        assert_eq!(req.archives, deserialized.archives);
        assert_eq!(req.limit, deserialized.limit);
        assert_eq!(req.enable_fusion, deserialized.enable_fusion);
    }

    #[test]
    fn test_attachment_search_request_optional_fields() {
        let req = AttachmentSearchRequest {
            note_id: None,
            content_type: Some("image/".to_string()),
            event_type: None,
            capture_after: None,
            capture_before: None,
            near_lat: Some(34.0),
            near_lon: Some(-118.0),
            radius_m: Some(5000.0),
            location_name: None,
            device_id: None,
            limit: 100,
        };

        assert!(req.note_id.is_none());
        assert!(req.content_type.is_some());
        assert_eq!(req.limit, 100);
    }

    #[test]
    fn test_timeline_group_structure() {
        let group = TimelineGroup {
            period: "2024-01".to_string(),
            start: Utc::now(),
            end: Utc::now(),
            memories: vec![],
            count: 0,
        };

        assert_eq!(group.period, "2024-01");
        assert_eq!(group.count, 0);
        assert!(group.memories.is_empty());
    }
}
