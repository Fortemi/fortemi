//! Temporal filtering types for the Unified Strict Filter system.
//!
//! This module provides time-based filtering that leverages UUIDv7's embedded
//! timestamps for efficient temporal queries without requiring separate timestamp
//! columns.
//!
//! # UUIDv7 Temporal Optimization
//!
//! UUIDv7 identifiers embed a 48-bit Unix timestamp (milliseconds) in their first
//! 48 bits. This enables temporal filtering directly on the primary key column:
//!
//! ```sql
//! -- Instead of: WHERE created_at >= $1 AND created_at < $2
//! -- We can use: WHERE id >= $floor_uuid AND id < $ceiling_uuid
//! ```
//!
//! This approach:
//! - Uses the primary key index (always present, always efficient)
//! - Eliminates need for separate timestamp indexes
//! - Provides natural time-ordering in UUID comparisons

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::uuid_utils::{range_boundaries, v7_ceiling_from_timestamp, v7_from_timestamp};
use uuid::Uuid;

// =============================================================================
// NAMED TEMPORAL RANGES
// =============================================================================

/// Named temporal ranges for common time-based queries.
///
/// These provide semantic shortcuts for frequently used time periods,
/// automatically computing the appropriate date boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NamedTemporalRange {
    /// Last 60 minutes
    LastHour,
    /// Last 24 hours
    Today,
    /// Last 7 days
    ThisWeek,
    /// Last 30 days
    ThisMonth,
    /// Last 90 days
    ThisQuarter,
    /// Last 365 days
    ThisYear,
    /// All time (no temporal restriction)
    #[default]
    AllTime,
}

impl NamedTemporalRange {
    /// Convert the named range to concrete DateTime boundaries.
    ///
    /// Returns `(start, end)` where start is inclusive and end is exclusive.
    /// For `AllTime`, returns `None` indicating no temporal restriction.
    pub fn to_boundaries(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        let now = Utc::now();
        match self {
            Self::LastHour => Some((now - Duration::hours(1), now)),
            Self::Today => Some((now - Duration::hours(24), now)),
            Self::ThisWeek => Some((now - Duration::days(7), now)),
            Self::ThisMonth => Some((now - Duration::days(30), now)),
            Self::ThisQuarter => Some((now - Duration::days(90), now)),
            Self::ThisYear => Some((now - Duration::days(365), now)),
            Self::AllTime => None,
        }
    }

    /// Convert to UUIDv7 boundaries for efficient primary key filtering.
    ///
    /// Returns `(floor_uuid, ceiling_uuid)` suitable for:
    /// `WHERE id >= $floor AND id < $ceiling`
    pub fn to_uuid_boundaries(&self) -> Option<(Uuid, Uuid)> {
        self.to_boundaries()
            .map(|(start, end)| range_boundaries(&start, &end))
    }
}

// =============================================================================
// STRICT TEMPORAL FILTER
// =============================================================================

/// Strict temporal filter for time-based note filtering.
///
/// This filter supports multiple temporal dimensions:
/// - Created time (when the note was first created)
/// - Updated time (when the note was last modified)
/// - Accessed time (when the note was last read/viewed)
/// - Custom date ranges (user-defined time periods)
///
/// # Example
///
/// ```
/// use matric_core::StrictTemporalFilter;
/// use matric_core::temporal::NamedTemporalRange;
/// use chrono::{Duration, Utc};
///
/// // Filter notes created in the last week
/// let filter = StrictTemporalFilter::new()
///     .created_within(NamedTemporalRange::ThisWeek);
///
/// // Filter notes updated in a specific date range
/// let start = Utc::now() - Duration::days(30);
/// let end = Utc::now() - Duration::days(7);
/// let filter = StrictTemporalFilter::new()
///     .updated_between(start, end);
///
/// // Filter notes accessed recently (hot content)
/// let filter = StrictTemporalFilter::new()
///     .accessed_within(NamedTemporalRange::Today);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrictTemporalFilter {
    /// Created time filter using named range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_range: Option<NamedTemporalRange>,

    /// Created time filter using explicit boundaries (overrides named range).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_after: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_before: Option<DateTime<Utc>>,

    /// Updated time filter using named range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_range: Option<NamedTemporalRange>,

    /// Updated time filter using explicit boundaries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_after: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_before: Option<DateTime<Utc>>,

    /// Accessed time filter using named range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed_range: Option<NamedTemporalRange>,

    /// Accessed time filter using explicit boundaries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed_after: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed_before: Option<DateTime<Utc>>,

    /// Whether to include notes that have never been accessed.
    /// Default: true (include unaccessed notes).
    #[serde(default = "default_true")]
    pub include_never_accessed: bool,
}

