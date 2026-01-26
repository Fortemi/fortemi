//! Few-shot prompt construction for AI revision.
//!
//! Implements in-context learning (ICL) for consistent, high-quality
//! AI revision by providing curated input/output examples in prompts.
//!
//! Research backing:
//! - REF-026 - Dong et al. (2023) "A Survey on In-context Learning"
//!
//! Key findings applied:
//! - 3-5 examples optimal for most tasks
//! - Most similar examples yield best results
//! - Best examples placed last (recency bias)
//! - Uniform example format improves consistency

use serde::{Deserialize, Serialize};

/// Type of few-shot example.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExampleType {
    /// AI note revision example
    Revision,
    /// Title generation example
    TitleGeneration,
    /// Tag extraction example
    TagExtraction,
    /// Summary generation
    Summarization,
}

impl ExampleType {
    /// Get the string representation.
    pub fn as_str(&self) -> &str {
        match self {
            ExampleType::Revision => "revision",
            ExampleType::TitleGeneration => "title_generation",
            ExampleType::TagExtraction => "tag_extraction",
            ExampleType::Summarization => "summarization",
        }
    }
}

/// Strategy for selecting few-shot examples.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStrategy {
    /// Use curated default examples (no retrieval needed)
    Default,
    /// Select most semantically similar to input
    Semantic,
    /// Select based on matching tags
    TagBased,
    /// Hybrid: semantic + tag weighting
    Hybrid {
        /// Weight for semantic similarity (0.0-1.0)
        semantic_weight: f32,
    },
}

/// Configuration for few-shot prompting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FewShotConfig {
    /// Number of examples to include (recommended: 3-5)
    pub num_examples: usize,
    /// Type of examples to use
    pub example_type: ExampleType,
    /// Strategy for selecting examples
    pub selection_strategy: SelectionStrategy,
}

impl Default for FewShotConfig {
    fn default() -> Self {
        Self {
            num_examples: 3,
            example_type: ExampleType::Revision,
            selection_strategy: SelectionStrategy::Default,
        }
    }
}

/// A single few-shot example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FewShotExample {
    /// Input text
    pub input: String,
    /// Expected output text
    pub output: String,
    /// Optional tags for matching
    pub tags: Vec<String>,
    /// Quality score (0.0-1.0)
    pub quality_score: f32,
}

/// Builds prompts with few-shot examples embedded.
pub struct FewShotPromptBuilder {
    examples: Vec<FewShotExample>,
    task_description: String,
}

impl FewShotPromptBuilder {
    /// Create a new builder with examples and task description.
    pub fn new(examples: Vec<FewShotExample>, task_description: String) -> Self {
        Self {
            examples,
            task_description,
        }
    }

    /// Build a revision prompt with embedded examples.
    pub fn build_revision_prompt(&self, input: &str, context: &str) -> String {
        let mut prompt = String::new();

        // Task description
        prompt.push_str(&format!(
            "You are revising a note for a knowledge base. {}\n\n",
            self.task_description
        ));

        // Few-shot examples (ordered by quality, best last for recency bias)
        if !self.examples.is_empty() {
            prompt.push_str("Here are examples of good revisions:\n\n");
            for (i, example) in self.examples.iter().enumerate() {
                prompt.push_str(&format!(
                    "### Example {}\n\n**Input:**\n{}\n\n**Output:**\n{}\n\n---\n\n",
                    i + 1,
                    example.input,
                    example.output
                ));
            }
        }

        // Current task
        prompt.push_str(&format!(
            "### Your Task\n\n**Context from related notes:**\n{}\n\n**Input:**\n{}\n\n**Output:**\n",
            context, input
        ));

        prompt
    }

    /// Build a title generation prompt with examples.
    pub fn build_title_prompt(&self, content: &str) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!("{}\n\n", self.task_description));

        if !self.examples.is_empty() {
            prompt.push_str("Examples of good titles:\n\n");
            for (i, example) in self.examples.iter().enumerate() {
                prompt.push_str(&format!(
                    "Content: {}\nTitle: {}\n\n",
                    truncate_for_prompt(&example.input, 200),
                    example.output
                ));
                if i < self.examples.len() - 1 {
                    prompt.push_str("---\n\n");
                }
            }
            prompt.push_str("---\n\n");
        }

        prompt.push_str(&format!(
            "Now generate a title for this content:\n\n{}\n\nTitle:",
            content
        ));

        prompt
    }

    /// Build a tag extraction prompt with examples.
    pub fn build_tag_prompt(&self, content: &str) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!("{}\n\n", self.task_description));

        if !self.examples.is_empty() {
            prompt.push_str("Examples:\n\n");
            for (i, example) in self.examples.iter().enumerate() {
                prompt.push_str(&format!(
                    "Content: {}\nTags: {}\n\n",
                    truncate_for_prompt(&example.input, 200),
                    example.output
                ));
                if i < self.examples.len() - 1 {
                    prompt.push_str("---\n\n");
                }
            }
            prompt.push_str("---\n\n");
        }

        prompt.push_str(&format!(
            "Extract relevant tags for this content:\n\n{}\n\nTags:",
            content
        ));

        prompt
    }

    /// Get the number of examples loaded.
    pub fn example_count(&self) -> usize {
        self.examples.len()
    }
}

