//! Model-specific configuration for Ollama inference.
//!
//! Some models require special handling to expose their internal reasoning
//! or thinking processes. This module provides configuration to detect and
//! handle these models appropriately.

use thiserror::Error;

/// Determines if a model requires raw mode to expose thinking tags.
///
/// Raw mode (`raw: true` in Ollama API) disables prompt templating and
/// allows models to output their native format, including `<think>` tags
/// for reasoning models.
///
/// # Models requiring raw mode
///
/// - `deepseek-r1:*` - DeepSeek R1 thinking models
/// - `Mistral-Nemo-*-Thinking` - Mistral Nemo thinking variants
///
/// # Examples
///
/// ```
/// use matric_inference::model_config::requires_raw_mode;
///
/// assert!(requires_raw_mode("deepseek-r1:14b"));
/// assert!(requires_raw_mode("deepseek-r1:70b"));
/// assert!(requires_raw_mode("Mistral-Nemo-12B-Thinking"));
/// assert!(!requires_raw_mode("llama3.1:8b"));
/// ```
pub fn requires_raw_mode(model_name: &str) -> bool {
    // Convert to lowercase for case-insensitive matching
    let model_lower = model_name.to_lowercase();

    // DeepSeek R1 models (all variants)
    if model_lower.starts_with("deepseek-r1:") || model_lower.starts_with("deepseek-r1-") {
        return true;
    }

    // Mistral Nemo Thinking models
    if model_lower.contains("mistral-nemo") && model_lower.contains("thinking") {
        return true;
    }

    false
}

/// Type of restriction applied to a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestrictionType {
    /// Model is completely blocked and should not be used.
    Blocked,
    /// Model has limitations, user should be warned.
    Warning,
    /// Model can be used with caution for limited use cases.
    LimitedUse,
}

/// Information about a model restriction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRestriction {
    /// Type of restriction.
    pub restriction_type: RestrictionType,
    /// Human-readable reason for the restriction.
    pub reason: String,
    /// Suggested alternative model, if any.
    pub alternative: Option<String>,
}

/// Error returned when validating a model.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelValidationError {
    /// Model is blocked and cannot be used.
    #[error("Model '{model}' is blocked: {reason}\nAlternative: {alternative}")]
    ModelBlocked {
        model: String,
        reason: String,
        alternative: String,
    },
}

/// Checks if a model has any restrictions.
///
/// Returns `Some(ModelRestriction)` if the model has restrictions, `None` otherwise.
///
/// # Examples
///
/// ```
/// use matric_inference::model_config::{is_model_restricted, RestrictionType};
///
/// // Blocked models
/// let restriction = is_model_restricted("starcoder2:7b").unwrap();
/// assert_eq!(restriction.restriction_type, RestrictionType::Blocked);
///
/// let restriction = is_model_restricted("mirai-nova-llama3").unwrap();
/// assert_eq!(restriction.restriction_type, RestrictionType::Blocked);
///
/// // Warned models
/// let restriction = is_model_restricted("granite-code:8b").unwrap();
/// assert_eq!(restriction.restriction_type, RestrictionType::Warning);
///
/// // Unrestricted models
/// assert!(is_model_restricted("llama3.1:8b").is_none());
/// ```
pub fn is_model_restricted(model_name: &str) -> Option<ModelRestriction> {
    let model_lower = model_name.to_lowercase();

    // Check blocked models first

    // starcoder2:7b - Only 32-71 token output
    if model_lower.contains("starcoder2") {
        return Some(ModelRestriction {
            restriction_type: RestrictionType::Blocked,
            reason: "Severely limited output capacity (only 32-71 tokens). Cannot generate meaningful responses.".to_string(),
            alternative: Some("qwen2.5-coder:7b".to_string()),
        });
    }

    // Mirai-Nova-Llama3 variants - Only 2 token output
    if model_lower.contains("mirai-nova-llama3") || model_lower.contains("mirai-nova") {
        return Some(ModelRestriction {
            restriction_type: RestrictionType::Blocked,
            reason: "Critically broken output (only 2 tokens). Model is unusable.".to_string(),
            alternative: Some("llama3.1:8b".to_string()),
        });
    }

    // nemotron-mini:4b - Only 34 tokens output
    if model_lower.contains("nemotron-mini") {
        return Some(ModelRestriction {
            restriction_type: RestrictionType::Blocked,
            reason: "Severely limited output capacity (only 34 tokens). Cannot generate meaningful responses.".to_string(),
            alternative: Some("qwen2.5-coder:1.5b".to_string()),
        });
    }

    // Check warning models

    // granite-code:8b - Only 76-281 token output
    if model_lower.contains("granite-code:8b") {
        return Some(ModelRestriction {
            restriction_type: RestrictionType::Warning,
            reason: "Limited output capacity (76-281 tokens). May truncate longer responses."
                .to_string(),
            alternative: Some("qwen2.5-coder:7b".to_string()),
        });
    }

    None
}