fn default_true() -> bool {
    true
}

impl StrictTemporalFilter {
    /// Create a new empty temporal filter.
    pub fn new() -> Self {
        Self::default()
    }

    // =========================================================================
    // CREATED TIME FILTERS
    // =========================================================================

    /// Filter by created time using a named range.
    pub fn created_within(mut self, range: NamedTemporalRange) -> Self {
        self.created_range = Some(range);
        self
    }

    /// Filter notes created after a specific time.
    pub fn created_after(mut self, time: DateTime<Utc>) -> Self {
        self.created_after = Some(time);
        self
    }

    /// Filter notes created before a specific time.
    pub fn created_before(mut self, time: DateTime<Utc>) -> Self {
        self.created_before = Some(time);
        self
    }

    /// Filter notes created between two times.
    pub fn created_between(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.created_after = Some(start);
        self.created_before = Some(end);
        self
    }

    // =========================================================================
    // UPDATED TIME FILTERS
    // =========================================================================

    /// Filter by updated time using a named range.
    pub fn updated_within(mut self, range: NamedTemporalRange) -> Self {
        self.updated_range = Some(range);
        self
    }

    /// Filter notes updated after a specific time.
    pub fn updated_after(mut self, time: DateTime<Utc>) -> Self {
        self.updated_after = Some(time);
        self
    }

    /// Filter notes updated before a specific time.
    pub fn updated_before(mut self, time: DateTime<Utc>) -> Self {
        self.updated_before = Some(time);
        self
    }

    /// Filter notes updated between two times.
    pub fn updated_between(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.updated_after = Some(start);
        self.updated_before = Some(end);
        self
    }

    // =========================================================================
    // ACCESSED TIME FILTERS
    // =========================================================================

    /// Filter by accessed time using a named range.
    pub fn accessed_within(mut self, range: NamedTemporalRange) -> Self {
        self.accessed_range = Some(range);
        self
    }

    /// Filter notes accessed after a specific time.
    pub fn accessed_after(mut self, time: DateTime<Utc>) -> Self {
        self.accessed_after = Some(time);
        self
    }

    /// Filter notes accessed before a specific time.
    pub fn accessed_before(mut self, time: DateTime<Utc>) -> Self {
        self.accessed_before = Some(time);
        self
    }

    /// Filter notes accessed between two times.
    pub fn accessed_between(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.accessed_after = Some(start);
        self.accessed_before = Some(end);
        self
    }

    /// Set whether to include notes that have never been accessed.
    pub fn with_include_never_accessed(mut self, include: bool) -> Self {
        self.include_never_accessed = include;
        self
    }

    // =========================================================================
    // UTILITY METHODS
    // =========================================================================

    /// Check if the filter is empty (no constraints).
    pub fn is_empty(&self) -> bool {
        self.created_range.is_none()
            && self.created_after.is_none()
            && self.created_before.is_none()
            && self.updated_range.is_none()
            && self.updated_after.is_none()
            && self.updated_before.is_none()
            && self.accessed_range.is_none()
            && self.accessed_after.is_none()
            && self.accessed_before.is_none()
    }

    /// Check if there are any created time constraints.
    pub fn has_created_constraints(&self) -> bool {
        self.created_range.is_some()
            || self.created_after.is_some()
            || self.created_before.is_some()
    }

    /// Check if there are any updated time constraints.
    pub fn has_updated_constraints(&self) -> bool {
        self.updated_range.is_some()
            || self.updated_after.is_some()
            || self.updated_before.is_some()
    }

    /// Check if there are any accessed time constraints.
    pub fn has_accessed_constraints(&self) -> bool {
        self.accessed_range.is_some()
            || self.accessed_after.is_some()
            || self.accessed_before.is_some()
    }

    /// Get the effective created time boundaries, resolving named ranges.
    ///
    /// Returns `(after, before)` where each is optional.
    /// Explicit boundaries take precedence over named ranges.
    pub fn get_created_boundaries(&self) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        let from_range = self
            .created_range
            .and_then(|r| r.to_boundaries())
            .map(|(start, end)| (Some(start), Some(end)))
            .unwrap_or((None, None));

