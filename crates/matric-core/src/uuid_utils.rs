//! UUID v7 utilities for time-ordered identifiers.
//!
//! This module provides utilities for working with UUIDv7 identifiers,
//! which embed millisecond-precision timestamps enabling time-based ordering
//! and efficient temporal queries.
//!
//! # UUIDv7 Structure (RFC 9562)
//!
//! ```text
//! 0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                          unix_ts_ms                          |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |          unix_ts_ms           |  ver  |       rand_a         |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |var|                        rand_b                            |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                            rand_b                            |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

/// Generate a new UUIDv7 identifier.
///
/// UUIDv7 embeds a Unix timestamp (milliseconds) in the first 48 bits,
/// providing natural time-ordering and enabling efficient temporal queries.
///
/// # Example
///
/// ```
/// use matric_core::uuid_utils::new_v7;
///
/// let id = new_v7();
/// // IDs generated later will be lexicographically greater
/// ```
#[inline]
pub fn new_v7() -> Uuid {
    Uuid::now_v7()
}

/// Generate a UUIDv7 for a specific timestamp.
///
/// Useful for creating boundary UUIDs for temporal range queries.
/// The random bits are set to zeros for "floor" boundaries.
///
/// # Example
///
/// ```
/// use matric_core::uuid_utils::v7_from_timestamp;
/// use chrono::Utc;
///
/// let boundary = v7_from_timestamp(&Utc::now());
/// ```
pub fn v7_from_timestamp(ts: &DateTime<Utc>) -> Uuid {
    let millis = ts.timestamp_millis() as u64;
    v7_from_millis(millis)
}

/// Generate a UUIDv7 from raw milliseconds since Unix epoch.
///
/// Creates a "floor" UUID with zeros in random bits, suitable for
/// range query boundaries.
pub fn v7_from_millis(millis: u64) -> Uuid {
    // Build UUIDv7 bytes manually for boundary value
    // First 48 bits: milliseconds timestamp
    // Next 4 bits: version (0111 = 7)
    // Next 12 bits: rand_a (zeros for floor)
    // Next 2 bits: variant (10)
    // Remaining 62 bits: rand_b (zeros for floor)
    let bytes = [
        ((millis >> 40) & 0xFF) as u8,
        ((millis >> 32) & 0xFF) as u8,
        ((millis >> 24) & 0xFF) as u8,
        ((millis >> 16) & 0xFF) as u8,
        ((millis >> 8) & 0xFF) as u8,
        (millis & 0xFF) as u8,
        0x70, // Version 7 + 4 zero bits of rand_a
        0x00, // Remaining 8 bits of rand_a (zeros)
        0x80, // Variant (10) + 6 zero bits of rand_b
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00,
        0x00, // rand_b (zeros)
    ];
    Uuid::from_bytes(bytes)
}

/// Generate a UUIDv7 "ceiling" boundary for a timestamp.
///
/// Creates a UUID with maximum random bits for the given timestamp,
/// suitable for exclusive upper bounds in range queries.
pub fn v7_ceiling_from_timestamp(ts: &DateTime<Utc>) -> Uuid {
    let millis = ts.timestamp_millis() as u64;
    v7_ceiling_from_millis(millis)
}

/// Generate a UUIDv7 ceiling from raw milliseconds.
pub fn v7_ceiling_from_millis(millis: u64) -> Uuid {
    let bytes = [
        ((millis >> 40) & 0xFF) as u8,
        ((millis >> 32) & 0xFF) as u8,
        ((millis >> 24) & 0xFF) as u8,
        ((millis >> 16) & 0xFF) as u8,
        ((millis >> 8) & 0xFF) as u8,
        (millis & 0xFF) as u8,
        0x7F, // Version 7 + max rand_a bits
        0xFF, // Remaining rand_a (max)
        0xBF, // Variant (10) + max rand_b bits
        0xFF,
        0xFF,
        0xFF,
        0xFF,
        0xFF,
        0xFF,
        0xFF, // rand_b (max)
    ];
    Uuid::from_bytes(bytes)
}

