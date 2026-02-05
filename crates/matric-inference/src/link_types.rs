//! Semantic link type classification.
//!
//! Classifies relationships between notes into typed semantic links
//! for richer knowledge graph queries and navigation.
//!
//! Reference: REF-032 - Ji et al. (2021) "A Survey on Knowledge Graphs"

use serde::{Deserialize, Serialize};
use std::fmt;

/// Semantic link types representing relationship classifications between notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticLinkType {
    /// Target supports or provides evidence for source
    Supports,
    /// Target contradicts or refutes source
    Contradicts,
    /// Target extends or builds upon source concepts
    Extends,
    /// Target implements or applies source theory/concept
    Implements,
    /// Target is referenced or cited by source
    References,
    /// Generic related content (default/fallback)
    Related,
}

impl SemanticLinkType {
    /// Returns string representation of link type.
    pub fn as_str(&self) -> &'static str {
        match self {
            SemanticLinkType::Supports => "supports",
            SemanticLinkType::Contradicts => "contradicts",
            SemanticLinkType::Extends => "extends",
            SemanticLinkType::Implements => "implements",
            SemanticLinkType::References => "references",
            SemanticLinkType::Related => "related",
        }
    }
}

impl fmt::Display for SemanticLinkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Classification result containing link type, confidence, and reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkClassification {
    /// The classified link type
    pub link_type: SemanticLinkType,
    /// Confidence score 0.0-1.0
    pub confidence: f32,
    /// Human-readable reasoning for the classification
    pub reasoning: String,
}

impl LinkClassification {
    /// Creates a new link classification.
    pub fn new(link_type: SemanticLinkType, confidence: f32, reasoning: String) -> Self {
        Self {
            link_type,
            confidence: confidence.clamp(0.0, 1.0),
            reasoning,
        }
    }
}

/// Generates an LLM prompt for classifying the relationship between two notes.
///
/// # Arguments
/// * `source_title` - Title of the source note
/// * `source_excerpt` - Excerpt/snippet from source note
/// * `target_title` - Title of the target note
/// * `target_excerpt` - Excerpt/snippet from target note
/// * `similarity_score` - Cosine similarity score (0.0-1.0)
pub fn link_classification_prompt(
    source_title: &str,
    source_excerpt: &str,
    target_title: &str,
    target_excerpt: &str,
    similarity_score: f32,
) -> String {
    format!(
        r#"Classify the semantic relationship between these two notes.

Source Note: "{}"
Excerpt: {}

Target Note: "{}"
Excerpt: {}

Similarity Score: {:.2}

Classify the relationship type:
- SUPPORTS: Target provides evidence or supports source claims
- CONTRADICTS: Target refutes or contradicts source
- EXTENDS: Target builds upon or extends source concepts
- IMPLEMENTS: Target applies or implements source theory/ideas
- REFERENCES: Target is cited or referenced by source
- RELATED: Generic topical relationship (default)

Respond in the format:
CLASSIFICATION: <type>
CONFIDENCE: <0.0-1.0>
REASONING: <brief explanation>
"#,
        source_title, source_excerpt, target_title, target_excerpt, similarity_score
    )
}

