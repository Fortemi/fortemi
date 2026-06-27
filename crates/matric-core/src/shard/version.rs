//! Shard format versioning.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Current shard format version.
pub const CURRENT_SHARD_VERSION: &str = "1.0.0";

/// Semantic version for shard format compatibility checking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// Parse a version string (e.g., "1.0.0").
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!(
                "Invalid version format; value_len={}",
                s.chars().count()
            ));
        }

        let major = parts[0].parse::<u64>().map_err(|_| {
            format!(
                "Invalid major version; component_len={}",
                parts[0].chars().count()
            )
        })?;
        let minor = parts[1].parse::<u64>().map_err(|_| {
            format!(
                "Invalid minor version; component_len={}",
                parts[1].chars().count()
            )
        })?;
        let patch = parts[2].parse::<u64>().map_err(|_| {
            format!(
                "Invalid patch version; component_len={}",
                parts[2].chars().count()
            )
        })?;

        Ok(Version {
            major,
            minor,
            patch,
        })
    }

    /// Check if this version is compatible with another version.
    /// Compatible means same major version and this version >= other version.
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        self.major == other.major && self >= other
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parse_valid() {
        let v = Version::parse("1.0.0").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);

        let v = Version::parse("2.3.4").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 3);
        assert_eq!(v.patch, 4);
    }

    #[test]
    fn test_version_parse_invalid() {
        assert!(Version::parse("1.0").is_err());
        assert!(Version::parse("1.0.0.0").is_err());
        assert!(Version::parse("a.b.c").is_err());
        assert!(Version::parse("1.0.x").is_err());
    }

    #[test]
    fn version_parse_errors_report_lengths_without_raw_values() {
        for raw in [
            "postgres://admin:秘密@db.internal/fortemi",
            "sk-live-秘密.1.0",
            "1.customer-秘密@example.com.0",
            "1.0./srv/private/秘密-path",
        ] {
            let err = Version::parse(raw).unwrap_err();
            assert!(err.contains("Invalid"));
            assert!(
                err.contains("value_len=") || err.contains("component_len="),
                "parse error should retain only length metadata: {err}"
            );
            assert!(
                err.contains("value_len=39")
                    || err.contains("component_len=10")
                    || err.contains("value_len=27")
                    || err.contains("component_len=20"),
                "parse error should use Unicode character-count lengths: {err}"
            );
            assert!(
                !err.contains(raw),
                "version parse error leaked raw input: {err}"
            );
            for fragment in [
                "postgres://",
                "admin:秘密",
                "db.internal",
                "sk-live",
                "customer-秘密@example.com",
                "秘密",
                "/srv/private",
            ] {
                assert!(
                    !err.contains(fragment),
                    "version parse error leaked raw fragment {fragment}: {err}"
                );
            }
        }
    }

    #[test]
    fn test_version_ordering() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.0.1").unwrap();
        let v3 = Version::parse("1.1.0").unwrap();
        let v4 = Version::parse("2.0.0").unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
        assert!(v1 < v4);
    }

    #[test]
    fn test_version_compatibility() {
        let v1_0_0 = Version::parse("1.0.0").unwrap();
        let v1_0_1 = Version::parse("1.0.1").unwrap();
        let v1_1_0 = Version::parse("1.1.0").unwrap();
        let v2_0_0 = Version::parse("2.0.0").unwrap();

        // Same version is compatible
        assert!(v1_0_0.is_compatible_with(&v1_0_0));

        // Newer patch is compatible
        assert!(v1_0_1.is_compatible_with(&v1_0_0));

        // Newer minor is compatible
        assert!(v1_1_0.is_compatible_with(&v1_0_0));

        // Older version is not compatible
        assert!(!v1_0_0.is_compatible_with(&v1_0_1));

        // Different major version is not compatible
        assert!(!v2_0_0.is_compatible_with(&v1_0_0));
        assert!(!v1_0_0.is_compatible_with(&v2_0_0));
    }

    #[test]
    fn test_version_display() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.to_string(), "1.2.3");
    }

    #[test]
    fn test_current_version_is_valid() {
        let current = Version::parse(CURRENT_SHARD_VERSION).unwrap();
        assert_eq!(current.major, 1);
        assert_eq!(current.minor, 0);
        assert_eq!(current.patch, 0);
    }
}
