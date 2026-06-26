//! Model evaluation suites for knowledge management capabilities.
//!
//! This module provides evaluation frameworks for testing model quality
//! on matric-memory knowledge management tasks.
//!
//! # Eval Suites
//!
//! - **Title Quality**: Tests title generation quality, format compliance
//! - **Revision Quality**: Tests content enhancement, structure, clarity
//! - **Semantic Accuracy**: Tests embedding similarity, MRR, recall
//! - **Format Compliance**: Tests instruction following, output format
//! - **Latency**: Tests response speed under various conditions

use crate::capabilities::{Capability, CapabilityRating, ModelCapabilities, QualityTier};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Evaluation result for a single test case.
#[derive(Clone, Serialize, Deserialize)]
pub struct EvalResult {
    /// Test case identifier.
    pub test_id: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Score (0.0 - 1.0).
    pub score: f32,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Model output.
    pub output: String,
    /// Expected output (if applicable).
    pub expected: Option<String>,
    /// Notes or error messages.
    pub notes: Option<String>,
}

impl fmt::Debug for EvalResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalResult")
            .field("test_id_len", &self.test_id.len())
            .field("passed", &self.passed)
            .field("score", &self.score)
            .field("latency_ms", &self.latency_ms)
            .field("output_len", &self.output.len())
            .field("expected_len", &self.expected.as_ref().map(String::len))
            .field("notes_len", &self.notes.as_ref().map(String::len))
            .finish()
    }
}

/// Summary of evaluation results for a suite.
#[derive(Clone, Serialize, Deserialize)]
pub struct EvalSummary {
    /// Suite name.
    pub suite: String,
    /// Model being evaluated.
    pub model: String,
    /// Capability being tested.
    pub capability: Capability,
    /// Number of tests passed.
    pub passed: usize,
    /// Total number of tests.
    pub total: usize,
    /// Pass rate (0.0 - 1.0).
    pub pass_rate: f32,
    /// Average score.
    pub avg_score: f32,
    /// Quality tier based on pass rate.
    pub tier: QualityTier,
    /// P50 latency.
    pub latency_p50_ms: u64,
    /// P95 latency.
    pub latency_p95_ms: u64,
}

impl fmt::Debug for EvalSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalSummary")
            .field("suite_len", &self.suite.len())
            .field("model_len", &self.model.len())
            .field("capability", &self.capability)
            .field("passed", &self.passed)
            .field("total", &self.total)
            .field("pass_rate", &self.pass_rate)
            .field("avg_score", &self.avg_score)
            .field("tier", &self.tier)
            .field("latency_p50_ms", &self.latency_p50_ms)
            .field("latency_p95_ms", &self.latency_p95_ms)
            .finish()
    }
}

impl EvalSummary {
    /// Create from a list of results.
    pub fn from_results(
        suite: impl Into<String>,
        model: impl Into<String>,
        capability: Capability,
        results: &[EvalResult],
    ) -> Self {
        let passed = results.iter().filter(|r| r.passed).count();
        let total = results.len();
        let pass_rate = if total > 0 {
            passed as f32 / total as f32
        } else {
            0.0
        };

        let avg_score = if !results.is_empty() {
            results.iter().map(|r| r.score).sum::<f32>() / results.len() as f32
        } else {
            0.0
        };

        // Calculate latency percentiles
        let mut latencies: Vec<u64> = results.iter().map(|r| r.latency_ms).collect();
        latencies.sort_unstable();

        let latency_p50 = latencies.get(latencies.len() / 2).copied().unwrap_or(0);
        let latency_p95 = latencies
            .get((latencies.len() as f64 * 0.95) as usize)
            .copied()
            .unwrap_or(0);

        Self {
            suite: suite.into(),
            model: model.into(),
            capability,
            passed,
            total,
            pass_rate,
            avg_score,
            tier: QualityTier::from_score(avg_score * 100.0),
            latency_p50_ms: latency_p50,
            latency_p95_ms: latency_p95,
        }
    }

    /// Convert to a capability rating.
    pub fn to_rating(&self) -> CapabilityRating {
        CapabilityRating::from_score(self.capability, self.avg_score * 100.0)
            .with_latency(self.latency_p95_ms)
    }
}

/// Test case for title generation.
#[derive(Clone, Serialize, Deserialize)]
pub struct TitleTestCase {
    /// Test case ID.
    pub id: String,
    /// Input note content.
    pub content: String,
    /// Expected title keywords (any of these should appear).
    pub expected_keywords: Vec<String>,
    /// Maximum acceptable length in characters.
    pub max_length: usize,
}

impl fmt::Debug for TitleTestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TitleTestCase")
            .field("id_len", &self.id.len())
            .field("content_len", &self.content.len())
            .field("expected_keyword_count", &self.expected_keywords.len())
            .field(
                "expected_keyword_total_len",
                &self
                    .expected_keywords
                    .iter()
                    .map(String::len)
                    .sum::<usize>(),
            )
            .field("max_length", &self.max_length)
            .finish()
    }
}

/// Test case for semantic similarity.
#[derive(Clone, Serialize, Deserialize)]
pub struct SemanticTestCase {
    /// Test case ID.
    pub id: String,
    /// Query text.
    pub query: String,
    /// Positive examples (should be similar).
    pub positive: Vec<String>,
    /// Negative examples (should be dissimilar).
    pub negative: Vec<String>,
}

impl fmt::Debug for SemanticTestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticTestCase")
            .field("id_len", &self.id.len())
            .field("query_len", &self.query.len())
            .field("positive_count", &self.positive.len())
            .field(
                "positive_total_len",
                &self.positive.iter().map(String::len).sum::<usize>(),
            )
            .field("negative_count", &self.negative.len())
            .field(
                "negative_total_len",
                &self.negative.iter().map(String::len).sum::<usize>(),
            )
            .finish()
    }
}

/// Test case for content revision.
#[derive(Clone, Serialize, Deserialize)]
pub struct RevisionTestCase {
    /// Test case ID.
    pub id: String,
    /// Input content to revise.
    pub input: String,
    /// Required elements in output.
    pub required_elements: Vec<String>,
    /// Forbidden elements in output.
    pub forbidden_elements: Vec<String>,
}