        (
            self.created_after.or(from_range.0),
            self.created_before.or(from_range.1),
        )
    }

    /// Get the effective updated time boundaries.
    pub fn get_updated_boundaries(&self) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        let from_range = self
            .updated_range
            .and_then(|r| r.to_boundaries())
            .map(|(start, end)| (Some(start), Some(end)))
            .unwrap_or((None, None));

        (
            self.updated_after.or(from_range.0),
            self.updated_before.or(from_range.1),
        )
    }

    /// Get the effective accessed time boundaries.
    pub fn get_accessed_boundaries(&self) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
        let from_range = self
            .accessed_range
            .and_then(|r| r.to_boundaries())
            .map(|(start, end)| (Some(start), Some(end)))
            .unwrap_or((None, None));

        (
            self.accessed_after.or(from_range.0),
            self.accessed_before.or(from_range.1),
        )
    }

    /// Get UUIDv7 boundaries for created time filtering.
    ///
    /// This enables efficient filtering using the primary key index.
    /// Returns `(floor_uuid, ceiling_uuid)` if both boundaries are set.
    pub fn get_created_uuid_boundaries(&self) -> (Option<Uuid>, Option<Uuid>) {
        let (after, before) = self.get_created_boundaries();
        (
            after.map(|t| v7_from_timestamp(&t)),
            before.map(|t| v7_ceiling_from_timestamp(&t)),
        )
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_range_boundaries() {
        let range = NamedTemporalRange::ThisWeek;
        let boundaries = range.to_boundaries();
        assert!(boundaries.is_some());

        let (start, end) = boundaries.unwrap();
        assert!(start < end);
        assert!(end <= Utc::now());
    }

    #[test]
    fn test_all_time_has_no_boundaries() {
        let range = NamedTemporalRange::AllTime;
        assert!(range.to_boundaries().is_none());
        assert!(range.to_uuid_boundaries().is_none());
    }

    #[test]
    fn test_named_range_uuid_boundaries() {
        let range = NamedTemporalRange::Today;
        let uuid_bounds = range.to_uuid_boundaries();
        assert!(uuid_bounds.is_some());

        let (floor, ceiling) = uuid_bounds.unwrap();
        assert!(floor < ceiling);
    }

    #[test]
    fn test_temporal_filter_is_empty() {
        let filter = StrictTemporalFilter::new();
        assert!(filter.is_empty());

        let filter = filter.created_within(NamedTemporalRange::ThisWeek);
        assert!(!filter.is_empty());
    }

    #[test]
    fn test_temporal_filter_created_boundaries() {
        let now = Utc::now();
        let week_ago = now - Duration::days(7);

        let filter = StrictTemporalFilter::new().created_between(week_ago, now);

        let (after, before) = filter.get_created_boundaries();
        assert_eq!(after, Some(week_ago));
        assert_eq!(before, Some(now));
    }

    #[test]
    fn test_temporal_filter_named_range_resolution() {
        let filter = StrictTemporalFilter::new().created_within(NamedTemporalRange::ThisWeek);

        let (after, before) = filter.get_created_boundaries();
        assert!(after.is_some());
        assert!(before.is_some());
        assert!(after.unwrap() < before.unwrap());
    }

    #[test]
    fn test_explicit_boundaries_override_named_range() {
        let explicit_start = Utc::now() - Duration::days(1);

        let filter = StrictTemporalFilter::new()
            .created_within(NamedTemporalRange::ThisMonth)
            .created_after(explicit_start);

        let (after, _) = filter.get_created_boundaries();
        assert_eq!(after, Some(explicit_start));
    }

    #[test]
    fn test_uuid_boundaries_generation() {
        let now = Utc::now();
        let week_ago = now - Duration::days(7);

        let filter = StrictTemporalFilter::new().created_between(week_ago, now);

        let (floor, ceiling) = filter.get_created_uuid_boundaries();
        assert!(floor.is_some());
        assert!(ceiling.is_some());
        assert!(floor.unwrap() < ceiling.unwrap());
    }

    #[test]
    fn test_builder_pattern_chaining() {
        let filter = StrictTemporalFilter::new()
            .created_within(NamedTemporalRange::ThisWeek)
            .updated_within(NamedTemporalRange::Today)
            .accessed_within(NamedTemporalRange::LastHour)
            .with_include_never_accessed(false);

        assert!(filter.has_created_constraints());
        assert!(filter.has_updated_constraints());
        assert!(filter.has_accessed_constraints());
        assert!(!filter.include_never_accessed);
    }
}