/// Parses LLM response to extract semantic link type.
///
/// Looks for "CLASSIFICATION: <type>" in response, defaulting to Related if not found.
pub fn parse_link_type(response: &str) -> SemanticLinkType {
    let response_lower = response.to_lowercase();

    // Look for classification line
    for line in response.lines() {
        let line_lower = line.to_lowercase();
        if line_lower.contains("classification:") {
            if line_lower.contains("supports") {
                return SemanticLinkType::Supports;
            } else if line_lower.contains("contradicts") {
                return SemanticLinkType::Contradicts;
            } else if line_lower.contains("extends") {
                return SemanticLinkType::Extends;
            } else if line_lower.contains("implements") {
                return SemanticLinkType::Implements;
            } else if line_lower.contains("references") {
                return SemanticLinkType::References;
            } else if line_lower.contains("related") {
                return SemanticLinkType::Related;
            }
        }
    }

    // Fallback: scan entire response
    if response_lower.contains("supports") {
        SemanticLinkType::Supports
    } else if response_lower.contains("contradicts") {
        SemanticLinkType::Contradicts
    } else if response_lower.contains("extends") {
        SemanticLinkType::Extends
    } else if response_lower.contains("implements") {
        SemanticLinkType::Implements
    } else if response_lower.contains("references") {
        SemanticLinkType::References
    } else {
        SemanticLinkType::Related
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_supports() {
        let response = "CLASSIFICATION: SUPPORTS\nCONFIDENCE: 0.85\nREASONING: Evidence provided";
        assert_eq!(parse_link_type(response), SemanticLinkType::Supports);
    }

    #[test]
    fn test_parse_contradicts() {
        let response =
            "CLASSIFICATION: CONTRADICTS\nCONFIDENCE: 0.92\nREASONING: Direct refutation";
        assert_eq!(parse_link_type(response), SemanticLinkType::Contradicts);
    }

    #[test]
    fn test_parse_extends() {
        let response = "CLASSIFICATION: EXTENDS\nCONFIDENCE: 0.78\nREASONING: Builds on concept";
        assert_eq!(parse_link_type(response), SemanticLinkType::Extends);
    }

    #[test]
    fn test_parse_implements() {
        let response =
            "CLASSIFICATION: IMPLEMENTS\nCONFIDENCE: 0.88\nREASONING: Practical application";
        assert_eq!(parse_link_type(response), SemanticLinkType::Implements);
    }

    #[test]
    fn test_parse_references() {
        let response = "CLASSIFICATION: REFERENCES\nCONFIDENCE: 0.95\nREASONING: Direct citation";
        assert_eq!(parse_link_type(response), SemanticLinkType::References);
    }

    #[test]
    fn test_parse_related() {
        let response = "CLASSIFICATION: RELATED\nCONFIDENCE: 0.65\nREASONING: Similar topic";
        assert_eq!(parse_link_type(response), SemanticLinkType::Related);
    }

    #[test]
    fn test_parse_unknown_defaults_to_related() {
        let response = "CLASSIFICATION: UNKNOWN\nCONFIDENCE: 0.5\nREASONING: No clear relationship";
        assert_eq!(parse_link_type(response), SemanticLinkType::Related);
    }

    #[test]
    fn test_parse_case_insensitive() {
        let response = "classification: supports\nconfidence: 0.85";
        assert_eq!(parse_link_type(response), SemanticLinkType::Supports);
    }

    #[test]
    fn test_prompt_contains_all_fields() {
        let prompt = link_classification_prompt(
            "Machine Learning Basics",
            "Introduction to neural networks",
            "Deep Learning Applications",
            "Practical CNN examples",
            0.82,
        );

        assert!(prompt.contains("Machine Learning Basics"));
        assert!(prompt.contains("Introduction to neural networks"));
        assert!(prompt.contains("Deep Learning Applications"));
        assert!(prompt.contains("Practical CNN examples"));
        assert!(prompt.contains("0.82"));
        assert!(prompt.contains("SUPPORTS"));
        assert!(prompt.contains("CONTRADICTS"));
        assert!(prompt.contains("EXTENDS"));
        assert!(prompt.contains("IMPLEMENTS"));
        assert!(prompt.contains("REFERENCES"));
        assert!(prompt.contains("RELATED"));
    }

    #[test]
    fn test_link_type_display() {
        assert_eq!(SemanticLinkType::Supports.to_string(), "supports");
        assert_eq!(SemanticLinkType::Contradicts.to_string(), "contradicts");
        assert_eq!(SemanticLinkType::Extends.to_string(), "extends");
        assert_eq!(SemanticLinkType::Implements.to_string(), "implements");
        assert_eq!(SemanticLinkType::References.to_string(), "references");
        assert_eq!(SemanticLinkType::Related.to_string(), "related");
    }

    #[test]
    fn test_link_type_as_str() {
        assert_eq!(SemanticLinkType::Supports.as_str(), "supports");
        assert_eq!(SemanticLinkType::Contradicts.as_str(), "contradicts");
        assert_eq!(SemanticLinkType::Extends.as_str(), "extends");
        assert_eq!(SemanticLinkType::Implements.as_str(), "implements");
        assert_eq!(SemanticLinkType::References.as_str(), "references");
        assert_eq!(SemanticLinkType::Related.as_str(), "related");
    }

    #[test]
    fn test_link_classification_confidence_clamping() {
        let classification = LinkClassification::new(
            SemanticLinkType::Supports,
            1.5, // Above 1.0
            "Test".to_string(),
        );
        assert_eq!(classification.confidence, 1.0);

        let classification = LinkClassification::new(
            SemanticLinkType::Supports,
            -0.5, // Below 0.0
            "Test".to_string(),
        );
        assert_eq!(classification.confidence, 0.0);
    }

    #[test]
    fn test_link_classification_serialization() {
        let classification = LinkClassification::new(
            SemanticLinkType::Extends,
            0.85,
            "Builds upon the concept".to_string(),
        );

        let json = serde_json::to_string(&classification).unwrap();
        let deserialized: LinkClassification = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.link_type, SemanticLinkType::Extends);
        assert_eq!(deserialized.confidence, 0.85);
        assert_eq!(deserialized.reasoning, "Builds upon the concept");
    }
}