impl fmt::Debug for RevisionTestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RevisionTestCase")
            .field("id_len", &self.id.len())
            .field("input_len", &self.input.len())
            .field("required_element_count", &self.required_elements.len())
            .field(
                "required_element_total_len",
                &self
                    .required_elements
                    .iter()
                    .map(String::len)
                    .sum::<usize>(),
            )
            .field("forbidden_element_count", &self.forbidden_elements.len())
            .field(
                "forbidden_element_total_len",
                &self
                    .forbidden_elements
                    .iter()
                    .map(String::len)
                    .sum::<usize>(),
            )
            .finish()
    }
}

/// Test case for tag generation.
#[derive(Clone, Serialize, Deserialize)]
pub struct TagTestCase {
    /// Test case ID.
    pub id: String,
    /// Input note content.
    pub content: String,
    /// Expected tags (should appear in output).
    pub expected_tags: Vec<String>,
    /// Forbidden tags (should not appear).
    pub forbidden_tags: Vec<String>,
    /// Maximum number of tags allowed.
    pub max_tags: usize,
}

impl fmt::Debug for TagTestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TagTestCase")
            .field("id_len", &self.id.len())
            .field("content_len", &self.content.len())
            .field("expected_tag_count", &self.expected_tags.len())
            .field(
                "expected_tag_total_len",
                &self.expected_tags.iter().map(String::len).sum::<usize>(),
            )
            .field("forbidden_tag_count", &self.forbidden_tags.len())
            .field(
                "forbidden_tag_total_len",
                &self.forbidden_tags.iter().map(String::len).sum::<usize>(),
            )
            .field("max_tags", &self.max_tags)
            .finish()
    }
}

/// Format constraint for format compliance testing.
#[derive(Clone, Serialize, Deserialize)]
pub struct FormatConstraint {
    /// Type of constraint (e.g., "max_words", "format", "language").
    #[serde(rename = "type")]
    pub constraint_type: String,
    /// Constraint value (varies by type).
    pub value: serde_json::Value,
}

impl fmt::Debug for FormatConstraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.value.to_string();
        f.debug_struct("FormatConstraint")
            .field("constraint_type_len", &self.constraint_type.len())
            .field("value_len", &value.len())
            .finish()
    }
}

/// Test case for format compliance (IFEval-style).
#[derive(Clone, Serialize, Deserialize)]
pub struct FormatTestCase {
    /// Test case ID.
    pub id: String,
    /// Prompt for the model.
    pub prompt: String,
    /// Format constraints to verify.
    pub constraints: Vec<FormatConstraint>,
}

impl fmt::Debug for FormatTestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormatTestCase")
            .field("id_len", &self.id.len())
            .field("prompt_len", &self.prompt.len())
            .field("constraint_count", &self.constraints.len())
            .finish()
    }
}

/// Test case for semantic linking/context generation.
#[derive(Clone, Serialize, Deserialize)]
pub struct ContextTestCase {
    /// Test case ID.
    pub id: String,
    /// Main note content.
    pub note_content: String,
    /// Related notes for context.
    pub related_notes: Vec<String>,
    /// Expected connections to identify.
    pub expected_connections: Vec<String>,
    /// Unrelated notes (should not be linked).
    pub unrelated_notes: Vec<String>,
}

impl fmt::Debug for ContextTestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextTestCase")
            .field("id_len", &self.id.len())
            .field("note_content_len", &self.note_content.len())
            .field("related_note_count", &self.related_notes.len())
            .field(
                "related_note_total_len",
                &self.related_notes.iter().map(String::len).sum::<usize>(),
            )
            .field(
                "expected_connection_count",
                &self.expected_connections.len(),
            )
            .field(
                "expected_connection_total_len",
                &self
                    .expected_connections
                    .iter()
                    .map(String::len)
                    .sum::<usize>(),
            )
            .field("unrelated_note_count", &self.unrelated_notes.len())
            .field(
                "unrelated_note_total_len",
                &self.unrelated_notes.iter().map(String::len).sum::<usize>(),
            )
            .finish()
    }
}

/// Test case for long context evaluation.
#[derive(Clone, Serialize, Deserialize)]
pub struct LongContextTestCase {
    /// Test case ID.
    pub id: String,
    /// Long context document.
    pub context: String,
    /// Query about the context.
    pub query: String,
    /// Expected facts in the answer.
    pub expected_facts: Vec<String>,
    /// Context length in words.
    pub context_length_words: usize,
}

impl fmt::Debug for LongContextTestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LongContextTestCase")
            .field("id_len", &self.id.len())
            .field("context_len", &self.context.len())
            .field("query_len", &self.query.len())
            .field("expected_fact_count", &self.expected_facts.len())
            .field(
                "expected_fact_total_len",
                &self.expected_facts.iter().map(String::len).sum::<usize>(),
            )
            .field("context_length_words", &self.context_length_words)
            .finish()
    }
}

/// Evaluation tier for test suite sizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvalTier {
    /// Smoke test: ~20 tests, <1 min
    Smoke,
    /// Core test: ~75 tests, ~5 min
    Core,
    /// Extended test: ~150 tests, ~15 min
    Extended,
    /// Full test: ~300 tests, ~30 min
    Full,
}

impl EvalTier {
    /// Get expected test count for this tier.
    pub fn test_count(&self) -> usize {
        match self {
            EvalTier::Smoke => 20,
            EvalTier::Core => 75,
            EvalTier::Extended => 150,
            EvalTier::Full => 300,
        }
    }

    /// Get expected duration in minutes.
    pub fn duration_minutes(&self) -> usize {
        match self {
            EvalTier::Smoke => 1,
            EvalTier::Core => 5,
            EvalTier::Extended => 15,
            EvalTier::Full => 30,
        }
    }
}

/// Judge prompt template for LLM-as-Judge evaluation.
#[derive(Clone, Serialize, Deserialize)]
pub struct JudgePrompt {
    pub name: String,
    #[serde(rename = "type")]
    pub prompt_type: String, // "single" or "pairwise"
    pub category: String,
    pub system_prompt: String,
    pub prompt_template: String,
    pub output_format: String,
    #[serde(default)]
    pub scoring: Option<ScoringConfig>,
}