/// Validates a model for use.
///
/// Returns `Ok(())` if the model is safe to use (no restrictions or only warnings).
/// Returns `Err(ModelValidationError)` if the model is blocked.
///
/// # Examples
///
/// ```
/// use matric_inference::model_config::validate_model;
///
/// // Valid models
/// assert!(validate_model("llama3.1:8b").is_ok());
/// assert!(validate_model("qwen2.5-coder:7b").is_ok());
///
/// // Warning models (allowed but user should be notified)
/// assert!(validate_model("granite-code:8b").is_ok());
///
/// // Blocked models
/// assert!(validate_model("starcoder2:7b").is_err());
/// assert!(validate_model("mirai-nova-llama3").is_err());
/// assert!(validate_model("nemotron-mini:4b").is_err());
/// ```
pub fn validate_model(model_name: &str) -> Result<(), ModelValidationError> {
    if let Some(restriction) = is_model_restricted(model_name) {
        match restriction.restriction_type {
            RestrictionType::Blocked => Err(ModelValidationError::ModelBlocked {
                model: model_name.to_string(),
                reason: restriction.reason,
                alternative: restriction
                    .alternative
                    .unwrap_or_else(|| "qwen2.5-coder:7b".to_string()),
            }),
            RestrictionType::Warning | RestrictionType::LimitedUse => {
                // Allow but caller should check restriction and warn user
                Ok(())
            }
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // DeepSeek R1 Models
    // ==========================================================================

    #[test]
    fn test_deepseek_r1_14b_requires_raw() {
        assert!(requires_raw_mode("deepseek-r1:14b"));
    }

    #[test]
    fn test_deepseek_r1_70b_requires_raw() {
        assert!(requires_raw_mode("deepseek-r1:70b"));
    }

    #[test]
    fn test_deepseek_r1_latest_requires_raw() {
        assert!(requires_raw_mode("deepseek-r1:latest"));
    }

    #[test]
    fn test_deepseek_r1_with_dash_requires_raw() {
        assert!(requires_raw_mode("deepseek-r1-14b"));
    }

    #[test]
    fn test_deepseek_r1_case_insensitive() {
        assert!(requires_raw_mode("DeepSeek-R1:14b"));
        assert!(requires_raw_mode("DEEPSEEK-R1:14B"));
    }

    // ==========================================================================
    // Mistral Nemo Thinking Models
    // ==========================================================================

    #[test]
    fn test_mistral_nemo_thinking_requires_raw() {
        assert!(requires_raw_mode("Mistral-Nemo-12B-Thinking"));
    }

    #[test]
    fn test_mistral_nemo_thinking_case_insensitive() {
        assert!(requires_raw_mode("mistral-nemo-12b-thinking"));
        assert!(requires_raw_mode("MISTRAL-NEMO-12B-THINKING"));
    }

    #[test]
    fn test_mistral_nemo_thinking_variants() {
        assert!(requires_raw_mode("Mistral-Nemo-Thinking"));
        assert!(requires_raw_mode("Mistral-Nemo-7B-Thinking"));
    }

    // ==========================================================================
    // Non-Thinking Models
    // ==========================================================================

    #[test]
    fn test_regular_models_dont_require_raw() {
        assert!(!requires_raw_mode("llama3.1:8b"));
        assert!(!requires_raw_mode("gpt-oss:20b"));
        assert!(!requires_raw_mode("qwen2.5-coder:7b"));
        assert!(!requires_raw_mode("mistral:latest"));
    }

    #[test]
    fn test_deepseek_coder_doesnt_require_raw() {
        // DeepSeek Coder is different from DeepSeek R1
        assert!(!requires_raw_mode("deepseek-coder-v2:16b"));
        assert!(!requires_raw_mode("deepseek-coder:6.7b"));
    }

    #[test]
    fn test_mistral_nemo_without_thinking_doesnt_require_raw() {
        // Regular Mistral Nemo without "Thinking" in name
        assert!(!requires_raw_mode("Mistral-Nemo-12B"));
        assert!(!requires_raw_mode("mistral-nemo:latest"));
    }

    #[test]
    fn test_empty_model_name() {
        assert!(!requires_raw_mode(""));
    }

    // ==========================================================================
    // Edge Cases
    // ==========================================================================

    #[test]
    fn test_model_name_with_whitespace() {
        // Whitespace is NOT trimmed - this is intentional
        // Callers should normalize input before calling this function
        assert!(!requires_raw_mode("  deepseek-r1:14b  "));
        // But without whitespace it works
        assert!(requires_raw_mode("deepseek-r1:14b"));
    }

    #[test]
    fn test_similar_but_different_names() {
        // Models that contain similar strings but aren't thinking models
        assert!(!requires_raw_mode("deepseek-r2:14b")); // R2, not R1
        assert!(!requires_raw_mode("deepseek-reasoning:14b")); // not R1
        assert!(!requires_raw_mode("thinking-llama:8b")); // not Mistral Nemo
    }

    // ==========================================================================
    // Model Restriction Tests - Blocked Models
    // ==========================================================================

    #[test]
    fn test_starcoder2_is_blocked() {
        let restriction = is_model_restricted("starcoder2:7b");
        assert!(restriction.is_some());
        let restriction = restriction.unwrap();
        assert_eq!(restriction.restriction_type, RestrictionType::Blocked);
        assert!(restriction.reason.contains("32-71 tokens"));
        assert_eq!(
            restriction.alternative,
            Some("qwen2.5-coder:7b".to_string())
        );
    }

    #[test]
    fn test_starcoder2_case_insensitive() {
        let restriction = is_model_restricted("StarCoder2:7b");
        assert!(restriction.is_some());
        assert_eq!(
            restriction.unwrap().restriction_type,
            RestrictionType::Blocked
        );
    }

    #[test]
    fn test_mirai_nova_is_blocked() {
        let restriction = is_model_restricted(
            "hf.co/mradermacher/Mirai-Nova-Llama3-LocalAI-Unchained-8B-v0.2-GGUF:Q4_K_M",
        );
        assert!(restriction.is_some());
        let restriction = restriction.unwrap();
        assert_eq!(restriction.restriction_type, RestrictionType::Blocked);
        assert!(restriction.reason.contains("2 tokens"));
        assert_eq!(restriction.alternative, Some("llama3.1:8b".to_string()));
    }

    #[test]
    fn test_mirai_nova_short_name_is_blocked() {
        let restriction = is_model_restricted("mirai-nova-llama3");
        assert!(restriction.is_some());
        assert_eq!(
            restriction.unwrap().restriction_type,
            RestrictionType::Blocked
        );
    }

    #[test]
    fn test_mirai_nova_case_insensitive() {
        let restriction = is_model_restricted("MIRAI-NOVA");
        assert!(restriction.is_some());
        assert_eq!(
            restriction.unwrap().restriction_type,
            RestrictionType::Blocked
        );
    }

    #[test]
    fn test_nemotron_mini_is_blocked() {
        let restriction = is_model_restricted("nemotron-mini:4b");
        assert!(restriction.is_some());
        let restriction = restriction.unwrap();
        assert_eq!(restriction.restriction_type, RestrictionType::Blocked);
        assert!(restriction.reason.contains("34 tokens"));
        assert_eq!(
            restriction.alternative,
            Some("qwen2.5-coder:1.5b".to_string())
        );
    }

    #[test]
    fn test_nemotron_mini_case_insensitive() {
        let restriction = is_model_restricted("Nemotron-Mini:4b");
        assert!(restriction.is_some());
        assert_eq!(
            restriction.unwrap().restriction_type,
            RestrictionType::Blocked
        );
    }

    // ==========================================================================
    // Model Restriction Tests - Warning Models
    // ==========================================================================

    #[test]
    fn test_granite_code_8b_has_warning() {
        let restriction = is_model_restricted("granite-code:8b");
        assert!(restriction.is_some());
        let restriction = restriction.unwrap();
        assert_eq!(restriction.restriction_type, RestrictionType::Warning);
        assert!(restriction.reason.contains("76-281 tokens"));
        assert_eq!(
            restriction.alternative,
            Some("qwen2.5-coder:7b".to_string())
        );
    }

    #[test]
    fn test_granite_code_8b_case_insensitive() {
        let restriction = is_model_restricted("Granite-Code:8B");
        assert!(restriction.is_some());
        assert_eq!(
            restriction.unwrap().restriction_type,
            RestrictionType::Warning
        );
    }

    // ==========================================================================
    // Model Restriction Tests - Unrestricted Models
    // ==========================================================================

    #[test]
    fn test_unrestricted_models() {
        assert!(is_model_restricted("llama3.1:8b").is_none());
        assert!(is_model_restricted("gpt-oss:20b").is_none());
        assert!(is_model_restricted("qwen2.5-coder:7b").is_none());
        assert!(is_model_restricted("deepseek-r1:14b").is_none());
        assert!(is_model_restricted("mistral:latest").is_none());
        assert!(is_model_restricted("qwen3:8b").is_none());
    }

    #[test]
    fn test_granite4_is_not_restricted() {
        // granite4:3b is different from granite-code:8b
        assert!(is_model_restricted("granite4:3b").is_none());
    }

    #[test]
    fn test_empty_model_name_not_restricted() {
        assert!(is_model_restricted("").is_none());
    }

    // ==========================================================================
    // Model Validation Tests - Valid Models
    // ==========================================================================

    #[test]
    fn test_validate_unrestricted_model() {
        assert!(validate_model("llama3.1:8b").is_ok());
        assert!(validate_model("qwen2.5-coder:7b").is_ok());
        assert!(validate_model("gpt-oss:20b").is_ok());
    }

    #[test]
    fn test_validate_warning_model_passes() {
        // Warning models should pass validation but caller should check restriction
        assert!(validate_model("granite-code:8b").is_ok());
    }

    // ==========================================================================
    // Model Validation Tests - Blocked Models
    // ==========================================================================

    #[test]
    fn test_validate_starcoder2_fails() {
        let result = validate_model("starcoder2:7b");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ModelValidationError::ModelBlocked { .. }));
        let err_string = err.to_string();
        assert!(err_string.contains("starcoder2:7b"));
        assert!(err_string.contains("32-71 tokens"));
        assert!(err_string.contains("qwen2.5-coder:7b"));
    }

    #[test]
    fn test_validate_mirai_nova_fails() {
        let result = validate_model("mirai-nova-llama3");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ModelValidationError::ModelBlocked { .. }));
        let err_string = err.to_string();
        assert!(err_string.contains("mirai-nova-llama3"));
        assert!(err_string.contains("2 tokens"));
        assert!(err_string.contains("llama3.1:8b"));
    }

    #[test]
    fn test_validate_nemotron_mini_fails() {
        let result = validate_model("nemotron-mini:4b");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ModelValidationError::ModelBlocked { .. }));
        let err_string = err.to_string();
        assert!(err_string.contains("nemotron-mini:4b"));
        assert!(err_string.contains("34 tokens"));
        assert!(err_string.contains("qwen2.5-coder:1.5b"));
    }

    // ==========================================================================
    // RestrictionType Tests
    // ==========================================================================

    #[test]
    fn test_restriction_type_equality() {
        assert_eq!(RestrictionType::Blocked, RestrictionType::Blocked);
        assert_eq!(RestrictionType::Warning, RestrictionType::Warning);
        assert_eq!(RestrictionType::LimitedUse, RestrictionType::LimitedUse);
        assert_ne!(RestrictionType::Blocked, RestrictionType::Warning);
    }

    #[test]
    fn test_restriction_type_debug() {
        assert_eq!(format!("{:?}", RestrictionType::Blocked), "Blocked");
        assert_eq!(format!("{:?}", RestrictionType::Warning), "Warning");
        assert_eq!(format!("{:?}", RestrictionType::LimitedUse), "LimitedUse");
    }

    // ==========================================================================
    // ModelRestriction Tests
    // ==========================================================================

    #[test]
    fn test_model_restriction_creation() {
        let restriction = ModelRestriction {
            restriction_type: RestrictionType::Blocked,
            reason: "Test reason".to_string(),
            alternative: Some("test-model".to_string()),
        };

        assert_eq!(restriction.restriction_type, RestrictionType::Blocked);
        assert_eq!(restriction.reason, "Test reason");
        assert_eq!(restriction.alternative, Some("test-model".to_string()));
    }

    #[test]
    fn test_model_restriction_without_alternative() {
        let restriction = ModelRestriction {
            restriction_type: RestrictionType::Warning,
            reason: "Test warning".to_string(),
            alternative: None,
        };

        assert_eq!(restriction.restriction_type, RestrictionType::Warning);
        assert_eq!(restriction.alternative, None);
    }

    // ==========================================================================
    // ModelValidationError Tests
    // ==========================================================================

    #[test]
    fn test_model_validation_error_display() {
        let err = ModelValidationError::ModelBlocked {
            model: "test-model".to_string(),
            reason: "Test reason".to_string(),
            alternative: "alternative-model".to_string(),
        };

        let display = err.to_string();
        assert!(display.contains("test-model"));
        assert!(display.contains("Test reason"));
        assert!(display.contains("alternative-model"));
    }

    #[test]
    fn test_model_validation_error_equality() {
        let err1 = ModelValidationError::ModelBlocked {
            model: "test".to_string(),
            reason: "reason".to_string(),
            alternative: "alt".to_string(),
        };

        let err2 = ModelValidationError::ModelBlocked {
            model: "test".to_string(),
            reason: "reason".to_string(),
            alternative: "alt".to_string(),
        };

        assert_eq!(err1, err2);
    }

    // ==========================================================================
    // Integration Tests
    // ==========================================================================

    #[test]
    fn test_blocked_model_has_restriction_and_fails_validation() {
        let model = "starcoder2:7b";

        // Should have restriction
        let restriction = is_model_restricted(model);
        assert!(restriction.is_some());
        assert_eq!(
            restriction.unwrap().restriction_type,
            RestrictionType::Blocked
        );

        // Should fail validation
        assert!(validate_model(model).is_err());
    }

    #[test]
    fn test_warning_model_has_restriction_but_passes_validation() {
        let model = "granite-code:8b";

        // Should have restriction
        let restriction = is_model_restricted(model);
        assert!(restriction.is_some());
        assert_eq!(
            restriction.unwrap().restriction_type,
            RestrictionType::Warning
        );

        // Should pass validation (but caller should warn user)
        assert!(validate_model(model).is_ok());
    }

    #[test]
    fn test_unrestricted_model_no_restriction_passes_validation() {
        let model = "llama3.1:8b";

        // Should have no restriction
        assert!(is_model_restricted(model).is_none());

        // Should pass validation
        assert!(validate_model(model).is_ok());
    }

    #[test]
    fn test_all_blocked_models_have_alternatives() {
        let blocked_models = vec!["starcoder2:7b", "mirai-nova-llama3", "nemotron-mini:4b"];

        for model in blocked_models {
            let restriction = is_model_restricted(model).unwrap();
            assert_eq!(restriction.restriction_type, RestrictionType::Blocked);
            assert!(
                restriction.alternative.is_some(),
                "Model {} should have alternative",
                model
            );
        }
    }

    #[test]
    fn test_all_warning_models_have_alternatives() {
        let warning_models = vec!["granite-code:8b"];

        for model in warning_models {
            let restriction = is_model_restricted(model).unwrap();
            assert_eq!(restriction.restriction_type, RestrictionType::Warning);
            assert!(
                restriction.alternative.is_some(),
                "Model {} should have alternative",
                model
            );
        }
    }
}
