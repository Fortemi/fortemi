//! Custom query parameter types with improved error handling
//!
//! This module provides wrapper types for query parameters that give
//! user-friendly error messages instead of cryptic deserialization failures.

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{de, Deserialize, Deserializer};
use std::fmt;
use std::ops::Deref;

/// A DateTime type that provides helpful error messages when parsing fails.
///
/// Accepts:
/// - RFC 3339 with timezone: `2024-01-15T10:30:00Z`
/// - RFC 3339 with offset: `2024-01-15T10:30:00+00:00`
/// - ISO 8601 without timezone (assumes UTC): `2024-01-15T10:30:00`
/// - Date only (assumes midnight UTC): `2024-01-15`
///
/// # Example
///
/// ```rust,ignore
/// use matric_api::query_types::FlexibleDateTime;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Query {
///     created_after: Option<FlexibleDateTime>,
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlexibleDateTime(pub DateTime<Utc>);

impl FlexibleDateTime {
    /// Returns the inner DateTime<Utc>
    pub fn into_inner(self) -> DateTime<Utc> {
        self.0
    }
}

impl Deref for FlexibleDateTime {
    type Target = DateTime<Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<FlexibleDateTime> for DateTime<Utc> {
    fn from(dt: FlexibleDateTime) -> Self {
        dt.0
    }
}

impl fmt::Display for FlexibleDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for FlexibleDateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_flexible_datetime(&s).map_err(de::Error::custom)
    }
}

/// Parse a datetime string with multiple format support and helpful errors.
fn parse_flexible_datetime(s: &str) -> Result<FlexibleDateTime, String> {
    let s = s.trim();

    if s.is_empty() {
        return Err(
            "Date value cannot be empty. Expected ISO 8601 format (e.g., '2024-01-15T10:30:00Z')"
                .to_string(),
        );
    }

    // Try RFC 3339 first (with timezone)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(FlexibleDateTime(dt.with_timezone(&Utc)));
    }

    // Try ISO 8601 without timezone - assume UTC
    // Format: 2024-01-15T10:30:00
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(FlexibleDateTime(naive.and_utc()));
    }

    // Try with fractional seconds
    // Format: 2024-01-15T10:30:00.123
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Ok(FlexibleDateTime(naive.and_utc()));
    }

    // Try date only - assume midnight UTC
    // Format: 2024-01-15
    if let Ok(naive) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = naive
            .and_hms_opt(0, 0, 0)
            .map(|n| n.and_utc())
            .ok_or_else(|| "Failed to create datetime from date".to_string())?;
        return Ok(FlexibleDateTime(dt));
    }

    // Try space-separated format (some clients use this)
    // Format: 2024-01-15 10:30:00Z
    let normalized = s.replace(' ', "T");
    if let Ok(dt) = DateTime::parse_from_rfc3339(&normalized) {
        return Ok(FlexibleDateTime(dt.with_timezone(&Utc)));
    }

    // None of the formats worked - provide helpful error message
    Err(format!(
        "Invalid date format: '{}'. Expected ISO 8601 format. \
        Examples: '2024-01-15T10:30:00Z' (with timezone), \
        '2024-01-15T10:30:00' (assumes UTC), \
        '2024-01-15' (date only, midnight UTC)",
        s
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc3339_with_z() {
        let result = parse_flexible_datetime("2024-01-15T10:30:00Z");
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.0.to_rfc3339(), "2024-01-15T10:30:00+00:00");
    }

    #[test]
    fn test_rfc3339_with_offset() {
        let result = parse_flexible_datetime("2024-01-15T10:30:00+05:00");
        assert!(result.is_ok());
        let dt = result.unwrap();
        // Should be converted to UTC
        assert_eq!(dt.0.to_rfc3339(), "2024-01-15T05:30:00+00:00");
    }

    #[test]
    fn test_without_timezone() {
        let result = parse_flexible_datetime("2024-01-15T10:30:00");
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.0.to_rfc3339(), "2024-01-15T10:30:00+00:00");
    }

    #[test]
    fn test_date_only() {
        let result = parse_flexible_datetime("2024-01-15");
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.0.to_rfc3339(), "2024-01-15T00:00:00+00:00");
    }

    #[test]
    fn test_space_separated() {
        let result = parse_flexible_datetime("2024-01-15 10:30:00Z");
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_format() {
        let result = parse_flexible_datetime("invalid-date");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid date format"));
        assert!(err.contains("Examples:"));
    }

    #[test]
    fn test_empty_string() {
        let result = parse_flexible_datetime("");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("cannot be empty"));
    }

    #[test]
    fn test_fractional_seconds() {
        let result = parse_flexible_datetime("2024-01-15T10:30:00.123");
        assert!(result.is_ok());
    }
}
