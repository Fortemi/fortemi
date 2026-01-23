//! Thinking model detection and response parsing.
//!
//! This module provides utilities to detect thinking patterns in model responses
//! and extract reasoning content from models that perform chain-of-thought or
//! step-by-step reasoning.
//!
//! Supports three main thinking types:
//! - Explicit tags: `<think>...</think>` delimiters
//! - Verbose reasoning: Step-by-step patterns like "Step 1:", "Let me think"
//! - Pattern-based: Structured patterns like "First,", "Second,", "Therefore"

use crate::profiles::ThinkingType;

/// Result of parsing a thinking model response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingResponse {
    /// The thinking/reasoning content extracted from the response.
    pub thinking_content: Option<String>,
    /// The final answer or output content (after reasoning).
    pub answer_content: String,
    /// The detected thinking type.
    pub thinking_type: ThinkingType,
}

/// Detects the thinking type from response text.
///
/// Analyzes the response for patterns that indicate thinking/reasoning:
/// - Explicit `<think>...</think>` tags
/// - Verbose reasoning patterns ("Step 1:", "Let me think", etc.)
/// - Structured reasoning patterns ("First,", "Second,", "Therefore", etc.)
///
/// # Examples
///
/// ```
/// use matric_inference::thinking::detect_thinking_type;
/// use matric_inference::ThinkingType;
///
/// let explicit = "<think>Let me analyze this...</think>The answer is 42.";
/// assert_eq!(detect_thinking_type(explicit), ThinkingType::ExplicitTags);
///
/// let verbose = "Step 1: First, let me understand the problem.\nStep 2: Now I'll solve it.\nAnswer: 42";
/// assert_eq!(detect_thinking_type(verbose), ThinkingType::VerboseReasoning);
///
/// let pattern = "First, we need to consider X. Second, we analyze Y. Therefore, the answer is 42.";
/// assert_eq!(detect_thinking_type(pattern), ThinkingType::PatternBased);
///
/// let none = "The answer is simply 42.";
/// assert_eq!(detect_thinking_type(none), ThinkingType::None);
/// ```
pub fn detect_thinking_type(response: &str) -> ThinkingType {
    // Check for explicit tags first (highest priority)
    if has_explicit_tags(response) {
        return ThinkingType::ExplicitTags;
    }

    // Check for verbose reasoning patterns
    if has_verbose_reasoning(response) {
        return ThinkingType::VerboseReasoning;
    }

    // Check for pattern-based reasoning
    if has_pattern_based_reasoning(response) {
        return ThinkingType::PatternBased;
    }

    ThinkingType::None
}

/// Parses a thinking model response and extracts thinking and answer content.
///
/// Automatically detects the thinking type and extracts the appropriate content.
///
/// # Examples
///
/// ```
/// use matric_inference::thinking::parse_thinking_response;
///
/// let response = "<think>Analyzing the problem...</think>The answer is 42.";
/// let parsed = parse_thinking_response(response);
/// assert_eq!(parsed.thinking_content, Some("Analyzing the problem...".to_string()));
/// assert_eq!(parsed.answer_content, "The answer is 42.");
/// ```
pub fn parse_thinking_response(response: &str) -> ThinkingResponse {
    let thinking_type = detect_thinking_type(response);

    match thinking_type {
        ThinkingType::ExplicitTags => parse_explicit_tags(response),
        ThinkingType::VerboseReasoning => parse_verbose_reasoning(response),
        ThinkingType::PatternBased => parse_pattern_based(response),
        ThinkingType::None | ThinkingType::NotTested => ThinkingResponse {
            thinking_content: None,
            answer_content: response.to_string(),
            thinking_type,
        },
    }
}

// =============================================================================
// Explicit Tags Detection and Parsing
// =============================================================================

/// Checks if response contains explicit `<think>...</think>` tags.
/// Detects even with just an opening tag (unclosed thinking block).
fn has_explicit_tags(response: &str) -> bool {
    response.contains("<think>")
}

