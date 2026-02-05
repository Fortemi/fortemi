//! Feature flags for full-text search (FTS) capabilities.
//!
//! This module provides runtime feature flags to control the rollout of FTS enhancements
//! across three phases:
//!
//! - **Phase 1**: websearch_to_tsquery migration (deployed)
//! - **Phase 2**: Trigram fallback and script detection (deployed)
//! - **Phase 3**: CJK bigram tokenization and multilingual configs (deployed)
//!
//! All features are enabled by default. Flags can be disabled via environment variables
//! if issues are detected, allowing easy rollback.

use std::env;

/// Feature flags controlling FTS behavior.
///
/// # Example
/// ```
/// use matric_search::fts_flags::FtsFeatureFlags;
///
/// let flags = FtsFeatureFlags::default();
/// assert!(flags.is_phase1_enabled());
/// assert!(flags.is_phase2_enabled());
/// assert!(flags.is_phase3_enabled());
///
/// let all_flags = FtsFeatureFlags::all_enabled();
/// assert!(all_flags.is_phase3_enabled());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsFeatureFlags {
    /// Enable websearch_to_tsquery for natural query parsing (Phase 1).
    /// Default: true (deployed and stable).
    pub websearch_to_tsquery: bool,

    /// Enable trigram fallback for queries with no stemmed results (Phase 2).
    /// Default: true (deployed and stable).
    pub trigram_fallback: bool,

    /// Enable CJK bigram tokenization for Chinese/Japanese/Korean (Phase 3).
    /// Default: true (deployed and stable).
    pub bigram_cjk: bool,

    /// Enable script detection for automatic language routing (Phase 2).
    /// Default: true (deployed and stable).
    pub script_detection: bool,

    /// Enable multilingual PostgreSQL text search configs (Phase 3).
    /// Default: true (deployed and stable).
    pub multilingual_configs: bool,
}

impl Default for FtsFeatureFlags {
    /// Returns default flags with all features enabled.
    ///
    /// All phases (1-3) are enabled by default, as they have been
    /// deployed and validated. Use environment variables to disable
    /// specific features if needed.
    fn default() -> Self {
        Self {
            websearch_to_tsquery: true,
            trigram_fallback: true,
            bigram_cjk: true,
            script_detection: true,
            multilingual_configs: true,
        }
    }
}

impl FtsFeatureFlags {
    /// Constructs flags from environment variables.
    ///
    /// Environment variables:
    /// - `FTS_WEBSEARCH_TO_TSQUERY` (default: true)
    /// - `FTS_TRIGRAM_FALLBACK` (default: true)
    /// - `FTS_BIGRAM_CJK` (default: true)
    /// - `FTS_SCRIPT_DETECTION` (default: true)
    /// - `FTS_MULTILINGUAL_CONFIGS` (default: true)
    ///
    /// Values are parsed as booleans: "true", "1", "yes", "on" (case-insensitive) are truthy.
    ///
    /// # Example
    /// ```no_run
    /// use matric_search::fts_flags::FtsFeatureFlags;
    ///
    /// std::env::set_var("FTS_TRIGRAM_FALLBACK", "false");
    /// let flags = FtsFeatureFlags::from_env();
    /// assert!(!flags.trigram_fallback);
    /// ```
    pub fn from_env() -> Self {
        Self {
            websearch_to_tsquery: parse_bool_env("FTS_WEBSEARCH_TO_TSQUERY", true),
            trigram_fallback: parse_bool_env("FTS_TRIGRAM_FALLBACK", true),
            bigram_cjk: parse_bool_env("FTS_BIGRAM_CJK", true),
            script_detection: parse_bool_env("FTS_SCRIPT_DETECTION", true),
            multilingual_configs: parse_bool_env("FTS_MULTILINGUAL_CONFIGS", true),
        }
    }

    /// Returns true if Phase 1 features are enabled.
    ///
    /// Phase 1: websearch_to_tsquery for natural query parsing.
    #[inline]
    pub fn is_phase1_enabled(&self) -> bool {
        self.websearch_to_tsquery
    }

    /// Returns true if Phase 2 features are fully enabled.
    ///
    /// Phase 2: Trigram fallback + script detection.
    #[inline]
    pub fn is_phase2_enabled(&self) -> bool {
        self.trigram_fallback && self.script_detection
    }

    /// Returns true if Phase 3 features are fully enabled.
    ///
    /// Phase 3: CJK bigram tokenization + multilingual configs.
    #[inline]
    pub fn is_phase3_enabled(&self) -> bool {
        self.bigram_cjk && self.multilingual_configs
    }

    /// Returns a configuration with all flags enabled.
    ///
    /// Useful for testing or environments where all FTS features should be active.
    #[inline]
    pub fn all_enabled() -> Self {
        Self {
            websearch_to_tsquery: true,
            trigram_fallback: true,
            bigram_cjk: true,
            script_detection: true,
            multilingual_configs: true,
        }
    }
}

