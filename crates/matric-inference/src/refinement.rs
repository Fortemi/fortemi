//! AI refinement strategies for iterative content improvement.
//!
//! Implements three research-backed patterns:
//! - **Self-Refine** (#163): Iterative self-critique and refinement loops
//! - **ReAct** (#164): Thought→Action→Observation reasoning traces
//! - **Reflexion** (#165): Episodic memory for learning from past revisions
//!
//! References:
//! - Madaan et al. (2023) "Self-Refine: Iterative Refinement with Self-Feedback"
//! - Yao et al. (2023) "ReAct: Synergizing Reasoning and Acting in Language Models"
//! - Shinn et al. (2023) "Reflexion: Language Agents with Verbal Reinforcement Learning"

use serde::{Deserialize, Serialize};

// ============================================================================
// SELF-REFINE (#163)
// ============================================================================

/// Configuration for the Self-Refine iterative improvement loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfRefineConfig {
    /// Maximum number of refinement iterations (default: 3)
    pub max_iterations: u32,
    /// Minimum quality score to accept (0.0-1.0, default: 0.7)
    pub quality_threshold: f32,
    /// Whether to stop early if quality improves less than this between iterations
    pub min_improvement: f32,
}

impl Default for SelfRefineConfig {
    fn default() -> Self {
        Self {
            max_iterations: 3,
            quality_threshold: 0.7,
            min_improvement: 0.05,
        }
    }
}

/// Result of a single self-refine iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineIteration {
    /// Iteration number (1-indexed)
    pub iteration: u32,
    /// The refined content produced
    pub content: String,
    /// Self-critique feedback
    pub critique: String,
    /// Estimated quality score (0.0-1.0)
    pub quality_score: f32,
    /// Whether this iteration was accepted as final
    pub accepted: bool,
}

/// Complete result of a self-refine loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfRefineResult {
    /// The final refined content
    pub final_content: String,
    /// All iterations performed
    pub iterations: Vec<RefineIteration>,
    /// Total iterations performed
    pub total_iterations: u32,
    /// Whether the quality threshold was met
    pub threshold_met: bool,
}

/// Generate a self-critique prompt for the given content.
pub fn self_critique_prompt(content: &str) -> String {
    format!(
        r#"You are a critical reviewer. Evaluate the following text and provide specific, actionable feedback.

Text to review:
{content}

Provide your critique in this exact format:
QUALITY_SCORE: [0.0-1.0 number]
STRENGTHS: [brief list of what works well]
WEAKNESSES: [specific issues to address]
SUGGESTIONS: [concrete improvements to make]

Be honest and specific. Focus on:
1. Clarity and organization
2. Completeness of information
3. Accuracy and precision
4. Readability and flow
5. Proper formatting"#
    )
}

/// Generate a refinement prompt incorporating critique feedback.
pub fn refine_with_critique_prompt(content: &str, critique: &str) -> String {
    format!(
        r#"You are an expert editor. Improve the following text based on the critique provided.

Original Text:
{content}

Critique:
{critique}

Produce an improved version that addresses the critique while preserving all original information.
Output only the improved text, no explanations."#
    )
}

/// Parse quality score from a critique response.
pub fn parse_quality_score(critique: &str) -> f32 {
    // Look for "QUALITY_SCORE: X.X" pattern
    for line in critique.lines() {
        let trimmed = line.trim();
        if let Some(score_str) = trimmed.strip_prefix("QUALITY_SCORE:") {
            if let Ok(score) = score_str.trim().parse::<f32>() {
                return score.clamp(0.0, 1.0);
            }
        }
    }
    // Default to moderate quality if parsing fails
    0.5
}

// ============================================================================
// REACT (#164)
// ============================================================================

/// A single step in a ReAct reasoning trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActStep {
    /// The step number (1-indexed)
    pub step: u32,
    /// Thought: the model's reasoning about what to do
    pub thought: String,
    /// Action: what the model decided to do
    pub action: String,
    /// Observation: the result of the action
    pub observation: String,
}

/// Complete ReAct reasoning trace for an AI operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActTrace {
    /// The original task/query
    pub task: String,
    /// All reasoning steps
    pub steps: Vec<ReActStep>,
    /// Final answer/output
    pub final_answer: String,
}

/// Generate a ReAct-style prompt for transparent AI reasoning.
pub fn react_revision_prompt(content: &str, context: &str) -> String {
    format!(
        r#"You are an AI assistant that thinks step-by-step. For each step, provide your Thought, Action, and Observation.

Task: Enhance the following note by incorporating relevant context.

Original Note:
{content}

Available Context:
{context}

Use this format for each step:
Thought: [your reasoning about what to do next]
Action: [what you decide to do - e.g., "Add context about X", "Restructure section Y"]
Observation: [what you notice after taking the action]

After your reasoning steps, provide the final enhanced note preceded by "FINAL_OUTPUT:" on its own line.

Begin your reasoning:"#
    )
}