/// Extracts content from explicit `<think>...</think>` tags.
fn parse_explicit_tags(response: &str) -> ThinkingResponse {
    let mut thinking_content = String::new();
    let mut answer_content = String::new();
    let mut current_pos = 0;

    while current_pos < response.len() {
        if let Some(think_start) = response[current_pos..].find("<think>") {
            let absolute_start = current_pos + think_start;

            // Add any content before the tag to answer
            if current_pos < absolute_start {
                answer_content.push_str(&response[current_pos..absolute_start]);
            }

            current_pos = absolute_start + "<think>".len();

            // Find the closing tag
            if let Some(think_end) = response[current_pos..].find("</think>") {
                let absolute_end = current_pos + think_end;

                // Extract thinking content
                if !thinking_content.is_empty() {
                    thinking_content.push('\n');
                }
                thinking_content.push_str(&response[current_pos..absolute_end]);

                current_pos = absolute_end + "</think>".len();
            } else {
                // Unclosed tag - treat rest as thinking content
                thinking_content.push_str(&response[current_pos..]);
                break;
            }
        } else {
            // No more tags - rest is answer content
            answer_content.push_str(&response[current_pos..]);
            break;
        }
    }

    ThinkingResponse {
        thinking_content: if thinking_content.is_empty() {
            None
        } else {
            Some(thinking_content.trim().to_string())
        },
        answer_content: answer_content.trim().to_string(),
        thinking_type: ThinkingType::ExplicitTags,
    }
}

// =============================================================================
// Verbose Reasoning Detection and Parsing
// =============================================================================

/// Checks if response contains verbose reasoning patterns.
fn has_verbose_reasoning(response: &str) -> bool {
    let patterns = [
        "step 1:",
        "step 2:",
        "step 3:",
        "let me think",
        "let's think",
        "let me analyze",
        "let's analyze",
        "thinking through",
        "reasoning:",
        "analysis:",
    ];

    let lower = response.to_lowercase();
    patterns.iter().any(|&pattern| lower.contains(pattern))
}

/// Parses verbose reasoning from response.
fn parse_verbose_reasoning(response: &str) -> ThinkingResponse {
    let lines: Vec<&str> = response.lines().collect();
    let mut thinking_lines = Vec::new();
    let mut answer_lines = Vec::new();
    let mut in_reasoning = false;

    for line in lines {
        let lower = line.to_lowercase();

        // Check if this line starts a reasoning section
        if lower.contains("step 1:")
            || lower.contains("let me think")
            || lower.contains("let's think")
            || lower.contains("let me analyze")
            || lower.contains("let's analyze")
            || lower.contains("thinking through")
        {
            in_reasoning = true;
            thinking_lines.push(line);
            continue;
        }

        // Check if we're still in a step-by-step section
        if in_reasoning
            && (lower.contains("step ")
                || lower.starts_with("- ")
                || lower.starts_with("* ")
                || (line.starts_with(' ') && !line.trim().is_empty()))
        {
            thinking_lines.push(line);
            continue;
        }

        // Check for explicit answer markers that end reasoning
        if lower.contains("answer:")
            || lower.contains("conclusion:")
            || lower.contains("result:")
            || lower.contains("therefore,")
        {
            in_reasoning = false;
        }

        // Not in reasoning section - add to answer
        if !in_reasoning && !line.trim().is_empty() {
            answer_lines.push(line);
        } else if in_reasoning {
            thinking_lines.push(line);
        }
    }

    let thinking_content = if thinking_lines.is_empty() {
        None
    } else {
        Some(thinking_lines.join("\n").trim().to_string())
    };

    let answer_content = if answer_lines.is_empty() {
        response.to_string()
    } else {
        answer_lines.join("\n").trim().to_string()
    };

    ThinkingResponse {
        thinking_content,
        answer_content,
        thinking_type: ThinkingType::VerboseReasoning,
    }
}

// =============================================================================
// Pattern-Based Reasoning Detection and Parsing
// =============================================================================

/// Checks if response contains pattern-based reasoning.
fn has_pattern_based_reasoning(response: &str) -> bool {
    let lower = response.to_lowercase();

    // Count transition words and conclusion markers separately
    let transition_patterns = ["first,", "second,", "third,", "next,", "then,", "finally,"];
    let conclusion_patterns = ["therefore,", "thus,", "hence,", "in conclusion,"];

    let transition_count = transition_patterns
        .iter()
        .filter(|&p| lower.contains(p))
        .count();
    let conclusion_count = conclusion_patterns
        .iter()
        .filter(|&p| lower.contains(p))
        .count();

    // Must have at least 2 patterns total, with at least one being a transition word
    // OR at least one transition word and one conclusion word
    let total_count = transition_count + conclusion_count;
    total_count >= 2 && (transition_count >= 1 || conclusion_count >= 2)
}