/// Parses a boolean environment variable with a default fallback.
///
/// Recognizes "true", "1", "yes", "on" (case-insensitive) as true.
/// Any other value or missing variable returns the default.
fn parse_bool_env(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|val| {
            let val_lower = val.to_lowercase();
            match val_lower.as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => None,
            }
        })
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify environment variables.
    // Environment variables are process-global, so tests must not run in parallel.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Helper to clear all FTS environment variables before a test.
    fn clear_fts_env() {
        env::remove_var("FTS_WEBSEARCH_TO_TSQUERY");
        env::remove_var("FTS_TRIGRAM_FALLBACK");
        env::remove_var("FTS_BIGRAM_CJK");
        env::remove_var("FTS_SCRIPT_DETECTION");
        env::remove_var("FTS_MULTILINGUAL_CONFIGS");
    }

    #[test]
    fn test_default_flags() {
        let flags = FtsFeatureFlags::default();

        // All features should be enabled by default
        assert!(flags.websearch_to_tsquery);
        assert!(flags.trigram_fallback);
        assert!(flags.bigram_cjk);
        assert!(flags.script_detection);
        assert!(flags.multilingual_configs);
    }

    #[test]
    fn test_all_enabled() {
        let flags = FtsFeatureFlags::all_enabled();

        assert!(flags.websearch_to_tsquery);
        assert!(flags.trigram_fallback);
        assert!(flags.bigram_cjk);
        assert!(flags.script_detection);
        assert!(flags.multilingual_configs);
    }

    #[test]
    fn test_default_equals_all_enabled() {
        // After deployment, default should equal all_enabled
        assert_eq!(FtsFeatureFlags::default(), FtsFeatureFlags::all_enabled());
    }

    #[test]
    fn test_phase1_enabled() {
        let flags = FtsFeatureFlags {
            websearch_to_tsquery: true,
            ..Default::default()
        };
        assert!(flags.is_phase1_enabled());

        let flags = FtsFeatureFlags {
            websearch_to_tsquery: false,
            ..Default::default()
        };
        assert!(!flags.is_phase1_enabled());
    }

    #[test]
    fn test_phase2_enabled_requires_both_flags() {
        // Neither flag set
        let flags = FtsFeatureFlags {
            trigram_fallback: false,
            script_detection: false,
            ..Default::default()
        };
        assert!(!flags.is_phase2_enabled());

        // Only trigram_fallback
        let flags = FtsFeatureFlags {
            trigram_fallback: true,
            script_detection: false,
            ..Default::default()
        };
        assert!(!flags.is_phase2_enabled());

        // Only script_detection
        let flags = FtsFeatureFlags {
            trigram_fallback: false,
            script_detection: true,
            ..Default::default()
        };
        assert!(!flags.is_phase2_enabled());

        // Both flags set (default)
        let flags = FtsFeatureFlags::default();
        assert!(flags.is_phase2_enabled());
    }

    #[test]
    fn test_phase3_enabled_requires_both_flags() {
        // Neither flag set
        let flags = FtsFeatureFlags {
            bigram_cjk: false,
            multilingual_configs: false,
            ..Default::default()
        };
        assert!(!flags.is_phase3_enabled());

        // Only bigram_cjk
        let flags = FtsFeatureFlags {
            bigram_cjk: true,
            multilingual_configs: false,
            ..Default::default()
        };
        assert!(!flags.is_phase3_enabled());

        // Only multilingual_configs
        let flags = FtsFeatureFlags {
            bigram_cjk: false,
            multilingual_configs: true,
            ..Default::default()
        };
        assert!(!flags.is_phase3_enabled());

        // Both flags set (default)
        let flags = FtsFeatureFlags::default();
        assert!(flags.is_phase3_enabled());
    }

    #[test]
    fn test_parse_bool_env_truthy_values() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        for val in &[
            "true", "TRUE", "True", "1", "yes", "YES", "Yes", "on", "ON", "On",
        ] {
            env::set_var("TEST_BOOL", val);
            assert!(
                parse_bool_env("TEST_BOOL", false),
                "Expected '{}' to parse as true",
                val
            );
            env::remove_var("TEST_BOOL");
        }
    }

    #[test]
    fn test_parse_bool_env_falsy_values() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        for val in &[
            "false", "FALSE", "False", "0", "no", "NO", "No", "off", "OFF", "Off",
        ] {
            env::set_var("TEST_BOOL", val);
            assert!(
                !parse_bool_env("TEST_BOOL", true),
                "Expected '{}' to parse as false",
                val
            );
            env::remove_var("TEST_BOOL");
        }
    }

    #[test]
    fn test_parse_bool_env_invalid_uses_default() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        env::set_var("TEST_BOOL", "invalid");
        assert!(parse_bool_env("TEST_BOOL", true));
        assert!(!parse_bool_env("TEST_BOOL", false));
        env::remove_var("TEST_BOOL");
    }

    #[test]
    fn test_parse_bool_env_missing_uses_default() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        env::remove_var("MISSING_VAR");
        assert!(parse_bool_env("MISSING_VAR", true));
        assert!(!parse_bool_env("MISSING_VAR", false));
    }

    #[test]
    fn test_from_env_with_defaults() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        let flags = FtsFeatureFlags::from_env();

        // Should match default()
        assert_eq!(flags, FtsFeatureFlags::default());
    }

    #[test]
    fn test_from_env_websearch_to_tsquery() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        env::set_var("FTS_WEBSEARCH_TO_TSQUERY", "false");
        let flags = FtsFeatureFlags::from_env();
        assert!(!flags.websearch_to_tsquery);
        clear_fts_env();

        env::set_var("FTS_WEBSEARCH_TO_TSQUERY", "true");
        let flags = FtsFeatureFlags::from_env();
        assert!(flags.websearch_to_tsquery);
        clear_fts_env();
    }

    #[test]
    fn test_from_env_trigram_fallback() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        // Test disabling
        env::set_var("FTS_TRIGRAM_FALLBACK", "false");
        let flags = FtsFeatureFlags::from_env();
        assert!(!flags.trigram_fallback);
        clear_fts_env();

        // Test enabling (should match default)
        env::set_var("FTS_TRIGRAM_FALLBACK", "true");
        let flags = FtsFeatureFlags::from_env();
        assert!(flags.trigram_fallback);
        clear_fts_env();
    }

    #[test]
    fn test_from_env_bigram_cjk() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        // Test disabling
        env::set_var("FTS_BIGRAM_CJK", "0");
        let flags = FtsFeatureFlags::from_env();
        assert!(!flags.bigram_cjk);
        clear_fts_env();

        // Test enabling (should match default)
        env::set_var("FTS_BIGRAM_CJK", "1");
        let flags = FtsFeatureFlags::from_env();
        assert!(flags.bigram_cjk);
        clear_fts_env();
    }

    #[test]
    fn test_from_env_script_detection() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        // Test disabling
        env::set_var("FTS_SCRIPT_DETECTION", "no");
        let flags = FtsFeatureFlags::from_env();
        assert!(!flags.script_detection);
        clear_fts_env();

        // Test enabling (should match default)
        env::set_var("FTS_SCRIPT_DETECTION", "yes");
        let flags = FtsFeatureFlags::from_env();
        assert!(flags.script_detection);
        clear_fts_env();
    }

    #[test]
    fn test_from_env_multilingual_configs() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        // Test disabling
        env::set_var("FTS_MULTILINGUAL_CONFIGS", "off");
        let flags = FtsFeatureFlags::from_env();
        assert!(!flags.multilingual_configs);
        clear_fts_env();

        // Test enabling (should match default)
        env::set_var("FTS_MULTILINGUAL_CONFIGS", "on");
        let flags = FtsFeatureFlags::from_env();
        assert!(flags.multilingual_configs);
        clear_fts_env();
    }

    #[test]
    fn test_from_env_all_flags() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        env::set_var("FTS_WEBSEARCH_TO_TSQUERY", "true");
        env::set_var("FTS_TRIGRAM_FALLBACK", "true");
        env::set_var("FTS_BIGRAM_CJK", "true");
        env::set_var("FTS_SCRIPT_DETECTION", "true");
        env::set_var("FTS_MULTILINGUAL_CONFIGS", "true");

        let flags = FtsFeatureFlags::from_env();
        assert_eq!(flags, FtsFeatureFlags::all_enabled());

        clear_fts_env();
    }

    #[test]
    fn test_from_env_mixed_flags() {
        let _guard = ENV_MUTEX.lock().unwrap();
        clear_fts_env();

        env::set_var("FTS_WEBSEARCH_TO_TSQUERY", "false");
        env::set_var("FTS_TRIGRAM_FALLBACK", "true");
        env::set_var("FTS_SCRIPT_DETECTION", "true");

        let flags = FtsFeatureFlags::from_env();
        assert!(!flags.websearch_to_tsquery);
        assert!(flags.trigram_fallback);
        assert!(flags.bigram_cjk); // Uses default (true)
        assert!(flags.script_detection);
        assert!(flags.multilingual_configs); // Uses default (true)

        clear_fts_env();
    }

    #[test]
    fn test_clone_and_equality() {
        let flags1 = FtsFeatureFlags::all_enabled();
        let flags2 = flags1.clone();
        assert_eq!(flags1, flags2);

        // Create a disabled config
        let flags3 = FtsFeatureFlags {
            websearch_to_tsquery: false,
            trigram_fallback: false,
            bigram_cjk: false,
            script_detection: false,
            multilingual_configs: false,
        };
        assert_ne!(flags1, flags3);
    }

    #[test]
    fn test_debug_formatting() {
        let flags = FtsFeatureFlags::default();
        let debug_str = format!("{:?}", flags);
        assert!(debug_str.contains("FtsFeatureFlags"));
        assert!(debug_str.contains("websearch_to_tsquery"));
    }
}