impl fmt::Debug for JudgePrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JudgePrompt")
            .field("name_len", &self.name.len())
            .field("prompt_type_len", &self.prompt_type.len())
            .field("category_len", &self.category.len())
            .field("system_prompt_len", &self.system_prompt.len())
            .field("prompt_template_len", &self.prompt_template.len())
            .field("output_format_len", &self.output_format.len())
            .field("scoring", &self.scoring)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    pub min: i32,
    pub max: i32,
}

/// Result from an LLM judge evaluation.
#[derive(Clone, Serialize, Deserialize)]
pub struct JudgeResult {
    pub prompt_name: String,
    pub score: Option<f32>,     // For single evaluations (1-10)
    pub winner: Option<String>, // For pairwise ("A", "B", or "C")
    pub reasoning: String,
    pub raw_output: String,
}

impl fmt::Debug for JudgeResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JudgeResult")
            .field("prompt_name_len", &self.prompt_name.len())
            .field("score", &self.score)
            .field("winner_len", &self.winner.as_ref().map(String::len))
            .field("reasoning_len", &self.reasoning.len())
            .field("raw_output_len", &self.raw_output.len())
            .finish()
    }
}

/// Load test cases from JSONL file.
fn load_jsonl<T: serde::de::DeserializeOwned>(
    path: impl AsRef<Path>,
) -> Result<Vec<T>, std::io::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut items = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            let item: T = serde_json::from_str(&line)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            items.push(item);
        }
    }

    Ok(items)
}

/// Load title generation test cases from JSONL.
pub fn load_title_tests(path: impl AsRef<Path>) -> Result<Vec<TitleTestCase>, std::io::Error> {
    load_jsonl(path)
}

/// Load semantic similarity test cases from JSONL.
pub fn load_semantic_tests(
    path: impl AsRef<Path>,
) -> Result<Vec<SemanticTestCase>, std::io::Error> {
    load_jsonl(path)
}

/// Load content revision test cases from JSONL.
pub fn load_revision_tests(
    path: impl AsRef<Path>,
) -> Result<Vec<RevisionTestCase>, std::io::Error> {
    load_jsonl(path)
}

/// Load tag generation test cases from JSONL.
pub fn load_tag_tests(path: impl AsRef<Path>) -> Result<Vec<TagTestCase>, std::io::Error> {
    load_jsonl(path)
}

/// Load format compliance test cases from JSONL.
pub fn load_format_tests(path: impl AsRef<Path>) -> Result<Vec<FormatTestCase>, std::io::Error> {
    load_jsonl(path)
}

/// Load context generation test cases from JSONL.
pub fn load_context_tests(path: impl AsRef<Path>) -> Result<Vec<ContextTestCase>, std::io::Error> {
    load_jsonl(path)
}

/// Load long context test cases from JSONL.
pub fn load_long_context_tests(
    path: impl AsRef<Path>,
) -> Result<Vec<LongContextTestCase>, std::io::Error> {
    load_jsonl(path)
}

/// Load judge prompts from JSONL.
pub fn load_judge_prompts(path: impl AsRef<Path>) -> Result<Vec<JudgePrompt>, std::io::Error> {
    load_jsonl(path)
}

/// Format a judge prompt template with variables.
pub fn format_judge_prompt(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

/// Parse a numeric score from judge output.
/// Looks for patterns like "Score: 8" or "Rating: 7/10" or just a number.
pub fn parse_judge_score(output: &str, config: &ScoringConfig) -> Option<f32> {
    // Try to find "Score: N" or "Rating: N" pattern
    let patterns = [
        r"[Ss]core:\s*(\d+(?:\.\d+)?)",
        r"[Rr]ating:\s*(\d+(?:\.\d+)?)",
        r"\*\*(\d+(?:\.\d+)?)\*\*/10",
        r"\*\*(\d+(?:\.\d+)?)\*\*",
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(caps) = re.captures(output) {
                if let Some(m) = caps.get(1) {
                    if let Ok(score) = m.as_str().parse::<f32>() {
                        // Normalize to 0-1 range
                        return Some(
                            (score - config.min as f32) / (config.max as f32 - config.min as f32),
                        );
                    }
                }
            }
        }
    }

    None
}

/// Parse the winner from a pairwise comparison.
/// Looks for [[A]], [[B]], or [[C]] (tie).
pub fn parse_pairwise_winner(output: &str) -> Option<String> {
    if output.contains("[[A]]") {
        Some("A".to_string())
    } else if output.contains("[[B]]") {
        Some("B".to_string())
    } else if output.contains("[[C]]") {
        Some("C".to_string())
    } else {
        None
    }
}

/// Default title generation test suite.
pub fn title_generation_suite() -> Vec<TitleTestCase> {
    vec![
        TitleTestCase {
            id: "title-tech-1".to_string(),
            content: "Rust is a systems programming language that focuses on safety, speed, and concurrency. It achieves memory safety without garbage collection through its ownership system.".to_string(),
            expected_keywords: vec!["Rust".to_string(), "programming".to_string(), "safety".to_string()],
            max_length: 80,
        },
        TitleTestCase {
            id: "title-tech-2".to_string(),
            content: "PostgreSQL is an advanced open-source relational database system. It supports both SQL and JSON querying, making it versatile for various data workloads.".to_string(),
            expected_keywords: vec!["PostgreSQL".to_string(), "database".to_string()],
            max_length: 80,
        },
        TitleTestCase {
            id: "title-meeting-1".to_string(),
            content: "Discussed Q4 budget allocation with finance team. Key decisions: increase marketing spend by 15%, reduce travel budget by 10%, allocate $50K for new tooling.".to_string(),
            expected_keywords: vec!["budget".to_string(), "Q4".to_string()],
            max_length: 80,
        },
        TitleTestCase {
            id: "title-recipe-1".to_string(),
            content: "Classic chocolate chip cookies: Mix butter, sugar, eggs. Add flour, baking soda, salt. Fold in chocolate chips. Bake at 375°F for 10-12 minutes.".to_string(),
            expected_keywords: vec!["chocolate".to_string(), "cookie".to_string()],
            max_length: 80,
        },
        TitleTestCase {
            id: "title-research-1".to_string(),
            content: "Study findings: Participants who meditated for 20 minutes daily showed 30% reduction in cortisol levels compared to control group. Sample size: 150, p < 0.01.".to_string(),
            expected_keywords: vec!["meditation".to_string(), "cortisol".to_string(), "study".to_string()],
            max_length: 80,
        },
    ]
}