/// Get default revision examples (curated set for bootstrapping).
pub fn default_revision_examples() -> Vec<FewShotExample> {
    vec![
        FewShotExample {
            input: "rust is good for systems programming. it has ownership and borrowing. memory safe without gc."
                .to_string(),
            output: "# Rust for Systems Programming\n\nRust is a systems programming language that provides memory safety without a garbage collector through its ownership and borrowing system.\n\n## Key Features\n\n- **Ownership model**: Each value has a single owner, preventing data races\n- **Borrowing**: References allow temporary access without transferring ownership\n- **Memory safety**: Compile-time guarantees eliminate common memory bugs"
                .to_string(),
            tags: vec!["rust".to_string(), "programming".to_string()],
            quality_score: 0.95,
        },
        FewShotExample {
            input: "met with team about Q1 goals. need to ship auth module. deadline march 15. also need to hire 2 more devs"
                .to_string(),
            output: "# Q1 Planning Meeting Notes\n\n## Key Decisions\n\n- **Auth module**: Must ship by March 15\n- **Hiring**: Approve 2 additional developer positions\n\n## Action Items\n\n1. Finalize auth module architecture (owner: TBD)\n2. Post developer job listings (owner: TBD)\n3. Set up weekly progress check-ins for Q1 goals"
                .to_string(),
            tags: vec!["meeting".to_string(), "planning".to_string()],
            quality_score: 0.9,
        },
        FewShotExample {
            input: "vector databases store embeddings. pgvector adds vector ops to postgres. hnsw is fast for ANN search"
                .to_string(),
            output: "# Vector Databases and pgvector\n\nVector databases specialize in storing and querying high-dimensional embeddings for similarity search.\n\n## pgvector Extension\n\npgvector adds vector operations to PostgreSQL, enabling:\n- Native vector column type\n- Distance operators (cosine, L2, inner product)\n- HNSW indexing for fast approximate nearest neighbor (ANN) search\n\n## HNSW Index\n\nHierarchical Navigable Small World (HNSW) provides sub-linear search time for ANN queries, making it practical for large-scale similarity search."
                .to_string(),
            tags: vec!["database".to_string(), "embeddings".to_string()],
            quality_score: 0.92,
        },
    ]
}

/// Get default title generation examples.
pub fn default_title_examples() -> Vec<FewShotExample> {
    vec![
        FewShotExample {
            input: "Rust provides memory safety without garbage collection through its ownership system. The borrow checker enforces these rules at compile time."
                .to_string(),
            output: "Rust Ownership and Memory Safety".to_string(),
            tags: vec![],
            quality_score: 0.95,
        },
        FewShotExample {
            input: "We discussed the Q1 roadmap and decided to prioritize the authentication module. The deadline is set for March 15th."
                .to_string(),
            output: "Q1 Roadmap: Auth Module Priority and Timeline".to_string(),
            tags: vec![],
            quality_score: 0.9,
        },
        FewShotExample {
            input: "pgvector adds vector similarity search to PostgreSQL. Combined with HNSW indexing, it enables fast approximate nearest neighbor queries."
                .to_string(),
            output: "pgvector: Vector Similarity Search in PostgreSQL".to_string(),
            tags: vec![],
            quality_score: 0.92,
        },
    ]
}