/// Parse a ReAct response into structured trace and output.
pub fn parse_react_response(response: &str) -> ReActTrace {
    let mut steps = Vec::new();
    let mut current_thought = String::new();
    let mut current_action = String::new();
    let mut current_observation = String::new();
    let mut step_num = 0u32;
    let mut final_answer = String::new();
    let mut in_final = false;

    for line in response.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("FINAL_OUTPUT:") || in_final {
            if trimmed.starts_with("FINAL_OUTPUT:") {
                in_final = true;
                let after = trimmed.strip_prefix("FINAL_OUTPUT:").unwrap_or("").trim();
                if !after.is_empty() {
                    final_answer.push_str(after);
                    final_answer.push('\n');
                }
            } else {
                final_answer.push_str(line);
                final_answer.push('\n');
            }
            continue;
        }

        if let Some(thought) = trimmed.strip_prefix("Thought:") {
            // Save previous step if exists
            if step_num > 0 && !current_thought.is_empty() {
                steps.push(ReActStep {
                    step: step_num,
                    thought: current_thought.trim().to_string(),
                    action: current_action.trim().to_string(),
                    observation: current_observation.trim().to_string(),
                });
            }
            step_num += 1;
            current_thought = thought.trim().to_string();
            current_action.clear();
            current_observation.clear();
        } else if let Some(action) = trimmed.strip_prefix("Action:") {
            current_action = action.trim().to_string();
        } else if let Some(obs) = trimmed.strip_prefix("Observation:") {
            current_observation = obs.trim().to_string();
        }
    }

    // Save last step
    if step_num > 0 && !current_thought.is_empty() {
        steps.push(ReActStep {
            step: step_num,
            thought: current_thought.trim().to_string(),
            action: current_action.trim().to_string(),
            observation: current_observation.trim().to_string(),
        });
    }

    // If no FINAL_OUTPUT found, use the whole response
    if final_answer.trim().is_empty() {
        final_answer = response.to_string();
    }

    ReActTrace {
        task: "AI revision with context".to_string(),
        steps,
        final_answer: final_answer.trim().to_string(),
    }
}

// ============================================================================
// REFLEXION (#165)
// ============================================================================

/// An episode from past AI revision experience.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// Unique episode identifier
    pub id: String,
    /// The type of task (revision, title_gen, etc.)
    pub task_type: String,
    /// What was attempted
    pub attempt_summary: String,
    /// The outcome (success/failure/partial)
    pub outcome: EpisodeOutcome,
    /// Lesson learned from this episode
    pub lesson: String,
    /// Quality score achieved (0.0-1.0)
    pub quality_score: f32,
}

/// Outcome of an episode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodeOutcome {
    Success,
    Failure,
    Partial,
}

impl std::fmt::Display for EpisodeOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EpisodeOutcome::Success => write!(f, "success"),
            EpisodeOutcome::Failure => write!(f, "failure"),
            EpisodeOutcome::Partial => write!(f, "partial"),
        }
    }
}

/// Reflexion memory that accumulates lessons from past episodes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReflexionMemory {
    /// Past episodes for learning
    pub episodes: Vec<Episode>,
    /// Maximum episodes to retain (oldest are pruned)
    pub max_episodes: usize,
}

impl ReflexionMemory {
    /// Create a new reflexion memory with default capacity.
    pub fn new() -> Self {
        Self {
            episodes: Vec::new(),
            max_episodes: 50,
        }
    }

    /// Create with custom capacity.
    pub fn with_capacity(max: usize) -> Self {
        Self {
            episodes: Vec::new(),
            max_episodes: max,
        }
    }

    /// Add an episode to memory, pruning oldest if at capacity.
    pub fn add_episode(&mut self, episode: Episode) {
        self.episodes.push(episode);
        if self.episodes.len() > self.max_episodes {
            self.episodes.remove(0);
        }
    }

    /// Get relevant lessons for a given task type.
    pub fn get_lessons(&self, task_type: &str) -> Vec<&Episode> {
        self.episodes
            .iter()
            .filter(|e| e.task_type == task_type)
            .collect()
    }

    /// Get lessons from failures (most valuable for learning).
    pub fn get_failure_lessons(&self, task_type: &str) -> Vec<&Episode> {
        self.episodes
            .iter()
            .filter(|e| e.task_type == task_type && e.outcome != EpisodeOutcome::Success)
            .collect()
    }

    /// Build a reflexion context string for prompts.
    pub fn build_context(&self, task_type: &str, max_lessons: usize) -> String {
        let lessons = self.get_lessons(task_type);
        if lessons.is_empty() {
            return String::new();
        }

        let mut context = String::from("Lessons from previous attempts:\n");
        for (i, episode) in lessons.iter().rev().take(max_lessons).enumerate() {
            context.push_str(&format!(
                "{}. [{}] {}: {}\n",
                i + 1,
                episode.outcome,
                episode.attempt_summary,
                episode.lesson
            ));
        }
        context.push('\n');
        context
    }
}