/// Default semantic similarity test suite.
pub fn semantic_similarity_suite() -> Vec<SemanticTestCase> {
    vec![
        SemanticTestCase {
            id: "semantic-animals-1".to_string(),
            query: "The cat sat on the mat.".to_string(),
            positive: vec!["A kitten rested on the rug.".to_string()],
            negative: vec!["Python is a programming language.".to_string()],
        },
        SemanticTestCase {
            id: "semantic-tech-1".to_string(),
            query: "Machine learning algorithms for image recognition.".to_string(),
            positive: vec![
                "Deep learning models for computer vision tasks.".to_string(),
                "Neural networks that classify images.".to_string(),
            ],
            negative: vec!["Recipe for chocolate cake.".to_string()],
        },
        SemanticTestCase {
            id: "semantic-finance-1".to_string(),
            query: "Stock market investment strategies.".to_string(),
            positive: vec!["Portfolio diversification techniques.".to_string()],
            negative: vec!["How to plant tomatoes in spring.".to_string()],
        },
    ]
}

/// Default content revision test suite.
pub fn content_revision_suite() -> Vec<RevisionTestCase> {
    vec![
        RevisionTestCase {
            id: "revision-structure-1".to_string(),
            input: "meeting notes from today talked about budget also discussed timeline and assigned tasks to team".to_string(),
            required_elements: vec!["#".to_string()], // Should add headers
            forbidden_elements: vec![],
        },
        RevisionTestCase {
            id: "revision-clarity-1".to_string(),
            input: "the thing we need to do is make the stuff work better".to_string(),
            required_elements: vec![], // Should clarify vague language
            forbidden_elements: vec!["thing".to_string(), "stuff".to_string()],
        },
    ]
}

/// Evaluate a title against test case criteria.
pub fn evaluate_title(output: &str, test_case: &TitleTestCase) -> EvalResult {
    let output_lower = output.to_lowercase();

    // Check length
    let length_ok = output.len() <= test_case.max_length;

    // Check for expected keywords
    let keyword_matches = test_case
        .expected_keywords
        .iter()
        .filter(|kw| output_lower.contains(&kw.to_lowercase()))
        .count();
    let keyword_ratio = keyword_matches as f32 / test_case.expected_keywords.len() as f32;

    // Check for no markdown or special characters
    let clean_format = !output.contains("```")
        && !output.starts_with('#')
        && !output.contains("**")
        && !output.contains("Title:");

    let score = (keyword_ratio * 0.6)
        + (if length_ok { 0.2 } else { 0.0 })
        + (if clean_format { 0.2 } else { 0.0 });
    let passed = score >= 0.7;

    EvalResult {
        test_id: test_case.id.clone(),
        passed,
        score,
        latency_ms: 0, // Set externally
        output: output.to_string(),
        expected: Some(test_case.expected_keywords.join(", ")),
        notes: if !passed {
            Some(format!(
                "Keywords: {}/{}, Length: {}, Clean: {}",
                keyword_matches,
                test_case.expected_keywords.len(),
                if length_ok { "OK" } else { "TOO LONG" },
                if clean_format { "OK" } else { "BAD FORMAT" }
            ))
        } else {
            None
        },
    }
}

