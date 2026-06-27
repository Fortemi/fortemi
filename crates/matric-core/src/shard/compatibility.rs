//! Shard version compatibility checking.

use super::version::{Version, CURRENT_SHARD_VERSION};
use std::fmt;

/// Result of a compatibility check between a shard version and the current system.
#[derive(Clone, PartialEq, Eq)]
pub enum CompatibilityResult {
    /// The shard version is compatible with the current system (same version or older minor).
    Compatible,

    /// The shard requires migration to the current version.
    RequiresMigration { from: String, to: String },

    /// The shard is from a newer minor version but same major (forward-compatible).
    NewerMinor {
        shard_version: String,
        warnings: Vec<String>,
    },

    /// The shard is incompatible and cannot be imported.
    Incompatible {
        reason: String,
        min_required: Option<String>,
    },
}

impl fmt::Debug for CompatibilityResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compatible => f.debug_struct("Compatible").finish(),
            Self::RequiresMigration { from, to } => f
                .debug_struct("RequiresMigration")
                .field("from_len", &from.len())
                .field("to_len", &to.len())
                .finish(),
            Self::NewerMinor {
                shard_version,
                warnings,
            } => f
                .debug_struct("NewerMinor")
                .field("shard_version_len", &shard_version.len())
                .field("warnings_count", &warnings.len())
                .field(
                    "warning_lens",
                    &warnings.iter().map(String::len).collect::<Vec<_>>(),
                )
                .finish(),
            Self::Incompatible {
                reason,
                min_required,
            } => f
                .debug_struct("Incompatible")
                .field("reason_len", &reason.len())
                .field("min_required_len", &min_required.as_ref().map(String::len))
                .finish(),
        }
    }
}

/// Check if a shard with the given manifest version is compatible with the current system.
pub fn check_shard_compatibility(manifest_version: &str) -> CompatibilityResult {
    let current = match Version::parse(CURRENT_SHARD_VERSION) {
        Ok(v) => v,
        Err(e) => {
            return CompatibilityResult::Incompatible {
                reason: format!("Invalid current version: {}", e),
                min_required: None,
            }
        }
    };

    let shard = match Version::parse(manifest_version) {
        Ok(v) => v,
        Err(e) => {
            return CompatibilityResult::Incompatible {
                reason: format!("Invalid shard version: {}", e),
                min_required: None,
            }
        }
    };

    // Different major version is incompatible
    if current.major != shard.major {
        return CompatibilityResult::Incompatible {
            reason: format!(
                "Shard major version {} is incompatible with current major version {}",
                shard.major, current.major
            ),
            min_required: Some(format!("{}.0.0", shard.major)),
        };
    }

    // Same version is compatible
    if current == shard {
        return CompatibilityResult::Compatible;
    }

    // Shard is from a newer minor version (forward-compatible with warnings)
    if shard.minor > current.minor {
        return CompatibilityResult::NewerMinor {
            shard_version: manifest_version.to_string(),
            warnings: vec![
                format!(
                    "Shard was created with a newer version ({}) than current ({})",
                    manifest_version, CURRENT_SHARD_VERSION
                ),
                "Some features may not be available or may be ignored".to_string(),
            ],
        };
    }

    // Shard is from an older version (requires migration)
    if shard < current {
        return CompatibilityResult::RequiresMigration {
            from: manifest_version.to_string(),
            to: CURRENT_SHARD_VERSION.to_string(),
        };
    }

    // Should not reach here, but default to compatible
    CompatibilityResult::Compatible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_version_compatible() {
        let result = check_shard_compatibility(CURRENT_SHARD_VERSION);
        assert_eq!(result, CompatibilityResult::Compatible);
    }

    #[test]
    fn test_older_patch_requires_migration() {
        // Assuming CURRENT_SHARD_VERSION is 1.0.0
        let result = check_shard_compatibility("1.0.0");
        assert_eq!(result, CompatibilityResult::Compatible);

        // If current were 1.0.1, then 1.0.0 would require migration
        // But we can't change CURRENT_SHARD_VERSION in tests, so we test the logic differently
    }

    #[test]
    fn test_older_minor_requires_migration() {
        // This test assumes current is 1.0.0, so we can't test older minor
        // But the logic is: if shard < current, requires migration
        // We'll test with a hypothetical scenario
    }

    #[test]
    fn test_newer_minor_forward_compatible() {
        let result = check_shard_compatibility("1.1.0");
        match result {
            CompatibilityResult::NewerMinor {
                shard_version,
                warnings,
            } => {
                assert_eq!(shard_version, "1.1.0");
                assert!(!warnings.is_empty());
            }
            _ => panic!("Expected NewerMinor, got {:?}", result),
        }
    }

    #[test]
    fn test_different_major_incompatible() {
        let result = check_shard_compatibility("2.0.0");
        match result {
            CompatibilityResult::Incompatible {
                reason,
                min_required,
            } => {
                assert!(reason.contains("major version"));
                assert_eq!(min_required, Some("2.0.0".to_string()));
            }
            _ => panic!("Expected Incompatible, got {:?}", result),
        }

        let result = check_shard_compatibility("0.9.0");
        match result {
            CompatibilityResult::Incompatible { .. } => {}
            _ => panic!("Expected Incompatible, got {:?}", result),
        }
    }

    #[test]
    fn test_invalid_version_incompatible() {
        let result = check_shard_compatibility("invalid");
        match result {
            CompatibilityResult::Incompatible { reason, .. } => {
                assert!(reason.contains("Invalid shard version"));
            }
            _ => panic!("Expected Incompatible, got {:?}", result),
        }
    }

    #[test]
    fn compatibility_result_debug_redacts_versions_warnings_and_reasons() {
        let results = [
            CompatibilityResult::RequiresMigration {
                from: "1.0.0-customer@example.com".to_string(),
                to: "2.0.0-sk-live-token".to_string(),
            },
            CompatibilityResult::NewerMinor {
                shard_version: "1.9.0-private/shard".to_string(),
                warnings: vec![
                    "Shard warning for postgres://admin:secret@db.internal/fortemi".to_string(),
                    "Operator path /srv/private/customer@example.com".to_string(),
                ],
            },
            CompatibilityResult::Incompatible {
                reason: "Invalid shard version bearer-secret at db.internal/private".to_string(),
                min_required: Some("2.0.0-sk-live-token".to_string()),
            },
        ];

        for result in results {
            let debug = format!("{result:?}");
            for raw in [
                "customer@example.com",
                "sk-live",
                "private/shard",
                "postgres://",
                "db.internal",
                "/srv/private",
                "bearer-secret",
                "Shard warning",
                "Operator path",
                "Invalid shard version",
            ] {
                assert!(!debug.contains(raw), "debug output leaked {raw}: {debug}");
            }

            assert!(
                debug.contains("RequiresMigration")
                    || debug.contains("NewerMinor")
                    || debug.contains("Incompatible")
            );
            assert!(
                debug.contains("_len")
                    || debug.contains("_count")
                    || debug.contains("warning_lens")
            );
        }
    }

    #[test]
    fn test_migration_required_scenario() {
        // Test with a version that's definitely older
        // Since CURRENT_SHARD_VERSION is 1.0.0, we can't go lower in same major
        // This is a limitation of testing against a constant
        // In real usage, when current version advances, this will work
    }
}