/// Generate a reflexion-enhanced prompt that incorporates past lessons.
pub fn reflexion_prompt(content: &str, context: &str, lessons: &str) -> String {
    if lessons.is_empty() {
        // No lessons available, fall back to standard prompt
        return format!(
            r#"Enhance the following note using the provided context.

Original Note:
{content}

Context:
{context}

Provide an enhanced version in clean markdown."#
        );
    }

    format!(
        r#"You are an AI assistant that learns from experience. Before enhancing the note, review the lessons from previous attempts.

{lessons}

Apply these lessons to avoid past mistakes and build on what worked.

Original Note:
{content}

Context:
{context}

Provide an enhanced version that incorporates these lessons. Output clean markdown only."#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Self-Refine tests
    #[test]
    fn test_parse_quality_score() {
        assert_eq!(
            parse_quality_score("QUALITY_SCORE: 0.85\nSTRENGTHS: good"),
            0.85
        );
        assert_eq!(parse_quality_score("QUALITY_SCORE: 1.5\nOther"), 1.0); // clamped
        assert_eq!(parse_quality_score("QUALITY_SCORE: -0.1"), 0.0); // clamped
        assert_eq!(parse_quality_score("no score here"), 0.5); // default
    }

    #[test]
    fn test_self_refine_config_default() {
        let config = SelfRefineConfig::default();
        assert_eq!(config.max_iterations, 3);
        assert_eq!(config.quality_threshold, 0.7);
        assert_eq!(config.min_improvement, 0.05);
    }

    #[test]
    fn test_self_critique_prompt_contains_content() {
        let prompt = self_critique_prompt("Test content");
        assert!(prompt.contains("Test content"));
        assert!(prompt.contains("QUALITY_SCORE"));
    }

    // ReAct tests
    #[test]
    fn test_parse_react_response() {
        let response = r#"Thought: I should add context about Rust.
Action: Add programming language context
Observation: The note now covers Rust features.

Thought: I should improve formatting.
Action: Add markdown headers
Observation: Structure is clearer now.

FINAL_OUTPUT:
# Enhanced Note
This is the final content."#;

        let trace = parse_react_response(response);
        assert_eq!(trace.steps.len(), 2);
        assert_eq!(trace.steps[0].step, 1);
        assert!(trace.steps[0].thought.contains("Rust"));
        assert_eq!(trace.steps[1].step, 2);
        assert!(trace.final_answer.contains("Enhanced Note"));
    }

    #[test]
    fn test_parse_react_no_final_output() {
        let response = "Just plain text without ReAct format";
        let trace = parse_react_response(response);
        assert_eq!(trace.steps.len(), 0);
        assert_eq!(trace.final_answer, response);
    }

    // Reflexion tests
    #[test]
    fn test_reflexion_memory_add_and_prune() {
        let mut memory = ReflexionMemory::with_capacity(3);
        for i in 0..5 {
            memory.add_episode(Episode {
                id: format!("ep-{}", i),
                task_type: "revision".to_string(),
                attempt_summary: format!("Attempt {}", i),
                outcome: EpisodeOutcome::Success,
                lesson: format!("Lesson {}", i),
                quality_score: 0.8,
            });
        }
        // Should have pruned to capacity
        assert_eq!(memory.episodes.len(), 3);
        // Oldest should be gone
        assert_eq!(memory.episodes[0].id, "ep-2");
    }

    #[test]
    fn test_reflexion_memory_get_lessons() {
        let mut memory = ReflexionMemory::new();
        memory.add_episode(Episode {
            id: "1".into(),
            task_type: "revision".into(),
            attempt_summary: "Rev attempt".into(),
            outcome: EpisodeOutcome::Success,
            lesson: "Keep it concise".into(),
            quality_score: 0.9,
        });
        memory.add_episode(Episode {
            id: "2".into(),
            task_type: "title_gen".into(),
            attempt_summary: "Title attempt".into(),
            outcome: EpisodeOutcome::Failure,
            lesson: "Avoid generic titles".into(),
            quality_score: 0.3,
        });

        let revision_lessons = memory.get_lessons("revision");
        assert_eq!(revision_lessons.len(), 1);

        let failure_lessons = memory.get_failure_lessons("title_gen");
        assert_eq!(failure_lessons.len(), 1);
    }

    #[test]
    fn test_reflexion_build_context() {
        let mut memory = ReflexionMemory::new();
        memory.add_episode(Episode {
            id: "1".into(),
            task_type: "revision".into(),
            attempt_summary: "First attempt".into(),
            outcome: EpisodeOutcome::Partial,
            lesson: "Include more context".into(),
            quality_score: 0.5,
        });

        let context = memory.build_context("revision", 5);
        assert!(context.contains("Lessons from previous attempts"));
        assert!(context.contains("Include more context"));

        let empty = memory.build_context("unknown_type", 5);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_episode_outcome_display() {
        assert_eq!(format!("{}", EpisodeOutcome::Success), "success");
        assert_eq!(format!("{}", EpisodeOutcome::Failure), "failure");
        assert_eq!(format!("{}", EpisodeOutcome::Partial), "partial");
    }
}
