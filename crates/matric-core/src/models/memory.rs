//! Memory search models for temporal-spatial queries.
//!
//! Supports searching memories by:
//! - Time: Timeline grouping, date ranges
//! - Location: Radius search, named places
//! - Both: "Photos from Paris last summer"

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn debug_len(value: &str) -> usize {
    value.chars().count()
}

fn optional_debug_len(value: Option<&String>) -> Option<usize> {
    value.map(|value| debug_len(value))
}

// =============================================================================
// MEMORY SEARCH TYPES
// =============================================================================

/// A memory result with temporal and spatial context.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for MemoryHit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryHit")
            .field("provenance_id_set", &true)
            .field("attachment_id_set", &true)
            .field("note_id_set", &true)
            .field("filename_len", &debug_len(&self.filename))
            .field("content_type_len", &optional_debug_len(self.content_type.as_ref()))
            .field("capture_time_set", &self.capture_time.is_some())
            .field("event_type_len", &optional_debug_len(self.event_type.as_ref()))
            .field("event_title_len", &optional_debug_len(self.event_title.as_ref()))
            .field("distance_m_set", &self.distance_m.is_some())
            .field(
                "location_name_len",
                &optional_debug_len(self.location_name.as_ref()),
            )
            .finish()
    }
}

/// Memory search response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemorySearchResponse {
    pub memories: Vec<MemoryHit>,
    pub total: usize,
}

impl fmt::Debug for MemorySearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemorySearchResponse")
            .field("memories_count", &self.memories.len())
            .field("total", &self.total)
            .finish()
    }
}

/// Timeline grouping response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TimelineResponse {
    pub groups: Vec<TimelineGroup>,
    pub total: usize,
}

impl fmt::Debug for TimelineResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimelineResponse")
            .field("groups_count", &self.groups.len())
            .field("total", &self.total)
            .finish()
    }
}

/// A group of memories within a time period.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for TimelineGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TimelineGroup")
            .field("period_len", &debug_len(&self.period))
            .field("start", &self.start)
            .field("end", &self.end)
            .field("memories_count", &self.memories.len())
            .field("count", &self.count)
            .finish()
    }
}

// =============================================================================
// CROSS-ARCHIVE SEARCH TYPES
// =============================================================================

/// Cross-archive search request.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for CrossArchiveSearchRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let archive_lens: Vec<usize> = self.archives.iter().map(|value| debug_len(value)).collect();
        f.debug_struct("CrossArchiveSearchRequest")
            .field("query_len", &debug_len(&self.query))
            .field("archives_count", &self.archives.len())
            .field("archive_lens", &archive_lens)
            .field("mode", &self.mode)
            .field("limit", &self.limit)
            .field("enable_fusion", &self.enable_fusion)
            .finish()
    }
}

fn default_limit() -> i64 {
    crate::defaults::PAGE_LIMIT_SEARCH
}

/// Cross-archive search result.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for CrossArchiveSearchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag_lens: Vec<usize> = self.tags.iter().map(|value| debug_len(value)).collect();
        f.debug_struct("CrossArchiveSearchResult")
            .field("archive_name_len", &debug_len(&self.archive_name))
            .field("note_id_set", &true)
            .field("score", &self.score)
            .field("snippet_len", &optional_debug_len(self.snippet.as_ref()))
            .field("title_len", &optional_debug_len(self.title.as_ref()))
            .field("tags_count", &self.tags.len())
            .field("tag_lens", &tag_lens)
            .finish()
    }
}

/// Cross-archive search response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CrossArchiveSearchResponse {
    pub results: Vec<CrossArchiveSearchResult>,
    pub archives_searched: Vec<String>,
    pub total: usize,
}

impl fmt::Debug for CrossArchiveSearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let archives_searched_lens: Vec<usize> =
            self.archives_searched.iter().map(|value| debug_len(value)).collect();
        f.debug_struct("CrossArchiveSearchResponse")
            .field("results_count", &self.results.len())
            .field("archives_searched_count", &self.archives_searched.len())
            .field("archives_searched_lens", &archives_searched_lens)
            .field("total", &self.total)
            .finish()
    }
}

// =============================================================================
// ATTACHMENT SEARCH TYPES
// =============================================================================

/// Attachment search request.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
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

impl fmt::Debug for AttachmentSearchRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentSearchRequest")
            .field("note_id_set", &self.note_id.is_some())
            .field("content_type_len", &optional_debug_len(self.content_type.as_ref()))
            .field("event_type_len", &optional_debug_len(self.event_type.as_ref()))
            .field("capture_after_set", &self.capture_after.is_some())
            .field("capture_before_set", &self.capture_before.is_some())
            .field("near_lat_set", &self.near_lat.is_some())
            .field("near_lon_set", &self.near_lon.is_some())
            .field("radius_m_set", &self.radius_m.is_some())
            .field(
                "location_name_len",
                &optional_debug_len(self.location_name.as_ref()),
            )
            .field("device_id_set", &self.device_id.is_some())
            .field("limit", &self.limit)
            .finish()
    }
}

/// Attachment search response.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AttachmentSearchResponse {
    pub attachments: Vec<MemoryHit>,
    pub total: usize,
}