/// Truncate text for use in prompts, adding ellipsis if truncated.
fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", &text[..max_chars])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ExampleType Tests
    // =========================================================================

    #[test]
    fn test_example_type_as_str() {
        assert_eq!(ExampleType::Revision.as_str(), "revision");
        assert_eq!(ExampleType::TitleGeneration.as_str(), "title_generation");
        assert_eq!(ExampleType::TagExtraction.as_str(), "tag_extraction");
        assert_eq!(ExampleType::Summarization.as_str(), "summarization");
    }

    #[test]
    fn test_example_type_serialization() {
        let json = serde_json::to_string(&ExampleType::Revision).unwrap();
        assert_eq!(json, "\"revision\"");
        let parsed: ExampleType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ExampleType::Revision);
    }

    // =========================================================================
    // FewShotConfig Tests
    // =========================================================================

    #[test]
    fn test_config_default() {
        let config = FewShotConfig::default();
        assert_eq!(config.num_examples, 3);
        assert_eq!(config.example_type, ExampleType::Revision);
        assert!(matches!(
            config.selection_strategy,
            SelectionStrategy::Default
        ));
    }

    #[test]
    fn test_config_serialization() {
        let config = FewShotConfig {
            num_examples: 5,
            example_type: ExampleType::TitleGeneration,
            selection_strategy: SelectionStrategy::Hybrid {
                semantic_weight: 0.7,
            },
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: FewShotConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.num_examples, 5);
    }

    // =========================================================================
    // FewShotPromptBuilder Tests
    // =========================================================================

    #[test]
    fn test_revision_prompt_with_examples() {
        let examples = default_revision_examples();
        let builder =
            FewShotPromptBuilder::new(examples, "Enhance with structure and clarity.".to_string());

        let prompt = builder.build_revision_prompt("some raw note", "related context");
        assert!(prompt.contains("Example 1"));
        assert!(prompt.contains("Example 2"));
        assert!(prompt.contains("Example 3"));
        assert!(prompt.contains("some raw note"));
        assert!(prompt.contains("related context"));
        assert!(prompt.contains("Your Task"));
    }

    #[test]
    fn test_revision_prompt_no_examples() {
        let builder = FewShotPromptBuilder::new(vec![], "Enhance with structure.".to_string());

        let prompt = builder.build_revision_prompt("raw note", "context");
        assert!(!prompt.contains("Example"));
        assert!(prompt.contains("raw note"));
        assert!(prompt.contains("context"));
    }

    #[test]
    fn test_title_prompt_with_examples() {
        let examples = default_title_examples();
        let builder = FewShotPromptBuilder::new(examples, "Generate a concise title.".to_string());

        let prompt = builder.build_title_prompt("Some content about databases.");
        assert!(prompt.contains("Content:"));
        assert!(prompt.contains("Title:"));
        assert!(prompt.contains("Some content about databases"));
        assert!(prompt.contains("Generate a concise title"));
    }

    #[test]
    fn test_tag_prompt_with_examples() {
        let examples = vec![FewShotExample {
            input: "Rust programming with async".to_string(),
            output: "rust, async, programming".to_string(),
            tags: vec![],
            quality_score: 0.9,
        }];
        let builder = FewShotPromptBuilder::new(examples, "Extract relevant tags.".to_string());

        let prompt = builder.build_tag_prompt("Content about Docker containers.");
        assert!(prompt.contains("Tags:"));
        assert!(prompt.contains("Content about Docker"));
    }

    #[test]
    fn test_example_count() {
        let examples = default_revision_examples();
        let count = examples.len();
        let builder = FewShotPromptBuilder::new(examples, "task".to_string());
        assert_eq!(builder.example_count(), count);
    }

    // =========================================================================
    // Default Examples Tests
    // =========================================================================

    #[test]
    fn test_default_revision_examples() {
        let examples = default_revision_examples();
        assert_eq!(examples.len(), 3); // 3 curated examples
        for ex in &examples {
            assert!(!ex.input.is_empty());
            assert!(!ex.output.is_empty());
            assert!(ex.quality_score > 0.0);
        }
    }

    #[test]
    fn test_default_title_examples() {
        let examples = default_title_examples();
        assert_eq!(examples.len(), 3);
        for ex in &examples {
            assert!(!ex.input.is_empty());
            assert!(!ex.output.is_empty());
            // Title outputs should be short
            assert!(ex.output.len() < 100);
        }
    }

    // =========================================================================
    // Utility Tests
    // =========================================================================

    #[test]
    fn test_truncate_short_text() {
        assert_eq!(truncate_for_prompt("short", 100), "short");
    }

    #[test]
    fn test_truncate_long_text() {
        let long = "a".repeat(300);
        let truncated = truncate_for_prompt(&long, 100);
        assert_eq!(truncated.len(), 103); // 100 chars + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_truncate_exact_length() {
        let exact = "a".repeat(100);
        assert_eq!(truncate_for_prompt(&exact, 100), exact);
    }

    // =========================================================================
    // SelectionStrategy Tests
    // =========================================================================

    #[test]
    fn test_selection_strategy_serialization() {
        let strategies = vec![
            SelectionStrategy::Default,
            SelectionStrategy::Semantic,
            SelectionStrategy::TagBased,
            SelectionStrategy::Hybrid {
                semantic_weight: 0.7,
            },
        ];

        for strategy in strategies {
            let json = serde_json::to_string(&strategy).unwrap();
            let parsed: SelectionStrategy = serde_json::from_str(&json).unwrap();
            // Just verify round-trip doesn't panic
            let _ = format!("{:?}", parsed);
        }
    }
}