/// Evaluate tag generation output against test case criteria.
pub fn evaluate_tags(output_tags: &[String], test_case: &TagTestCase) -> EvalResult {
    // Check tag count
    let count_ok = output_tags.len() <= test_case.max_tags;

    // Check for expected tags
    let expected_matches = test_case
        .expected_tags
        .iter()
        .filter(|tag| output_tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
        .count();
    let expected_ratio = if test_case.expected_tags.is_empty() {
        1.0
    } else {
        expected_matches as f32 / test_case.expected_tags.len() as f32
    };

    // Check for forbidden tags
    let forbidden_matches = test_case
        .forbidden_tags
        .iter()
        .filter(|tag| output_tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
        .count();
    let no_forbidden = forbidden_matches == 0;

    let score = (expected_ratio * 0.6)
        + (if count_ok { 0.2 } else { 0.0 })
        + (if no_forbidden { 0.2 } else { 0.0 });
    let passed = score >= 0.7 && no_forbidden;

    EvalResult {
        test_id: test_case.id.clone(),
        passed,
        score,
        latency_ms: 0,
        output: output_tags.join(", "),
        expected: Some(test_case.expected_tags.join(", ")),
        notes: if !passed {
            Some(format!(
                "Expected: {}/{}, Forbidden: {}, Count: {}/{}",
                expected_matches,
                test_case.expected_tags.len(),
                forbidden_matches,
                output_tags.len(),
                test_case.max_tags
            ))
        } else {
            None
        },
    }
}

/// Evaluate format compliance against constraints.
pub fn evaluate_format(output: &str, test_case: &FormatTestCase) -> EvalResult {
    let mut total_score = 0.0;
    let mut constraint_count = test_case.constraints.len() as f32;
    let mut notes = Vec::new();

    if constraint_count == 0.0 {
        constraint_count = 1.0; // Avoid division by zero
    }

    for constraint in &test_case.constraints {
        let constraint_passed = match constraint.constraint_type.as_str() {
            "max_words" => {
                if let Some(max) = constraint.value.as_u64() {
                    let word_count = output.split_whitespace().count() as u64;
                    let passed = word_count <= max;
                    if !passed {
                        notes.push(format!("Word count {} > {}", word_count, max));
                    }
                    passed
                } else {
                    false
                }
            }
            "min_words" => {
                if let Some(min) = constraint.value.as_u64() {
                    let word_count = output.split_whitespace().count() as u64;
                    let passed = word_count >= min;
                    if !passed {
                        notes.push(format!("Word count {} < {}", word_count, min));
                    }
                    passed
                } else {
                    false
                }
            }
            "format" => {
                if let Some(format) = constraint.value.as_str() {
                    let passed = match format {
                        "json" => output.trim().starts_with('{') || output.trim().starts_with('['),
                        "markdown" => output.contains('#') || output.contains("**"),
                        "bullet_list" => output.contains("- ") || output.contains("* "),
                        "numbered_list" => output.lines().any(|l| {
                            l.trim_start()
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_digit())
                        }),
                        _ => true,
                    };
                    if !passed {
                        notes.push(format!("Format '{}' not detected", format));
                    }
                    passed
                } else {
                    false
                }
            }
            "contains" => {
                if let Some(text) = constraint.value.as_str() {
                    let passed = output.contains(text);
                    if !passed {
                        notes.push(format!("Missing required text: '{}'", text));
                    }
                    passed
                } else {
                    false
                }
            }
            "excludes" => {
                if let Some(text) = constraint.value.as_str() {
                    let passed = !output.contains(text);
                    if !passed {
                        notes.push(format!("Contains forbidden text: '{}'", text));
                    }
                    passed
                } else {
                    false
                }
            }
            _ => true, // Unknown constraint type passes
        };

        if constraint_passed {
            total_score += 1.0;
        }
    }

    let score = total_score / constraint_count;
    let passed = score >= 0.8;

    EvalResult {
        test_id: test_case.id.clone(),
        passed,
        score,
        latency_ms: 0,
        output: output.to_string(),
        expected: Some(format!("{} constraints", test_case.constraints.len())),
        notes: if notes.is_empty() {
            None
        } else {
            Some(notes.join("; "))
        },
    }
}

/// Evaluate context/semantic linking against test case criteria.
pub fn evaluate_context(linked_notes: &[String], test_case: &ContextTestCase) -> EvalResult {
    // Check for expected connections
    let expected_matches = test_case
        .expected_connections
        .iter()
        .filter(|conn| linked_notes.contains(conn))
        .count();
    let expected_ratio = if test_case.expected_connections.is_empty() {
        1.0
    } else {
        expected_matches as f32 / test_case.expected_connections.len() as f32
    };

    // Check for unrelated notes (should not be linked)
    let unrelated_matches = test_case
        .unrelated_notes
        .iter()
        .filter(|note| linked_notes.contains(note))
        .count();
    let no_unrelated = unrelated_matches == 0;

    let score = (expected_ratio * 0.7) + (if no_unrelated { 0.3 } else { 0.0 });
    // Must have no unrelated notes AND meet score threshold
    let passed = score >= 0.7 && no_unrelated;

    EvalResult {
        test_id: test_case.id.clone(),
        passed,
        score,
        latency_ms: 0,
        output: linked_notes.join(", "),
        expected: Some(test_case.expected_connections.join(", ")),
        notes: if !passed {
            Some(format!(
                "Expected: {}/{}, Unrelated: {}",
                expected_matches,
                test_case.expected_connections.len(),
                unrelated_matches
            ))
        } else {
            None
        },
    }
}

/// Evaluate long context understanding.
pub fn evaluate_long_context(answer: &str, test_case: &LongContextTestCase) -> EvalResult {
    let answer_lower = answer.to_lowercase();

    // Check for expected facts in the answer
    let facts_found = test_case
        .expected_facts
        .iter()
        .filter(|fact| answer_lower.contains(&fact.to_lowercase()))
        .count();

    let fact_ratio = if test_case.expected_facts.is_empty() {
        1.0
    } else {
        facts_found as f32 / test_case.expected_facts.len() as f32
    };

    // Check that answer is not empty
    let has_answer = !answer.trim().is_empty();

    let score = (fact_ratio * 0.8) + (if has_answer { 0.2 } else { 0.0 });
    let passed = score >= 0.7;

    EvalResult {
        test_id: test_case.id.clone(),
        passed,
        score,
        latency_ms: 0,
        output: answer.to_string(),
        expected: Some(format!(
            "{} facts from {}w context",
            test_case.expected_facts.len(),
            test_case.context_length_words
        )),
        notes: if !passed {
            Some(format!(
                "Facts found: {}/{}, Answer length: {}",
                facts_found,
                test_case.expected_facts.len(),
                answer.len()
            ))
        } else {
            None
        },
    }
}

/// Calculate cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Evaluate semantic similarity test case.
pub fn evaluate_semantic(
    query_embedding: &[f32],
    positive_embeddings: &[Vec<f32>],
    negative_embeddings: &[Vec<f32>],
    test_case: &SemanticTestCase,
) -> EvalResult {
    // Calculate similarities
    let positive_sims: Vec<f32> = positive_embeddings
        .iter()
        .map(|e| cosine_similarity(query_embedding, e))
        .collect();

    let negative_sims: Vec<f32> = negative_embeddings
        .iter()
        .map(|e| cosine_similarity(query_embedding, e))
        .collect();

    let min_positive = positive_sims
        .iter()
        .cloned()
        .reduce(f32::min)
        .unwrap_or(0.0);
    let max_negative = negative_sims
        .iter()
        .cloned()
        .reduce(f32::max)
        .unwrap_or(1.0);

    // Pass if all positive similarities > all negative similarities
    let passed = min_positive > max_negative;
    let score = min_positive - max_negative + 0.5; // Normalize to 0-1 range approximately

    EvalResult {
        test_id: test_case.id.clone(),
        passed,
        score: score.clamp(0.0, 1.0),
        latency_ms: 0,
        output: format!(
            "Positive sims: {:?}, Negative sims: {:?}",
            positive_sims, negative_sims
        ),
        expected: Some("positive > negative".to_string()),
        notes: if !passed {
            Some(format!(
                "Min positive: {:.3}, Max negative: {:.3}",
                min_positive, max_negative
            ))
        } else {
            None
        },
    }
}

/// Full evaluation report.
#[derive(Clone, Serialize, Deserialize)]
pub struct EvalReport {
    /// Model evaluated.
    pub model: String,
    /// Evaluation timestamp.
    pub timestamp: String,
    /// Summary per capability.
    pub summaries: HashMap<Capability, EvalSummary>,
    /// Derived model capabilities.
    pub capabilities: ModelCapabilities,
}

impl fmt::Debug for EvalReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalReport")
            .field("model_len", &self.model.len())
            .field("timestamp_len", &self.timestamp.len())
            .field("summary_count", &self.summaries.len())
            .field("capabilities_present", &true)
            .finish()
    }
}

impl EvalReport {
    /// Create a new report.
    pub fn new(model: impl Into<String>) -> Self {
        let model = model.into();
        Self {
            model: model.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            summaries: HashMap::new(),
            capabilities: ModelCapabilities::new(model),
        }
    }