impl fmt::Debug for AttachmentSearchResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttachmentSearchResponse")
            .field("attachments_count", &self.attachments.len())
            .field("total", &self.total)
            .finish()
    }
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

    #[test]
    fn memory_search_debug_redacts_filenames_titles_locations_and_ids() {
        let provenance_id = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa").unwrap();
        let attachment_id = Uuid::parse_str("bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb").unwrap();
        let note_id = Uuid::parse_str("cccccccc-cccc-4ccc-cccc-cccccccccccc").unwrap();
        let hit = MemoryHit {
            provenance_id,
            attachment_id,
            note_id,
            filename: "秘密-photo-customer@example.com-sk-live.jpg".to_string(),
            content_type: Some("image/秘密".to_string()),
            capture_time: Some((Utc::now(), None)),
            event_type: Some("写真-secret".to_string()),
            event_title: Some("海 with bearer-secret at /srv/private".to_string()),
            distance_m: Some(150.5),
            location_name: Some("自宅 customer@example.com".to_string()),
        };
        let response = MemorySearchResponse {
            memories: vec![hit.clone()],
            total: 1,
        };
        let group = TimelineGroup {
            period: "2024-六月-customer@example.com-sk-live".to_string(),
            start: Utc::now(),
            end: Utc::now(),
            memories: vec![hit.clone()],
            count: 1,
        };
        let timeline = TimelineResponse {
            groups: vec![group],
            total: 1,
        };
        let attachments = AttachmentSearchResponse {
            attachments: vec![hit],
            total: 1,
        };

        let debug = format!("{response:?} {timeline:?} {attachments:?}");

        for raw in [
            provenance_id.to_string(),
            attachment_id.to_string(),
            note_id.to_string(),
            "private-path",
            "秘密",
            "customer@example.com",
            "sk-live",
            "image/秘密",
            "写真-secret",
            "bearer-secret",
            "/srv/private",
            "自宅",
            "2024-customer",
        ] {
            assert!(!debug.contains(&raw), "debug leaked {raw}: {debug}");
        }

        assert!(debug.contains("filename_len: 39"));
        assert!(debug.contains("period_len: 36"));
        assert!(debug.contains("memories_count"));
        assert!(debug.contains("groups_count"));
        assert!(debug.contains("attachments_count"));
        assert!(debug.contains("total"));
    }

    #[test]
    fn cross_archive_and_attachment_search_debug_redacts_queries_archives_snippets_tags_and_locations(
    ) {
        let note_id = Uuid::parse_str("dddddddd-dddd-4ddd-dddd-dddddddddddd").unwrap();
        let device_id = Uuid::parse_str("eeeeeeee-eeee-4eee-eeee-eeeeeeeeeeee").unwrap();
        let request = CrossArchiveSearchRequest {
            query: "探す private@example.com sk-live /srv/private".to_string(),
            archives: vec![
                "tenant_日本_customer@example.com".to_string(),
                "postgres://admin:secret@db".to_string(),
            ],
            mode: crate::SearchMode::Hybrid,
            limit: 25,
            enable_fusion: true,
        };
        let result = CrossArchiveSearchResult {
            archive_name: "tenant_日本_customer@example.com".to_string(),
            note_id,
            score: 0.92,
            snippet: Some("抜粋 has sk-live and /srv/private".to_string()),
            title: Some("秘密 title customer@example.com".to_string()),
            tags: vec![
                "秘密-tag".to_string(),
                "postgres://admin:secret@db".to_string(),
            ],
        };
        let response = CrossArchiveSearchResponse {
            results: vec![result],
            archives_searched: vec!["tenant_日本_customer@example.com".to_string()],
            total: 1,
        };
        let attachment_request = AttachmentSearchRequest {
            note_id: Some(note_id),
            content_type: Some("image/秘密".to_string()),
            event_type: Some("scan-秘密".to_string()),
            capture_after: Some(Utc::now()),
            capture_before: Some(Utc::now()),
            near_lat: Some(34.0195),
            near_lon: Some(-118.4912),
            radius_m: Some(500.0),
            location_name: Some("自宅 private@example.com".to_string()),
            device_id: Some(device_id),
            limit: 10,
        };

        let debug = format!("{request:?} {response:?} {attachment_request:?}");

        for raw in [
            note_id.to_string(),
            device_id.to_string(),
            "探す private@example.com sk-live /srv/private",
            "tenant_customer@example.com",
            "tenant_日本_customer@example.com",
            "postgres://admin:secret@db",
            "抜粋 has",
            "秘密 title",
            "秘密-tag",
            "image/秘密",
            "scan-秘密",
            "自宅 private@example.com",
            "34.0195",
            "-118.4912",
        ] {
            assert!(!debug.contains(&raw), "debug leaked {raw}: {debug}");
        }

        assert!(debug.contains("query_len: 44"));
        assert!(debug.contains("archive_lens: [28, 26]"));
        assert!(debug.contains("archive_name_len: 28"));
        assert!(debug.contains("snippet_len: 31"));
        assert!(debug.contains("title_len: 29"));
        assert!(debug.contains("tag_lens: [6, 26]"));
        assert!(debug.contains("archives_searched_lens: [28]"));
        assert!(debug.contains("content_type_len: Some(8)"));
        assert!(debug.contains("event_type_len: Some(7)"));
        assert!(debug.contains("location_name_len: Some(26)"));
        assert!(debug.contains("query_len"));
        assert!(debug.contains("archives_count"));
        assert!(debug.contains("archive_lens"));
        assert!(debug.contains("results_count"));
        assert!(debug.contains("tags_count"));
        assert!(debug.contains("location_name_len"));
        assert!(debug.contains("device_id_set"));
    }
}