/// Extract the timestamp from a UUIDv7.
///
/// Returns `None` if the UUID is not version 7.
///
/// # Example
///
/// ```
/// use matric_core::uuid_utils::{new_v7, extract_timestamp};
///
/// let id = new_v7();
/// let ts = extract_timestamp(&id).expect("should be v7");
/// ```
pub fn extract_timestamp(uuid: &Uuid) -> Option<DateTime<Utc>> {
    // Check version (bits 48-51 should be 0111)
    let bytes = uuid.as_bytes();
    if (bytes[6] >> 4) != 7 {
        return None;
    }

    // Extract 48-bit timestamp from first 6 bytes
    let millis = ((bytes[0] as u64) << 40)
        | ((bytes[1] as u64) << 32)
        | ((bytes[2] as u64) << 24)
        | ((bytes[3] as u64) << 16)
        | ((bytes[4] as u64) << 8)
        | (bytes[5] as u64);

    // Convert to DateTime
    Utc.timestamp_millis_opt(millis as i64).single()
}

/// Extract raw milliseconds from a UUIDv7.
///
/// Returns `None` if the UUID is not version 7.
pub fn extract_millis(uuid: &Uuid) -> Option<u64> {
    let bytes = uuid.as_bytes();
    if (bytes[6] >> 4) != 7 {
        return None;
    }

    Some(
        ((bytes[0] as u64) << 40)
            | ((bytes[1] as u64) << 32)
            | ((bytes[2] as u64) << 24)
            | ((bytes[3] as u64) << 16)
            | ((bytes[4] as u64) << 8)
            | (bytes[5] as u64),
    )
}

/// Check if a UUID is version 7.
#[inline]
pub fn is_v7(uuid: &Uuid) -> bool {
    uuid.get_version_num() == 7
}

/// Check if a UUID is version 4 (random).
#[inline]
pub fn is_v4(uuid: &Uuid) -> bool {
    uuid.get_version_num() == 4
}

/// Generate UUIDv7 boundaries for a time range.
///
/// Returns (floor_uuid, ceiling_uuid) suitable for SQL range queries:
/// `WHERE id >= floor AND id < ceiling`
pub fn range_boundaries(start: &DateTime<Utc>, end: &DateTime<Utc>) -> (Uuid, Uuid) {
    (v7_from_timestamp(start), v7_ceiling_from_timestamp(end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_new_v7_is_version_7() {
        let id = new_v7();
        assert!(is_v7(&id));
        assert!(!is_v4(&id));
    }

    #[test]
    fn test_v7_ordering() {
        let id1 = new_v7();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = new_v7();

        // Later UUIDs should be greater
        assert!(id2 > id1);
    }

    #[test]
    fn test_timestamp_extraction() {
        let before = Utc::now();
        let id = new_v7();
        let after = Utc::now();

        let extracted = extract_timestamp(&id).expect("should extract timestamp");

        // Extracted timestamp should be between before and after
        assert!(extracted >= before - Duration::milliseconds(1));
        assert!(extracted <= after + Duration::milliseconds(1));
    }

    #[test]
    fn test_v7_from_timestamp() {
        let ts = Utc::now();
        let id = v7_from_timestamp(&ts);

        assert!(is_v7(&id));

        let extracted = extract_timestamp(&id).expect("should extract");
        // Should match to millisecond precision
        assert_eq!(ts.timestamp_millis(), extracted.timestamp_millis());
    }

    #[test]
    fn test_range_boundaries() {
        let start = Utc::now();
        let end = start + Duration::hours(1);

        let (floor, ceiling) = range_boundaries(&start, &end);

        assert!(is_v7(&floor));
        assert!(is_v7(&ceiling));
        assert!(ceiling > floor);

        // IDs generated between start and end should be in range
        std::thread::sleep(std::time::Duration::from_millis(1));
        let mid = new_v7();
        assert!(mid >= floor);
        // Note: mid might not be < ceiling since we're still before 'end'
    }

    #[test]
    fn test_v4_detection() {
        let v4_id = Uuid::new_v4();
        assert!(is_v4(&v4_id));
        assert!(!is_v7(&v4_id));
        assert!(extract_timestamp(&v4_id).is_none());
    }

    #[test]
    fn test_millis_roundtrip() {
        let original_millis = 1706000000000u64; // Some fixed timestamp
        let id = v7_from_millis(original_millis);
        let extracted = extract_millis(&id).expect("should extract millis");
        assert_eq!(original_millis, extracted);
    }

    #[test]
    fn test_floor_ceiling_ordering() {
        let ts = Utc::now();
        let floor = v7_from_timestamp(&ts);
        let ceiling = v7_ceiling_from_timestamp(&ts);

        // Ceiling should be greater than floor for same timestamp
        assert!(ceiling > floor);
    }
}
