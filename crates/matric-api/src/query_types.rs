//! Custom query parameter types with improved error handling
//!
//! This module provides wrapper types for query parameters that give
//! user-friendly error messages instead of cryptic deserialization failures.

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Utc};
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
/// - Relative shorthand: `7d`, `1w`, `2h`, `30min`
/// - Natural language: `now`, `today`, `yesterday`, `tomorrow`
/// - Named periods: `last week`, `last month`, `last year`, `this week`, `this month`, `this year`
/// - N units ago: `3 days ago`, `2 weeks ago`, `1 hour ago`
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
            "Date value cannot be empty. Expected ISO 8601 format (e.g., '2024-01-15T10:30:00Z') or natural language (e.g., 'now', '7d', 'yesterday')"
                .to_string(),
        );
    }

    let s_lower = s.to_lowercase();

    // Natural language keywords
    match s_lower.as_str() {
        "now" => {
            return Ok(FlexibleDateTime(Utc::now()));
        }
        "today" => {
            let now = Utc::now();
            let start_of_day = now
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|n| n.and_utc())
                .ok_or_else(|| "Failed to create start of today".to_string())?;
            return Ok(FlexibleDateTime(start_of_day));
        }
        "yesterday" => {
            let now = Utc::now();
            let yesterday_date = now.date_naive() - Duration::days(1);
            let start_of_yesterday = yesterday_date
                .and_hms_opt(0, 0, 0)
                .map(|n| n.and_utc())
                .ok_or_else(|| "Failed to create start of yesterday".to_string())?;
            return Ok(FlexibleDateTime(start_of_yesterday));
        }
        "tomorrow" => {
            let now = Utc::now();
            let tomorrow_date = now.date_naive() + Duration::days(1);
            let start_of_tomorrow = tomorrow_date
                .and_hms_opt(0, 0, 0)
                .map(|n| n.and_utc())
                .ok_or_else(|| "Failed to create start of tomorrow".to_string())?;
            return Ok(FlexibleDateTime(start_of_tomorrow));
        }
        "last week" => {
            return Ok(FlexibleDateTime(Utc::now() - Duration::weeks(1)));
        }
        "last month" => {
            return Ok(FlexibleDateTime(Utc::now() - Duration::days(30)));
        }
        "last year" => {
            return Ok(FlexibleDateTime(Utc::now() - Duration::days(365)));
        }
        "this week" => {
            // Start of current Monday
            let now = Utc::now();
            let weekday = now.weekday().num_days_from_monday();
            let monday = now.date_naive() - Duration::days(weekday as i64);
            let start_of_week = monday
                .and_hms_opt(0, 0, 0)
                .map(|n| n.and_utc())
                .ok_or_else(|| "Failed to create start of week".to_string())?;
            return Ok(FlexibleDateTime(start_of_week));
        }
        "this month" => {
            // Start of current month
            let now = Utc::now();
            let start_of_month = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|n| n.and_utc())
                .ok_or_else(|| "Failed to create start of month".to_string())?;
            return Ok(FlexibleDateTime(start_of_month));
        }
        "this year" => {
            // Start of current year
            let now = Utc::now();
            let start_of_year = chrono::NaiveDate::from_ymd_opt(now.year(), 1, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|n| n.and_utc())
                .ok_or_else(|| "Failed to create start of year".to_string())?;
            return Ok(FlexibleDateTime(start_of_year));
        }
        "day before yesterday" => {
            let now = Utc::now();
            let dby_date = now.date_naive() - Duration::days(2);
            let start_of_dby = dby_date
                .and_hms_opt(0, 0, 0)
                .map(|n| n.and_utc())
                .ok_or_else(|| "Failed to create day before yesterday".to_string())?;
            return Ok(FlexibleDateTime(start_of_dby));
        }
        _ => {}
    }

    // "N units ago" pattern - e.g., "3 days ago", "2 weeks ago"
    if s_lower.ends_with(" ago") {
        let without_ago = s_lower.trim_end_matches(" ago").trim();
        let parts: Vec<&str> = without_ago.split_whitespace().collect();

        if parts.len() == 2 {
            if let Ok(num) = parts[0].parse::<i64>() {
                let unit = parts[1];
                let duration = match unit {
                    "hour" | "hours" => Duration::hours(num),
                    "day" | "days" => Duration::days(num),
                    "week" | "weeks" => Duration::weeks(num),
                    "month" | "months" => Duration::days(num * 30),
                    "minute" | "minutes" => Duration::minutes(num),
                    _ => {
                        return Err(format!(
                            "Unrecognized time unit in '{}'. Supported units: hour, day, week, month, minute",
                            s
                        ));
                    }
                };
                return Ok(FlexibleDateTime(Utc::now() - duration));
            }
        }
    }

    // Relative shorthand: 7d, 1w, 2h, 30min
    if let Some(dt) = parse_relative_shorthand(&s_lower) {
        return Ok(FlexibleDateTime(dt));
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
        "Invalid date format: '{}'. Expected ISO 8601 format or natural language. \
        Examples: '2024-01-15T10:30:00Z' (with timezone), \
        '2024-01-15T10:30:00' (assumes UTC), \
        '2024-01-15' (date only, midnight UTC), \
        '7d' (7 days ago), 'yesterday', 'last week', '3 days ago'",
        s
    ))
}