/// Parses pattern-based reasoning from response.
fn parse_pattern_based(response: &str) -> ThinkingResponse {
    let lower = response.to_lowercase();

    // Find the last conclusion marker
    let conclusion_patterns = [
        "therefore,",
        "thus,",
        "hence,",
        "in conclusion,",
        "to conclude,",
        "in summary,",
    ];

    let mut conclusion_pos = None;
    for pattern in conclusion_patterns {
        if let Some(pos) = lower.rfind(pattern) {
            conclusion_pos = Some(pos);
            break;
        }
    }

    if let Some(pos) = conclusion_pos {
        // Everything before conclusion is thinking
        let thinking = response[..pos].trim();
        // Everything from conclusion onward is answer
        let answer = response[pos..].trim();

        ThinkingResponse {
            thinking_content: if thinking.is_empty() {
                None
            } else {
                Some(thinking.to_string())
            },
            answer_content: answer.to_string(),
            thinking_type: ThinkingType::PatternBased,
        }
    } else {
        // No clear conclusion marker - treat first 60% as thinking, rest as answer
        let split_point = (response.len() as f32 * 0.6) as usize;

        // Find a sentence boundary near the split point
        let actual_split = response[..split_point]
            .rfind('.')
            .map(|pos| pos + 1)
            .unwrap_or(split_point);

        let thinking = response[..actual_split].trim();
        let answer = response[actual_split..].trim();

        ThinkingResponse {
            thinking_content: if thinking.is_empty() {
                None
            } else {
                Some(thinking.to_string())
            },
            answer_content: if answer.is_empty() {
                response.to_string()
            } else {
                answer.to_string()
            },
            thinking_type: ThinkingType::PatternBased,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // ThinkingResponse Tests
    // ==========================================================================

    #[test]
    fn test_thinking_response_creation() {
        let response = ThinkingResponse {
            thinking_content: Some("reasoning here".to_string()),
            answer_content: "answer here".to_string(),
            thinking_type: ThinkingType::ExplicitTags,
        };

        assert_eq!(
            response.thinking_content,
            Some("reasoning here".to_string())
        );
        assert_eq!(response.answer_content, "answer here");
        assert_eq!(response.thinking_type, ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_thinking_response_no_thinking() {
        let response = ThinkingResponse {
            thinking_content: None,
            answer_content: "just an answer".to_string(),
            thinking_type: ThinkingType::None,
        };

        assert!(response.thinking_content.is_none());
        assert_eq!(response.answer_content, "just an answer");
    }

    // ==========================================================================
    // Explicit Tags Detection Tests
    // ==========================================================================

    #[test]
    fn test_detect_explicit_tags_basic() {
        let response = "<think>reasoning</think>answer";
        assert_eq!(detect_thinking_type(response), ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_detect_explicit_tags_multiple() {
        let response = "<think>step1</think>partial<think>step2</think>final";
        assert_eq!(detect_thinking_type(response), ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_detect_explicit_tags_multiline() {
        let response = "<think>\nline1\nline2\n</think>\nanswer";
        assert_eq!(detect_thinking_type(response), ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_detect_explicit_tags_only_opening() {
        let response = "<think>reasoning but no closing tag";
        assert_eq!(detect_thinking_type(response), ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_detect_explicit_tags_only_closing() {
        let response = "no opening tag</think>";
        assert_eq!(detect_thinking_type(response), ThinkingType::None);
    }

    #[test]
    fn test_detect_explicit_tags_empty() {
        let response = "<think></think>answer";
        assert_eq!(detect_thinking_type(response), ThinkingType::ExplicitTags);
    }

    // ==========================================================================
    // Explicit Tags Parsing Tests
    // ==========================================================================

    #[test]
    fn test_parse_explicit_tags_basic() {
        let response = "<think>reasoning here</think>answer here";
        let parsed = parse_thinking_response(response);

        assert_eq!(parsed.thinking_content, Some("reasoning here".to_string()));
        assert_eq!(parsed.answer_content, "answer here");
        assert_eq!(parsed.thinking_type, ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_parse_explicit_tags_multiple_blocks() {
        let response = "<think>first thought</think>partial<think>second thought</think>final";
        let parsed = parse_thinking_response(response);

        assert_eq!(
            parsed.thinking_content,
            Some("first thought\nsecond thought".to_string())
        );
        assert_eq!(parsed.answer_content, "partialfinal");
    }

    #[test]
    fn test_parse_explicit_tags_with_whitespace() {
        let response = "  <think>  reasoning  </think>  answer  ";
        let parsed = parse_thinking_response(response);

        assert_eq!(parsed.thinking_content, Some("reasoning".to_string()));
        assert_eq!(parsed.answer_content, "answer");
    }

    #[test]
    fn test_parse_explicit_tags_multiline_content() {
        let response = "<think>\nStep 1: analyze\nStep 2: solve\n</think>\nThe answer is 42.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        let thinking = parsed.thinking_content.unwrap();
        assert!(thinking.contains("Step 1"));
        assert!(thinking.contains("Step 2"));
        assert_eq!(parsed.answer_content, "The answer is 42.");
    }

    #[test]
    fn test_parse_explicit_tags_empty_tags() {
        let response = "<think></think>answer";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_none());
        assert_eq!(parsed.answer_content, "answer");
    }

    #[test]
    fn test_parse_explicit_tags_unclosed() {
        let response = "<think>reasoning without closing";
        let parsed = parse_thinking_response(response);

        assert_eq!(
            parsed.thinking_content,
            Some("reasoning without closing".to_string())
        );
        assert_eq!(parsed.answer_content, "");
    }

    #[test]
    fn test_parse_explicit_tags_no_answer() {
        let response = "<think>only thinking</think>";
        let parsed = parse_thinking_response(response);

        assert_eq!(parsed.thinking_content, Some("only thinking".to_string()));
        assert_eq!(parsed.answer_content, "");
    }

    #[test]
    fn test_parse_explicit_tags_answer_before_thinking() {
        let response = "Some intro text <think>reasoning</think> final answer";
        let parsed = parse_thinking_response(response);

        assert_eq!(parsed.thinking_content, Some("reasoning".to_string()));
        assert_eq!(parsed.answer_content, "Some intro text  final answer");
    }

    // ==========================================================================
    // Verbose Reasoning Detection Tests
    // ==========================================================================

    #[test]
    fn test_detect_verbose_step_by_step() {
        let response = "Step 1: analyze\nStep 2: solve\nAnswer: 42";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    #[test]
    fn test_detect_verbose_let_me_think() {
        let response = "Let me think about this problem.\nThe answer is 42.";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    #[test]
    fn test_detect_verbose_lets_think() {
        let response = "Let's think through this carefully.\nSolution: 42";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    #[test]
    fn test_detect_verbose_analyze() {
        let response = "Let me analyze this situation.\nResult: success";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    #[test]
    fn test_detect_verbose_thinking_through() {
        let response = "Thinking through the problem step by step...";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    #[test]
    fn test_detect_verbose_reasoning_label() {
        let response = "Reasoning:\n- Point 1\n- Point 2\nConclusion: yes";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    #[test]
    fn test_detect_verbose_case_insensitive() {
        let response = "STEP 1: FIRST STEP\nSTEP 2: SECOND STEP";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    // ==========================================================================
    // Verbose Reasoning Parsing Tests
    // ==========================================================================

    #[test]
    fn test_parse_verbose_step_by_step() {
        let response = "Step 1: understand problem\nStep 2: solve it\nAnswer: The solution is 42.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        let thinking = parsed.thinking_content.unwrap();
        assert!(thinking.contains("Step 1"));
        assert!(thinking.contains("Step 2"));
        assert_eq!(parsed.answer_content, "Answer: The solution is 42.");
    }

    #[test]
    fn test_parse_verbose_let_me_think() {
        let response = "Let me think about this.\nThe answer is clear now.\nFinal answer: 42";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("answer"));
    }

    #[test]
    fn test_parse_verbose_with_answer_marker() {
        let response = "Step 1: first\nStep 2: second\nAnswer: 42";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        assert_eq!(parsed.answer_content, "Answer: 42");
    }

    #[test]
    fn test_parse_verbose_with_conclusion_marker() {
        let response = "Let me analyze:\n- Point A\n- Point B\nConclusion: The result is positive.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("Conclusion"));
    }

    #[test]
    fn test_parse_verbose_indented_list() {
        let response = "Step 1: first\n  - sub point a\n  - sub point b\nStep 2: second\nDone.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        let thinking = parsed.thinking_content.unwrap();
        assert!(thinking.contains("sub point"));
    }

    // ==========================================================================
    // Pattern-Based Detection Tests
    // ==========================================================================

    #[test]
    fn test_detect_pattern_basic_sequence() {
        let response = "First, we analyze. Second, we conclude. Therefore, the answer is 42.";
        assert_eq!(detect_thinking_type(response), ThinkingType::PatternBased);
    }

    #[test]
    fn test_detect_pattern_three_steps() {
        let response = "First, examine X. Next, consider Y. Finally, we get Z.";
        assert_eq!(detect_thinking_type(response), ThinkingType::PatternBased);
    }

    #[test]
    fn test_detect_pattern_thus_hence() {
        let response = "The data shows X and Y. Thus, we conclude Z. Hence, the answer is clear.";
        assert_eq!(detect_thinking_type(response), ThinkingType::PatternBased);
    }

    #[test]
    fn test_detect_pattern_in_conclusion() {
        let response = "We observe A and B. Therefore, C is true. In conclusion, the answer is D.";
        assert_eq!(detect_thinking_type(response), ThinkingType::PatternBased);
    }

    #[test]
    fn test_detect_pattern_insufficient() {
        // Only one pattern word - should not qualify
        let response = "First, we note that the answer is 42.";
        assert_eq!(detect_thinking_type(response), ThinkingType::None);
    }

    #[test]
    fn test_detect_pattern_case_insensitive() {
        let response = "FIRST, analyze. SECOND, conclude. THEREFORE, answer is 42.";
        assert_eq!(detect_thinking_type(response), ThinkingType::PatternBased);
    }

    // ==========================================================================
    // Pattern-Based Parsing Tests
    // ==========================================================================

    #[test]
    fn test_parse_pattern_with_therefore() {
        let response = "First, we see X. Second, we note Y. Therefore, the answer is 42.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        let thinking = parsed.thinking_content.unwrap();
        assert!(thinking.contains("First"));
        assert!(thinking.contains("Second"));
        assert!(parsed.answer_content.contains("Therefore"));
        assert!(parsed.answer_content.contains("42"));
    }

    #[test]
    fn test_parse_pattern_with_thus() {
        let response = "We observe A and B. First, consider X. Thus, C is the conclusion.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.starts_with("Thus"));
    }

    #[test]
    fn test_parse_pattern_with_in_conclusion() {
        let response =
            "First, point one is X. Second, point two is Y. In conclusion, the result is Z.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("In conclusion"));
    }

    #[test]
    fn test_parse_pattern_no_explicit_conclusion() {
        let response =
            "First, examine the data. Second, analyze patterns. Third, draw insights. The final answer is 42.";
        let parsed = parse_thinking_response(response);

        // Should split around 60% mark
        assert!(parsed.thinking_content.is_some());
        assert!(!parsed.answer_content.is_empty());
    }

    #[test]
    fn test_parse_pattern_multiple_conclusions() {
        let response = "First, A. Therefore, B. Second, C. Hence, the final answer is D.";
        let parsed = parse_thinking_response(response);

        // Should use the LAST conclusion marker
        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("Hence"));
    }

    // ==========================================================================
    // No Thinking Detection Tests
    // ==========================================================================

    #[test]
    fn test_detect_none_simple_answer() {
        let response = "The answer is 42.";
        assert_eq!(detect_thinking_type(response), ThinkingType::None);
    }

    #[test]
    fn test_detect_none_narrative() {
        let response = "This is a story about a person who found an answer.";
        assert_eq!(detect_thinking_type(response), ThinkingType::None);
    }

    #[test]
    fn test_detect_none_empty() {
        let response = "";
        assert_eq!(detect_thinking_type(response), ThinkingType::None);
    }

    #[test]
    fn test_detect_none_whitespace_only() {
        let response = "   \n\n\t  ";
        assert_eq!(detect_thinking_type(response), ThinkingType::None);
    }

    // ==========================================================================
    // No Thinking Parsing Tests
    // ==========================================================================

    #[test]
    fn test_parse_none_returns_full_content() {
        let response = "Just a simple answer without any thinking.";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_none());
        assert_eq!(parsed.answer_content, response);
        assert_eq!(parsed.thinking_type, ThinkingType::None);
    }

    #[test]
    fn test_parse_none_empty() {
        let response = "";
        let parsed = parse_thinking_response(response);

        assert!(parsed.thinking_content.is_none());
        assert_eq!(parsed.answer_content, "");
        assert_eq!(parsed.thinking_type, ThinkingType::None);
    }

    // ==========================================================================
    // Priority and Edge Case Tests
    // ==========================================================================

    #[test]
    fn test_priority_explicit_tags_over_verbose() {
        // Has both explicit tags and verbose patterns
        let response = "<think>Step 1: analyze</think>Answer: 42";
        assert_eq!(detect_thinking_type(response), ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_priority_explicit_tags_over_pattern() {
        // Has both explicit tags and pattern-based
        let response = "<think>First, consider X. Second, Y.</think>Therefore, 42.";
        assert_eq!(detect_thinking_type(response), ThinkingType::ExplicitTags);
    }

    #[test]
    fn test_priority_verbose_over_pattern() {
        // Has both verbose and pattern-based
        let response = "Step 1: First, examine. Step 2: Second, analyze. Therefore, 42.";
        assert_eq!(
            detect_thinking_type(response),
            ThinkingType::VerboseReasoning
        );
    }

    #[test]
    fn test_mixed_content_realistic() {
        let response = r#"<think>
Let me analyze this problem.
Step 1: Understand the requirements
Step 2: Design the solution
</think>
Based on my analysis, the answer is to use a hybrid approach combining A and B."#;

        let parsed = parse_thinking_response(response);
        assert_eq!(parsed.thinking_type, ThinkingType::ExplicitTags);
        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("hybrid approach"));
    }

    #[test]
    fn test_real_world_deepseek_style() {
        let response = r#"<think>
Okay, so I need to figure out how to implement the thinking detection module.

First, I should look at the existing ThinkingType enum. It has ExplicitTags, VerboseReasoning, PatternBased, None, and NotTested.

For detection, I need to check for:
1. Explicit <think> tags - highest priority
2. Verbose patterns like "Step N:" or "Let me think"
3. Pattern-based like "First,", "Second,", "Therefore"

For parsing, I need to extract the thinking content separately from the answer.
</think>

To implement the thinking detection module, we need to create three main functions:
1. detect_thinking_type() - analyzes text for thinking patterns
2. parse_thinking_response() - extracts thinking and answer content
3. Helper functions for each detection type"#;

        let parsed = parse_thinking_response(response);
        assert_eq!(parsed.thinking_type, ThinkingType::ExplicitTags);
        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("three main functions"));
    }

    #[test]
    fn test_real_world_verbose_style() {
        let response = r#"Let me think through this problem step by step.

Step 1: First, I need to understand what thinking types we support.
We have explicit tags, verbose reasoning, and pattern-based.

Step 2: For each type, I need detection and parsing logic.
Detection checks if the response matches patterns for that type.
Parsing extracts the thinking content from the response.

Step 3: The detection should have priority - explicit tags first, then verbose, then pattern-based.

Answer: We need to implement detect_thinking_type(), parse_thinking_response(), and type-specific helpers."#;

        let parsed = parse_thinking_response(response);
        assert_eq!(parsed.thinking_type, ThinkingType::VerboseReasoning);
        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("Answer:"));
    }

    #[test]
    fn test_real_world_pattern_style() {
        let response = r#"First, we need to identify the different thinking types that models use. These include explicit tags like think tags, verbose step-by-step reasoning, and structured patterns.

Second, we must implement detection logic that can recognize these patterns in model responses. The detection should be robust and handle various formatting styles.

Third, we need parsing functions that can extract the thinking content separately from the final answer. This allows us to present them differently in the UI.

Therefore, the implementation should consist of a detection function, a parsing function, and specialized handlers for each thinking type."#;

        let parsed = parse_thinking_response(response);
        assert_eq!(parsed.thinking_type, ThinkingType::PatternBased);
        assert!(parsed.thinking_content.is_some());
        assert!(parsed.answer_content.contains("Therefore"));
    }
}
