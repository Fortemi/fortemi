//! Configurable, bounded relaxation for operator-free English/simple queries.
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct QueryPolicyConfig {
    /// Disable adaptive retrieval for strict baseline/rollback.
    pub enabled: bool,
    /// Relax when strict candidates are fewer than this count (default 5).
    pub scarcity_threshold: usize,
    /// Relax at this many distinct analyzed terms (default 6; 0 disables).
    pub long_query_terms: usize,
    /// Minimum matching fraction, rounded up (default 0 = one term).
    pub minimum_should_match_ratio: f32,
    /// Absolute bound for each lexical/vector candidate list (default 200).
    pub candidate_limit: usize,
}

impl Default for QueryPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scarcity_threshold: 5,
            long_query_terms: 6,
            minimum_should_match_ratio: 0.0,
            candidate_limit: 200,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct QueryDecision {
    pub policy: &'static str,
    pub trigger: &'static str,
    pub required_terms: usize,
}

impl QueryPolicyConfig {
    /// Server defaults can be overridden without changing query text. Invalid
    /// values retain defaults; all retrieval bounds are clamped when used.
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(value) = std::env::var("SEARCH_ADAPTIVE_ENABLED") {
            if let Ok(value) = value.parse() {
                config.enabled = value;
            }
        }
        for (name, target) in [
            ("SEARCH_ADAPTIVE_SCARCITY", &mut config.scarcity_threshold),
            (
                "SEARCH_ADAPTIVE_LONG_QUERY_TERMS",
                &mut config.long_query_terms,
            ),
            (
                "SEARCH_ADAPTIVE_CANDIDATE_LIMIT",
                &mut config.candidate_limit,
            ),
        ] {
            if let Ok(value) = std::env::var(name) {
                if let Ok(value) = value.parse() {
                    *target = value;
                }
            }
        }
        if let Ok(value) = std::env::var("SEARCH_ADAPTIVE_MIN_MATCH_RATIO") {
            if let Ok(value) = value.parse::<f32>() {
                if value.is_finite() {
                    config.minimum_should_match_ratio = value.clamp(0.0, 1.0);
                }
            }
        }
        config
    }
    pub fn decide(&self, explicit: bool, terms: usize, strict_hits: usize) -> QueryDecision {
        let trigger = if explicit {
            "explicit_syntax"
        } else if !self.enabled || terms < 2 {
            "strict_default"
        } else if strict_hits
            < self
                .scarcity_threshold
                .min(self.candidate_limit.clamp(1, 1000))
        {
            "strict_scarcity"
        } else if self.long_query_terms > 0 && terms >= self.long_query_terms {
            "long_query"
        } else {
            "strict_default"
        };
        let relaxed = matches!(trigger, "strict_scarcity" | "long_query");
        QueryDecision {
            policy: if explicit {
                "explicit"
            } else if relaxed {
                "minimum_should_match"
            } else {
                "strict_and"
            },
            trigger,
            required_terms: if relaxed {
                ((terms as f32 * self.minimum_should_match_ratio.clamp(0.0, 1.0)).ceil() as usize)
                    .clamp(1, terms)
            } else {
                terms
            },
        }
    }

    /// The OR/minimum-should-match rerun appends only partial matches, which the
    /// final full-match partition orders after every exact hit. When the exact
    /// hits already fill the requested page the rerun cannot change the page,
    /// so it is skipped (`mmr_active` re-ranks the whole union, so it never skips).
    pub fn skips_expansion(&self, strict_hits: usize, requested: i64, mmr_active: bool) -> bool {
        !mmr_active && requested > 0 && strict_hits >= requested as usize
    }

    pub fn bounded_limit(&self, requested: i64) -> i64 {
        (requested.saturating_mul(2) as usize)
            .max(self.scarcity_threshold.min(1000))
            .min(self.candidate_limit.clamp(1, 1000)) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triggers_and_boundaries() {
        let config = QueryPolicyConfig::default();
        for (explicit, terms, hits, trigger) in [
            (true, 10, 0, "explicit_syntax"),
            (true, 10, 1, "explicit_syntax"),
            (false, 0, 0, "strict_default"),
            (false, 1, 0, "strict_default"),
            (false, 2, 0, "strict_scarcity"),
            (false, 2, 4, "strict_scarcity"),
            (false, 2, 5, "strict_default"),
            (false, 5, 10, "strict_default"),
            (false, 6, 10, "long_query"),
        ] {
            assert_eq!(config.decide(explicit, terms, hits).trigger, trigger);
        }
        let strict = QueryPolicyConfig {
            enabled: false,
            ..config.clone()
        };
        assert_eq!(strict.decide(false, 10, 0).policy, "strict_and");
        let bounded = QueryPolicyConfig {
            minimum_should_match_ratio: 0.5,
            ..config.clone()
        };
        assert_eq!(bounded.decide(false, 5, 0).required_terms, 3);
        assert_eq!(config.bounded_limit(1000), 200);
        assert!(config.skips_expansion(1, 1, false));
        assert!(config.skips_expansion(10, 10, false));
        assert!(!config.skips_expansion(9, 10, false));
        assert!(!config.skips_expansion(10, 10, true));
        assert!(!config.skips_expansion(0, 0, false));
        assert_eq!(config.bounded_limit(1), 5);
    }
}