/// Parse relative time shorthand (e.g., "7d", "1w", "2h", "30min") into a DateTime.
fn parse_relative_shorthand(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Parse the number and unit
    let mut num_str = String::new();
    let mut unit = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num_str.push(c);
        } else {
            unit.push(c);
        }
    }

    let num: i64 = num_str.parse().ok()?;
    let duration = match unit.as_str() {
        "h" | "hr" | "hrs" | "hour" | "hours" => Duration::hours(num),
        "d" | "day" | "days" => Duration::days(num),
        "w" | "wk" | "week" | "weeks" => Duration::weeks(num),
        "m" | "mo" | "month" | "months" => Duration::days(num * 30),
        "min" | "mins" | "minute" | "minutes" => Duration::minutes(num),
        _ => return None,
    };

    Some(Utc::now() - duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    // =========================================================================
    // Existing ISO 8601 format tests
    // =========================================================================

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

    #[test]
    fn test_existing_iso_still_works() {
        let result = parse_flexible_datetime("2024-01-15T10:30:00Z");
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.0.to_rfc3339(), "2024-01-15T10:30:00+00:00");
    }

    #[test]
    fn test_existing_date_only_still_works() {
        let result = parse_flexible_datetime("2024-01-15");
        assert!(result.is_ok());
        let dt = result.unwrap();
        assert_eq!(dt.0.to_rfc3339(), "2024-01-15T00:00:00+00:00");
    }

    // =========================================================================
    // Relative shorthand tests
    // =========================================================================

    #[test]
    fn test_relative_shorthand_days() {
        let result = parse_flexible_datetime("7d");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::days(7);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed time should be within 2 seconds of expected"
        );
    }

    #[test]
    fn test_relative_shorthand_weeks() {
        let result = parse_flexible_datetime("1w");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::weeks(1);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed time should be within 2 seconds of expected"
        );
    }

    #[test]
    fn test_relative_shorthand_hours() {
        let result = parse_flexible_datetime("2h");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::hours(2);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed time should be within 2 seconds of expected"
        );
    }

    #[test]
    fn test_relative_shorthand_minutes() {
        let result = parse_flexible_datetime("30min");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::minutes(30);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed time should be within 2 seconds of expected"
        );
    }

    // =========================================================================
    // Natural language keyword tests
    // =========================================================================

    #[test]
    fn test_now_keyword() {
        let result = parse_flexible_datetime("now");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let now = Utc::now();
        let diff = (dt.0 - now).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed 'now' should be within 2 seconds of current time"
        );
    }

    #[test]
    fn test_today_keyword() {
        let result = parse_flexible_datetime("today");
        assert!(result.is_ok());
        let dt = result.unwrap();

        // Should be midnight UTC today
        let now = Utc::now();
        let expected = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();

        assert_eq!(dt.0, expected);
        assert_eq!(dt.0.hour(), 0);
        assert_eq!(dt.0.minute(), 0);
        assert_eq!(dt.0.second(), 0);
    }

    #[test]
    fn test_yesterday_keyword() {
        let result = parse_flexible_datetime("yesterday");
        assert!(result.is_ok());
        let dt = result.unwrap();

        // Should be midnight UTC yesterday
        let now = Utc::now();
        let yesterday_date = now.date_naive() - Duration::days(1);
        let expected = yesterday_date.and_hms_opt(0, 0, 0).unwrap().and_utc();

        assert_eq!(dt.0, expected);
        assert_eq!(dt.0.hour(), 0);
        assert_eq!(dt.0.minute(), 0);
        assert_eq!(dt.0.second(), 0);
    }

    #[test]
    fn test_tomorrow_keyword() {
        let result = parse_flexible_datetime("tomorrow");
        assert!(result.is_ok());
        let dt = result.unwrap();

        // Should be midnight UTC tomorrow
        let now = Utc::now();
        let tomorrow_date = now.date_naive() + Duration::days(1);
        let expected = tomorrow_date.and_hms_opt(0, 0, 0).unwrap().and_utc();

        assert_eq!(dt.0, expected);
        assert_eq!(dt.0.hour(), 0);
        assert_eq!(dt.0.minute(), 0);
        assert_eq!(dt.0.second(), 0);
    }

    // =========================================================================
    // Named relative period tests
    // =========================================================================

    #[test]
    fn test_last_week() {
        let result = parse_flexible_datetime("last week");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::weeks(1);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed 'last week' should be approximately 7 days ago"
        );
    }

    #[test]
    fn test_last_month() {
        let result = parse_flexible_datetime("last month");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::days(30);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed 'last month' should be approximately 30 days ago"
        );
    }

    #[test]
    fn test_last_year() {
        let result = parse_flexible_datetime("last year");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::days(365);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed 'last year' should be approximately 365 days ago"
        );
    }

    #[test]
    fn test_this_week() {
        let result = parse_flexible_datetime("this week");
        assert!(result.is_ok());
        let dt = result.unwrap();

        // Should be start of current Monday at midnight
        let now = Utc::now();
        let weekday = now.weekday().num_days_from_monday();
        let monday = now.date_naive() - Duration::days(weekday as i64);
        let expected = monday.and_hms_opt(0, 0, 0).unwrap().and_utc();

        assert_eq!(dt.0, expected);
        assert_eq!(dt.0.weekday().num_days_from_monday(), 0); // Monday
        assert_eq!(dt.0.hour(), 0);
        assert_eq!(dt.0.minute(), 0);
        assert_eq!(dt.0.second(), 0);
    }

    #[test]
    fn test_this_month() {
        let result = parse_flexible_datetime("this month");
        assert!(result.is_ok());
        let dt = result.unwrap();

        // Should be start of current month at midnight
        let now = Utc::now();
        let expected = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        assert_eq!(dt.0, expected);
        assert_eq!(dt.0.day(), 1);
        assert_eq!(dt.0.hour(), 0);
        assert_eq!(dt.0.minute(), 0);
        assert_eq!(dt.0.second(), 0);
    }

    #[test]
    fn test_this_year() {
        let result = parse_flexible_datetime("this year");
        assert!(result.is_ok());
        let dt = result.unwrap();

        // Should be January 1st of current year at midnight
        let now = Utc::now();
        let expected = chrono::NaiveDate::from_ymd_opt(now.year(), 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        assert_eq!(dt.0, expected);
        assert_eq!(dt.0.month(), 1);
        assert_eq!(dt.0.day(), 1);
        assert_eq!(dt.0.hour(), 0);
        assert_eq!(dt.0.minute(), 0);
        assert_eq!(dt.0.second(), 0);
    }

    #[test]
    fn test_day_before_yesterday() {
        let result = parse_flexible_datetime("day before yesterday");
        assert!(result.is_ok());
        let dt = result.unwrap();

        // Should be midnight UTC 2 days ago
        let now = Utc::now();
        let dby_date = now.date_naive() - Duration::days(2);
        let expected = dby_date.and_hms_opt(0, 0, 0).unwrap().and_utc();

        assert_eq!(dt.0, expected);
        assert_eq!(dt.0.hour(), 0);
        assert_eq!(dt.0.minute(), 0);
        assert_eq!(dt.0.second(), 0);
    }

    // =========================================================================
    // "N units ago" pattern tests
    // =========================================================================

    #[test]
    fn test_n_days_ago() {
        let result = parse_flexible_datetime("3 days ago");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::days(3);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed '3 days ago' should be within 2 seconds of expected"
        );
    }

    #[test]
    fn test_n_weeks_ago() {
        let result = parse_flexible_datetime("2 weeks ago");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::weeks(2);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed '2 weeks ago' should be within 2 seconds of expected"
        );
    }

    #[test]
    fn test_n_hours_ago() {
        let result = parse_flexible_datetime("1 hour ago");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::hours(1);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed '1 hour ago' should be within 2 seconds of expected"
        );
    }

    #[test]
    fn test_n_minutes_ago() {
        let result = parse_flexible_datetime("45 minutes ago");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::minutes(45);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed '45 minutes ago' should be within 2 seconds of expected"
        );
    }

    #[test]
    fn test_n_months_ago() {
        let result = parse_flexible_datetime("6 months ago");
        assert!(result.is_ok());
        let dt = result.unwrap();
        let expected = Utc::now() - Duration::days(6 * 30);
        let diff = (dt.0 - expected).num_seconds().abs();
        assert!(
            diff < 2,
            "Parsed '6 months ago' should be within 2 seconds of expected"
        );
    }
}