    /// Add a summary for a capability.
    pub fn add_summary(&mut self, summary: EvalSummary) {
        let rating = summary.to_rating();
        self.capabilities.add_rating(rating);
        self.summaries.insert(summary.capability, summary);
    }

    /// Get overall pass rate.
    pub fn overall_pass_rate(&self) -> f32 {
        if self.summaries.is_empty() {
            return 0.0;
        }

        let total_passed: usize = self.summaries.values().map(|s| s.passed).sum();
        let total_tests: usize = self.summaries.values().map(|s| s.total).sum();

        if total_tests > 0 {
            total_passed as f32 / total_tests as f32
        } else {
            0.0
        }
    }

    /// Generate a text summary.
    pub fn text_summary(&self) -> String {
        let mut lines = vec![
            format!("# Evaluation Report: {}", self.model),
            format!("Timestamp: {}", self.timestamp),
            format!(
                "Overall Pass Rate: {:.1}%",
                self.overall_pass_rate() * 100.0
            ),
            String::new(),
            "## Capability Scores".to_string(),
        ];

        for (cap, summary) in &self.summaries {
            lines.push(format!(
                "- {:?}: {:.1}% ({}/{} passed, P95: {}ms)",
                cap,
                summary.avg_score * 100.0,
                summary.passed,
                summary.total,
                summary.latency_p95_ms
            ));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_title_test_suite() {
        let suite = title_generation_suite();
        assert!(!suite.is_empty());
        assert!(suite.iter().all(|t| !t.expected_keywords.is_empty()));
    }

    #[test]
    fn test_semantic_test_suite() {
        let suite = semantic_similarity_suite();
        assert!(!suite.is_empty());
        assert!(suite.iter().all(|t| !t.positive.is_empty()));
    }

    #[test]
    fn test_evaluate_title_good() {
        let test = TitleTestCase {
            id: "test-1".to_string(),
            content: "Rust programming language features".to_string(),
            expected_keywords: vec!["Rust".to_string(), "programming".to_string()],
            max_length: 50,
        };

        let result = evaluate_title("Rust Programming Guide", &test);
        assert!(result.passed);
        assert!(result.score >= 0.7);
    }

    #[test]
    fn test_evaluate_title_bad_format() {
        let test = TitleTestCase {
            id: "test-2".to_string(),
            content: "test".to_string(),
            expected_keywords: vec!["test".to_string()],
            max_length: 100,
        };

        let result = evaluate_title("# Test Title", &test);
        assert!(result.score < 1.0); // Should be penalized for markdown
    }

    #[test]
    fn test_evaluate_tags_good() {
        let test = TagTestCase {
            id: "tag-1".to_string(),
            content: "Rust programming article".to_string(),
            expected_tags: vec!["rust".to_string(), "programming".to_string()],
            forbidden_tags: vec!["java".to_string()],
            max_tags: 5,
        };

        let output_tags = vec![
            "rust".to_string(),
            "programming".to_string(),
            "systems".to_string(),
        ];
        let result = evaluate_tags(&output_tags, &test);
        assert!(result.passed);
        assert!(result.score >= 0.7);
    }

    #[test]
    fn test_evaluate_tags_forbidden() {
        let test = TagTestCase {
            id: "tag-2".to_string(),
            content: "Test content".to_string(),
            expected_tags: vec!["test".to_string()],
            forbidden_tags: vec!["forbidden".to_string()],
            max_tags: 3,
        };

        let output_tags = vec!["test".to_string(), "forbidden".to_string()];
        let result = evaluate_tags(&output_tags, &test);
        assert!(!result.passed); // Should fail due to forbidden tag
    }

    #[test]
    fn test_evaluate_tags_too_many() {
        let test = TagTestCase {
            id: "tag-3".to_string(),
            content: "Test content".to_string(),
            expected_tags: vec!["test".to_string()],
            forbidden_tags: vec![],
            max_tags: 2,
        };

        let output_tags = vec!["test".to_string(), "tag1".to_string(), "tag2".to_string()];
        let result = evaluate_tags(&output_tags, &test);
        assert!(result.score < 1.0); // Should be penalized for too many tags
    }

    #[test]
    fn test_evaluate_format_max_words() {
        let test = FormatTestCase {
            id: "format-1".to_string(),
            prompt: "Write a short response".to_string(),
            constraints: vec![FormatConstraint {
                constraint_type: "max_words".to_string(),
                value: serde_json::json!(10),
            }],
        };

        let output = "This is a short response with exactly nine words total.";
        let result = evaluate_format(output, &test);
        assert!(result.passed);
        assert!(result.score >= 0.8);
    }

    #[test]
    fn test_evaluate_format_contains() {
        let test = FormatTestCase {
            id: "format-2".to_string(),
            prompt: "Include the word ANSWER".to_string(),
            constraints: vec![FormatConstraint {
                constraint_type: "contains".to_string(),
                value: serde_json::json!("ANSWER"),
            }],
        };

        let output = "The ANSWER is 42.";
        let result = evaluate_format(output, &test);
        assert!(result.passed);
    }

    #[test]
    fn test_evaluate_format_multiple_constraints() {
        let test = FormatTestCase {
            id: "format-3".to_string(),
            prompt: "Test".to_string(),
            constraints: vec![
                FormatConstraint {
                    constraint_type: "max_words".to_string(),
                    value: serde_json::json!(20),
                },
                FormatConstraint {
                    constraint_type: "contains".to_string(),
                    value: serde_json::json!("test"),
                },
            ],
        };

        let output = "This is a test response.";
        let result = evaluate_format(output, &test);
        assert!(result.passed);
    }

    #[test]
    fn test_evaluate_context_good() {
        let test = ContextTestCase {
            id: "context-1".to_string(),
            note_content: "Rust programming concepts".to_string(),
            related_notes: vec!["Systems programming".to_string()],
            expected_connections: vec!["note-1".to_string(), "note-2".to_string()],
            unrelated_notes: vec!["note-99".to_string()],
        };

        let linked = vec!["note-1".to_string(), "note-2".to_string()];
        let result = evaluate_context(&linked, &test);
        assert!(result.passed);
        assert!(result.score >= 0.7);
    }

    #[test]
    fn test_evaluate_context_with_unrelated() {
        let test = ContextTestCase {
            id: "context-2".to_string(),
            note_content: "Test".to_string(),
            related_notes: vec![],
            expected_connections: vec!["note-1".to_string()],
            unrelated_notes: vec!["note-99".to_string()],
        };

        let linked = vec!["note-1".to_string(), "note-99".to_string()];
        let result = evaluate_context(&linked, &test);
        assert!(!result.passed); // Should fail due to unrelated note
    }

    #[test]
    fn test_evaluate_long_context_good() {
        let test = LongContextTestCase {
            id: "long-1".to_string(),
            context: "A long document...".to_string(),
            query: "What is the answer?".to_string(),
            expected_facts: vec!["fact1".to_string(), "fact2".to_string()],
            context_length_words: 1000,
        };

        let answer = "The answer includes fact1 and fact2.";
        let result = evaluate_long_context(answer, &test);
        assert!(result.passed);
        assert!(result.score >= 0.7);
    }

    #[test]
    fn test_evaluate_long_context_missing_facts() {
        let test = LongContextTestCase {
            id: "long-2".to_string(),
            context: "Context".to_string(),
            query: "Query".to_string(),
            expected_facts: vec![
                "fact1".to_string(),
                "fact2".to_string(),
                "fact3".to_string(),
            ],
            context_length_words: 500,
        };

        let answer = "The answer includes fact1.";
        let result = evaluate_long_context(answer, &test);
        assert!(!result.passed); // Should fail - missing facts
    }

    #[test]
    fn test_eval_tier() {
        assert_eq!(EvalTier::Smoke.test_count(), 20);
        assert_eq!(EvalTier::Core.test_count(), 75);
        assert_eq!(EvalTier::Extended.test_count(), 150);
        assert_eq!(EvalTier::Full.test_count(), 300);

        assert_eq!(EvalTier::Smoke.duration_minutes(), 1);
        assert_eq!(EvalTier::Full.duration_minutes(), 30);
    }

    #[test]
    fn test_load_jsonl_title() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_titles.jsonl");

        {
            let mut file = File::create(&test_file).unwrap();
            writeln!(file, r#"{{"id":"t1","content":"Test content","expected_keywords":["test"],"max_length":80}}"#).unwrap();
            writeln!(file, r#"{{"id":"t2","content":"More content","expected_keywords":["more"],"max_length":100}}"#).unwrap();
        }

        let cases = load_title_tests(&test_file).unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].id, "t1");
        assert_eq!(cases[1].id, "t2");

        std::fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_load_jsonl_tags() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_tags.jsonl");

        {
            let mut file = File::create(&test_file).unwrap();
            writeln!(file, r#"{{"id":"tag1","content":"Test","expected_tags":["test"],"forbidden_tags":["bad"],"max_tags":5}}"#).unwrap();
        }

        let cases = load_tag_tests(&test_file).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].id, "tag1");
        assert_eq!(cases[0].max_tags, 5);

        std::fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_load_jsonl_format() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_format.jsonl");

        {
            let mut file = File::create(&test_file).unwrap();
            writeln!(file, r#"{{"id":"f1","prompt":"Test prompt","constraints":[{{"type":"max_words","value":10}}]}}"#).unwrap();
        }

        let cases = load_format_tests(&test_file).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].id, "f1");
        assert_eq!(cases[0].constraints.len(), 1);

        std::fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_load_jsonl_context() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_context.jsonl");

        {
            let mut file = File::create(&test_file).unwrap();
            writeln!(file, r#"{{"id":"c1","note_content":"Test","related_notes":["n1"],"expected_connections":["n1"],"unrelated_notes":["n99"]}}"#).unwrap();
        }

        let cases = load_context_tests(&test_file).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].id, "c1");

        std::fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_load_jsonl_long_context() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_long_context.jsonl");

        {
            let mut file = File::create(&test_file).unwrap();
            writeln!(file, r#"{{"id":"l1","context":"Long text","query":"What?","expected_facts":["fact"],"context_length_words":1000}}"#).unwrap();
        }

        let cases = load_long_context_tests(&test_file).unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].id, "l1");
        assert_eq!(cases[0].context_length_words, 1000);

        std::fs::remove_file(test_file).ok();
    }

    #[test]
    fn eval_debug_redacts_cases_outputs_prompts_and_reports() {
        let result = EvalResult {
            test_id: "case-jane@example.com".to_string(),
            passed: false,
            score: 0.4,
            latency_ms: 123,
            output: "Generated output with sk-private and +1-555-0100".to_string(),
            expected: Some("Expected private answer for jane@example.com".to_string()),
            notes: Some("Backend note https://tenant.example".to_string()),
        };
        let summary = EvalSummary::from_results(
            "suite-jane@example.com",
            "model-sk-private",
            Capability::TitleGeneration,
            std::slice::from_ref(&result),
        );
        let title = TitleTestCase {
            id: "title-jane@example.com".to_string(),
            content: "Private note sk-private".to_string(),
            expected_keywords: vec!["jane@example.com".to_string()],
            max_length: 80,
        };
        let semantic = SemanticTestCase {
            id: "semantic-private".to_string(),
            query: "Query with https://tenant.example".to_string(),
            positive: vec!["positive jane@example.com".to_string()],
            negative: vec!["negative sk-private".to_string()],
        };
        let revision = RevisionTestCase {
            id: "revision-private".to_string(),
            input: "Input +1-555-0100".to_string(),
            required_elements: vec!["required jane@example.com".to_string()],
            forbidden_elements: vec!["forbidden sk-private".to_string()],
        };
        let tag = TagTestCase {
            id: "tag-private".to_string(),
            content: "Content jane@example.com".to_string(),
            expected_tags: vec!["tag/sk-private".to_string()],
            forbidden_tags: vec!["private".to_string()],
            max_tags: 5,
        };
        let format_case = FormatTestCase {
            id: "format-private".to_string(),
            prompt: "Prompt with sk-private".to_string(),
            constraints: vec![FormatConstraint {
                constraint_type: "contains".to_string(),
                value: serde_json::json!("jane@example.com"),
            }],
        };
        let context = ContextTestCase {
            id: "context-private".to_string(),
            note_content: "Note jane@example.com".to_string(),
            related_notes: vec!["Related sk-private".to_string()],
            expected_connections: vec!["https://tenant.example".to_string()],
            unrelated_notes: vec!["+1-555-0100".to_string()],
        };
        let long_context = LongContextTestCase {
            id: "long-private".to_string(),
            context: "Long context sk-private".to_string(),
            query: "Query jane@example.com".to_string(),
            expected_facts: vec!["fact https://tenant.example".to_string()],
            context_length_words: 999,
        };
        let judge_prompt = JudgePrompt {
            name: "judge-jane@example.com".to_string(),
            prompt_type: "single".to_string(),
            category: "private".to_string(),
            system_prompt: "System sk-private".to_string(),
            prompt_template: "Template https://tenant.example".to_string(),
            output_format: "JSON with jane@example.com".to_string(),
            scoring: Some(ScoringConfig { min: 1, max: 10 }),
        };
        let judge_result = JudgeResult {
            prompt_name: "judge-jane@example.com".to_string(),
            score: Some(6.0),
            winner: Some("winner-sk-private".to_string()),
            reasoning: "Reasoning mentions https://tenant.example".to_string(),
            raw_output: "Raw output jane@example.com".to_string(),
        };
        let mut report = EvalReport::new("model-jane@example.com");
        report.add_summary(summary.clone());

        let debug = format!(
            "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
            result,
            summary,
            title,
            semantic,
            revision,
            tag,
            format_case,
            context,
            long_context,
            judge_prompt,
            judge_result,
            report
        );

        assert!(debug.contains("output_len"));
        assert!(debug.contains("model_len"));
        assert!(debug.contains("content_len"));
        assert!(debug.contains("query_len"));
        assert!(debug.contains("prompt_len"));
        assert!(debug.contains("reasoning_len"));
        assert!(debug.contains("summary_count"));
        assert!(!debug.contains("jane@example.com"));
        assert!(!debug.contains("sk-private"));
        assert!(!debug.contains("tenant.example"));
        assert!(!debug.contains("+1-555-0100"));
        assert!(!debug.contains("Generated output"));
        assert!(!debug.contains("Expected private"));
        assert!(!debug.contains("System sk"));
        assert!(!debug.contains("Reasoning mentions"));
        assert!(!debug.contains("Raw output"));
    }

    #[test]
    fn test_load_jsonl_empty_lines() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_empty.jsonl");

        {
            let mut file = File::create(&test_file).unwrap();
            writeln!(
                file,
                r#"{{"id":"t1","content":"Test","expected_keywords":["test"],"max_length":80}}"#
            )
            .unwrap();
            writeln!(file).unwrap(); // Empty line
            writeln!(file, "   ").unwrap(); // Whitespace only
            writeln!(
                file,
                r#"{{"id":"t2","content":"More","expected_keywords":["more"],"max_length":80}}"#
            )
            .unwrap();
        }

        let cases = load_title_tests(&test_file).unwrap();
        assert_eq!(cases.len(), 2); // Should skip empty lines

        std::fs::remove_file(test_file).ok();
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_evaluate_semantic() {
        let test = SemanticTestCase {
            id: "test-1".to_string(),
            query: "cat".to_string(),
            positive: vec!["kitten".to_string()],
            negative: vec!["python".to_string()],
        };

        // Simulated embeddings where positive is more similar
        let query = vec![1.0, 0.0, 0.0];
        let positive = vec![vec![0.9, 0.1, 0.0]];
        let negative = vec![vec![0.0, 0.0, 1.0]];

        let result = evaluate_semantic(&query, &positive, &negative, &test);
        assert!(result.passed);
    }

    #[test]
    fn test_eval_summary() {
        let results = vec![
            EvalResult {
                test_id: "1".to_string(),
                passed: true,
                score: 0.9,
                latency_ms: 100,
                output: "test".to_string(),
                expected: None,
                notes: None,
            },
            EvalResult {
                test_id: "2".to_string(),
                passed: true,
                score: 0.8,
                latency_ms: 200,
                output: "test".to_string(),
                expected: None,
                notes: None,
            },
        ];

        let summary =
            EvalSummary::from_results("title", "test-model", Capability::TitleGeneration, &results);

        assert_eq!(summary.passed, 2);
        assert_eq!(summary.total, 2);
        assert!((summary.avg_score - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_eval_report() {
        let mut report = EvalReport::new("test-model");

        let results = vec![EvalResult {
            test_id: "1".to_string(),
            passed: true,
            score: 0.9,
            latency_ms: 100,
            output: "test".to_string(),
            expected: None,
            notes: None,
        }];

        let summary =
            EvalSummary::from_results("title", "test-model", Capability::TitleGeneration, &results);

        report.add_summary(summary);

        assert_eq!(report.overall_pass_rate(), 1.0);
        assert!(report.text_summary().contains("test-model"));
    }

    // LLM-as-Judge tests
    #[test]
    fn test_parse_judge_score() {
        let config = ScoringConfig { min: 1, max: 10 };

        assert!((parse_judge_score("Score: 8", &config).unwrap() - 0.778).abs() < 0.01);
        assert!((parse_judge_score("Rating: 5", &config).unwrap() - 0.444).abs() < 0.01);
        assert!((parse_judge_score("**7**/10", &config).unwrap() - 0.667).abs() < 0.01);
        assert!(parse_judge_score("No score here", &config).is_none());
    }

    #[test]
    fn test_parse_pairwise_winner() {
        assert_eq!(
            parse_pairwise_winner("Based on my analysis, [[A]] is better"),
            Some("A".to_string())
        );
        assert_eq!(
            parse_pairwise_winner("I prefer [[B]]"),
            Some("B".to_string())
        );
        assert_eq!(
            parse_pairwise_winner("It's a tie [[C]]"),
            Some("C".to_string())
        );
        assert_eq!(parse_pairwise_winner("No clear winner"), None);
    }

    #[test]
    fn test_format_judge_prompt() {
        let template = "Content: {content}\nTitle: {title}";
        let mut vars = HashMap::new();
        vars.insert("content".to_string(), "My note".to_string());
        vars.insert("title".to_string(), "My Title".to_string());

        let result = format_judge_prompt(template, &vars);
        assert_eq!(result, "Content: My note\nTitle: My Title");
    }
}
