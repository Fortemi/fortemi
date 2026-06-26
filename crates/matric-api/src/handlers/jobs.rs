//! Job handlers for background processing.
//!
//! Ported from HOTM's enhanced NLP pipeline for contextual note enhancement.
//! Supports multiple revision modes to control AI enhancement aggressiveness.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tracing::{debug, info, instrument, warn};

use matric_core::{
    AttachmentStatus, CreateFileProvenanceRequest, CreateProvDeviceRequest,
    CreateProvLocationRequest, CreateSemanticRelationRequest, DocumentTypeRepository,
    EmbeddingBackend, EmbeddingRepository, GenerationBackend, JobRepository, JobType,
    LinkRepository, NoteRepository, ProvRelation, RevisionMode, SkosSemanticRelation,
};
use matric_db::{
    Chunker, ChunkerConfig, Database, SchemaContext, SemanticChunker, SkosRelationRepository,
};
use matric_inference::{NerBackend, OllamaBackend, ProviderRegistry};
use matric_jobs::adapters::exif::{
    extract_exif_metadata, parse_exif_datetime, prepare_attachment_metadata,
};
use matric_jobs::{JobContext, JobHandler, JobResult};
use sqlx;

const AI_GENERATION_JOB_FAILURE: &str = "AI generation failed. Check server logs for diagnostics.";
const MODEL_RESOLUTION_JOB_FAILURE: &str =
    "Model resolution failed. Check server logs for diagnostics.";
const GRAPH_MAINTENANCE_STEP_FAILURE: &str =
    "Graph maintenance step failed. Check server logs for diagnostics.";
const AI_REVISION_JOB_FAILURE: &str = "AI revision failed. Check server logs for diagnostics.";
const AI_CONTEXTUAL_REVISION_JOB_FAILURE: &str =
    "AI contextual revision failed. Check server logs for diagnostics.";
const ATTACHMENT_PROCESSING_JOB_FAILURE: &str =
    "Attachment processing failed. Check server logs for diagnostics.";
const SCHEMA_CONTEXT_JOB_FAILURE: &str =
    "Job schema context failed. Check server logs for diagnostics.";
const METADATA_EXTRACTION_JOB_FAILURE: &str =
    "Metadata extraction failed. Check server logs for diagnostics.";
const DOCUMENT_TYPE_INFERENCE_JOB_FAILURE: &str =
    "Document type inference failed. Check server logs for diagnostics.";
const EMBEDDING_JOB_FAILURE: &str = "Embedding job failed. Check server logs for diagnostics.";
const TITLE_GENERATION_JOB_FAILURE: &str =
    "Title generation failed. Check server logs for diagnostics.";
const CONTEXT_UPDATE_JOB_FAILURE: &str =
    "Context update failed. Check server logs for diagnostics.";
const LINKING_JOB_FAILURE: &str = "Linking job failed. Check server logs for diagnostics.";
const PURGE_JOB_FAILURE: &str = "Purge job failed. Check server logs for diagnostics.";
const CONCEPT_TAGGING_JOB_FAILURE: &str =
    "Concept tagging failed. Check server logs for diagnostics.";
const RELATED_CONCEPT_JOB_FAILURE: &str =
    "Related concept inference failed. Check server logs for diagnostics.";
const REFERENCE_EXTRACTION_JOB_FAILURE: &str =
    "Reference extraction failed. Check server logs for diagnostics.";
const REEMBED_ALL_JOB_FAILURE: &str =
    "Bulk re-embedding failed. Check server logs for diagnostics.";
const REFRESH_EMBEDDING_SET_JOB_FAILURE: &str =
    "Embedding set refresh failed. Check server logs for diagnostics.";
const JOB_CHUNK_MERGE_PARSE_FAILURE_DETAIL: &str = "job_chunk_merge_parse_failed";
const JOB_AI_GENERATION_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_ai_generation_diagnostic_failed";
const JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_ai_revision_diagnostic_failed";
const JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL: &str =
    "job_ai_contextual_revision_diagnostic_failed";
const JOB_PROVENANCE_WRITE_FAILURE_DETAIL: &str = "job_provenance_write_failed";
const JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL: &str = "job_queue_followup_failed";
const JOB_REVISION_NOTE_UPDATE_FAILURE_DETAIL: &str = "job_revision_note_update_failed";
const JOB_CONTEXT_DISCOVERY_FAILURE_DETAIL: &str = "job_context_discovery_failed";
const JOB_TITLE_ESCALATION_FAILURE_DETAIL: &str = "job_title_escalation_failed";
const JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_linking_diagnostic_failed";
const JOB_PURGE_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_purge_diagnostic_failed";
const JOB_CONTEXT_UPDATE_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_context_update_diagnostic_failed";
const JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_concept_tagging_diagnostic_failed";
const JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL: &str =
    "job_reference_extraction_diagnostic_failed";
const JOB_RELATED_CONCEPT_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_related_concept_diagnostic_failed";
const JOB_METADATA_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_metadata_diagnostic_failed";
const JOB_DOCUMENT_TYPE_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_document_type_diagnostic_failed";
const JOB_REEMBED_QUEUE_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_reembed_queue_diagnostic_failed";
const JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL: &str = "job_exif_diagnostic_failed";

fn diagnostic_len(error: impl std::fmt::Display) -> usize {
    error.to_string().chars().count()
}

fn ai_generation_job_failure(error: impl std::fmt::Display, operation: &'static str) -> JobResult {
    warn!(
        error_len = diagnostic_len(error),
        detail = JOB_AI_GENERATION_DIAGNOSTIC_FAILURE_DETAIL,
        operation,
        "AI generation job failed"
    );
    JobResult::Failed(AI_GENERATION_JOB_FAILURE.to_string())
}

fn model_resolution_job_failure(error: impl std::fmt::Display) -> JobResult {
    let diagnostic = error.to_string();
    warn!(error_len = diagnostic.len(), "Model resolution failed");
    JobResult::Failed(MODEL_RESOLUTION_JOB_FAILURE.to_string())
}

fn graph_maintenance_step_failure(
    error: impl std::fmt::Display,
    step: &'static str,
) -> serde_json::Value {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        step, "Graph maintenance step failed"
    );
    serde_json::json!({
        "status": "failed",
        "error": GRAPH_MAINTENANCE_STEP_FAILURE,
    })
}

fn ai_revision_job_failure(error: impl std::fmt::Display, operation: &'static str) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "AI revision job failed"
    );
    JobResult::Failed(AI_REVISION_JOB_FAILURE.to_string())
}

fn ai_contextual_revision_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "AI contextual revision job failed"
    );
    JobResult::Failed(AI_CONTEXTUAL_REVISION_JOB_FAILURE.to_string())
}

fn attachment_processing_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Attachment processing job failed"
    );
    JobResult::Failed(ATTACHMENT_PROCESSING_JOB_FAILURE.to_string())
}

fn metadata_extraction_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Metadata extraction job failed"
    );
    JobResult::Failed(METADATA_EXTRACTION_JOB_FAILURE.to_string())
}

fn document_type_inference_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Document type inference job failed"
    );
    JobResult::Failed(DOCUMENT_TYPE_INFERENCE_JOB_FAILURE.to_string())
}

fn embedding_job_failure(error: impl std::fmt::Display, operation: &'static str) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Embedding job failed"
    );
    JobResult::Failed(EMBEDDING_JOB_FAILURE.to_string())
}

fn title_generation_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Title generation job failed"
    );
    JobResult::Failed(TITLE_GENERATION_JOB_FAILURE.to_string())
}

fn context_update_job_failure(error: impl std::fmt::Display, operation: &'static str) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Context update job failed"
    );
    JobResult::Failed(CONTEXT_UPDATE_JOB_FAILURE.to_string())
}

fn linking_job_failure(error: impl std::fmt::Display, operation: &'static str) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Linking job failed"
    );
    JobResult::Failed(LINKING_JOB_FAILURE.to_string())
}

fn linking_step_failure(error: impl std::fmt::Display, operation: &'static str) -> String {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Linking step failed"
    );
    LINKING_JOB_FAILURE.to_string()
}

fn purge_job_failure(error: impl std::fmt::Display, operation: &'static str) -> JobResult {
    let diagnostic = error.to_string();
    warn!(error_len = diagnostic.len(), operation, "Purge job failed");
    JobResult::Failed(PURGE_JOB_FAILURE.to_string())
}

fn concept_tagging_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Concept tagging job failed"
    );
    JobResult::Failed(CONCEPT_TAGGING_JOB_FAILURE.to_string())
}

fn related_concept_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Related concept inference job failed"
    );
    JobResult::Failed(RELATED_CONCEPT_JOB_FAILURE.to_string())
}

fn reference_extraction_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Reference extraction job failed"
    );
    JobResult::Failed(REFERENCE_EXTRACTION_JOB_FAILURE.to_string())
}

fn reembed_all_job_failure(error: impl std::fmt::Display, operation: &'static str) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Bulk re-embedding job failed"
    );
    JobResult::Failed(REEMBED_ALL_JOB_FAILURE.to_string())
}

fn refresh_embedding_set_job_failure(
    error: impl std::fmt::Display,
    operation: &'static str,
) -> JobResult {
    let diagnostic = error.to_string();
    warn!(
        error_len = diagnostic.len(),
        operation, "Embedding set refresh job failed"
    );
    JobResult::Failed(REFRESH_EMBEDDING_SET_JOB_FAILURE.to_string())
}

/// Extract the target schema from a job's payload.
///
/// Returns the schema name for multi-memory archive support (Issue #413).
/// Defaults to "public" for backward compatibility with jobs queued before
/// the multi-memory feature.
fn extract_schema(ctx: &JobContext) -> &str {
    ctx.payload()
        .and_then(|p| p.get("schema"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("public")
}

/// Create a SchemaContext for the given schema, returning a JobResult error on failure.
fn schema_context(db: &Database, schema: &str) -> Result<SchemaContext, JobResult> {
    db.for_schema(schema).map_err(|e| {
        let diagnostic = e.to_string();
        warn!(
            schema_len = schema.len(),
            error_len = diagnostic.len(),
            "Job schema context failed"
        );
        JobResult::Failed(SCHEMA_CONTEXT_JOB_FAILURE.to_string())
    })
}

/// Extract an optional model override from a job's payload.
///
/// When present, the job handler should use this model slug instead of the
/// globally configured default. Returns `None` if no override is specified,
/// meaning the handler should use its default backend.
fn extract_model_override(ctx: &JobContext) -> Option<String> {
    ctx.payload()
        .and_then(|p| p.get("model"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Resolve an optional model override to a generation backend via the provider registry.
///
/// Supports provider-qualified slugs (e.g., `"openai:gpt-4o"`, `"qwen3:32b"`).
/// Returns `Ok(None)` if no override is needed (caller uses default backend).
/// Returns `Ok(Some(boxed_backend))` for any override — local or external.
fn resolve_gen_backend(
    registry: &ProviderRegistry,
    model_override: Option<&str>,
) -> Result<Option<Box<dyn GenerationBackend>>, JobResult> {
    registry
        .resolve_gen_override(model_override)
        .map_err(model_resolution_job_failure)
}

/// Try to parse a JSON string as `T`. If it's an object wrapping a single array
/// value (e.g. `{"tags": [...]}` or `{"references": [...]}`), unwrap the array
/// and parse that instead. Models frequently wrap bare arrays in an object even
/// when the prompt asks for a plain array.
fn parse_json_lenient<T: serde::de::DeserializeOwned>(
    raw: &str,
) -> std::result::Result<T, serde_json::Error> {
    // Try direct parse first
    let direct_err = match serde_json::from_str::<T>(raw) {
        Ok(v) => return Ok(v),
        Err(e) => e,
    };
    // If that failed, check if it's an object wrapping a single array value,
    // or a bare object that should be wrapped in an array.
    if let Ok(obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw) {
        // Case 1: object wrapping an array value, e.g. {"pairs": [...]}
        for (_key, value) in &obj {
            if value.is_array() {
                if let Ok(v) = serde_json::from_str::<T>(&value.to_string()) {
                    return Ok(v);
                }
            }
        }
        // Case 2: bare single object that should be an array element,
        // e.g. {"concept_a": "x", "concept_b": "y"} instead of [{...}]
        let wrapped = serde_json::Value::Array(vec![serde_json::Value::Object(obj)]);
        if let Ok(v) = serde_json::from_value::<T>(wrapped) {
            return Ok(v);
        }
    }
    Err(direct_err)
}

/// Compute extraction chunk size from a model's context window.
///
/// Larger models can handle more content per chunk with better quality.
/// The chunk size is tuned for extraction quality, not just context capacity:
/// - 3B models: ~3750 chars — focused extraction per chunk
/// - 8B models: ~10000 chars — most notes fit in single chunk
/// - 14B+ models: ~17500 chars — handles large documents directly
///
/// Returns the fallback constant if no profile is available.
fn extraction_chunk_size(backend: Option<&OllamaBackend>) -> usize {
    if let Some(b) = backend {
        if let Some(profile) = b.gen_model_profile() {
            // Budget based on model size: ~1250 chars per billion parameters.
            // This accounts for quality, not just capacity — smaller models
            // extract better from focused chunks.
            let param_billions = profile
                .name
                .split(':')
                .next_back()
                .and_then(|s| s.trim_end_matches('b').parse::<f64>().ok())
                .unwrap_or(3.0);
            let quality_budget = (param_billions * 1250.0) as usize;

            // Also cap at ~25% of context window in chars (~4 chars/token)
            let context_budget = profile.native_context;

            let size = quality_budget
                .min(context_budget)
                .max(matric_core::defaults::EXTRACTION_CHUNK_SIZE_MIN);

            info!(
                model = %profile.name,
                context_tokens = profile.native_context,
                chunk_chars = size,
                "Computed extraction chunk size from model profile"
            );
            return size;
        }
    }
    matric_core::defaults::EXTRACTION_CHUNK_SIZE_FALLBACK
}

/// Chunk content for fast-model extraction if it exceeds the given chunk size.
///
/// Returns a single chunk for small content, multiple for large content.
/// Uses `SemanticChunker` for natural boundary splitting with overlap.
fn chunk_for_extraction(content: &str, max_chars: usize) -> Vec<String> {
    if content.len() <= max_chars {
        return vec![content.to_string()];
    }
    let config = ChunkerConfig {
        max_chunk_size: max_chars,
        min_chunk_size: (max_chars / 10).max(100),
        overlap: 200,
    };
    let chunker = SemanticChunker::new(config);
    chunker.chunk(content).into_iter().map(|c| c.text).collect()
}

/// Compute revision chunk size from the model's actual context window.
///
/// Revision needs different sizing than extraction because both input content
/// AND generated output must fit in the context window. The formula:
///   (context_tokens * ~4 chars/token - prompt_overhead) / 2
/// The /2 accounts for the model needing roughly equal space for input and output.
///
/// Capped at 200K chars (above this, chunking adds overhead without benefit)
/// and floored at 8K chars (below this, chunks lose coherence).
///
/// `running_ctx` is the actual context Ollama allocated (from `/api/ps`).
/// When available, it reflects real VRAM constraints. Falls back to the model
/// profile's native context, which may exceed what VRAM can support.
fn revision_chunk_size(backend: &OllamaBackend, running_ctx: Option<usize>) -> usize {
    if let Some(profile) = backend.gen_model_profile() {
        // Priority: actual running context > OLLAMA_CONTEXT_LENGTH > profile native
        let effective_ctx = running_ctx
            .or_else(|| {
                std::env::var("OLLAMA_CONTEXT_LENGTH")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|&v| v > 0)
            })
            .unwrap_or(profile.native_context);

        let context_chars = effective_ctx * 4;
        let available =
            context_chars.saturating_sub(matric_core::defaults::REVISION_PROMPT_OVERHEAD);
        let size = (available / 2).clamp(matric_core::defaults::REVISION_CHUNK_SIZE_MIN, 200_000);

        info!(
            model = %profile.name,
            context_tokens = effective_ctx,
            profile_context = profile.native_context,
            running_context = running_ctx.unwrap_or(0),
            chunk_chars = size,
            "Computed revision chunk size from model context"
        );
        return size;
    }
    matric_core::defaults::REVISION_CHUNK_SIZE_FALLBACK
}

/// Compute an adaptive timeout for a generation request based on input size.
///
/// Scales linearly with content length, clamped between `base_timeout` (or
/// `GEN_TIMEOUT_MIN_SECS`) and `GEN_TIMEOUT_MAX_SECS`. For short content the
/// base timeout applies unchanged; for multi-hour video transcripts the
/// timeout grows proportionally so Ollama has time to finish.
fn adaptive_timeout_secs(content_len: usize, base_timeout: u64) -> u64 {
    let scaled = (content_len as u64 * matric_core::defaults::GEN_TIMEOUT_MS_PER_CHAR) / 1000;
    let minimum = base_timeout.max(matric_core::defaults::GEN_TIMEOUT_MIN_SECS);
    scaled
        .max(minimum)
        .min(matric_core::defaults::GEN_TIMEOUT_MAX_SECS)
}

/// Chunk content for revision if it exceeds the given chunk size.
///
/// `overlap` controls character overlap between adjacent chunks for context
/// continuity. Default is 0 — revision chunks are independent prose sections
/// that get concatenated. Set overlap > 0 when context continuity matters. (#572)
/// Returns a single chunk for small content.
fn chunk_for_revision(content: &str, max_chars: usize, overlap: usize) -> Vec<String> {
    if content.len() <= max_chars {
        return vec![content.to_string()];
    }
    let config = ChunkerConfig {
        max_chunk_size: max_chars,
        min_chunk_size: (max_chars / 10).max(500),
        overlap,
    };
    let chunker = SemanticChunker::new(config);
    chunker.chunk(content).into_iter().map(|c| c.text).collect()
}

/// Chunk video timeline content at scene boundaries for revision.
///
/// Video timelines produced by `KeyframeAssemblyHandler` have explicit `### Scene N`
/// markers. These are natural atomic units that should not be split mid-scene.
/// Groups consecutive scenes until the chunk budget is reached. The metadata
/// header (duration, frames) stays with the first chunk.
///
/// Falls back to `chunk_for_revision()` if no scene boundaries are found.
fn chunk_video_timeline(content: &str, max_chars: usize) -> Vec<String> {
    if content.len() <= max_chars {
        return vec![content.to_string()];
    }

    let scene_marker = "### Scene ";
    let parts: Vec<&str> = content.split(scene_marker).collect();

    if parts.len() <= 1 {
        // No scene boundaries — fall back to generic chunking (zero overlap for video)
        return chunk_for_revision(content, max_chars, 0);
    }

    // First part is the metadata header (before the first scene)
    let header = parts[0];
    let scenes: Vec<String> = parts[1..]
        .iter()
        .map(|s| format!("{}{}", scene_marker, s))
        .collect();

    let mut chunks = Vec::new();
    let mut current_chunk = header.to_string();

    for scene in &scenes {
        if !current_chunk.is_empty() && current_chunk.len() + scene.len() > max_chars {
            if !current_chunk.trim().is_empty() {
                chunks.push(current_chunk);
            }
            current_chunk = scene.clone();
        } else {
            current_chunk.push_str(scene);
        }
    }

    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk);
    }

    if chunks.is_empty() {
        vec![content.to_string()]
    } else {
        chunks
    }
}

/// Merge multiple JSON array results from chunked extraction, deduplicating by string value.
/// Skips chunks that fail to parse rather than failing the entire merge.
fn merge_json_arrays(results: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for raw in results {
        match parse_json_lenient::<Vec<String>>(&raw) {
            Ok(items) => {
                for item in items {
                    let key = item.to_lowercase();
                    if seen.insert(key) {
                        merged.push(item);
                    }
                }
            }
            Err(e) => {
                info!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_CHUNK_MERGE_PARSE_FAILURE_DETAIL,
                    "Skipping unparseable chunk result in merge"
                );
            }
        }
    }
    merged
}

/// Maximum number of related notes to retrieve for AI context.
/// Based on Miller's Law (7±2): cognitive limit for working memory items.
/// We use 7 as the default, which is the center of the 5-9 range.
const MAX_CONTEXT_NOTES: usize = 7;

/// Maximum number of related note snippets to include in prompts.
/// Capped lower than MAX_CONTEXT_NOTES to keep prompt size manageable
/// while still respecting Miller's Law bounds (minimum of 5).
const MAX_PROMPT_SNIPPETS: usize = 5;

/// Build a revision prompt tailored to the note's document type.
///
/// When a `DocumentType` is available, uses its `agentic_config.required_sections`
/// and `generation_prompt` to produce type-specific output (e.g., meeting notes get
/// Decisions/Action Items, movies get Synopsis/Cast). Falls back to the existing
/// generic prompt when no type is available. Light mode is always unchanged.
fn build_type_aware_prompt(
    doc_type: Option<&matric_core::DocumentType>,
    mode: RevisionMode,
    chunk_content: &str,
    continuity_note: &str,
    chunk_idx: usize,
    total_chunks: usize,
    is_video_timeline: bool,
) -> String {
    // Light mode: formatting-only, no summary, no structural additions
    if mode == RevisionMode::Light {
        return format!(
            r#"You are a formatting assistant. Your task is to improve the structure and readability of the following note WITHOUT adding any new information.
{continuity}
Original Note:
{content}

STRICT RULES - You MUST follow these:
1. DO NOT add any technical details, architecture, APIs, or integrations not explicitly stated in the original
2. DO NOT invent, expand, or elaborate on topics - only reformat what exists
3. DO NOT add analysis, explanations, or context the author did not provide
4. DO NOT turn opinions into factual statements or analysis
5. DO NOT add tables, diagrams, or structured data unless the original clearly warrants it

What you MAY do:
- Fix grammar and spelling errors
- Add markdown headers to organize existing content
- Convert existing lists to cleaner bullet points
- Improve sentence clarity without changing meaning
- Add appropriate markdown formatting (bold, italic, code blocks for actual code)

If the note is very short or simple, keep it short and simple. A one-line note should remain approximately one line.

Output the formatted note. Do not add any labels, markers, or metadata."#,
            continuity = continuity_note,
            content = chunk_content,
        );
    }

    // Standard mode: build type-aware prompt
    let is_first_chunk = chunk_idx == 0;
    let is_single_chunk = total_chunks <= 1;

    // Summary instruction: only on first chunk or single-chunk documents
    let summary_instruction = if is_first_chunk || is_single_chunk {
        "\n- Begin your output with a ## Summary section (2-4 sentences capturing the essence of this content)"
    } else {
        ""
    };

    // If we have a document type with agentic_config, use it
    if let Some(dt) = doc_type {
        let sections = &dt.agentic_config.required_sections;
        if !sections.is_empty() {
            // Build role from generation_prompt or category
            let role = dt
                .agentic_config
                .generation_prompt
                .as_deref()
                .unwrap_or("You are an intelligent note-taking assistant.");

            // Build required sections list (exclude Summary — handled separately)
            let sections_list: Vec<&str> = sections
                .iter()
                .map(|s| s.as_str())
                .filter(|s| *s != "Summary")
                .collect();

            let sections_instruction = if sections_list.is_empty() {
                String::new()
            } else {
                format!(
                    "\n- After the summary, organize the content into these sections: {}",
                    sections_list.join(", ")
                )
            };

            return format!(
                r#"{role}

Revise the following content into a polished, well-structured document.
{summary}{sections}

STRICT RULES:
- Work ONLY with the content provided below
- Do NOT reference, infer, or add information from any external source
- Do NOT invent details, examples, or context not present in the original
- Preserve ALL original meaning and information
- If the content is a transcript, clean it up but preserve the speaker's actual words and meaning
- If the content contains raw keyframe captures (### Scene N at regular intervals), group consecutive keyframes sharing the same setting into coherent scenes and describe visual progression across frames rather than restating each independently; use time ranges for merged scenes
- Preserve the chronological order and approximate time coverage
- Preserve ALL spoken dialog — do not omit or paraphrase quotes
{continuity}
Original Note:
{content}

Output the revised document in clean markdown format. Do not add any labels, markers, or metadata."#,
                role = role,
                summary = summary_instruction,
                sections = sections_instruction,
                continuity = continuity_note,
                content = chunk_content,
            );
        }
    }

    // Fallback: no document type or empty required_sections
    if is_video_timeline {
        format!(
            r##"You are a video content editor. The following note contains raw keyframe captures taken at regular intervals (~10 seconds) interleaved with timestamped speaker dialog. Each Scene N heading is a single keyframe snapshot, NOT a true scene boundary.

Your task is to revise this into a polished document that groups keyframes into coherent scenes and weaves visual descriptions with dialog into a flowing narrative.
{summary}

SCENE MERGING:
- Group consecutive keyframes that share the same setting, subject, or visual continuity into a single scene
- Within each merged scene, describe the visual PROGRESSION across frames -- camera movement, action unfolding, changes in subject -- rather than restating each frame independently
- Start a new scene only when there is a clear change in location, subject, or topic
- Title each scene descriptively with a time range (e.g., Opening Montage -- 0:00-0:45)
- Preserve chronological order

DIALOG INTEGRATION:
- Weave transcript segments into the visual narrative at the point they occur
- Attribute dialog to speakers when labels are present
- Clean up transcript artifacts (filler words, repetition, disfluencies)
- Use natural attribution (direct quotes or "the host explains...")
- Preserve ALL spoken dialog -- do not omit or paraphrase quotes

FORMAT:
- Use ## for the document title (derived from overall content)
- Use ### for each merged scene heading with time range
- Within each scene: flowing prose interleaving what is shown with what is said
{continuity}
Original Note:
{content}

Output the revised document in clean markdown format. Do not add any labels, markers, or metadata."##,
            summary = summary_instruction,
            continuity = continuity_note,
            content = chunk_content
        )
    } else {
        format!(
            r#"You are an intelligent note-taking assistant. Revise the following note to improve clarity, structure, and readability. Extract and highlight key concepts.
{summary}

STRICT RULES:
- Work ONLY with the content provided below
- Do NOT reference, infer, or add information from any external source
- Do NOT invent details, examples, or context not present in the original
- Preserve ALL original meaning and information
- If the content is a transcript, clean it up but preserve the speaker's actual words and meaning
{continuity}
Original Note:
{content}

Output the revised note in clean markdown format. Do not add any labels, markers, or metadata."#,
            summary = summary_instruction,
            continuity = continuity_note,
            content = chunk_content
        )
    }
}

/// Handler for AI revision jobs - enhanced with context from related notes.
pub struct AiRevisionHandler {
    db: Database,
    backend: OllamaBackend,
    registry: Arc<ProviderRegistry>,
}

impl AiRevisionHandler {
    pub fn new(db: Database, backend: OllamaBackend, registry: Arc<ProviderRegistry>) -> Self {
        Self {
            db,
            backend,
            registry,
        }
    }
}

#[async_trait]
impl JobHandler for AiRevisionHandler {
    fn job_type(&self) -> JobType {
        JobType::AiRevision
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "ai_revision", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        // Extract revision mode from payload (default to Light)
        let revision_mode = ctx
            .payload()
            .and_then(|p| p.get("revision_mode"))
            .and_then(|v| serde_json::from_value::<RevisionMode>(v.clone()).ok())
            .unwrap_or(RevisionMode::Light);

        let schema = extract_schema(&ctx);
        let model_override = extract_model_override(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        // Skip if mode is None (shouldn't happen as we don't queue, but safety check)
        if revision_mode == RevisionMode::None {
            return JobResult::Success(Some(serde_json::json!({
                "skipped": true,
                "reason": "revision_mode is none"
            })));
        }

        // Resolve model override via provider registry (supports provider-qualified slugs)
        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };
        let backend: &dyn GenerationBackend = match &overridden {
            Some(b) => b.as_ref(),
            None => &self.backend,
        };

        ctx.report_progress(10, Some("Fetching note content..."));

        // Get the note
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return ai_revision_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return ai_revision_job_failure(e, "fetch_note"),
        };
        if let Err(e) = tx.commit().await {
            return ai_revision_job_failure(e, "fetch_note_commit");
        }

        let original_content = &note.original.content;
        if original_content.trim().is_empty() {
            return JobResult::Failed("Note has no content to revise".into());
        }

        // Defer revision for notes with media attachments (video/audio).
        // The extraction pipeline (ExtractionHandler / KeyframeAssembly) queues
        // AiRevision after content assembly, producing much richer input.
        // Running now on stub content wastes GPU time and the dedup guard would
        // silently discard the extraction pipeline's properly-timed re-queue.
        //
        // The extraction pipeline sets `post_extraction: true` in the payload
        // when re-queuing, so we only defer the initial (note-creation) trigger.
        // The reprocess endpoint sets `force: true` to bypass deferral (#578).
        let is_post_extraction = ctx
            .payload()
            .and_then(|p| p.get("post_extraction"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let is_force = ctx
            .payload()
            .and_then(|p| p.get("force"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !is_post_extraction && !is_force {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return ai_revision_job_failure(e, "media_check_begin_tx"),
            };
            let has_media: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM attachment a
                     JOIN attachment_blob ab ON a.blob_id = ab.id
                     WHERE a.note_id = $1
                       AND (ab.content_type LIKE 'video/%' OR ab.content_type LIKE 'audio/%')
                 )",
            )
            .bind(note_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(false);
            let _ = tx.commit().await;

            if has_media {
                info!(
                    note_id_present = true,
                    detail = JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "defer_ai_revision_for_media",
                    "Deferring AI revision — note has media attachments; \
                     extraction pipeline will re-queue after content assembly"
                );
                return JobResult::Success(Some(serde_json::json!({
                    "deferred": true,
                    "reason": "media attachments present — extraction pipeline owns revision timing"
                })));
            }
        }

        // Look up document type for type-aware prompt building.
        // Primary: use the note's assigned document_type_id.
        // Fallback: heuristic detection from content (first 1000 chars).
        let doc_type = if let Some(dt_id) = note.note.document_type_id {
            self.db.document_types.get(dt_id).await.ok().flatten()
        } else {
            let preview: String = original_content.chars().take(1000).collect();
            match self
                .db
                .document_types
                .detect(None, Some(&preview), None)
                .await
            {
                Ok(Some(result)) => self
                    .db
                    .document_types
                    .get(result.document_type.id)
                    .await
                    .ok()
                    .flatten(),
                _ => None,
            }
        };

        // Start provenance activity
        let activity_id = self
            .db
            .provenance
            .start_activity(
                note_id,
                "ai_revision",
                Some(matric_core::GenerationBackend::model_name(backend)),
            )
            .await
            .ok();

        // For contextual modes, Phase 1 runs Standard (isolated) first.
        // Phase 2 (AiRevisionContextual) is queued after saving Phase 1 output.
        let effective_mode = revision_mode.phase1_mode();

        // Detect video timeline content (produced by format_video_markdown with
        // interleaved scenes + dialog) so we can use a specialized prompt that
        // produces a scene-by-scene document rather than a generic revision.
        let is_video_timeline = original_content.contains("### Scene ")
            && (original_content.contains("**Duration**:")
                || original_content.contains("**Frames**:"));

        // Extract optional per-call chunking overrides from job payload (#572).
        // Priority: per-call override > auto-computed from model context.
        let chunk_max_override: Option<usize> = ctx
            .payload()
            .and_then(|p| p.get("chunk_max_chars"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let chunk_overlap: usize = ctx
            .payload()
            .and_then(|p| p.get("chunk_overlap"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);

        // Query Ollama's actual context allocation for accurate chunk sizing.
        // This reflects real VRAM constraints (OLLAMA_CONTEXT_LENGTH, model reload
        // layer offloading, etc.) rather than the theoretical model maximum.
        let running_ctx = self.backend.running_context_length().await;

        // Layered chunk size resolution (#572, #573):
        //   per-call override → document type default → env var → auto-computed from model
        // Per-call override: 0 = disable chunking (single-pass), >0 = explicit max.
        let doc_type_chunking = doc_type
            .as_ref()
            .and_then(|dt| dt.agentic_config.revision_chunking.as_ref());

        let chunk_max = chunk_max_override
            .map(|v| if v == 0 { usize::MAX } else { v })
            .or_else(|| doc_type_chunking.and_then(|c| c.max_chars))
            .or_else(|| {
                std::env::var(matric_core::defaults::ENV_REVISION_CHUNK_MAX_CHARS)
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .map(|v| if v == 0 { usize::MAX } else { v })
            })
            .unwrap_or_else(|| revision_chunk_size(&self.backend, running_ctx));

        let chunk_overlap = if chunk_overlap > 0 {
            chunk_overlap // Per-call override takes precedence
        } else {
            doc_type_chunking
                .and_then(|c| c.overlap)
                .or_else(|| {
                    std::env::var(matric_core::defaults::ENV_REVISION_CHUNK_OVERLAP)
                        .ok()
                        .and_then(|v| v.parse::<usize>().ok())
                })
                .unwrap_or(0)
        };

        if chunk_max_override.is_some() || doc_type_chunking.is_some() {
            info!(
                note_id_present = true,
                chunk_max_override = ?chunk_max_override,
                doc_type_max = ?doc_type_chunking.and_then(|c| c.max_chars),
                chunk_max,
                chunk_overlap,
                detail = JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "resolve_ai_revision_chunking_config",
                "Chunking config resolved (per-call > doc-type > env > auto)"
            );
        }

        // Compute video-specific max from env or constant.
        let video_chunk_max =
            std::env::var(matric_core::defaults::ENV_REVISION_VIDEO_CHUNK_MAX_CHARS)
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(matric_core::defaults::REVISION_VIDEO_CHUNK_SIZE_MAX);

        let chunks = if is_video_timeline {
            // Video timelines are self-contained scenes — smaller chunks produce
            // better results and avoid generation timeouts on multi-hour content.
            let effective_video_max = chunk_max.min(video_chunk_max);
            chunk_video_timeline(original_content, effective_video_max)
        } else {
            chunk_for_revision(original_content, chunk_max, chunk_overlap)
        };
        let total_chunks = chunks.len();
        let is_chunked = total_chunks > 1;

        // Compute per-chunk adaptive timeouts up front so we can derive the
        // total revision budget. Budget = sum(per_chunk_timeout_i), which scales
        // with both chunk count and individual chunk size:
        //   10 chunks × 60s = 600s budget
        //   20 chunks × 60s = 1200s budget
        // This deadline is checked before each chunk starts, so the worst-case
        // overshoot is one additional chunk timeout (not the full remaining budget).
        let base_backend_timeout = self.backend.gen_timeout_secs();
        let per_chunk_timeouts: Vec<u64> = chunks
            .iter()
            .map(|c| adaptive_timeout_secs(c.len(), base_backend_timeout))
            .collect();
        let total_revision_secs: u64 = per_chunk_timeouts
            .iter()
            .sum::<u64>()
            .max(matric_core::defaults::GEN_TIMEOUT_MIN_SECS);

        if is_chunked {
            info!(
                note_id_present = true,
                total_chunks,
                chunk_max,
                content_len = original_content.len(),
                is_video_timeline,
                total_revision_secs,
                detail = JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "split_ai_revision_chunks",
                "Splitting oversized content for chunked revision"
            );
        }

        let revision_deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(total_revision_secs);

        // Generate revision for each chunk (sequential to preserve ordering).
        // `single_chunk_error` captures failures on non-chunked jobs (where we
        // fail the job rather than gracefully skipping) without an early return
        // inside the loop body.
        let mut revised_parts: Vec<String> = Vec::with_capacity(total_chunks);
        let mut single_chunk_error: Option<String> = None;
        for (chunk_idx, chunk_content) in chunks.iter().enumerate() {
            // Enforce total revision budget: stop before the next chunk if we
            // have already consumed the computed total time. This prevents
            // long-running multi-chunk jobs from exceeding the outer worker
            // timeout when content is unexpectedly dense.
            if is_chunked && std::time::Instant::now() >= revision_deadline {
                warn!(
                    note_id_present = true,
                    chunk = chunk_idx + 1,
                    total = total_chunks,
                    total_revision_secs,
                    completed = revised_parts.len(),
                    detail = JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "ai_revision_budget_exhausted",
                    "Total revision budget exhausted, stopping early"
                );
                break;
            }

            let chunk_progress: i32 = if is_chunked {
                40 + ((chunk_idx as i32 * 35) / total_chunks as i32)
            } else {
                40
            };
            let progress_msg = if is_chunked {
                format!(
                    "Generating AI revision (chunk {}/{})...",
                    chunk_idx + 1,
                    total_chunks
                )
            } else {
                match effective_mode {
                    RevisionMode::Standard if is_video_timeline => {
                        "Generating AI revision (video timeline)...".to_string()
                    }
                    RevisionMode::Standard => {
                        "Generating AI revision (standard mode)...".to_string()
                    }
                    RevisionMode::Light => "Generating AI revision (light mode)...".to_string(),
                    _ => unreachable!(),
                }
            };
            ctx.report_progress(chunk_progress, Some(&progress_msg));

            // Continuity preamble for multi-chunk documents
            let continuity_note = if is_chunked {
                format!(
                    "\nNOTE: This is section {} of {} of a larger document. \
                     Revise this section while maintaining continuity.\n\n",
                    chunk_idx + 1,
                    total_chunks
                )
            } else {
                String::new()
            };

            // Build prompt based on effective mode and document type
            let prompt = build_type_aware_prompt(
                doc_type.as_ref(),
                effective_mode,
                chunk_content,
                &continuity_note,
                chunk_idx,
                total_chunks,
                is_video_timeline,
            );

            // Adaptive timeout: scale with content size for large documents.
            // When a model override is active, use the trait (their timeout).
            // Otherwise, use the concrete backend with per-chunk adaptive timeout.
            let chunk_timeout = per_chunk_timeouts[chunk_idx];
            let result = match &overridden {
                Some(b) => b.generate(&prompt).await,
                None => {
                    self.backend
                        .generate_with_timeout(&prompt, chunk_timeout)
                        .await
                }
            };

            match result {
                Ok(r) => {
                    let cleaned = clean_enhanced_content(r.trim(), &prompt);
                    if !cleaned.is_empty() {
                        revised_parts.push(cleaned);
                    }
                }
                Err(e) => {
                    if is_chunked {
                        // Graceful degradation: skip failed chunks rather than failing the job
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                            chunk = chunk_idx + 1,
                            total = total_chunks,
                            timeout_secs = chunk_timeout,
                            "Chunk revision failed, skipping"
                        );
                    } else {
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                            timeout_secs = chunk_timeout,
                            "AI revision generation failed"
                        );
                        single_chunk_error = Some(AI_GENERATION_JOB_FAILURE.to_string());
                        break;
                    }
                }
            }
        }

        if let Some(e) = single_chunk_error {
            return JobResult::Failed(e);
        }

        let revised = revised_parts.join("\n\n");

        if revised.is_empty() {
            let operation = if revision_mode.is_contextual() {
                "empty_contextual_revision_after_cleaning"
            } else {
                "empty_revision_after_cleaning"
            };
            return ai_revision_job_failure("empty revision after content cleaning", operation);
        }

        ctx.report_progress(80, Some("Saving revision..."));

        // Save the revision with mode indicator
        let revision_note = match effective_mode {
            RevisionMode::Standard => {
                if revision_mode.is_contextual() {
                    "AI revision (phase 1 of contextual pipeline)"
                } else {
                    "AI-enhanced revision (isolated, no external context)"
                }
            }
            RevisionMode::Light => "Light formatting revision (no expansion)",
            _ => "Original preserved",
        };

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return ai_revision_job_failure(e, "save_revision_begin_tx"),
        };
        if let Err(e) = self
            .db
            .notes
            .update_revised_tx(&mut tx, note_id, &revised, Some(revision_note))
            .await
        {
            return ai_revision_job_failure(e, "save_revision");
        }
        if let Err(e) = tx.commit().await {
            return ai_revision_job_failure(e, "save_revision_commit");
        }

        // Record W3C PROV provenance for the AI revision
        ctx.report_progress(90, Some("Recording provenance..."));

        if let Ok(Some(chain)) = self.db.provenance.get_chain(note_id).await {
            let rev_id = chain.revision_id;

            // Complete the provenance activity
            if let Some(act_id) = activity_id {
                let metadata = serde_json::json!({
                    "revision_mode": format!("{:?}", revision_mode),
                    "effective_mode": format!("{:?}", effective_mode),
                    "revised_length": revised.len(),
                    "is_phase1": revision_mode.is_contextual(),
                });
                if let Err(e) = self
                    .db
                    .provenance
                    .complete_activity(act_id, Some(rev_id), Some(metadata))
                    .await
                {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                        "Failed to complete provenance activity"
                    );
                }
            }
        }

        // If the original mode was contextual, queue Phase 2 (AiRevisionContextual)
        // which will chain ConceptTagging on its completion.
        if revision_mode.is_contextual() {
            ctx.report_progress(95, Some("Queuing contextual re-revision (phase 2)..."));
            let mut phase2_payload = serde_json::json!({
                "revision_mode": revision_mode,
            });
            if schema != "public" {
                phase2_payload["schema"] = serde_json::json!(schema);
            }
            if let Some(m) = &model_override {
                phase2_payload["model"] = serde_json::json!(m);
            }
            // Pass context_filter through from original payload if present
            if let Some(cf) = ctx.payload().and_then(|p| p.get("context_filter").cloned()) {
                phase2_payload["context_filter"] = cf;
            }
            // Pass detected content type so Phase 2 can preserve type-specific structure
            if let Some(ref dt) = doc_type {
                phase2_payload["content_type"] = serde_json::json!(dt.name);
            }
            match self
                .db
                .jobs
                .queue_deduplicated(
                    Some(note_id),
                    JobType::AiRevisionContextual,
                    JobType::AiRevisionContextual.default_priority(),
                    Some(phase2_payload),
                    JobType::AiRevisionContextual.default_cost_tier(),
                )
                .await
            {
                Ok(Some(job_id)) => {
                    ctx.emit_job_queued(job_id, JobType::AiRevisionContextual, Some(note_id));
                    info!(
                        note_id_present = true,
                        job_id_present = true,
                        detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                        operation = "queue_ai_revision_contextual_phase_2",
                        "Queued AiRevisionContextual (phase 2)"
                    );
                }
                Ok(None) => {} // Deduplicated
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                        "Failed to queue contextual re-revision phase 2"
                    );
                }
            }
        } else {
            // Non-contextual: chain ConceptTagging now that revised content is available.
            // For contextual mode, AiRevisionContextual will chain it after Phase 2.
            // Pipeline: AiRevision → ConceptTagging → RelatedConceptInference → Embedding → Linking (#538).
            let mut ct_payload = serde_json::Map::new();
            if schema != "public" {
                ct_payload.insert("schema".to_string(), serde_json::json!(schema));
            }
            if let Some(m) = &model_override {
                ct_payload.insert("model".to_string(), serde_json::json!(m));
            }
            let ct_payload = if ct_payload.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(ct_payload))
            };
            match self
                .db
                .jobs
                .queue_deduplicated(
                    Some(note_id),
                    JobType::ConceptTagging,
                    JobType::ConceptTagging.default_priority(),
                    ct_payload,
                    JobType::ConceptTagging.default_cost_tier(),
                )
                .await
            {
                Ok(Some(job_id)) => {
                    ctx.emit_job_queued(job_id, JobType::ConceptTagging, Some(note_id));
                    info!(
                        note_id_present = true,
                        job_id_present = true,
                        detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                        operation = "queue_concept_tagging_after_revision",
                        "Queued ConceptTagging after revision"
                    );
                }
                Ok(None) => {} // Deduplicated
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                        "Failed to queue ConceptTagging after revision"
                    );
                }
            }
        }

        ctx.report_progress(100, Some("Revision complete"));
        info!(
            note_id_present = true,
            mode = ?revision_mode,
            effective_mode = ?effective_mode,
            duration_ms = start.elapsed().as_millis() as u64,
            detail = JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
            operation = "complete_ai_revision",
            "AI revision completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "revised_length": revised.len(),
            "revision_mode": revision_mode,
            "effective_mode": effective_mode,
            "phase2_queued": revision_mode.is_contextual(),
            "chunked": is_chunked,
            "chunk_count": total_chunks,
            "content_type": doc_type.as_ref().map(|dt| &dt.name)
        })))
    }
}

/// Handler for Phase 2 contextual re-revision.
///
/// This job is queued automatically by `AiRevisionHandler` when the revision mode
/// is `Contextual`, `ContextualFiltered`, or `Full`. It takes the Phase 1 output
/// (clean isolated revision), generates an intermediate embedding, finds semantically
/// similar notes, and performs a contextual re-revision with strong guardrails
/// separating PRIMARY content from REFERENCE context.
///
/// See issue #494 for the two-phase architecture rationale.
pub struct AiRevisionContextualHandler {
    db: Database,
    backend: OllamaBackend,
    registry: Arc<ProviderRegistry>,
}

impl AiRevisionContextualHandler {
    pub fn new(db: Database, backend: OllamaBackend, registry: Arc<ProviderRegistry>) -> Self {
        Self {
            db,
            backend,
            registry,
        }
    }

    /// Chain ConceptTagging after the final revision step completes.
    /// Called on all success paths (including early returns where Phase 2 is skipped).
    /// Pipeline: AiRevision → AiRevisionContextual → ConceptTagging → ... (#538).
    async fn queue_concept_tagging(
        &self,
        ctx: &JobContext,
        note_id: uuid::Uuid,
        schema: &str,
        model_override: &Option<String>,
    ) {
        let mut ct_payload = serde_json::Map::new();
        if schema != "public" {
            ct_payload.insert("schema".to_string(), serde_json::json!(schema));
        }
        if let Some(m) = model_override {
            ct_payload.insert("model".to_string(), serde_json::json!(m));
        }
        let ct_payload = if ct_payload.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(ct_payload))
        };
        match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::ConceptTagging,
                JobType::ConceptTagging.default_priority(),
                ct_payload,
                JobType::ConceptTagging.default_cost_tier(),
            )
            .await
        {
            Ok(Some(job_id)) => {
                ctx.emit_job_queued(job_id, JobType::ConceptTagging, Some(note_id));
                info!(
                    note_id_present = true,
                    job_id_present = true,
                    detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                    operation = "queue_concept_tagging_after_contextual_revision",
                    "Queued ConceptTagging after contextual revision"
                );
            }
            Ok(None) => {} // Deduplicated
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                    "Failed to queue ConceptTagging after contextual revision"
                );
            }
        }
    }

    /// Update the note's revision_note to reflect the final state of the contextual pipeline.
    /// Called when Phase 2 is skipped so users see an accurate description of what happened.
    async fn update_revision_note(
        &self,
        schema_ctx: &matric_db::SchemaContext,
        note_id: uuid::Uuid,
        revision_note: &str,
    ) {
        if let Ok(mut tx) = schema_ctx.begin_tx().await {
            if let Err(e) = self
                .db
                .notes
                .update_revision_note_tx(&mut tx, note_id, revision_note)
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_REVISION_NOTE_UPDATE_FAILURE_DETAIL,
                    "Failed to update revision note for skipped Phase 2"
                );
            }
            let _ = tx.commit().await;
        }
    }
}

#[async_trait]
impl JobHandler for AiRevisionContextualHandler {
    fn job_type(&self) -> JobType {
        JobType::AiRevisionContextual
    }

    #[instrument(
        skip(self, ctx),
        fields(
            subsystem = "jobs",
            component = "ai_revision_contextual",
            op = "execute"
        )
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let revision_mode = ctx
            .payload()
            .and_then(|p| p.get("revision_mode"))
            .and_then(|v| serde_json::from_value::<RevisionMode>(v.clone()).ok())
            .unwrap_or(RevisionMode::Contextual);

        let schema = extract_schema(&ctx);
        let model_override = extract_model_override(&ctx);
        // Read content_type from Phase 1 payload (if present) to preserve type-specific structure
        let content_type_name: Option<String> = ctx
            .payload()
            .and_then(|p| p.get("content_type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        // Resolve model override via provider registry
        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };
        let backend: &dyn GenerationBackend = match &overridden {
            Some(b) => b.as_ref(),
            None => &self.backend,
        };

        ctx.report_progress(10, Some("Fetching Phase 1 revision..."));

        // Read the Phase 1 revised content (output of AiRevision standard mode)
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return ai_contextual_revision_job_failure(e, "fetch_phase1_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return ai_contextual_revision_job_failure(e, "fetch_phase1_note"),
        };
        if let Err(e) = tx.commit().await {
            return ai_contextual_revision_job_failure(e, "fetch_phase1_commit");
        }

        // Use revised content as Phase 1 output (the clean isolated revision).
        // Fall back to original content if Phase 1 didn't produce output.
        let phase1_content = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            warn!(
                note_id_present = true,
                detail = JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "fallback_to_original_for_contextual_revision",
                "Phase 1 revised content is empty, falling back to original content for Phase 2"
            );
            &note.original.content
        };

        if phase1_content.trim().is_empty() {
            return JobResult::Failed("No Phase 1 revision content available".into());
        }

        // Start provenance activity
        let activity_id = self
            .db
            .provenance
            .start_activity(
                note_id,
                "ai_revision_contextual",
                Some(matric_core::GenerationBackend::model_name(backend)),
            )
            .await
            .ok();

        // --- Intermediate step: embed Phase 1 output and find related notes ---
        ctx.report_progress(
            20,
            Some("Embedding Phase 1 revision for context discovery..."),
        );

        let embed_backend = matric_inference::OllamaBackend::from_env();
        let chunks = vec![phase1_content
            .chars()
            .take(matric_core::defaults::PREVIEW_EMBEDDING)
            .collect::<String>()];
        let vectors = match embed_backend.embed_texts(&chunks).await {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_CONTEXT_DISCOVERY_FAILURE_DETAIL,
                    "Failed to embed Phase 1 content for context discovery"
                );
                // Fall back: skip contextual revision, Phase 1 output stands as final.
                // Update the revision note so users know contextual enrichment was skipped.
                self.update_revision_note(
                    &schema_ctx,
                    note_id,
                    "AI standard revision (contextual enrichment skipped: embedding unavailable)",
                )
                .await;
                self.queue_concept_tagging(&ctx, note_id, schema, &model_override)
                    .await;
                return JobResult::Success(Some(serde_json::json!({
                    "skipped": true,
                    "reason": "embedding failed, Phase 1 output preserved",
                })));
            }
        };

        let query_vec = match vectors.into_iter().next() {
            Some(v) => v,
            None => {
                self.update_revision_note(
                    &schema_ctx,
                    note_id,
                    "AI standard revision (contextual enrichment skipped: no embedding produced)",
                )
                .await;
                self.queue_concept_tagging(&ctx, note_id, schema, &model_override)
                    .await;
                return JobResult::Success(Some(serde_json::json!({
                    "skipped": true,
                    "reason": "no embedding vector produced",
                })));
            }
        };

        ctx.report_progress(40, Some("Finding related notes for context..."));

        // Parse optional context_filter from payload (for ContextualFiltered mode)
        let context_filter: Option<matric_core::StrictTagFilter> = ctx
            .payload()
            .and_then(|p| p.get("context_filter"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        let fetch_limit = (MAX_CONTEXT_NOTES * 2) as i64;
        let related_notes = if let Some(filter) = &context_filter {
            // ContextualFiltered: scoped search
            match self
                .db
                .embeddings
                .find_similar_with_strict_filter(&query_vec, filter, fetch_limit, true)
                .await
            {
                Ok(hits) => hits
                    .into_iter()
                    .filter(|h| {
                        h.score > matric_core::defaults::RELATED_NOTES_MIN_SIMILARITY
                            && h.note_id != note_id
                    })
                    .take(MAX_CONTEXT_NOTES)
                    .collect::<Vec<_>>(),
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_CONTEXT_DISCOVERY_FAILURE_DETAIL,
                        "Filtered similarity search failed"
                    );
                    vec![]
                }
            }
        } else {
            // Contextual/Full: unscoped search
            match self
                .db
                .embeddings
                .find_similar(&query_vec, fetch_limit, true)
                .await
            {
                Ok(hits) => hits
                    .into_iter()
                    .filter(|h| {
                        h.score > matric_core::defaults::RELATED_NOTES_MIN_SIMILARITY
                            && h.note_id != note_id
                    })
                    .take(MAX_CONTEXT_NOTES)
                    .collect::<Vec<_>>(),
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_CONTEXT_DISCOVERY_FAILURE_DETAIL,
                        "Similarity search failed"
                    );
                    vec![]
                }
            }
        };

        let related_count = related_notes.len();
        let related_note_ids: Vec<uuid::Uuid> = related_notes.iter().map(|h| h.note_id).collect();

        if related_notes.is_empty() {
            // No related notes found — Phase 1 output stands as final.
            // Update revision note so users know contextual enrichment was attempted.
            info!(
                note_id_present = true,
                detail = JOB_CONTEXT_DISCOVERY_FAILURE_DETAIL,
                operation = "no_related_notes_for_contextual_revision",
                "No related notes found, Phase 1 revision is final"
            );
            self.update_revision_note(
                &schema_ctx,
                note_id,
                "AI standard revision (no related notes found for contextual enrichment)",
            )
            .await;
            self.queue_concept_tagging(&ctx, note_id, schema, &model_override)
                .await;
            return JobResult::Success(Some(serde_json::json!({
                "skipped": true,
                "reason": "no related notes found above similarity threshold",
                "phase1_preserved": true,
            })));
        }

        // --- Phase 2: Contextual re-revision with strong guardrails ---
        ctx.report_progress(60, Some("Generating contextual revision (phase 2)..."));

        // Build reference context from related notes (using original content for snippets)
        let mut reference_context = String::new();
        for hit in related_notes.iter().take(MAX_PROMPT_SNIPPETS) {
            if let Some(snippet) = &hit.snippet {
                let preview: String = snippet
                    .chars()
                    .take(matric_core::defaults::PREVIEW_CONTEXT_SNIPPET)
                    .collect();
                reference_context.push_str(&format!("- {}\n", preview));
            }
        }

        // Compute Phase 2 chunk budget, accounting for reference context overhead.
        // The reference context is included in every chunk's prompt, so it reduces
        // the space available for the primary content.
        let running_ctx = self.backend.running_context_length().await;
        let base_chunk_size = revision_chunk_size(&self.backend, running_ctx);
        let reference_overhead = reference_context.len();
        let chunk_max_phase2 = base_chunk_size
            .saturating_sub(reference_overhead / 2)
            .max(matric_core::defaults::REVISION_CHUNK_SIZE_MIN);
        let chunks = chunk_for_revision(phase1_content, chunk_max_phase2, 0);
        let total_chunks = chunks.len();
        let is_chunked = total_chunks > 1;

        let p2_base_timeout = self.backend.gen_timeout_secs();
        let p2_per_chunk_timeouts: Vec<u64> = chunks
            .iter()
            .map(|c| adaptive_timeout_secs(c.len(), p2_base_timeout))
            .collect();
        let p2_total_revision_secs: u64 = p2_per_chunk_timeouts
            .iter()
            .sum::<u64>()
            .max(matric_core::defaults::GEN_TIMEOUT_MIN_SECS);

        if is_chunked {
            info!(
                note_id_present = true,
                total_chunks,
                chunk_max = chunk_max_phase2,
                reference_overhead,
                content_len = phase1_content.len(),
                p2_total_revision_secs,
                detail = JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "split_contextual_revision_chunks",
                "Splitting Phase 2 content for chunked contextual revision"
            );
        }

        let p2_revision_deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(p2_total_revision_secs);

        // Generate contextual revision for each chunk
        let mut revised_parts: Vec<String> = Vec::with_capacity(total_chunks);
        let mut p2_single_chunk_error: Option<String> = None;
        for (chunk_idx, chunk_content) in chunks.iter().enumerate() {
            if is_chunked && std::time::Instant::now() >= p2_revision_deadline {
                warn!(
                    note_id_present = true,
                    chunk = chunk_idx + 1,
                    total = total_chunks,
                    p2_total_revision_secs,
                    completed = revised_parts.len(),
                    detail = JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "contextual_revision_budget_exhausted",
                    "Phase 2 total revision budget exhausted, stopping early"
                );
                break;
            }

            let chunk_progress: i32 = if is_chunked {
                60 + ((chunk_idx as i32 * 15) / total_chunks as i32)
            } else {
                60
            };
            let progress_msg = if is_chunked {
                format!(
                    "Generating contextual revision (chunk {}/{})...",
                    chunk_idx + 1,
                    total_chunks
                )
            } else {
                "Generating contextual revision (phase 2)...".to_string()
            };
            ctx.report_progress(chunk_progress, Some(&progress_msg));

            let continuity_note = if is_chunked {
                format!(
                    "\nNOTE: This is section {} of {} of a larger document. \
                     The REFERENCE CONTEXT applies to the entire document. \
                     Revise this section while maintaining continuity.\n\n",
                    chunk_idx + 1,
                    total_chunks
                )
            } else {
                String::new()
            };

            // Add type hint so Phase 2 preserves type-specific structure from Phase 1
            let type_hint = if let Some(ref ct) = content_type_name {
                format!(
                    "\nIMPORTANT: This content is a {}. Preserve the structural sections (Summary, headings, etc.) from the primary content.\n",
                    ct.replace('-', " ")
                )
            } else {
                String::new()
            };

            let prompt = format!(
                r#"You are an intelligent note-taking assistant performing a contextual revision.
{continuity}{type_hint}
## PRIMARY CONTENT (this is the note you are revising — your output MUST be a revision of this):
{phase1}

## REFERENCE CONTEXT (supplementary only — use ONLY if directly relevant to the primary content):
{context}

STRICT RULES:
1. Your output MUST be a revision of the PRIMARY CONTENT section above
2. NEVER replace or override the primary content with reference material
3. Reference context is supplementary — mention connections ONLY when they genuinely clarify the primary content
4. If no reference items are relevant to the primary content, output the primary content unchanged
5. Preserve ALL original meaning and information from the primary content
6. Do NOT fabricate cross-references that are not genuinely supported by the reference context

What you MAY do:
- Note genuine connections between the primary content and reference items
- Add brief contextual annotations where a reference item directly relates
- Improve organization if the connection adds clarity

Output the revised note in clean markdown format. Do not add any labels, markers, or metadata."#,
                continuity = continuity_note,
                type_hint = type_hint,
                phase1 = chunk_content,
                context = reference_context
            );

            let chunk_timeout = p2_per_chunk_timeouts[chunk_idx];
            let result = match &overridden {
                Some(b) => b.generate(&prompt).await,
                None => {
                    self.backend
                        .generate_with_timeout(&prompt, chunk_timeout)
                        .await
                }
            };

            match result {
                Ok(r) => {
                    let cleaned = clean_enhanced_content(r.trim(), &prompt);
                    if !cleaned.is_empty() {
                        revised_parts.push(cleaned);
                    }
                }
                Err(e) => {
                    if is_chunked {
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                            chunk = chunk_idx + 1,
                            total = total_chunks,
                            timeout_secs = chunk_timeout,
                            "Phase 2 chunk revision failed, skipping"
                        );
                    } else {
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
                            timeout_secs = chunk_timeout,
                            "AI contextual revision generation failed"
                        );
                        p2_single_chunk_error = Some(AI_GENERATION_JOB_FAILURE.to_string());
                        break;
                    }
                }
            }
        }

        if let Some(e) = p2_single_chunk_error {
            return JobResult::Failed(e);
        }

        let revised = revised_parts.join("\n\n");

        if revised.is_empty() {
            return ai_contextual_revision_job_failure(
                "AI contextual revision returned empty after content cleaning \
                 (model may have echoed the prompt instead of generating a revision)"
                    .to_string(),
                "empty_contextual_revision_after_cleaning",
            );
        }

        ctx.report_progress(80, Some("Saving contextual revision..."));

        let revision_note = match revision_mode {
            RevisionMode::ContextualFiltered => "AI contextual revision (filtered scope)",
            _ => "AI contextual revision with cross-references",
        };

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return ai_contextual_revision_job_failure(e, "save_contextual_begin_tx"),
        };
        if let Err(e) = self
            .db
            .notes
            .update_revised_tx(&mut tx, note_id, &revised, Some(revision_note))
            .await
        {
            return ai_contextual_revision_job_failure(e, "save_contextual_revision");
        }
        if let Err(e) = tx.commit().await {
            return ai_contextual_revision_job_failure(e, "save_contextual_commit");
        }

        // Record provenance: edges to each related note used as context
        ctx.report_progress(90, Some("Recording provenance..."));

        if let Ok(Some(chain)) = self.db.provenance.get_chain(note_id).await {
            let rev_id = chain.revision_id;

            if !related_note_ids.is_empty() {
                if let Err(e) = self
                    .db
                    .provenance
                    .record_edges_batch(rev_id, &related_note_ids, &ProvRelation::Used)
                    .await
                {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                        "Failed to record provenance edges"
                    );
                }
            }

            if let Some(act_id) = activity_id {
                let metadata = serde_json::json!({
                    "revision_mode": format!("{:?}", revision_mode),
                    "related_notes_used": related_count,
                    "revised_length": revised.len(),
                    "context_filtered": context_filter.is_some(),
                });
                if let Err(e) = self
                    .db
                    .provenance
                    .complete_activity(act_id, Some(rev_id), Some(metadata))
                    .await
                {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                        "Failed to complete provenance activity"
                    );
                }
            }
        }

        // Chain ConceptTagging now that the final revised content is available.
        self.queue_concept_tagging(&ctx, note_id, schema, &model_override)
            .await;

        ctx.report_progress(100, Some("Contextual revision complete"));
        info!(
            note_id_present = true,
            mode = ?revision_mode,
            related_count = related_count,
            duration_ms = start.elapsed().as_millis() as u64,
            detail = JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
            operation = "complete_contextual_revision",
            "AI contextual revision completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "revised_length": revised.len(),
            "revision_mode": revision_mode,
            "related_notes_used": related_count,
            "context_filtered": context_filter.is_some(),
            "content_type": content_type_name,
            "chunked": is_chunked,
            "chunk_count": total_chunks
        })))
    }
}

/// Handler for embedding generation jobs.
pub struct EmbeddingHandler {
    db: Database,
    backend: OllamaBackend,
}

impl EmbeddingHandler {
    pub fn new(db: Database, backend: OllamaBackend) -> Self {
        Self { db, backend }
    }
}

#[async_trait]
impl JobHandler for EmbeddingHandler {
    fn job_type(&self) -> JobType {
        JobType::Embedding
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "embedding", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        // Start provenance activity (#430)
        let activity_id = self
            .db
            .provenance
            .start_activity(
                note_id,
                "embedding",
                Some(matric_core::EmbeddingBackend::model_name(&self.backend)),
            )
            .await
            .ok();

        ctx.report_progress(10, Some("Fetching note..."));

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return embedding_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return embedding_job_failure(e, "fetch_note"),
        };

        // Fetch SKOS concept labels for embedding enrichment (#424, #475).
        // Tags are available because ConceptTagging runs before Embedding in the pipeline.
        // TF-IDF filtering (#475): exclude "stopword" concepts that appear in >80% of docs
        // (configurable via EMBED_CONCEPT_MAX_DOC_FREQ). Only discriminating concepts
        // are prepended — high-frequency concepts make all embeddings more uniform.
        let max_doc_freq = matric_core::defaults::embed_concept_max_doc_freq();
        let concept_labels: Vec<String> = sqlx::query_scalar(
            "SELECT l.value FROM note_skos_concept nc \
             JOIN skos_concept_label l ON nc.concept_id = l.concept_id \
             JOIN skos_concept c ON nc.concept_id = c.id \
             WHERE nc.note_id = $1 AND l.label_type = 'pref_label' \
               AND c.note_count::float / GREATEST((SELECT COUNT(*) FROM note WHERE deleted_at IS NULL), 1) <= $2 \
             ORDER BY nc.is_primary DESC, nc.relevance_score DESC",
        )
        .bind(note_id)
        .bind(max_doc_freq)
        .fetch_all(&mut *tx)
        .await
        .unwrap_or_default();

        // Fetch concept relationships for structured embedding context (#435).
        // Includes broader (parent), narrower (child), and related (associative)
        // relationships so the embedding captures the full semantic graph.
        let concept_relations: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT COALESCE(sl.value, sc.notation) as source_label, \
                    sre.relation_type::text as rel_type, \
                    COALESCE(tl.value, tc.notation) as target_label \
             FROM note_skos_concept nc \
             JOIN skos_concept sc ON nc.concept_id = sc.id \
             JOIN skos_semantic_relation_edge sre ON sre.subject_id = sc.id \
             JOIN skos_concept tc ON sre.object_id = tc.id \
             LEFT JOIN skos_concept_label sl ON sc.id = sl.concept_id \
                 AND sl.label_type = 'pref_label' AND sl.language = 'en' \
             LEFT JOIN skos_concept_label tl ON tc.id = tl.concept_id \
                 AND tl.label_type = 'pref_label' AND tl.language = 'en' \
             WHERE nc.note_id = $1 \
             ORDER BY sre.relation_type, sl.value",
        )
        .bind(note_id)
        .fetch_all(&mut *tx)
        .await
        .unwrap_or_default();

        if let Err(e) = tx.commit().await {
            return embedding_job_failure(e, "fetch_note_commit");
        }

        // Use revised content if available, otherwise original
        let base_content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if base_content.trim().is_empty() {
            return JobResult::Success(Some(serde_json::json!({"chunks": 0})));
        }

        // DOCUMENT COMPOSITION (#485):
        // Resolve embedding config to determine what properties go into the
        // embedding text. The config's DocumentComposition controls whether
        // title, content, tags, and/or concepts are included.
        //
        // Default composition (title+content only) was chosen because including
        // tags created artificial topic-cluster gravity in the graph — notes
        // sharing tags clustered in vector space regardless of content similarity.
        // Tags influence linking via tag_boost_weight and FTS via BM25F weight-B.
        //
        // Priority: set-specific config > default config > hardcoded defaults.
        let embedding_set_id = ctx
            .payload()
            .and_then(|p| p.get("embedding_set_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok());

        let embed_config = if let Some(set_id) = embedding_set_id {
            // Fetch config for the specific embedding set
            if let Ok(Some(set)) = self.db.embedding_sets.get_by_id(set_id).await {
                if let Some(config_id) = set.embedding_config_id {
                    self.db
                        .embedding_sets
                        .get_config(config_id)
                        .await
                        .ok()
                        .flatten()
                } else {
                    self.db
                        .embedding_sets
                        .get_default_config()
                        .await
                        .ok()
                        .flatten()
                }
            } else {
                self.db
                    .embedding_sets
                    .get_default_config()
                    .await
                    .ok()
                    .flatten()
            }
        } else {
            self.db
                .embedding_sets
                .get_default_config()
                .await
                .ok()
                .flatten()
        };

        let composition = embed_config
            .as_ref()
            .map(|c| c.document_composition.clone())
            .unwrap_or_default();

        let title = note.note.title.as_deref().unwrap_or("");
        let content = composition.build_text(title, base_content, &concept_labels);

        ctx.report_progress(30, Some("Chunking content..."));

        // Resolve chunking config with priority:
        // 1. Note's document type (if assigned)
        // 2. Resolved embedding config (from set or default)
        // 3. ChunkerConfig::default() (hardcoded fallback)
        let chunker_config = if let Some(doc_type_id) = note.note.document_type_id {
            if let Ok(Some(doc_type)) = self.db.document_types.get(doc_type_id).await {
                let max = doc_type.chunk_size_default as usize;
                ChunkerConfig {
                    max_chunk_size: max,
                    min_chunk_size: (max / 10).max(50),
                    overlap: doc_type.chunk_overlap_default as usize,
                }
            } else if let Some(ref config) = embed_config {
                let max = config.chunk_size as usize;
                ChunkerConfig {
                    max_chunk_size: max,
                    min_chunk_size: (max / 10).max(50),
                    overlap: config.chunk_overlap as usize,
                }
            } else {
                ChunkerConfig::default()
            }
        } else if let Some(ref config) = embed_config {
            let max = config.chunk_size as usize;
            ChunkerConfig {
                max_chunk_size: max,
                min_chunk_size: (max / 10).max(50),
                overlap: config.chunk_overlap as usize,
            }
        } else {
            ChunkerConfig::default()
        };

        let chunker = SemanticChunker::new(chunker_config);
        let semantic_chunks = chunker.chunk(&content);
        let chunks: Vec<String> = semantic_chunks.into_iter().map(|c| c.text).collect();
        if chunks.is_empty() {
            return JobResult::Success(Some(serde_json::json!({"chunks": 0})));
        }

        ctx.report_progress(50, Some("Generating embeddings..."));

        let vectors = match self.backend.embed_texts(&chunks).await {
            Ok(v) => v,
            Err(e) => return embedding_job_failure(e, "generate_embeddings"),
        };

        ctx.report_progress(70, Some("Storing embeddings..."));

        // Pair chunks with vectors
        let chunk_vectors: Vec<(String, pgvector::Vector)> =
            chunks.into_iter().zip(vectors).collect();

        let chunk_count = chunk_vectors.len();

        // Store embeddings — use set-scoped storage if embedding_set_id is in payload
        let model_name = EmbeddingBackend::model_name(&self.backend);

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return embedding_job_failure(e, "store_begin_tx"),
        };
        let store_result = if let Some(set_id) = embedding_set_id {
            // Delete existing embeddings for this specific set
            if let Err(e) =
                sqlx::query("DELETE FROM embedding WHERE note_id = $1 AND embedding_set_id = $2")
                    .bind(note_id)
                    .bind(set_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(matric_core::Error::Database)
            {
                return embedding_job_failure(e, "delete_existing_embeddings");
            }
            if !chunk_vectors.is_empty() {
                let now = chrono::Utc::now();
                for (i, (text, vector)) in chunk_vectors.into_iter().enumerate() {
                    if let Err(e) = sqlx::query(
                        "INSERT INTO embedding (id, note_id, chunk_index, text, vector, model, created_at, embedding_set_id)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    )
                    .bind(matric_db::new_v7())
                    .bind(note_id)
                    .bind(i as i32)
                    .bind(&text)
                    .bind(&vector)
                    .bind(model_name)
                    .bind(now)
                    .bind(set_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(matric_core::Error::Database)
                    {
                        return embedding_job_failure(e, "insert_embedding");
                    }
                }
            }
            Ok(())
        } else {
            self.db
                .embeddings
                .store_tx(&mut tx, note_id, chunk_vectors, model_name)
                .await
        };
        if let Err(e) = tx.commit().await.map_err(matric_core::Error::Database) {
            return embedding_job_failure(e, "commit_embeddings");
        }

        if let Err(e) = store_result {
            return embedding_job_failure(e, "store_embeddings");
        }

        // Complete provenance activity (#430)
        if let Some(act_id) = activity_id {
            let prov_metadata = serde_json::json!({
                "chunks": chunk_count,
                "model": model_name,
                "concept_labels_available": concept_labels.len(),
                "concept_relations_available": concept_relations.len(),
                "composition": serde_json::to_value(&composition).unwrap_or_default(),
                "embedding_set_id": embedding_set_id.map(|id| id.to_string()),
            });
            if let Err(e) = self
                .db
                .provenance
                .complete_activity(act_id, None, Some(prov_metadata))
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                    "Failed to complete embedding provenance activity"
                );
            }
        }

        ctx.report_progress(100, Some("Embeddings complete"));
        info!(
            note_id_present = true,
            chunk_count = chunk_count,
            duration_ms = start.elapsed().as_millis() as u64,
            detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
            operation = "complete_embedding_generation",
            "Embeddings generated"
        );

        JobResult::Success(Some(serde_json::json!({
            "chunks": chunk_count
        })))
    }
}

/// Handler for title generation jobs.
///
/// Generates a concise title from note content using the fast model.
/// Follows the same pattern as MetadataExtractionHandler: fetch note,
/// preview content, generate with fast-first model routing.
pub struct TitleGenerationHandler {
    db: Database,
    backend: OllamaBackend,
    /// Fast model backend (#439).
    fast_backend: Option<OllamaBackend>,
    registry: Arc<ProviderRegistry>,
}

impl TitleGenerationHandler {
    pub fn new(
        db: Database,
        backend: OllamaBackend,
        fast_backend: Option<OllamaBackend>,
        registry: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            db,
            backend,
            fast_backend,
            registry,
        }
    }

    /// Queue a tier-escalation job for title generation.
    /// Returns the new job ID if queued (None if deduplicated or on error).
    async fn queue_tier_escalation(
        &self,
        note_id: uuid::Uuid,
        schema: &str,
        next_tier: i16,
    ) -> Option<uuid::Uuid> {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::TitleGeneration,
                JobType::TitleGeneration.default_priority(),
                payload,
                Some(next_tier),
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_TITLE_ESCALATION_FAILURE_DETAIL,
                    next_tier,
                    operation = "queue_title_generation_tier_escalation",
                    "Failed to queue title generation tier escalation"
                );
                None
            }
        }
    }
}

#[async_trait]
impl JobHandler for TitleGenerationHandler {
    fn job_type(&self) -> JobType {
        JobType::TitleGeneration
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "title_gen", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let model_override = extract_model_override(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };

        ctx.report_progress(10, Some("Fetching note..."));

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return title_generation_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return title_generation_job_failure(e, "fetch_note"),
        };
        tx.commit().await.ok();

        // Skip if already has a title (unless force=true from reprocess, #578)
        let is_force = ctx
            .payload()
            .and_then(|p| p.get("force"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if note.note.title.is_some() && !is_force {
            return JobResult::Success(Some(serde_json::json!({"skipped": true})));
        }

        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            return JobResult::Failed("Note has no content".into());
        }

        ctx.report_progress(20, Some("Generating title..."));

        // Tiered model routing:
        // Tier 1 (FAST_GPU, default): fast model → escalate to tier-2 on failure
        // Tier 2 (STANDARD_GPU): standard model → fail cleanly
        let is_standard_tier = ctx.job.cost_tier == Some(matric_core::cost_tier::STANDARD_GPU);
        let use_fast = !is_standard_tier && overridden.is_none() && self.fast_backend.is_some();

        let backend: &dyn GenerationBackend = match (&overridden, use_fast) {
            (Some(b), _) => b.as_ref(),
            (_, true) => self.fast_backend.as_ref().unwrap(),
            (_, false) => &self.backend,
        };

        // Start provenance activity (#430)
        let activity_id = self
            .db
            .provenance
            .start_activity(
                note_id,
                "title_generation",
                Some(matric_core::GenerationBackend::model_name(backend)),
            )
            .await
            .ok();

        let content_preview: String = content
            .chars()
            .take(matric_core::defaults::PREVIEW_EMBEDDING)
            .collect();

        let prompt = format!(
            r#"Generate a concise, descriptive title (3-8 words) for this content. Be specific. Avoid generic words like "Note", "Document", "Text". Output only the title, no quotes or explanation.

Content:
{}"#,
            content_preview
        );

        let clean_title = |raw: String| -> String {
            let cleaned = raw
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .replace('\n', " ");
            cleaned
                .chars()
                .take(matric_core::defaults::TITLE_MAX_LENGTH)
                .collect::<String>()
                .trim()
                .to_string()
        };

        let title = match backend.generate(&prompt).await {
            Ok(t) => {
                let t = clean_title(t);
                if t.is_empty() || t.len() < matric_core::defaults::TITLE_MIN_LENGTH {
                    if use_fast {
                        info!("Fast model generated invalid title, escalating to tier-2");
                        if let Some(job_id) = self
                            .queue_tier_escalation(
                                note_id,
                                schema,
                                matric_core::cost_tier::STANDARD_GPU,
                            )
                            .await
                        {
                            ctx.emit_job_queued(job_id, JobType::TitleGeneration, Some(note_id));
                        }
                        return JobResult::Success(Some(serde_json::json!({
                            "escalated": true,
                            "reason": "fast_model_invalid_title"
                        })));
                    }
                    return JobResult::Failed("Invalid title generated".into());
                }
                t
            }
            Err(e) => {
                if use_fast {
                    info!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_TITLE_ESCALATION_FAILURE_DETAIL,
                        "Fast model failed for title generation, escalating to tier-2"
                    );
                    if let Some(job_id) = self
                        .queue_tier_escalation(
                            note_id,
                            schema,
                            matric_core::cost_tier::STANDARD_GPU,
                        )
                        .await
                    {
                        ctx.emit_job_queued(job_id, JobType::TitleGeneration, Some(note_id));
                    }
                    return JobResult::Success(Some(serde_json::json!({
                        "escalated": true,
                        "reason": "fast_model_failed"
                    })));
                }
                return title_generation_job_failure(e, "generate_title");
            }
        };

        ctx.report_progress(80, Some("Saving title..."));

        // Save the title
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return title_generation_job_failure(e, "save_title_begin_tx"),
        };
        if let Err(e) = self
            .db
            .notes
            .update_title_tx(&mut tx, note_id, &title)
            .await
        {
            return title_generation_job_failure(e, "save_title");
        }
        if let Err(e) = tx.commit().await {
            return title_generation_job_failure(e, "save_title_commit");
        }

        // Complete provenance activity (#430)
        if let Some(act_id) = activity_id {
            let prov_metadata = serde_json::json!({
                "title": &title,
            });
            if let Err(e) = self
                .db
                .provenance
                .complete_activity(act_id, None, Some(prov_metadata))
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                    "Failed to complete title generation provenance activity"
                );
            }
        }

        info!(
            note_id_present = true,
            title_len = diagnostic_len(&title),
            duration_ms = start.elapsed().as_millis() as u64,
            detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
            operation = "complete_title_generation",
            "Title generated"
        );

        ctx.report_progress(100, Some("Title generation completed"));

        JobResult::Success(Some(serde_json::json!({
            "title": title,
        })))
    }
}

/// Handler for link detection jobs - creates both semantic and keyword links.
///
/// Supports two strategies:
/// - **Threshold** (legacy): Link all notes above a cosine similarity threshold.
/// - **HNSW Heuristic** (Algorithm 4, Malkov & Yashunin 2018): Diverse neighbor
///   selection that approximates the Relative Neighborhood Graph, preventing
///   star topology on clustered data.
pub struct LinkingHandler {
    db: Database,
}

impl LinkingHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Parse [[wiki-style]] links from content and return target titles.
    fn parse_wiki_links(content: &str) -> Vec<String> {
        let re = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
        re.captures_iter(content)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Resolve a wiki-link title to a note ID by searching for matching titles.
    async fn resolve_wiki_link(&self, title: &str) -> Option<uuid::Uuid> {
        let results = self.db.search.search(title, 5, true).await.ok()?;

        for hit in results {
            if let Some(hit_title) = &hit.title {
                if hit_title.to_lowercase() == title.to_lowercase() {
                    return Some(hit.note_id);
                }
            }
        }
        None
    }

    /// Cosine similarity between two vectors.
    fn cosine_similarity(a: &pgvector::Vector, b: &pgvector::Vector) -> f32 {
        let a = a.as_slice();
        let b = b.as_slice();
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }

    /// Compute tag overlap score between two sets of SKOS concept IDs.
    ///
    /// Returns shared_count / max(count_a, count_b), or 0.0 if both sets are empty.
    fn tag_overlap_score(
        tags_a: &std::collections::HashSet<uuid::Uuid>,
        tags_b: &std::collections::HashSet<uuid::Uuid>,
    ) -> f32 {
        let max_count = tags_a.len().max(tags_b.len());
        if max_count == 0 {
            return 0.0;
        }
        let shared = tags_a.intersection(tags_b).count();
        shared as f32 / max_count as f32
    }

    /// Fetch SKOS concepts for source + candidates and compute blended scores.
    ///
    /// Blended score = (embedding_sim * (1 - tag_weight)) + (tag_overlap * tag_weight).
    /// Updates SearchHit scores in-place and re-sorts by blended score descending.
    async fn apply_tag_boost(
        &self,
        source_id: uuid::Uuid,
        candidates: &mut [(matric_core::SearchHit, pgvector::Vector)],
        tag_weight: f32,
    ) {
        if tag_weight <= 0.0 || candidates.is_empty() {
            return;
        }

        // Collect all note IDs (source + candidates)
        let mut note_ids: Vec<uuid::Uuid> = candidates.iter().map(|(h, _)| h.note_id).collect();
        note_ids.push(source_id);

        // Bulk fetch concept IDs
        let concept_map = match self.db.skos.get_concept_ids_bulk(&note_ids).await {
            Ok(m) => m,
            Err(e) => {
                debug!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "tag_boost_concept_fetch",
                    candidate_count = candidates.len(),
                    "Failed to fetch concepts for tag boost, using pure embedding similarity"
                );
                return;
            }
        };

        let empty_set = std::collections::HashSet::new();
        let source_concepts = concept_map.get(&source_id).unwrap_or(&empty_set);

        // If source has no tags, tag_overlap will be 0 for all candidates.
        // Skip blending to avoid needlessly penalizing embedding scores.
        if source_concepts.is_empty() {
            return;
        }

        let embed_weight = 1.0 - tag_weight;

        for (hit, _) in candidates.iter_mut() {
            let candidate_concepts = concept_map.get(&hit.note_id).unwrap_or(&empty_set);
            let overlap = Self::tag_overlap_score(source_concepts, candidate_concepts);
            hit.score = (hit.score * embed_weight) + (overlap * tag_weight);
        }

        // Re-sort by blended score (descending)
        candidates.sort_by(|a, b| {
            b.0.score
                .partial_cmp(&a.0.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// HNSW Algorithm 4: SELECT-NEIGHBORS-HEURISTIC (Malkov & Yashunin 2018).
    ///
    /// Selects up to `m` neighbors from candidates by accepting a candidate only
    /// if it is closer to the source than to any already-accepted neighbor. This
    /// creates connections in diverse directions, approximating the Relative
    /// Neighborhood Graph and preventing star topology on clustered data.
    ///
    /// Returns selected (hit, vector) pairs.
    fn select_neighbors_heuristic(
        source_vec: &pgvector::Vector,
        candidates: Vec<(matric_core::SearchHit, pgvector::Vector)>,
        m: usize,
        _extend_candidates: bool,
        keep_pruned: bool,
    ) -> Vec<(matric_core::SearchHit, pgvector::Vector)> {
        // candidates should already be sorted by descending similarity to source
        let mut result: Vec<(matric_core::SearchHit, pgvector::Vector)> = Vec::with_capacity(m);
        let mut discarded: Vec<(matric_core::SearchHit, pgvector::Vector)> = Vec::new();

        for (hit, vec) in candidates {
            if result.len() >= m {
                break;
            }

            let sim_to_source = Self::cosine_similarity(&vec, source_vec);

            // Check diversity: is this candidate closer to source than to
            // ANY already-accepted neighbor?
            let is_diverse = result.iter().all(|(_, accepted_vec)| {
                let sim_to_accepted = Self::cosine_similarity(&vec, accepted_vec);
                sim_to_source > sim_to_accepted
            });

            if is_diverse {
                result.push((hit, vec));
            } else {
                discarded.push((hit, vec));
            }
        }

        // Fill remaining slots from discarded candidates (keepPrunedConnections)
        if keep_pruned {
            for item in discarded {
                if result.len() >= m {
                    break;
                }
                result.push(item);
            }
        }

        result
    }

    /// Link using HNSW Algorithm 4 (diverse neighbor selection heuristic).
    async fn link_by_hnsw_heuristic(
        &self,
        note_id: uuid::Uuid,
        source_vec: &pgvector::Vector,
        config: &matric_core::defaults::GraphConfig,
        note_count: usize,
        schema_ctx: &SchemaContext,
    ) -> std::result::Result<usize, String> {
        let k = config.effective_k(note_count);
        // Fetch 3*k candidates to give the heuristic enough to work with
        let candidate_limit = (k * 3).max(15) as i64;

        let candidates = {
            let mut tx = schema_ctx
                .begin_tx()
                .await
                .map_err(|e| linking_step_failure(e, "hnsw_candidate_tx"))?;
            let c = self
                .db
                .embeddings
                .find_similar_with_vectors_tx(&mut tx, source_vec, candidate_limit, true)
                .await;
            tx.commit().await.ok();
            match c {
                Ok(c) => c,
                Err(e) => return Err(linking_step_failure(e, "hnsw_find_candidates")),
            }
        };

        // Filter self and below minimum similarity
        let mut filtered: Vec<_> = candidates
            .into_iter()
            .filter(|(hit, _)| hit.note_id != note_id && hit.score >= config.min_similarity)
            .collect();

        if filtered.is_empty() {
            debug!(note_id = %note_id, "No candidates above min_similarity");
            return Ok(0);
        }

        // Apply tag-based boost: blend embedding similarity with SKOS tag overlap (#420)
        self.apply_tag_boost(note_id, &mut filtered, config.tag_boost_weight)
            .await;

        // Run Algorithm 4: diverse neighbor selection
        let selected = Self::select_neighbors_heuristic(
            source_vec,
            filtered,
            k,
            config.extend_candidates,
            config.keep_pruned,
        );

        let mut created = 0;
        for (i, (hit, _)) in selected.iter().enumerate() {
            let metadata = serde_json::json!({
                "strategy": "hnsw_heuristic",
                "k": k,
                "rank": i + 1,
                "tag_boost_weight": config.tag_boost_weight,
            });
            let result = {
                let mut tx = schema_ctx
                    .begin_tx()
                    .await
                    .map_err(|e| linking_step_failure(e, "hnsw_create_link_tx"))?;
                let res = self
                    .db
                    .links
                    .create_reciprocal_tx(
                        &mut tx,
                        note_id,
                        hit.note_id,
                        "semantic",
                        hit.score,
                        Some(metadata),
                    )
                    .await;
                if let Err(e) = tx.commit().await {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "hnsw_link_commit",
                        "Link commit failed"
                    );
                }
                res
            };
            if let Err(e) = result {
                debug!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "hnsw_create_reciprocal_link",
                    "Failed to create reciprocal link"
                );
            } else {
                created += 1;
            }
        }

        // Isolated node fallback: if heuristic selected nothing but we had
        // candidates, link to single best match to prevent graph isolation.
        if created == 0 {
            let fallback_candidates = {
                let mut tx = schema_ctx
                    .begin_tx()
                    .await
                    .map_err(|e| linking_step_failure(e, "hnsw_fallback_candidate_tx"))?;
                let c = self
                    .db
                    .embeddings
                    .find_similar_tx(&mut tx, source_vec, 2, true)
                    .await;
                tx.commit().await.ok();
                match c {
                    Ok(c) => c,
                    Err(_) => return Ok(0),
                }
            };
            if let Some(best_hit) = fallback_candidates
                .into_iter()
                .find(|h| h.note_id != note_id && h.score >= config.min_similarity)
            {
                let metadata = serde_json::json!({
                    "strategy": "hnsw_fallback",
                    "k": k,
                    "reason": "no_diverse_neighbors",
                });
                let result = {
                    let mut tx = schema_ctx
                        .begin_tx()
                        .await
                        .map_err(|e| linking_step_failure(e, "hnsw_fallback_link_tx"))?;
                    let res = self
                        .db
                        .links
                        .create_reciprocal_tx(
                            &mut tx,
                            note_id,
                            best_hit.note_id,
                            "semantic",
                            best_hit.score,
                            Some(metadata),
                        )
                        .await;
                    if let Err(e) = tx.commit().await {
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                            operation = "hnsw_fallback_link_commit",
                            "Fallback link commit failed"
                        );
                    }
                    res
                };
                if let Err(e) = result {
                    debug!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "hnsw_create_fallback_link",
                        "Failed to create fallback link"
                    );
                } else {
                    created += 1;
                }
            }
        }

        Ok(created)
    }

    /// Link using legacy threshold approach (unchanged from pre-#386 behavior).
    async fn link_by_threshold(
        &self,
        note_id: uuid::Uuid,
        source_vec: &pgvector::Vector,
        link_threshold: f32,
        tag_boost_weight: f32,
        schema_ctx: &SchemaContext,
    ) -> std::result::Result<usize, String> {
        let similar = {
            let mut tx = schema_ctx
                .begin_tx()
                .await
                .map_err(|e| linking_step_failure(e, "threshold_candidate_tx"))?;
            let s = self
                .db
                .embeddings
                .find_similar_tx(&mut tx, source_vec, 10, true)
                .await
                .map_err(|e| linking_step_failure(e, "threshold_find_candidates"))?;
            tx.commit()
                .await
                .map_err(|e| linking_step_failure(e, "threshold_candidate_commit"))?;
            s
        };

        // Apply tag-based score boost (#420)
        let boosted = if tag_boost_weight > 0.0 {
            let mut note_ids: Vec<uuid::Uuid> = similar.iter().map(|h| h.note_id).collect();
            note_ids.push(note_id);
            let concept_map = self.db.skos.get_concept_ids_bulk(&note_ids).await.ok();
            if let Some(ref cmap) = concept_map {
                let empty_set = std::collections::HashSet::new();
                let source_concepts = cmap.get(&note_id).unwrap_or(&empty_set);
                if !source_concepts.is_empty() {
                    let embed_weight = 1.0 - tag_boost_weight;
                    similar
                        .into_iter()
                        .map(|mut h| {
                            let candidate_concepts = cmap.get(&h.note_id).unwrap_or(&empty_set);
                            let overlap =
                                Self::tag_overlap_score(source_concepts, candidate_concepts);
                            h.score = (h.score * embed_weight) + (overlap * tag_boost_weight);
                            h
                        })
                        .collect()
                } else {
                    similar
                }
            } else {
                similar
            }
        } else {
            similar
        };

        let mut created = 0;
        for hit in boosted {
            if hit.note_id == note_id || hit.score < link_threshold {
                continue;
            }

            // Forward link (new -> old)
            {
                let mut tx = schema_ctx
                    .begin_tx()
                    .await
                    .map_err(|e| linking_step_failure(e, "threshold_forward_link_tx"))?;
                let res = self
                    .db
                    .links
                    .create_tx(&mut tx, note_id, hit.note_id, "semantic", hit.score, None)
                    .await;
                tx.commit().await.ok();
                if let Err(e) = res {
                    debug!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "threshold_create_forward_link",
                        "Failed to create forward link"
                    );
                } else {
                    created += 1;
                }
            }

            // Backward link (old -> new)
            {
                let mut tx = schema_ctx
                    .begin_tx()
                    .await
                    .map_err(|e| linking_step_failure(e, "threshold_backward_link_tx"))?;
                let res = self
                    .db
                    .links
                    .create_tx(&mut tx, hit.note_id, note_id, "semantic", hit.score, None)
                    .await;
                tx.commit().await.ok();
                if let Err(e) = res {
                    debug!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "threshold_create_backward_link",
                        "Failed to create backward link"
                    );
                } else {
                    created += 1;
                }
            }
        }

        Ok(created)
    }
}

#[async_trait]
impl JobHandler for LinkingHandler {
    fn job_type(&self) -> JobType {
        JobType::Linking
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "linking", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        // Start provenance activity (#430)
        let activity_id = self
            .db
            .provenance
            .start_activity(note_id, "linking", None)
            .await
            .ok();

        let mut created = 0;
        #[allow(clippy::needless_late_init)]
        let wiki_links_found;
        let mut wiki_links_resolved = 0;

        // Load graph configuration from environment
        let graph_config = matric_core::defaults::GraphConfig::from_env();

        // First, parse wiki-style [[links]] from note content
        ctx.report_progress(10, Some("Parsing wiki-style links..."));

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return linking_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return linking_job_failure(e, "fetch_note"),
        };
        if let Err(e) = tx.commit().await {
            return linking_job_failure(e, "fetch_note_commit");
        }

        // Determine content-type-aware similarity threshold (for threshold strategy).
        let link_threshold = if let Some(dt_id) = note.note.document_type_id {
            match self.db.document_types.get(dt_id).await {
                Ok(Some(dt)) => matric_core::defaults::semantic_link_threshold_for(dt.category),
                _ => matric_core::defaults::SEMANTIC_LINK_THRESHOLD,
            }
        } else {
            matric_core::defaults::SEMANTIC_LINK_THRESHOLD
        };

        // Use revised content if available, otherwise original
        let content = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        let wiki_links = Self::parse_wiki_links(content);
        wiki_links_found = wiki_links.len();

        ctx.report_progress(20, Some(&format!("Found {} wiki-links", wiki_links_found)));

        // Resolve and create explicit links from wiki-style links
        for link_title in &wiki_links {
            if let Some(target_id) = self.resolve_wiki_link(link_title).await {
                if target_id != note_id {
                    let metadata = serde_json::json!({"wiki_title": link_title});
                    if let Err(e) = self
                        .db
                        .links
                        .create(note_id, target_id, "wiki", 1.0, Some(metadata))
                        .await
                    {
                        debug!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                            operation = "create_wiki_link",
                            title_len = link_title.chars().count(),
                            "Failed to create wiki link"
                        );
                    } else {
                        created += 1;
                        wiki_links_resolved += 1;
                    }
                }
            } else {
                debug!(target = %link_title, "Wiki-link target not found");
            }
        }

        ctx.report_progress(40, Some("Finding embeddings for semantic linking..."));

        // Get embeddings for this note
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return linking_job_failure(e, "fetch_embeddings_begin_tx"),
        };
        let embeddings = match self.db.embeddings.get_for_note_tx(&mut tx, note_id).await {
            Ok(e) => {
                tx.commit().await.ok();
                e
            }
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "fetch_note_embeddings",
                    "No embeddings for note, skipping semantic linking"
                );
                return JobResult::Success(Some(serde_json::json!({
                    "links_created": created,
                    "wiki_links_found": wiki_links_found,
                    "wiki_links_resolved": wiki_links_resolved
                })));
            }
        };

        if embeddings.is_empty() {
            return JobResult::Success(Some(serde_json::json!({
                "links_created": created,
                "wiki_links_found": wiki_links_found,
                "wiki_links_resolved": wiki_links_resolved
            })));
        }

        ctx.report_progress(60, Some("Creating semantic links..."));

        // Dispatch to strategy
        let semantic_created = match graph_config.strategy {
            matric_core::defaults::GraphLinkingStrategy::HnswHeuristic => {
                // Count notes in schema for adaptive k
                let note_count = {
                    let mut tx = schema_ctx.begin_tx().await.map(Some).unwrap_or(None);
                    if let Some(ref mut tx) = tx {
                        sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(*) FROM note WHERE deleted_at IS NULL",
                        )
                        .fetch_one(&mut **tx)
                        .await
                        .unwrap_or(0) as usize
                    } else {
                        100
                    }
                };
                let effective_k = graph_config.effective_k(note_count.max(100));
                info!(
                    strategy = "hnsw_heuristic",
                    k = effective_k,
                    "Linking with HNSW Algorithm 4"
                );
                self.link_by_hnsw_heuristic(
                    note_id,
                    &embeddings[0].vector,
                    &graph_config,
                    note_count.max(100),
                    &schema_ctx,
                )
                .await
            }
            matric_core::defaults::GraphLinkingStrategy::Threshold => {
                info!(
                    strategy = "threshold",
                    threshold = link_threshold,
                    "Linking with threshold strategy"
                );
                self.link_by_threshold(
                    note_id,
                    &embeddings[0].vector,
                    link_threshold,
                    graph_config.tag_boost_weight,
                    &schema_ctx,
                )
                .await
            }
        };

        match semantic_created {
            Ok(n) => created += n,
            Err(e) => return JobResult::Failed(e),
        }

        ctx.report_progress(100, Some("Linking complete"));
        info!(
            note_id_present = true,
            result_count = created,
            strategy = %graph_config.strategy,
            wiki_found = wiki_links_found,
            wiki_resolved = wiki_links_resolved,
            duration_ms = start.elapsed().as_millis() as u64,
            detail = JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
            operation = "complete_linking",
            "Linking completed"
        );

        // Complete provenance activity (#430)
        if let Some(act_id) = activity_id {
            let prov_metadata = serde_json::json!({
                "links_created": created,
                "wiki_links_found": wiki_links_found,
                "wiki_links_resolved": wiki_links_resolved,
                "strategy": graph_config.strategy.to_string(),
            });
            if let Err(e) = self
                .db
                .provenance
                .complete_activity(act_id, None, Some(prov_metadata))
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                    "Failed to complete linking provenance activity"
                );
            }
        }

        // Queue a deduplicated GraphMaintenance job so SNN/PFNET run after new
        // links are created.  Deduplication ensures only one pending maintenance job
        // exists even if many linking jobs complete in rapid succession.
        let schema = extract_schema(&ctx);
        let maint_payload = serde_json::json!({ "schema": schema });
        match self
            .db
            .jobs
            .queue_deduplicated(
                None,
                JobType::GraphMaintenance,
                JobType::GraphMaintenance.default_priority(),
                Some(maint_payload),
                None,
            )
            .await
        {
            Ok(Some(job_id)) => {
                ctx.emit_job_queued(job_id, JobType::GraphMaintenance, None);
            }
            Ok(None) => {} // Deduplicated
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                    "Failed to queue post-linking graph maintenance job"
                );
            }
        }

        JobResult::Success(Some(serde_json::json!({
            "links_created": created,
            "wiki_links_found": wiki_links_found,
            "wiki_links_resolved": wiki_links_resolved,
            "strategy": graph_config.strategy.to_string()
        })))
    }
}

/// Handler for permanently deleting a note and all related data.
/// This triggers CASCADE DELETE on all dependent records (embeddings, links, tags, etc.)
/// and updates embedding set stats afterward.
pub struct PurgeNoteHandler {
    db: Database,
}

impl PurgeNoteHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for PurgeNoteHandler {
    fn job_type(&self) -> JobType {
        JobType::PurgeNote
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "purge", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(10, Some("Finding affected embedding sets..."));

        // Get embedding sets this note is a member of (to update stats after deletion)
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return purge_job_failure(e, "affected_sets_begin_tx"),
        };
        let affected_sets = match self
            .db
            .embedding_sets
            .get_sets_for_note_tx(&mut tx, note_id)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PURGE_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "get_embedding_sets_for_note",
                    "Failed to get embedding sets for note, continuing with deletion"
                );
                vec![]
            }
        };
        tx.commit().await.ok();

        ctx.report_progress(30, Some("Verifying note exists..."));

        // Verify note exists before attempting deletion
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return purge_job_failure(e, "verify_note_begin_tx"),
        };
        let exists = self
            .db
            .notes
            .exists_tx(&mut tx, note_id)
            .await
            .unwrap_or(false);
        tx.commit().await.ok();
        if !exists {
            return JobResult::Failed("Note does not exist".into());
        }

        ctx.report_progress(50, Some("Deleting note and all related data..."));

        // Perform hard delete - this triggers CASCADE DELETE for:
        // - note_original
        // - note_revised_current
        // - note_revision
        // - note_tag
        // - link (both from_note_id and to_note_id)
        // - context
        // - embedding
        // - embedding_set_member
        // - job_queue (for this note)
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return purge_job_failure(e, "delete_begin_tx"),
        };
        if let Err(e) = self.db.notes.hard_delete_tx(&mut tx, note_id).await {
            return purge_job_failure(e, "delete_note");
        }
        if let Err(e) = tx.commit().await {
            return purge_job_failure(e, "delete_commit");
        }

        ctx.report_progress(80, Some("Updating embedding set statistics..."));

        // Update stats for all affected embedding sets
        let mut stats_updated = 0;
        for set_id in &affected_sets {
            if let Err(e) = self.db.embedding_sets.refresh_stats(*set_id).await {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PURGE_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "refresh_embedding_set_stats",
                    "Failed to update embedding set stats"
                );
            } else {
                stats_updated += 1;
            }
        }

        ctx.report_progress(100, Some("Note permanently deleted"));
        info!(
            note_id_present = true,
            affected_sets = affected_sets.len(),
            stats_updated = stats_updated,
            duration_ms = start.elapsed().as_millis() as u64,
            detail = JOB_PURGE_DIAGNOSTIC_FAILURE_DETAIL,
            operation = "complete_note_purge",
            "Note purged"
        );

        JobResult::Success(Some(serde_json::json!({
            "deleted_note_id": note_id.to_string(),
            "affected_embedding_sets": affected_sets.len(),
            "stats_updated": stats_updated
        })))
    }
}

/// Handler for context update jobs - adds "Related Context" section based on links.
pub struct ContextUpdateHandler {
    db: Database,
    backend: OllamaBackend,
    registry: Arc<ProviderRegistry>,
}

impl ContextUpdateHandler {
    pub fn new(db: Database, backend: OllamaBackend, registry: Arc<ProviderRegistry>) -> Self {
        Self {
            db,
            backend,
            registry,
        }
    }
}

#[async_trait]
impl JobHandler for ContextUpdateHandler {
    fn job_type(&self) -> JobType {
        JobType::ContextUpdate
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "context_update", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let model_override = extract_model_override(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };
        let backend: &dyn GenerationBackend = match &overridden {
            Some(b) => b.as_ref(),
            None => &self.backend,
        };

        ctx.report_progress(20, Some("Finding linked notes..."));

        // Get outgoing semantic links with high scores (limit per Miller's Law)
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return context_update_job_failure(e, "links_begin_tx"),
        };
        let links = match self.db.links.get_outgoing_tx(&mut tx, note_id).await {
            Ok(l) => l
                .into_iter()
                .filter(|l| l.score > matric_core::defaults::CONTEXT_LINK_THRESHOLD)
                .take(MAX_PROMPT_SNIPPETS)
                .collect::<Vec<_>>(),
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_CONTEXT_UPDATE_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "get_outgoing_links",
                    "Failed to get links"
                );
                return JobResult::Success(Some(
                    serde_json::json!({"updated": false, "reason": "no_links"}),
                ));
            }
        };
        tx.commit().await.ok();

        if links.is_empty() {
            return JobResult::Success(Some(
                serde_json::json!({"updated": false, "reason": "no_high_quality_links"}),
            ));
        }

        ctx.report_progress(40, Some("Fetching linked content..."));

        // Get current note content
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return context_update_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return context_update_job_failure(e, "fetch_note"),
        };
        tx.commit().await.ok();

        let current_content = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        // Skip if already has Related Context section
        if current_content.contains("## Related Context") {
            return JobResult::Success(Some(
                serde_json::json!({"updated": false, "reason": "already_has_context"}),
            ));
        }

        // Build context from linked notes
        let mut linked_context = String::new();
        for link in &links {
            if let Some(to_note_id) = link.to_note_id {
                if let Ok(linked_note) = self.db.notes.fetch(to_note_id).await {
                    let preview: String = if !linked_note.revised.content.is_empty() {
                        linked_note
                            .revised
                            .content
                            .chars()
                            .take(matric_core::defaults::PREVIEW_LINKED_NOTE)
                            .collect()
                    } else {
                        linked_note
                            .original
                            .content
                            .chars()
                            .take(matric_core::defaults::PREVIEW_LINKED_NOTE)
                            .collect()
                    };
                    linked_context.push_str(&format!(
                        "\n- Related note (similarity {:.0}%): {}\n",
                        link.score * 100.0,
                        preview
                    ));
                }
            }
        }

        if linked_context.is_empty() {
            return JobResult::Success(Some(
                serde_json::json!({"updated": false, "reason": "no_linked_content"}),
            ));
        }

        ctx.report_progress(60, Some("Generating context section..."));

        // Generate updated content with context (ported from HOTM)
        let prompt = format!(
            r#"You have an enhanced note that has been linked to related notes. Add a 'Related Context' section at the end that briefly mentions the connections.

Current Enhanced Note:
{}

Related Notes Found:
{}

Add a brief '## Related Context' section at the end that mentions these connections naturally.
Keep it concise (2-3 sentences). Output the full note with the new section added."#,
            current_content, linked_context
        );

        let updated_content = match backend.generate(&prompt).await {
            Ok(c) => clean_enhanced_content(c.trim(), &prompt),
            Err(e) => return ai_generation_job_failure(e, "context_update"),
        };

        if updated_content.is_empty() {
            return JobResult::Failed("Empty content generated".into());
        }

        ctx.report_progress(80, Some("Saving updated content..."));

        // Save the updated revision
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return context_update_job_failure(e, "save_begin_tx"),
        };
        if let Err(e) = self
            .db
            .notes
            .update_revised_tx(
                &mut tx,
                note_id,
                &updated_content,
                Some("Added related context section"),
            )
            .await
        {
            return context_update_job_failure(e, "save_revision");
        }
        if let Err(e) = tx.commit().await {
            return context_update_job_failure(e, "save_commit");
        }

        ctx.report_progress(100, Some("Context update complete"));
        info!(
            note_id = %note_id,
            result_count = links.len(),
            duration_ms = start.elapsed().as_millis() as u64,
            "Context update completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "updated": true,
            "links_referenced": links.len()
        })))
    }
}

/// Handler for AI-driven SKOS concept tagging - replaces flat tags with hierarchical concepts.
pub struct ConceptTaggingHandler {
    db: Database,
    backend: OllamaBackend,
    /// Fast model backend for simple documents (#439). None if MATRIC_FAST_GEN_MODEL not set.
    fast_backend: Option<OllamaBackend>,
    /// Optional GLiNER NER backend for fast concept extraction.
    /// When available, GLiNER runs first as the fastest path (<300ms).
    /// If GLiNER produces fewer than `target_concepts`, fast/standard LLM supplements.
    ner_backend: Option<Arc<dyn NerBackend>>,
    registry: Arc<ProviderRegistry>,
    /// Target number of concepts per note. Configurable via EXTRACTION_TARGET_CONCEPTS.
    target_concepts: usize,
}

impl ConceptTaggingHandler {
    pub fn new(
        db: Database,
        backend: OllamaBackend,
        fast_backend: Option<OllamaBackend>,
        ner_backend: Option<Arc<dyn NerBackend>>,
        registry: Arc<ProviderRegistry>,
    ) -> Self {
        let target_concepts = std::env::var(matric_core::defaults::ENV_EXTRACTION_TARGET_CONCEPTS)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(matric_core::defaults::EXTRACTION_TARGET_CONCEPTS);
        Self {
            db,
            backend,
            fast_backend,
            ner_backend,
            registry,
            target_concepts,
        }
    }

    /// Queue Phase 2 (RelatedConceptInference) after concept tagging completes.
    ///
    /// Called on ALL exit paths so downstream jobs run even if tagging produces
    /// no tags. Pipeline order: ConceptTagging → RelatedConceptInference → Embedding → Linking (#420, #424, #435).
    ///
    /// RelatedConceptInference infers associative (skos:related) relationships
    /// between the concepts just tagged, then queues Embedding + Linking.
    /// Returns the new job ID if queued (None if deduplicated or on error).
    async fn queue_phase2_jobs(&self, note_id: uuid::Uuid, schema: &str) -> Option<uuid::Uuid> {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        // RelatedConceptInference starts at tier-1 (fast GPU).
        match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::RelatedConceptInference,
                JobType::RelatedConceptInference.default_priority(),
                payload,
                Some(matric_core::cost_tier::FAST_GPU),
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                    operation = "queue_phase2_related_concept_inference",
                    "Failed to queue phase-2 related concept inference job"
                );
                None
            }
        }
    }

    /// Extract prior concepts from job payload (set by tier escalation chaining).
    fn extract_prior_concepts(ctx: &JobContext) -> Vec<String> {
        ctx.payload()
            .and_then(|p| p.get("prior_concepts"))
            .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Build the LLM prompt for concept extraction.
    ///
    /// `existing_prior`: concepts from prior tier escalation (e.g. GLiNER results).
    /// `existing_db`: concepts already tagged on this note in the database.
    fn make_concept_prompt(
        text: &str,
        existing_prior: &[String],
        existing_db: &[String],
        target: usize,
    ) -> String {
        let all_existing: Vec<&str> = existing_prior
            .iter()
            .chain(existing_db.iter())
            .map(|s| s.as_str())
            .collect();
        let unique_existing: Vec<&str> = {
            let mut seen = HashSet::new();
            all_existing
                .into_iter()
                .filter(|s| seen.insert(s.to_lowercase()))
                .collect()
        };

        let mut context_hint = String::new();

        if !existing_db.is_empty() {
            context_hint.push_str(&format!(
                "This note already has {} concepts in the knowledge base: {:?}\n\
                 PREFER reusing these if they are still relevant. Only replace if clearly wrong.\n\n",
                existing_db.len(),
                existing_db
            ));
        }

        if !existing_prior.is_empty() {
            let needed = target.saturating_sub(unique_existing.len());
            context_hint.push_str(&format!(
                "We have {} concepts from entity extraction. Suggest {} MORE distinct concepts \
                 that cover different dimensions. Do NOT repeat: {:?}\n\n",
                existing_prior.len(),
                needed,
                existing_prior
            ));
        }

        let total_needed = target.saturating_sub(unique_existing.len());
        if total_needed == 0 && !unique_existing.is_empty() {
            context_hint.push_str(&format!(
                "We already have {} concepts meeting the target of {}. \
                 Only suggest additional concepts if there are clearly important dimensions missing.\n\n",
                unique_existing.len(),
                target
            ));
        }

        format!(
            r#"You are a knowledge organization specialist using SKOS (Simple Knowledge Organization System). Analyze the following content and suggest concept tags organized as hierarchical paths across MULTIPLE dimensions.

{context_hint}Content:
{text}

REQUIRED DIMENSIONS (include at least one tag from each applicable dimension):
1. **Domain**: Primary subject area (e.g., "science/machine-learning", "engineering/software")
2. **Topic**: Specific topics covered (e.g., "nlp/transformers", "databases/vector-search")
3. **Methodology**: Research/work methodology (e.g., "methodology/experimental", "methodology/survey", "methodology/case-study")
4. **Application**: Practical applications (e.g., "application/healthcare", "application/search-engines")
5. **Technique**: Specific techniques used (e.g., "technique/attention-mechanism", "technique/reinforcement-learning")
6. **Content-type**: What kind of content (e.g., "content-type/research-paper", "content-type/tutorial", "content-type/documentation")

OPTIONAL DIMENSIONS (include if clearly applicable):
7. **Evaluation**: How results are evaluated (e.g., "evaluation/benchmark", "evaluation/ablation-study")
8. **Tool/Framework**: Specific tools mentioned (e.g., "tool/pytorch", "tool/postgresql")
9. **Era/Context**: Temporal context (e.g., "era/foundation-models", "era/pre-transformer")

Guidelines:
1. Use hierarchical paths with "/" separators
2. Use 1-2 levels of hierarchy. Top level = dimension/domain, leaf = specific concept
3. Use kebab-case for multi-word terms
4. Focus on actual subject matter, not generic terms
5. Order by relevance (most relevant first)
6. Reuse top-level categories across notes for cross-cutting queries
7. Aim for 5-8 tags total — breadth across dimensions is more valuable than depth in one
8. PREFER reusing existing concept paths from the knowledge base when they are relevant

Output ONLY a JSON array of tag paths, nothing else. Example:
["science/machine-learning", "nlp/transformers", "technique/attention-mechanism", "methodology/experimental", "evaluation/benchmark", "application/translation", "tool/pytorch", "content-type/research-paper"]"#
        )
    }

    /// Tier-0: GLiNER NER only. Chains to tier-1 if insufficient concepts.
    async fn execute_ner(
        &self,
        ctx: &JobContext,
        note_id: uuid::Uuid,
        schema: &str,
        content_preview: &str,
        existing_db_concepts: &[String],
    ) -> (Vec<String>, &'static str, bool) {
        const CONCEPT_ENTITY_TYPES: &[&str] = &[
            "domain",
            "topic",
            "technique",
            "methodology",
            "application",
            "tool",
            "framework",
            "concept",
            "technology",
        ];

        let mut concept_labels: Vec<String> = Vec::new();

        if let Some(ner) = &self.ner_backend {
            match ner
                .extract(content_preview, CONCEPT_ENTITY_TYPES, None)
                .await
            {
                Ok(result) if !result.entities.is_empty() => {
                    ctx.report_progress(25, Some("Mapping GLiNER entities to concepts..."));
                    let mut seen = HashSet::new();
                    for ent in &result.entities {
                        let slug = ent.text.trim().to_lowercase().replace(' ', "-");
                        if slug.len() < 2 {
                            continue;
                        }
                        let path = format!("{}/{}", ent.label, slug);
                        if seen.insert(path.clone()) {
                            concept_labels.push(path);
                        }
                    }
                    info!(
                        note_id = %note_id,
                        gliner_concepts = concept_labels.len(),
                        target = self.target_concepts,
                        "Tier-0 GLiNER produced {} concepts (target: {})",
                        concept_labels.len(),
                        self.target_concepts
                    );
                }
                Ok(_) => {
                    info!(note_id = %note_id, "Tier-0 GLiNER returned no entities");
                }
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "tier0_gliner_concept_extraction",
                        "Tier-0 GLiNER concept extraction failed"
                    );
                }
            }
        }

        // Chain to tier-1 if total concepts (existing DB + new) below target
        let total_concepts = {
            let mut seen: HashSet<String> = existing_db_concepts
                .iter()
                .map(|s| s.to_lowercase())
                .collect();
            let unique_new = concept_labels
                .iter()
                .filter(|l| seen.insert(l.to_lowercase()))
                .count();
            existing_db_concepts.len() + unique_new
        };
        let escalating = total_concepts < self.target_concepts;
        if escalating {
            if let Some(job_id) = self
                .queue_escalation(
                    note_id,
                    schema,
                    matric_core::cost_tier::FAST_GPU,
                    &concept_labels,
                    matric_core::cost_tier::CPU_NER,
                )
                .await
            {
                ctx.emit_job_queued(job_id, JobType::ConceptTagging, Some(note_id));
            }
        }

        (concept_labels, "gliner", escalating)
    }

    /// Tier-1: Fast model extraction with chunking. Merges prior results. Chains to tier-2 if insufficient.
    async fn execute_fast(
        &self,
        ctx: &JobContext,
        note_id: uuid::Uuid,
        schema: &str,
        content_preview: &str,
        overridden: Option<&dyn GenerationBackend>,
        existing_db_concepts: &[String],
    ) -> (Vec<String>, &'static str, bool) {
        let mut concept_labels = Self::extract_prior_concepts(ctx);
        let prior_count = concept_labels.len();

        let backend: &dyn GenerationBackend = match overridden {
            Some(b) => b,
            None => match &self.fast_backend {
                Some(fb) => fb,
                None => {
                    // No fast backend — escalate directly to tier-2
                    if let Some(job_id) = self
                        .queue_escalation(
                            note_id,
                            schema,
                            matric_core::cost_tier::STANDARD_GPU,
                            &concept_labels,
                            matric_core::cost_tier::FAST_GPU,
                        )
                        .await
                    {
                        ctx.emit_job_queued(job_id, JobType::ConceptTagging, Some(note_id));
                    }
                    return (concept_labels, "fast_unavailable", true);
                }
            },
        };

        ctx.report_progress(30, Some("Running fast LLM concept extraction..."));

        let chunk_size = extraction_chunk_size(self.fast_backend.as_ref());
        let chunks = chunk_for_extraction(content_preview, chunk_size);
        let mut chunk_results: Vec<String> = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let prompt = Self::make_concept_prompt(
                chunk,
                &concept_labels,
                existing_db_concepts,
                self.target_concepts,
            );
            match backend.generate_json(&prompt).await {
                Ok(r) => chunk_results.push(r.trim().to_string()),
                Err(e) => {
                    info!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                        chunk = i,
                        chunks = chunks.len(),
                        operation = "tier1_fast_concept_chunk",
                        "Tier-1 fast model failed on chunk, skipping"
                    );
                }
            }
        }

        let llm_concepts: Vec<String> = merge_json_arrays(chunk_results);

        // Merge with prior results (deduplicate)
        if !llm_concepts.is_empty() {
            let mut seen: HashSet<String> =
                concept_labels.iter().map(|l| l.to_lowercase()).collect();
            for label in llm_concepts {
                if seen.insert(label.to_lowercase()) {
                    concept_labels.push(label);
                }
            }
        }

        // Chain to tier-2 if total (existing DB + new) still below half target
        let standard_threshold = self.target_concepts.div_ceil(2);
        let total_concepts = {
            let mut seen: HashSet<String> = existing_db_concepts
                .iter()
                .map(|s| s.to_lowercase())
                .collect();
            let unique_new = concept_labels
                .iter()
                .filter(|l| seen.insert(l.to_lowercase()))
                .count();
            existing_db_concepts.len() + unique_new
        };
        let escalating = total_concepts < standard_threshold;
        if escalating {
            info!(
                note_id = %note_id,
                count = concept_labels.len(),
                threshold = standard_threshold,
                "Tier-1 below escalation threshold, chaining to tier-2"
            );
            if let Some(job_id) = self
                .queue_escalation(
                    note_id,
                    schema,
                    matric_core::cost_tier::STANDARD_GPU,
                    &concept_labels,
                    matric_core::cost_tier::FAST_GPU,
                )
                .await
            {
                ctx.emit_job_queued(job_id, JobType::ConceptTagging, Some(note_id));
            }
        }

        let method = if prior_count > 0 {
            "gliner+fast"
        } else {
            "fast"
        };
        (concept_labels, method, escalating)
    }

    /// Tier-2: Standard model extraction. Merges prior results. No further escalation.
    async fn execute_standard(
        &self,
        ctx: &JobContext,
        _note_id: uuid::Uuid,
        _schema: &str,
        content_preview: &str,
        overridden: Option<&dyn GenerationBackend>,
        existing_db_concepts: &[String],
    ) -> (Vec<String>, &'static str, bool) {
        let mut concept_labels = Self::extract_prior_concepts(ctx);
        let prior_count = concept_labels.len();

        let backend: &dyn GenerationBackend = match overridden {
            Some(b) => b,
            None => &self.backend,
        };

        ctx.report_progress(30, Some("Running standard model concept extraction..."));

        let existing_snapshot: Vec<String> = concept_labels.clone();
        let prompt = Self::make_concept_prompt(
            content_preview,
            &existing_snapshot,
            existing_db_concepts,
            self.target_concepts,
        );

        match backend.generate_json(&prompt).await {
            Ok(r) => {
                let ai_response = r.trim().to_string();
                let parsed: Vec<String> = match parse_json_lenient(&ai_response) {
                    Ok(labels) => labels,
                    Err(_) => {
                        let cleaned = ai_response
                            .trim()
                            .trim_start_matches("```json")
                            .trim_start_matches("```")
                            .trim_end_matches("```")
                            .trim();
                        parse_json_lenient(cleaned).unwrap_or_default()
                    }
                };
                let mut seen: HashSet<String> =
                    concept_labels.iter().map(|l| l.to_lowercase()).collect();
                for label in parsed {
                    if seen.insert(label.to_lowercase()) {
                        concept_labels.push(label);
                    }
                }
            }
            Err(e) => {
                if concept_labels.is_empty() {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "tier2_standard_concept_extraction",
                        prior_concept_count = 0usize,
                        "Tier-2 standard model failed with no prior concepts"
                    );
                } else {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "tier2_standard_concept_extraction",
                        prior_concept_count = concept_labels.len(),
                        "Tier-2 standard model failed, proceeding with prior concepts"
                    );
                }
            }
        }

        let method = if prior_count > 0 {
            "gliner+fast+standard"
        } else {
            "standard"
        };
        // Terminal tier — no further escalation
        (concept_labels, method, false)
    }

    /// Queue a tier-escalation job for concept tagging with prior results in payload.
    /// Returns the new job ID if queued (None if deduplicated or on error).
    async fn queue_escalation(
        &self,
        note_id: uuid::Uuid,
        schema: &str,
        next_tier: i16,
        prior_concepts: &[String],
        prior_tier: i16,
    ) -> Option<uuid::Uuid> {
        let mut payload = serde_json::json!({
            "prior_concepts": prior_concepts,
            "prior_tier": prior_tier,
            "prior_count": prior_concepts.len(),
        });
        if schema != "public" {
            payload["schema"] = serde_json::json!(schema);
        }
        match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::ConceptTagging,
                JobType::ConceptTagging.default_priority(),
                Some(payload),
                Some(next_tier),
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                    next_tier,
                    operation = "queue_concept_tagging_tier_escalation",
                    "Failed to queue concept tagging tier escalation"
                );
                None
            }
        }
    }
}

#[async_trait]
impl JobHandler for ConceptTaggingHandler {
    fn job_type(&self) -> JobType {
        JobType::ConceptTagging
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "concept_tagging", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let model_override = extract_model_override(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };

        // Early exit: tier-0 with no NER backend — skip note fetch and escalate directly
        if ctx.job.cost_tier == Some(matric_core::cost_tier::CPU_NER) && self.ner_backend.is_none()
        {
            info!(note_id = %note_id, "Tier-0 requested but no NER backend — escalating to tier-1");
            if let Some(job_id) = self
                .queue_escalation(
                    note_id,
                    schema,
                    matric_core::cost_tier::FAST_GPU,
                    &[],
                    matric_core::cost_tier::CPU_NER,
                )
                .await
            {
                ctx.emit_job_queued(job_id, JobType::ConceptTagging, Some(note_id));
            }
            return JobResult::Success(Some(serde_json::json!({
                "concepts": 0,
                "escalating": true,
                "reason": "no_ner_backend"
            })));
        }

        ctx.report_progress(10, Some("Fetching note content..."));

        // Get the note and existing concepts in one transaction
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return concept_tagging_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return concept_tagging_job_failure(e, "fetch_note"),
        };
        // Fetch existing SKOS concepts already tagged on this note so the LLM
        // can reuse them and fill in gaps rather than starting from scratch.
        let existing_db_concepts: Vec<String> = match self
            .db
            .skos
            .get_note_tags_with_labels_tx(&mut tx, note_id)
            .await
        {
            Ok(tags) => tags
                .iter()
                .filter_map(|(_, c)| {
                    c.concept
                        .notation
                        .as_ref()
                        .or(c.pref_label.as_ref())
                        .cloned()
                })
                .collect(),
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "fetch_existing_concepts",
                    "Failed to fetch existing concepts, proceeding without"
                );
                vec![]
            }
        };
        tx.commit().await.ok();

        // Use revised content if available, otherwise original
        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            if let Some(job_id) = self.queue_phase2_jobs(note_id, schema).await {
                ctx.emit_job_queued(job_id, JobType::RelatedConceptInference, Some(note_id));
            }
            return JobResult::Success(Some(
                serde_json::json!({"concepts": 0, "reason": "empty_content"}),
            ));
        }

        ctx.report_progress(20, Some("Analyzing content for concepts..."));

        // Take content preview for analysis
        let content_preview: String = content
            .chars()
            .take(matric_core::defaults::PREVIEW_TAGGING)
            .collect();

        // Dispatch based on cost_tier for tiered atomic execution.
        // None = legacy inline cascade (backward compat for in-flight jobs).
        // Some(0) = GLiNER only, chain to tier-1 if insufficient.
        // Some(1) = Fast model, merge prior results, chain to tier-2 if insufficient.
        // Some(2) = Standard model, merge prior results.
        // existing_db_concepts are passed so each tier can include them in prompts.
        let (concept_labels, extraction_method, escalating) = match ctx.job.cost_tier {
            Some(matric_core::cost_tier::CPU_NER) => {
                self.execute_ner(
                    &ctx,
                    note_id,
                    schema,
                    &content_preview,
                    &existing_db_concepts,
                )
                .await
            }
            Some(matric_core::cost_tier::FAST_GPU) => {
                self.execute_fast(
                    &ctx,
                    note_id,
                    schema,
                    &content_preview,
                    overridden.as_deref(),
                    &existing_db_concepts,
                )
                .await
            }
            Some(matric_core::cost_tier::STANDARD_GPU) => {
                self.execute_standard(
                    &ctx,
                    note_id,
                    schema,
                    &content_preview,
                    overridden.as_deref(),
                    &existing_db_concepts,
                )
                .await
            }
            _ => {
                // Treat NULL cost_tier as CPU_NER (tier-0 entry point).
                // Escalation to tier-1/tier-2 happens via job queue chaining.
                self.execute_ner(
                    &ctx,
                    note_id,
                    schema,
                    &content_preview,
                    &existing_db_concepts,
                )
                .await
            }
        };

        // Start provenance activity (#430)
        let prov_model = match ctx.job.cost_tier {
            Some(matric_core::cost_tier::CPU_NER) => self
                .ner_backend
                .as_ref()
                .map(|n| n.model_name().to_string())
                .unwrap_or_else(|| "gliner".to_string()),
            Some(matric_core::cost_tier::FAST_GPU) => self
                .fast_backend
                .as_ref()
                .map(|b| matric_core::GenerationBackend::model_name(b).to_string())
                .unwrap_or_else(|| "fast".to_string()),
            _ => matric_core::GenerationBackend::model_name(&self.backend).to_string(),
        };
        let activity_id = self
            .db
            .provenance
            .start_activity(note_id, "concept_tagging", Some(&prov_model))
            .await
            .ok();

        ctx.report_progress(50, Some("Parsing concept suggestions..."));

        if concept_labels.is_empty() {
            if !escalating {
                if let Some(job_id) = self.queue_phase2_jobs(note_id, schema).await {
                    ctx.emit_job_queued(job_id, JobType::RelatedConceptInference, Some(note_id));
                }
            }
            return JobResult::Success(Some(serde_json::json!({
                "concepts": 0,
                "reason": "no_concepts_suggested",
                "escalating": escalating,
            })));
        }

        ctx.report_progress(60, Some("Resolving SKOS concepts..."));

        // Resolve or create hierarchical SKOS concepts and tag the note (#425).
        // Uses resolve_or_create_tag_tx which handles scheme resolution, hierarchy
        // wiring (broader/narrower), and notation-based deduplication.
        let mut tagged_count = 0;
        let total = concept_labels.len();

        for (i, label) in concept_labels.iter().enumerate() {
            // Skip empty or too-short labels
            if label.trim().len() < 2 {
                continue;
            }

            let is_primary = i == 0; // First concept is primary
            let relevance = 1.0_f32 - (i as f32 * matric_core::defaults::RELEVANCE_DECAY_FACTOR);

            // Parse label as hierarchical tag path (e.g., "science/machine-learning")
            let tag_input = matric_core::TagInput::parse(label.trim());

            // Resolve or create the concept hierarchy in a single transaction
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return concept_tagging_job_failure(e, "resolve_concept_begin_tx"),
            };

            let resolved = match self
                .db
                .skos
                .resolve_or_create_tag_tx(&mut tx, &tag_input)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                        label_len = label.chars().count(),
                        operation = "resolve_concept_tag",
                        "Failed to resolve concept"
                    );
                    tx.commit().await.ok();
                    continue;
                }
            };

            // Tag the note with the leaf concept
            let tag_req = matric_core::TagNoteRequest {
                note_id,
                concept_id: resolved.concept_id,
                source: "ai_auto".to_string(),
                confidence: Some(matric_core::defaults::AI_TAGGING_CONFIDENCE),
                relevance_score: relevance,
                is_primary,
                created_by: None,
            };

            let result = self.db.skos.tag_note_tx(&mut tx, tag_req).await;
            tx.commit().await.ok();
            if let Err(e) = result {
                debug!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "tag_note_with_concept",
                    "Failed to tag note"
                );
            } else {
                tagged_count += 1;
            }

            // Update progress
            let progress = 60 + ((i + 1) * 30 / total) as i32;
            ctx.report_progress(progress, Some(&format!("Tagged with: {}", label)));
        }

        // Queue Phase 2 only if this tier is NOT escalating to a higher tier.
        // When escalating, the higher-tier job will queue phase-2 after it completes.
        if !escalating {
            ctx.report_progress(95, Some("Queuing phase-2 related concept inference..."));
            if let Some(job_id) = self.queue_phase2_jobs(note_id, schema).await {
                ctx.emit_job_queued(job_id, JobType::RelatedConceptInference, Some(note_id));
            }
        } else {
            ctx.report_progress(95, Some("Escalating to higher tier — phase-2 deferred"));
        }

        // Complete provenance activity (#430)
        if let Some(act_id) = activity_id {
            let prov_metadata = serde_json::json!({
                "concepts_tagged": tagged_count,
                "concepts_suggested": concept_labels.len(),
                "extraction_method": extraction_method,
                "target_concepts": self.target_concepts,
                "labels": &concept_labels,
                "content_preview_chars": content_preview.len(),
            });
            if let Err(e) = self
                .db
                .provenance
                .complete_activity(act_id, None, Some(prov_metadata))
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                    "Failed to complete concept tagging provenance activity"
                );
            }
        }

        ctx.report_progress(100, Some("Concept tagging complete"));
        info!(
            note_id = %note_id,
            result_count = tagged_count,
            concepts_suggested = concept_labels.len(),
            extraction_method,
            duration_ms = start.elapsed().as_millis() as u64,
            "Concept tagging completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "concepts_tagged": tagged_count,
            "concepts_suggested": concept_labels.len(),
            "extraction_method": extraction_method,
            "labels": concept_labels
        })))
    }
}

/// Handler for extracting named entity references from note content.
///
/// Runs in Phase 1 alongside ConceptTagging (parallel, not serial). Extracts
/// specific named references (companies, people, tools, datasets, venues, etc.)
/// and creates SKOS concepts in entity-specific dimensions. Unlike thematic
/// concept tags, reference concepts are immediately promoted to `approved` status
/// since even a single mention of a named entity is meaningful.
///
/// Does NOT queue downstream jobs — ConceptTagging owns the Phase 2 chain.
pub struct ReferenceExtractionHandler {
    db: Database,
    backend: OllamaBackend,
    /// Fast model backend for extraction pipeline. None if explicitly disabled.
    fast_backend: Option<OllamaBackend>,
    /// Optional GLiNER NER backend for fast entity extraction (#437).
    /// When available, uses GLiNER (CPU, <300ms) instead of LLM (GPU, 10-24s).
    /// Falls back to LLM when GLiNER is unavailable.
    ner_backend: Option<Arc<dyn NerBackend>>,
    registry: Arc<ProviderRegistry>,
}

impl ReferenceExtractionHandler {
    pub fn new(
        db: Database,
        backend: OllamaBackend,
        fast_backend: Option<OllamaBackend>,
        ner_backend: Option<Arc<dyn NerBackend>>,
        registry: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            db,
            backend,
            fast_backend,
            ner_backend,
            registry,
        }
    }

    /// Queue a tier-escalation job for reference extraction.
    /// Returns the new job ID if queued (None if deduplicated or on error).
    async fn queue_ref_tier_escalation(
        &self,
        note_id: uuid::Uuid,
        schema: &str,
        next_tier: i16,
    ) -> Option<uuid::Uuid> {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::ReferenceExtraction,
                JobType::ReferenceExtraction.default_priority(),
                payload,
                Some(next_tier),
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                    next_tier,
                    operation = "queue_reference_extraction_tier_escalation",
                    "Failed to queue reference extraction tier escalation"
                );
                None
            }
        }
    }
}

#[async_trait]
impl JobHandler for ReferenceExtractionHandler {
    fn job_type(&self) -> JobType {
        JobType::ReferenceExtraction
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "reference_extraction", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        // Determine provenance model name: GLiNER if available, otherwise LLM.
        let prov_model = match &self.ner_backend {
            Some(ner) => ner.model_name().to_string(),
            None => matric_core::GenerationBackend::model_name(&self.backend).to_string(),
        };

        // Start provenance activity
        let activity_id = self
            .db
            .provenance
            .start_activity(note_id, "reference_extraction", Some(&prov_model))
            .await
            .ok();

        ctx.report_progress(10, Some("Fetching note content..."));

        // Get the note
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return reference_extraction_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return reference_extraction_job_failure(e, "fetch_note"),
        };
        tx.commit().await.ok();

        // Use revised content if available, otherwise original
        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            return JobResult::Success(Some(
                serde_json::json!({"references": 0, "reason": "empty_content"}),
            ));
        }

        ctx.report_progress(20, Some("Analyzing content for named references..."));

        // Take content preview for analysis
        let content_preview: String = content
            .chars()
            .take(matric_core::defaults::PREVIEW_TAGGING)
            .collect();

        // Internal struct for normalized reference entities from GLiNER.
        #[derive(serde::Deserialize)]
        struct RefEntity {
            category: String,
            name: String,
            #[allow(dead_code)]
            label: String,
        }

        // Entity type labels for GLiNER NER extraction.
        const NER_ENTITY_TYPES: &[&str] = &[
            "organization",
            "person",
            "tool",
            "dataset",
            "standard",
            "venue",
            "product",
            "language",
            "author",
            "cited-source",
            "sponsor",
            "publisher",
            "affiliation",
        ];

        // Tiered dispatch for entity extraction.
        // Tier 0: GLiNER only → if empty, chain to tier-1
        // Tier 1: Fast LLM only → if fail, chain to tier-2
        // Tier 2: Standard LLM only
        // None: Legacy cascade (GLiNER → fast → standard inline)
        let is_tiered = ctx.job.cost_tier.is_some();

        // Tier 0: GLiNER extraction
        let (entities, extraction_method) = if ctx.job.cost_tier
            == Some(matric_core::cost_tier::STANDARD_GPU)
        {
            // Tier-2: skip GLiNER, go directly to standard LLM below
            (Vec::new(), "tier2_standard")
        } else if ctx.job.cost_tier == Some(matric_core::cost_tier::FAST_GPU) {
            // Tier-1: skip GLiNER, go directly to fast LLM below
            (Vec::new(), "tier1_fast")
        } else if let Some(ner) = &self.ner_backend {
            match ner.extract(&content_preview, NER_ENTITY_TYPES, None).await {
                Ok(result) if !result.entities.is_empty() => {
                    info!(
                        note_id = %note_id,
                        entities = result.entities.len(),
                        model = %result.model,
                        "GLiNER extraction succeeded"
                    );
                    ctx.report_progress(50, Some("Parsing GLiNER entities..."));

                    let mapped: Vec<RefEntity> = result
                        .entities
                        .into_iter()
                        .map(|e| {
                            let name = e
                                .text
                                .to_lowercase()
                                .replace([' ', '_'], "-")
                                .chars()
                                .filter(|c| c.is_alphanumeric() || *c == '-')
                                .collect::<String>();
                            RefEntity {
                                category: e.label.clone(),
                                name,
                                label: e.text,
                            }
                        })
                        .collect();
                    (mapped, "gliner")
                }
                Ok(_) => {
                    info!(note_id = %note_id, "GLiNER returned no entities, falling back to LLM");
                    if is_tiered {
                        // Tier-0: chain to tier-1 on empty results
                        if let Some(job_id) = self
                            .queue_ref_tier_escalation(
                                note_id,
                                schema,
                                matric_core::cost_tier::FAST_GPU,
                            )
                            .await
                        {
                            ctx.emit_job_queued(
                                job_id,
                                JobType::ReferenceExtraction,
                                Some(note_id),
                            );
                        }
                        return JobResult::Success(Some(serde_json::json!({
                            "references": 0,
                            "escalated": true,
                            "reason": "gliner_empty"
                        })));
                    }
                    (Vec::new(), "gliner_empty")
                }
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "gliner_reference_extraction",
                        "GLiNER extraction failed, falling back to LLM"
                    );
                    if is_tiered {
                        if let Some(job_id) = self
                            .queue_ref_tier_escalation(
                                note_id,
                                schema,
                                matric_core::cost_tier::FAST_GPU,
                            )
                            .await
                        {
                            ctx.emit_job_queued(
                                job_id,
                                JobType::ReferenceExtraction,
                                Some(note_id),
                            );
                        }
                        return JobResult::Success(Some(serde_json::json!({
                            "references": 0,
                            "escalated": true,
                            "reason": "gliner_failed"
                        })));
                    }
                    (Vec::new(), "gliner_failed")
                }
            }
        } else {
            if is_tiered && ctx.job.cost_tier == Some(matric_core::cost_tier::CPU_NER) {
                // Tier-0 but no GLiNER backend — chain to tier-1
                if let Some(job_id) = self
                    .queue_ref_tier_escalation(note_id, schema, matric_core::cost_tier::FAST_GPU)
                    .await
                {
                    ctx.emit_job_queued(job_id, JobType::ReferenceExtraction, Some(note_id));
                }
                return JobResult::Success(Some(serde_json::json!({
                    "references": 0,
                    "escalated": true,
                    "reason": "no_gliner"
                })));
            }
            (Vec::new(), "no_gliner")
        };

        // Resolve model override via provider registry
        let model_override = extract_model_override(&ctx);
        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };

        // LLM fallback when GLiNER is unavailable, failed, or returned empty.
        // Fast-first with chunking: try fast model → escalate to standard on failure.
        let (entities, extraction_method) = if entities.is_empty() {
            ctx.report_progress(30, Some("Extracting references via LLM..."));

            let make_ref_prompt = |text: &str| {
                format!(
                    "Extract specific named references from this text. For each reference, provide:\n\
                     - category: one of [organization, person, tool, dataset, standard, venue, product, language, author, cited-source, sponsor, publisher, affiliation]\n\
                     - name: lowercase slug (e.g., \"google-deepmind\")\n\
                     - label: original text as it appears\n\n\
                     Return a JSON array of objects. If no references found, return [].\n\n\
                     Text:\n{text}"
                )
            };

            // Tiered model selection: tier-1 = fast only, tier-2 = standard only, None = cascade.
            let use_fast = overridden.is_none()
                && self.fast_backend.is_some()
                && ctx.job.cost_tier != Some(matric_core::cost_tier::STANDARD_GPU);
            let skip_standard = ctx.job.cost_tier == Some(matric_core::cost_tier::FAST_GPU);

            // Try fast model with chunking first.
            // Resilient: skip failed/unparseable chunks rather than discarding everything.
            let fast_result: Option<Vec<RefEntity>> = if use_fast {
                let fast = self.fast_backend.as_ref().unwrap();
                let chunk_size = extraction_chunk_size(Some(fast));
                let chunks = chunk_for_extraction(&content_preview, chunk_size);
                let mut all_results = Vec::new();
                let mut succeeded = 0usize;

                for (i, chunk) in chunks.iter().enumerate() {
                    let prompt = make_ref_prompt(chunk);
                    match fast.generate_json(&prompt).await {
                        Ok(json_str) => match parse_json_lenient::<Vec<RefEntity>>(&json_str) {
                            Ok(parsed) => {
                                all_results.extend(parsed);
                                succeeded += 1;
                            }
                            Err(e) => {
                                info!(
                                    chunk = i,
                                    chunks = chunks.len(),
                                    error_len = diagnostic_len(&e),
                                    detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                                    operation = "fast_reference_parse",
                                    "Fast model ref parse failed, skipping chunk"
                                );
                            }
                        },
                        Err(e) => {
                            info!(
                                chunk = i,
                                chunks = chunks.len(),
                                error_len = diagnostic_len(&e),
                                detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                                operation = "fast_reference_extraction",
                                "Fast model ref extraction failed, skipping chunk"
                            );
                        }
                    }
                }

                if succeeded > 0 {
                    Some(all_results)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(parsed) = fast_result {
                info!(
                    note_id = %note_id,
                    entities = parsed.len(),
                    "Fast LLM reference extraction succeeded"
                );
                ctx.report_progress(50, Some("Parsing LLM entities..."));
                (parsed, "llm_fast")
            } else if skip_standard {
                // Tier-1: fast model failed/unavailable — chain to tier-2
                info!(note_id = %note_id, "Tier-1 fast model failed for references, chaining to tier-2");
                if let Some(job_id) = self
                    .queue_ref_tier_escalation(
                        note_id,
                        schema,
                        matric_core::cost_tier::STANDARD_GPU,
                    )
                    .await
                {
                    ctx.emit_job_queued(job_id, JobType::ReferenceExtraction, Some(note_id));
                }
                return JobResult::Success(Some(serde_json::json!({
                    "references": 0,
                    "escalated": true,
                    "reason": "fast_model_failed"
                })));
            } else {
                // Standard model fallback (no chunking — uses full preview)
                let gen_backend: &dyn GenerationBackend = match &overridden {
                    Some(b) => b.as_ref(),
                    None => &self.backend,
                };
                let prompt = make_ref_prompt(&content_preview);
                match gen_backend.generate_json(&prompt).await {
                    Ok(json_str) => match parse_json_lenient::<Vec<RefEntity>>(&json_str) {
                        Ok(parsed) => {
                            info!(
                                note_id = %note_id,
                                entities = parsed.len(),
                                "LLM reference extraction succeeded"
                            );
                            ctx.report_progress(50, Some("Parsing LLM entities..."));
                            (parsed, "llm")
                        }
                        Err(e) => {
                            warn!(
                                error_len = diagnostic_len(&e),
                                detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                                operation = "standard_reference_parse",
                                "Failed to parse LLM reference response"
                            );
                            return JobResult::Success(Some(serde_json::json!({
                                "references": 0,
                                "reason": "parse_error",
                                "extraction_method": extraction_method,
                            })));
                        }
                    },
                    Err(e) => {
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                            operation = "standard_reference_extraction",
                            "LLM reference extraction failed"
                        );
                        return JobResult::Success(Some(serde_json::json!({
                            "references": 0,
                            "reason": "llm_error",
                            "extraction_method": "llm_failed",
                        })));
                    }
                }
            }
        } else {
            (entities, extraction_method)
        };

        if entities.is_empty() {
            return JobResult::Success(Some(serde_json::json!({
                "references": 0,
                "reason": "no_references_found"
            })));
        }

        ctx.report_progress(60, Some("Resolving reference concepts..."));

        let mut tagged_count = 0;
        let total = entities.len();
        let mut labels: Vec<String> = Vec::new();

        for (i, entity) in entities.iter().enumerate() {
            // Skip empty or invalid entries
            if entity.name.trim().is_empty() || entity.category.trim().is_empty() {
                continue;
            }

            // Build tag path: "{category}/{name}" (e.g., "organization/google-deepmind")
            let tag_path = format!("{}/{}", entity.category.trim(), entity.name.trim());

            if tag_path.len() < 4 {
                continue;
            }

            let is_primary = false; // Reference entities are not primary concepts
            let relevance = 0.8_f32 - (i as f32 * 0.02); // Slight decay but high baseline

            // Parse label as hierarchical tag path
            let tag_input = matric_core::TagInput::parse(&tag_path);

            // Resolve or create the concept hierarchy in a single transaction
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return reference_extraction_job_failure(e, "resolve_reference_begin_tx"),
            };

            let resolved = match self
                .db
                .skos
                .resolve_or_create_tag_tx(&mut tx, &tag_input)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                        tag_path_len = tag_path.chars().count(),
                        operation = "resolve_reference_concept",
                        "Failed to resolve reference concept"
                    );
                    tx.commit().await.ok();
                    continue;
                }
            };

            // Tag the note with the leaf concept
            let tag_req = matric_core::TagNoteRequest {
                note_id,
                concept_id: resolved.concept_id,
                source: "ai_reference".to_string(),
                confidence: Some(matric_core::defaults::AI_TAGGING_CONFIDENCE),
                relevance_score: relevance,
                is_primary,
                created_by: None,
            };

            let tag_result = self.db.skos.tag_note_tx(&mut tx, tag_req).await;

            // Immediately promote to approved if still a candidate
            if tag_result.is_ok() {
                let _ = sqlx::query(
                    "UPDATE skos_concept SET status = 'approved'::concept_status, promoted_at = NOW() \
                     WHERE id = $1 AND status = 'candidate'::concept_status"
                )
                .bind(resolved.concept_id)
                .execute(&mut *tx)
                .await;
            }

            tx.commit().await.ok();
            if let Err(e) = tag_result {
                debug!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "tag_note_with_reference",
                    "Failed to tag note with reference"
                );
            } else {
                tagged_count += 1;
                labels.push(tag_path);
            }

            // Update progress
            let progress = 60 + ((i + 1) * 30 / total) as i32;
            ctx.report_progress(progress, Some(&format!("Referenced: {}", entity.name)));
        }

        // Complete provenance activity
        if let Some(act_id) = activity_id {
            let prov_metadata = serde_json::json!({
                "references_tagged": tagged_count,
                "references_found": entities.len(),
                "labels": &labels,
                "content_preview_chars": content_preview.len(),
                "extraction_method": extraction_method,
            });
            if let Err(e) = self
                .db
                .provenance
                .complete_activity(act_id, None, Some(prov_metadata))
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                    "Failed to complete reference extraction provenance activity"
                );
            }
        }

        ctx.report_progress(100, Some("Reference extraction complete"));
        info!(
            note_id = %note_id,
            result_count = tagged_count,
            references_found = entities.len(),
            extraction_method = extraction_method,
            duration_ms = start.elapsed().as_millis() as u64,
            "Reference extraction completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "references_tagged": tagged_count,
            "references_found": entities.len(),
            "labels": labels,
            "extraction_method": extraction_method,
        })))
    }
}

/// Handler for inferring SKOS related (associative) concept relationships (#435).
///
/// Runs as Phase 2 in the NLP pipeline, after ConceptTagging creates hierarchical
/// concepts. Analyzes concepts tagged on a note and uses LLM to identify
/// cross-dimensional associative relationships (e.g., "attention-mechanism" related
/// to "machine-learning"). Creates `skos:related` edges with confidence scores.
///
/// Pipeline: ConceptTagging → RelatedConceptInference → Embedding → Linking
pub struct RelatedConceptHandler {
    db: Database,
    backend: OllamaBackend,
    /// Fast model backend for extraction pipeline.
    fast_backend: Option<OllamaBackend>,
    registry: Arc<ProviderRegistry>,
}

impl RelatedConceptHandler {
    pub fn new(
        db: Database,
        backend: OllamaBackend,
        fast_backend: Option<OllamaBackend>,
        registry: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            db,
            backend,
            fast_backend,
            registry,
        }
    }

    /// Queue Phase 3 jobs (Embedding + Linking) after related concept inference completes.
    ///
    /// Called on ALL exit paths so downstream jobs run even if inference produces
    /// no relations. Pipeline order: ConceptTagging → RelatedConceptInference → Embedding → Linking (#435).
    /// Returns job IDs for successfully queued jobs as (embedding_id, linking_id).
    async fn queue_phase3_jobs(
        &self,
        note_id: uuid::Uuid,
        schema: &str,
    ) -> (Option<uuid::Uuid>, Option<uuid::Uuid>) {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        // Embedding and Linking are tier-agnostic (NULL).
        let embed_id = match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::Embedding,
                JobType::Embedding.default_priority(),
                payload.clone(),
                None,
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                    operation = "queue_phase3_embedding_job",
                    "Failed to queue phase-3 embedding job"
                );
                None
            }
        };
        let link_id = match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::Linking,
                JobType::Linking.default_priority(),
                payload,
                None,
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
                    operation = "queue_phase3_linking_job",
                    "Failed to queue phase-3 linking job"
                );
                None
            }
        };
        (embed_id, link_id)
    }

    /// Queue a tier-2 escalation job for related concept inference.
    /// Returns the new job ID if queued (None if deduplicated or on error).
    async fn queue_related_tier_escalation(
        &self,
        note_id: uuid::Uuid,
        schema: &str,
    ) -> Option<uuid::Uuid> {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::RelatedConceptInference,
                JobType::RelatedConceptInference.default_priority(),
                payload,
                Some(matric_core::cost_tier::STANDARD_GPU),
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_RELATED_CONCEPT_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "queue_related_concept_tier2_escalation",
                    "Failed to queue related concept tier-2 escalation"
                );
                None
            }
        }
    }
}

/// A single related concept pair inferred by the LLM.
#[derive(Debug, serde::Deserialize)]
struct RelatedPair {
    concept_a: String,
    concept_b: String,
    confidence: f32,
}

#[async_trait]
impl JobHandler for RelatedConceptHandler {
    fn job_type(&self) -> JobType {
        JobType::RelatedConceptInference
    }

    #[instrument(
        skip(self, ctx),
        fields(
            subsystem = "jobs",
            component = "related_concept_inference",
            op = "execute"
        )
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let model_override = extract_model_override(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };

        // Tiered model routing based on cost_tier.
        let use_fast = overridden.is_none() && self.fast_backend.is_some();
        let backend: &dyn GenerationBackend = match ctx.job.cost_tier {
            Some(matric_core::cost_tier::FAST_GPU) if self.fast_backend.is_some() => {
                self.fast_backend.as_ref().unwrap()
            }
            Some(matric_core::cost_tier::STANDARD_GPU) => &self.backend,
            _ => match (&overridden, use_fast) {
                (Some(b), _) => b.as_ref(),
                (_, true) => self.fast_backend.as_ref().unwrap(),
                (_, false) => &self.backend,
            },
        };

        ctx.report_progress(10, Some("Fetching note concepts..."));

        // Query concepts tagged on this note with their dimension context.
        // We need notation (used as identifier), label (human-readable), and
        // the broader parent notation to identify which dimension each concept belongs to.
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => {
                let (embed_id, link_id) = self.queue_phase3_jobs(note_id, schema).await;
                if let Some(jid) = embed_id {
                    ctx.emit_job_queued(jid, JobType::Embedding, Some(note_id));
                }
                if let Some(jid) = link_id {
                    ctx.emit_job_queued(jid, JobType::Linking, Some(note_id));
                }
                return related_concept_job_failure(e, "fetch_concepts_begin_tx");
            }
        };

        #[derive(Debug)]
        struct ConceptInfo {
            id: uuid::Uuid,
            notation: String,
            label: String,
            broader_notation: Option<String>,
            depth: i32,
        }

        let rows: Vec<ConceptInfo> =
            sqlx::query_as::<_, (uuid::Uuid, String, String, Option<String>, i32)>(
                r#"SELECT c.id, c.notation, COALESCE(l.value, c.notation) as label,
                      bc.notation as broader_notation, c.depth
               FROM note_skos_concept nc
               JOIN skos_concept c ON nc.concept_id = c.id
               LEFT JOIN skos_concept_label l ON c.id = l.concept_id
                   AND l.label_type = 'pref_label' AND l.language = 'en'
               LEFT JOIN skos_semantic_relation_edge sre
                   ON sre.subject_id = c.id AND sre.relation_type = 'broader'
               LEFT JOIN skos_concept bc ON sre.object_id = bc.id AND bc.depth = 0
               WHERE nc.note_id = $1
               ORDER BY nc.relevance_score DESC"#,
            )
            .bind(note_id)
            .fetch_all(&mut *tx)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(
                |(id, notation, label, broader_notation, depth)| ConceptInfo {
                    id,
                    notation,
                    label,
                    broader_notation,
                    depth,
                },
            )
            .collect();

        // Also fetch existing related relations to avoid duplicates
        let existing_related: Vec<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
            r#"SELECT sre.subject_id, sre.object_id
               FROM skos_semantic_relation_edge sre
               WHERE sre.relation_type = 'related'
                 AND sre.subject_id = ANY($1)"#,
        )
        .bind(rows.iter().map(|r| r.id).collect::<Vec<_>>())
        .fetch_all(&mut *tx)
        .await
        .unwrap_or_default();

        tx.commit().await.ok();

        // Filter out root-level dimension concepts (depth=0) — only relate leaf/intermediate concepts
        let concepts: Vec<&ConceptInfo> = rows.iter().filter(|c| c.depth > 0).collect();

        // Need at least 3 concepts for meaningful cross-dimensional pairs
        if concepts.len() < 3 {
            let (embed_id, link_id) = self.queue_phase3_jobs(note_id, schema).await;
            if let Some(jid) = embed_id {
                ctx.emit_job_queued(jid, JobType::Embedding, Some(note_id));
            }
            if let Some(jid) = link_id {
                ctx.emit_job_queued(jid, JobType::Linking, Some(note_id));
            }
            return JobResult::Success(Some(serde_json::json!({
                "relations_created": 0,
                "reason": if rows.is_empty() { "no_concepts" } else { "too_few_leaf_concepts" },
                "total_concepts": rows.len(),
                "leaf_concepts": concepts.len()
            })));
        }

        ctx.report_progress(30, Some("Inferring related concept pairs..."));

        // Build a set of existing related pairs for deduplication
        let existing_set: std::collections::HashSet<(uuid::Uuid, uuid::Uuid)> = existing_related
            .into_iter()
            .flat_map(|(a, b)| [(a, b), (b, a)])
            .collect();

        // Build LLM prompt listing concepts with dimension context
        let concept_list: String = concepts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let dimension = c.broader_notation.as_deref().unwrap_or("unknown");
                format!("{}. {} (dimension: {})", i + 1, c.label, dimension)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"You are a knowledge organization specialist. Given SKOS concepts tagged on a document, identify pairs that are semantically RELATED (associative, non-hierarchical).

Rules:
- Do NOT pair concepts that already have broader/narrower relationships
- Focus on cross-dimensional associations (e.g., a technique related to a domain)
- Only suggest pairs with genuine semantic association
- Confidence: 0.7+ for strong associations, 0.5-0.7 for moderate
- Use the exact concept labels from the list below

Concepts:
{concept_list}

Output ONLY a JSON array (no markdown, no explanation):
[{{"concept_a": "label-a", "concept_b": "label-b", "confidence": 0.85}}]

If no meaningful related pairs exist, output an empty array: []"#
        );

        let ai_response = match backend.generate_json(&prompt).await {
            Ok(r) => r.trim().to_string(),
            Err(e) => {
                if use_fast {
                    // Fast model failed — escalate to tier-2 via job queue.
                    // Do NOT queue phase-3 here — the tier-2 job will do it after completion.
                    info!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_RELATED_CONCEPT_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "fast_related_concept_generation",
                        "Fast model failed for related concepts, escalating to tier-2"
                    );
                    if let Some(job_id) = self.queue_related_tier_escalation(note_id, schema).await
                    {
                        ctx.emit_job_queued(
                            job_id,
                            JobType::RelatedConceptInference,
                            Some(note_id),
                        );
                    }
                    return JobResult::Success(Some(serde_json::json!({
                        "relations_created": 0,
                        "escalated": true,
                        "reason": "fast_model_failed"
                    })));
                }
                let (embed_id, link_id) = self.queue_phase3_jobs(note_id, schema).await;
                if let Some(jid) = embed_id {
                    ctx.emit_job_queued(jid, JobType::Embedding, Some(note_id));
                }
                if let Some(jid) = link_id {
                    ctx.emit_job_queued(jid, JobType::Linking, Some(note_id));
                }
                return ai_generation_job_failure(e, "related_concept_inference");
            }
        };

        ctx.report_progress(60, Some("Parsing related pairs..."));

        // Parse the AI response.
        // With format:"json" enforcement, output is guaranteed valid JSON from Ollama.
        let pairs: Vec<RelatedPair> = match parse_json_lenient(&ai_response) {
            Ok(p) => p,
            Err(_) => {
                let cleaned = ai_response
                    .trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();
                match parse_json_lenient(cleaned) {
                    Ok(p) => p,
                    Err(e) => {
                        if use_fast {
                            info!(
                                error_len = diagnostic_len(&e),
                                detail = JOB_RELATED_CONCEPT_DIAGNOSTIC_FAILURE_DETAIL,
                                operation = "fast_related_concept_parse",
                                "Fast model output unparseable for related concepts, escalating to tier-2"
                            );
                            if let Some(job_id) =
                                self.queue_related_tier_escalation(note_id, schema).await
                            {
                                ctx.emit_job_queued(
                                    job_id,
                                    JobType::RelatedConceptInference,
                                    Some(note_id),
                                );
                            }
                            return JobResult::Success(Some(serde_json::json!({
                                "relations_created": 0,
                                "escalated": true,
                                "reason": "fast_model_parse_failed"
                            })));
                        }
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_RELATED_CONCEPT_DIAGNOSTIC_FAILURE_DETAIL,
                            response_len = ai_response.len(),
                            parser = "related_concept_pairs",
                            "Failed to parse related concept pairs"
                        );
                        let (embed_id, link_id) = self.queue_phase3_jobs(note_id, schema).await;
                        if let Some(jid) = embed_id {
                            ctx.emit_job_queued(jid, JobType::Embedding, Some(note_id));
                        }
                        if let Some(jid) = link_id {
                            ctx.emit_job_queued(jid, JobType::Linking, Some(note_id));
                        }
                        return ai_generation_job_failure(e, "related_concept_parse");
                    }
                }
            }
        };

        if pairs.is_empty() {
            let (embed_id, link_id) = self.queue_phase3_jobs(note_id, schema).await;
            if let Some(jid) = embed_id {
                ctx.emit_job_queued(jid, JobType::Embedding, Some(note_id));
            }
            if let Some(jid) = link_id {
                ctx.emit_job_queued(jid, JobType::Linking, Some(note_id));
            }
            return JobResult::Success(Some(serde_json::json!({
                "relations_created": 0,
                "reason": "no_pairs_suggested",
                "concepts_analyzed": concepts.len()
            })));
        }

        ctx.report_progress(70, Some("Creating related relations..."));

        // Build a lookup from label → concept info
        let label_to_concept: std::collections::HashMap<String, &ConceptInfo> = concepts
            .iter()
            .flat_map(|c| {
                // Match by both label and notation for robustness
                let mut entries = vec![(c.label.to_lowercase(), *c)];
                entries.push((c.notation.to_lowercase(), *c));
                entries
            })
            .collect();

        let mut relations_created = 0u32;
        let total_pairs = pairs.len();

        for (i, pair) in pairs.iter().enumerate() {
            // Clamp confidence to valid range
            let confidence = pair.confidence.clamp(0.0, 1.0);
            if confidence < 0.5 {
                debug!(concept_a = %pair.concept_a, concept_b = %pair.concept_b, confidence, "Skipping low-confidence pair");
                continue;
            }

            // Look up concepts by label
            let concept_a = label_to_concept.get(&pair.concept_a.to_lowercase());
            let concept_b = label_to_concept.get(&pair.concept_b.to_lowercase());

            let (a, b) = match (concept_a, concept_b) {
                (Some(a), Some(b)) => (a, b),
                _ => {
                    debug!(
                        concept_a = %pair.concept_a,
                        concept_b = %pair.concept_b,
                        "Skipping pair: concept not found in note's tagged concepts"
                    );
                    continue;
                }
            };

            // Skip self-relations
            if a.id == b.id {
                continue;
            }

            // Skip if relation already exists
            if existing_set.contains(&(a.id, b.id)) {
                debug!(a_notation = %a.notation, b_notation = %b.notation, "Skipping existing related pair");
                continue;
            }

            // Create the related relation (is_inferred=false triggers reciprocal via trigger)
            match self
                .db
                .skos
                .create_semantic_relation(CreateSemanticRelationRequest {
                    subject_id: a.id,
                    object_id: b.id,
                    relation_type: SkosSemanticRelation::Related,
                    inference_score: Some(confidence),
                    is_inferred: false,
                    created_by: Some("related_concept_inference".to_string()),
                })
                .await
            {
                Ok(_) => {
                    relations_created += 1;
                    debug!(
                        a_notation = %a.notation,
                        b_notation = %b.notation,
                        confidence,
                        "Created related concept relation"
                    );
                }
                Err(e) => {
                    // Unique constraint violation means it already exists — not an error
                    debug!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_RELATED_CONCEPT_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "create_related_relation",
                        "Failed to create related relation"
                    );
                }
            }

            let progress = 70 + ((i + 1) * 25 / total_pairs) as i32;
            ctx.report_progress(
                progress,
                Some(&format!("Related: {} ↔ {}", a.label, b.label)),
            );
        }

        ctx.report_progress(98, Some("Queuing embedding and linking..."));
        let (embed_id, link_id) = self.queue_phase3_jobs(note_id, schema).await;
        if let Some(jid) = embed_id {
            ctx.emit_job_queued(jid, JobType::Embedding, Some(note_id));
        }
        if let Some(jid) = link_id {
            ctx.emit_job_queued(jid, JobType::Linking, Some(note_id));
        }

        ctx.report_progress(100, Some("Related concept inference complete"));
        info!(
            note_id = %note_id,
            relations_created,
            pairs_suggested = total_pairs,
            concepts_analyzed = concepts.len(),
            duration_ms = start.elapsed().as_millis() as u64,
            "Related concept inference completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "relations_created": relations_created,
            "pairs_suggested": total_pairs,
            "concepts_analyzed": concepts.len()
        })))
    }
}

/// Handler for extracting rich metadata from note content using AI analysis (#430).
///
/// Extracts structured metadata fields from content (authors, dates, DOI, venues,
/// institutions, etc.) and merges them into the note's JSONB metadata column.
/// Runs in Phase 1 of the NLP pipeline alongside ConceptTagging.
pub struct MetadataExtractionHandler {
    db: Database,
    backend: OllamaBackend,
    /// Fast model backend for simple documents (#439).
    fast_backend: Option<OllamaBackend>,
    registry: Arc<ProviderRegistry>,
}

impl MetadataExtractionHandler {
    pub fn new(
        db: Database,
        backend: OllamaBackend,
        fast_backend: Option<OllamaBackend>,
        registry: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            db,
            backend,
            fast_backend,
            registry,
        }
    }

    /// Queue a tier-escalation job for metadata extraction.
    /// Returns the new job ID if queued (None if deduplicated or on error).
    async fn queue_tier_escalation(
        &self,
        note_id: uuid::Uuid,
        schema: &str,
        next_tier: i16,
    ) -> Option<uuid::Uuid> {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        match self
            .db
            .jobs
            .queue_deduplicated(
                Some(note_id),
                JobType::MetadataExtraction,
                JobType::MetadataExtraction.default_priority(),
                payload,
                Some(next_tier),
            )
            .await
        {
            Ok(job_id) => job_id,
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_METADATA_DIAGNOSTIC_FAILURE_DETAIL,
                    next_tier,
                    operation = "queue_metadata_extraction_tier_escalation",
                    "Failed to queue metadata extraction tier escalation"
                );
                None
            }
        }
    }
}

#[async_trait]
impl JobHandler for MetadataExtractionHandler {
    fn job_type(&self) -> JobType {
        JobType::MetadataExtraction
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "metadata_extraction", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let model_override = extract_model_override(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let overridden = match resolve_gen_backend(&self.registry, model_override.as_deref()) {
            Ok(b) => b,
            Err(e) => return e,
        };

        ctx.report_progress(10, Some("Fetching note content..."));

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return metadata_extraction_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return metadata_extraction_job_failure(e, "fetch_note"),
        };
        tx.commit().await.ok();

        // Use revised content if available, otherwise original
        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            return JobResult::Success(Some(
                serde_json::json!({"fields_extracted": 0, "reason": "empty_content"}),
            ));
        }

        ctx.report_progress(20, Some("Analyzing content for metadata..."));

        let content_preview: String = content
            .chars()
            .take(matric_core::defaults::PREVIEW_METADATA)
            .collect();

        // Tiered model routing:
        // Tier 1 (FAST_GPU, default): fast model → escalate to tier-2 on failure
        // Tier 2 (STANDARD_GPU): standard model → fail cleanly
        let is_standard_tier = ctx.job.cost_tier == Some(matric_core::cost_tier::STANDARD_GPU);
        let use_fast = !is_standard_tier && overridden.is_none() && self.fast_backend.is_some();

        let backend: &dyn GenerationBackend = match (&overridden, use_fast) {
            (Some(b), _) => b.as_ref(),
            (_, true) => self.fast_backend.as_ref().unwrap(),
            (_, false) => &self.backend,
        };

        // Start provenance activity
        let activity_id = self
            .db
            .provenance
            .start_activity(
                note_id,
                "metadata_extraction",
                Some(matric_core::GenerationBackend::model_name(backend)),
            )
            .await
            .ok();

        // Use AI to extract structured metadata from content
        let prompt = format!(
            r#"You are a metadata extraction specialist. Analyze the following content and extract all available structured metadata fields.

Content:
{}

Extract any of the following fields that are present or can be inferred from the content. Return ONLY valid JSON, nothing else.

Fields to extract:
- "authors": array of author names (strings)
- "year": publication year (number)
- "venue": publication venue/journal/conference (string)
- "doi": DOI identifier (string)
- "arxiv_id": arXiv paper ID (string)
- "isbn": ISBN (string)
- "url": source URL (string)
- "institutions": array of affiliated institutions (strings)
- "abstract": paper abstract or summary if present (string, max 500 chars)
- "document_type": type of document e.g. "research_paper", "tutorial", "blog_post", "documentation", "report", "book_chapter", "thesis" (string)
- "language": primary language of the content (string, ISO 639-1 code)
- "keywords": array of key terms/phrases from the content (strings, 5-15 items)
- "references_count": number of references/citations mentioned (number)
- "methodology": research methodology used if applicable (string)
- "domain": primary domain/field (string)
- "sub_domain": specific sub-field (string)

Only include fields where you have reasonable confidence. Omit fields you cannot determine.

Example output:
{{"authors": ["John Smith", "Jane Doe"], "year": 2024, "venue": "NeurIPS", "domain": "machine-learning", "keywords": ["transformers", "attention", "NLP"]}}"#,
            content_preview
        );

        let ai_response = match backend.generate_json(&prompt).await {
            Ok(r) => r.trim().to_string(),
            Err(e) => {
                if use_fast {
                    info!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_METADATA_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "fast_metadata_extraction",
                        "Fast model failed for metadata extraction, escalating to tier-2"
                    );
                    if let Some(job_id) = self
                        .queue_tier_escalation(
                            note_id,
                            schema,
                            matric_core::cost_tier::STANDARD_GPU,
                        )
                        .await
                    {
                        ctx.emit_job_queued(job_id, JobType::MetadataExtraction, Some(note_id));
                    }
                    return JobResult::Success(Some(serde_json::json!({
                        "fields_extracted": 0,
                        "escalated": true,
                        "reason": "fast_model_failed"
                    })));
                }
                return ai_generation_job_failure(e, "metadata_extraction");
            }
        };

        ctx.report_progress(60, Some("Parsing extracted metadata..."));

        // Parse the AI response as JSON object.
        // With format:"json" enforcement, output is guaranteed valid JSON from Ollama.
        let extracted: serde_json::Value = match serde_json::from_str(&ai_response) {
            Ok(v) => v,
            Err(_) => {
                let cleaned = ai_response
                    .trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();
                match serde_json::from_str(cleaned) {
                    Ok(v) => v,
                    Err(e) => {
                        if use_fast {
                            info!(
                                error_len = diagnostic_len(&e),
                                detail = JOB_METADATA_DIAGNOSTIC_FAILURE_DETAIL,
                                operation = "fast_metadata_parse",
                                "Fast model returned unparseable metadata, escalating to tier-2"
                            );
                            if let Some(job_id) = self
                                .queue_tier_escalation(
                                    note_id,
                                    schema,
                                    matric_core::cost_tier::STANDARD_GPU,
                                )
                                .await
                            {
                                ctx.emit_job_queued(
                                    job_id,
                                    JobType::MetadataExtraction,
                                    Some(note_id),
                                );
                            }
                            return JobResult::Success(Some(serde_json::json!({
                                "fields_extracted": 0,
                                "escalated": true,
                                "reason": "fast_model_parse_failed"
                            })));
                        }
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_METADATA_DIAGNOSTIC_FAILURE_DETAIL,
                            response_len = ai_response.len(),
                            parser = "metadata_json",
                            "Failed to parse AI metadata response"
                        );
                        return metadata_extraction_job_failure(e, "parse_ai_response");
                    }
                }
            }
        };

        let fields_extracted = if let Some(obj) = extracted.as_object() {
            obj.len()
        } else {
            0
        };

        if fields_extracted == 0 {
            return JobResult::Success(Some(serde_json::json!({
                "fields_extracted": 0,
                "reason": "no_metadata_found"
            })));
        }

        ctx.report_progress(80, Some("Merging metadata into note..."));

        // Merge extracted metadata into existing note metadata
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return metadata_extraction_job_failure(e, "update_metadata_begin_tx"),
        };

        // Get existing metadata and merge
        let existing_metadata = note.note.metadata.clone();
        let mut merged: serde_json::Map<String, serde_json::Value> =
            if let Some(obj) = existing_metadata.as_object() {
                obj.clone()
            } else {
                serde_json::Map::new()
            };

        // Merge AI-extracted fields under an "ai_extracted" namespace to avoid
        // overwriting user-provided metadata
        if let Some(extracted_obj) = extracted.as_object() {
            let mut ai_fields = serde_json::Map::new();
            for (key, value) in extracted_obj {
                ai_fields.insert(key.clone(), value.clone());
            }
            merged.insert(
                "ai_extracted".to_string(),
                serde_json::Value::Object(ai_fields),
            );
        }

        // Also promote certain high-value fields to top level if not already set
        if let Some(extracted_obj) = extracted.as_object() {
            for key in &[
                "authors",
                "year",
                "venue",
                "doi",
                "arxiv_id",
                "domain",
                "language",
                "document_type",
            ] {
                if !merged.contains_key(*key) {
                    if let Some(val) = extracted_obj.get(*key) {
                        merged.insert(key.to_string(), val.clone());
                    }
                }
            }
        }

        let merged_value = serde_json::Value::Object(merged);
        if let Err(e) = sqlx::query("UPDATE note SET metadata = $1 WHERE id = $2")
            .bind(&merged_value)
            .bind(note_id)
            .execute(&mut *tx)
            .await
        {
            return metadata_extraction_job_failure(e, "update_metadata");
        }

        if let Err(e) = tx.commit().await {
            return metadata_extraction_job_failure(e, "update_metadata_commit");
        }

        // Complete provenance activity
        if let Some(act_id) = activity_id {
            let prov_metadata = serde_json::json!({
                "fields_extracted": fields_extracted,
                "content_preview_chars": content_preview.len(),
            });
            if let Err(e) = self
                .db
                .provenance
                .complete_activity(act_id, None, Some(prov_metadata))
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                    diagnostic = JOB_METADATA_DIAGNOSTIC_FAILURE_DETAIL,
                    "Failed to complete metadata extraction provenance activity"
                );
            }
        }

        ctx.report_progress(100, Some("Metadata extraction complete"));
        info!(
            note_id = %note_id,
            fields_extracted = fields_extracted,
            duration_ms = start.elapsed().as_millis() as u64,
            "Metadata extraction completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "fields_extracted": fields_extracted,
            "extracted_keys": extracted.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()
        })))
    }
}

/// Handler for auto-detecting document type during ingest (#430).
///
/// Uses filename patterns, MIME type, and content analysis to classify
/// notes into document types from the registry. Runs in Phase 1 of the
/// NLP pipeline.
pub struct DocumentTypeInferenceHandler {
    db: Database,
}

impl DocumentTypeInferenceHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for DocumentTypeInferenceHandler {
    fn job_type(&self) -> JobType {
        JobType::DocumentTypeInference
    }

    #[instrument(
        skip(self, ctx),
        fields(
            subsystem = "jobs",
            component = "document_type_inference",
            op = "execute"
        )
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        // Start provenance activity
        let activity_id = self
            .db
            .provenance
            .start_activity(note_id, "document_type_inference", None)
            .await
            .ok();

        ctx.report_progress(10, Some("Fetching note..."));

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return document_type_inference_job_failure(e, "fetch_note_begin_tx"),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return document_type_inference_job_failure(e, "fetch_note"),
        };
        tx.commit().await.ok();

        // Skip if document type already assigned
        if note.note.document_type_id.is_some() {
            return JobResult::Success(Some(serde_json::json!({
                "skipped": true,
                "reason": "document_type_already_assigned"
            })));
        }

        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            return JobResult::Success(Some(
                serde_json::json!({"detected": false, "reason": "empty_content"}),
            ));
        }

        ctx.report_progress(30, Some("Detecting document type..."));

        // Extract filename hint from metadata if available
        let filename_hint = note
            .note
            .metadata
            .get("source_file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Use the document type detection system
        let content_preview: String = content.chars().take(1000).collect();
        let detection = self
            .db
            .document_types
            .detect(
                filename_hint.as_deref(),
                Some(&content_preview),
                None, // no MIME type for notes
            )
            .await;

        let (doc_type_id, detection_method, confidence) = match detection {
            Ok(Some(result)) => {
                ctx.report_progress(
                    60,
                    Some(&format!("Detected: {}", result.document_type.name)),
                );
                (
                    result.document_type.id,
                    result.detection_method,
                    result.confidence,
                )
            }
            Ok(None) => {
                if let Some(act_id) = activity_id {
                    let _ = self
                        .db
                        .provenance
                        .complete_activity(
                            act_id,
                            None,
                            Some(serde_json::json!({"detected": false})),
                        )
                        .await;
                }
                return JobResult::Success(Some(serde_json::json!({
                    "detected": false,
                    "reason": "no_match"
                })));
            }
            Err(e) => {
                return document_type_inference_job_failure(e, "detect_document_type");
            }
        };

        ctx.report_progress(80, Some("Assigning document type..."));

        // Update note with detected document type
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return document_type_inference_job_failure(e, "assign_begin_tx"),
        };
        if let Err(e) = sqlx::query("UPDATE note SET document_type_id = $1 WHERE id = $2")
            .bind(doc_type_id)
            .bind(note_id)
            .execute(&mut *tx)
            .await
        {
            return document_type_inference_job_failure(e, "assign_document_type");
        }
        if let Err(e) = tx.commit().await {
            return document_type_inference_job_failure(e, "assign_commit");
        }

        // Complete provenance activity
        if let Some(act_id) = activity_id {
            let prov_metadata = serde_json::json!({
                "document_type_id": doc_type_id.to_string(),
                "detection_method": detection_method,
                "confidence": confidence,
            });
            if let Err(e) = self
                .db
                .provenance
                .complete_activity(act_id, None, Some(prov_metadata))
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
                    diagnostic = JOB_DOCUMENT_TYPE_DIAGNOSTIC_FAILURE_DETAIL,
                    "Failed to complete document type inference provenance activity"
                );
            }
        }

        ctx.report_progress(100, Some("Document type inference complete"));
        info!(
            note_id = %note_id,
            document_type_id = %doc_type_id,
            detection_method = %detection_method,
            confidence = confidence,
            duration_ms = start.elapsed().as_millis() as u64,
            "Document type inference completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "detected": true,
            "document_type_id": doc_type_id.to_string(),
            "detection_method": detection_method,
            "confidence": confidence,
        })))
    }
}

/// Handler for re-embedding all notes.
/// This bulk operation queries notes and queues individual embedding jobs for each.
/// Supports optional filtering by embedding set via payload.
pub struct ReEmbedAllHandler {
    db: Database,
}

impl ReEmbedAllHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for ReEmbedAllHandler {
    fn job_type(&self) -> JobType {
        JobType::ReEmbedAll
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "re_embed_all", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        ctx.report_progress(5, Some("Starting bulk re-embedding..."));

        // Check if we're filtering by embedding set
        let embedding_set_slug = ctx
            .payload()
            .and_then(|p| p.get("embedding_set"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Get note IDs to process
        let note_ids: Vec<uuid::Uuid> = if let Some(slug) = &embedding_set_slug {
            ctx.report_progress(10, Some(&format!("Getting notes from set: {}", slug)));

            // Get notes from specific embedding set
            // Use a large limit to get all members
            match self.db.embedding_sets.list_members(slug, 100000, 0).await {
                Ok(members) => members.into_iter().map(|m| m.note_id).collect(),
                Err(e) => return reembed_all_job_failure(e, "list_embedding_set_members"),
            }
        } else {
            ctx.report_progress(10, Some("Getting all active notes..."));

            // Get all active notes
            match self.db.notes.list_all_ids().await {
                Ok(ids) => ids,
                Err(e) => return reembed_all_job_failure(e, "list_all_notes"),
            }
        };

        let total_notes = note_ids.len();
        if total_notes == 0 {
            return JobResult::Success(Some(serde_json::json!({
                "notes_queued": 0,
                "message": "No notes to re-embed"
            })));
        }

        ctx.report_progress(
            20,
            Some(&format!(
                "Queueing embedding jobs for {} notes...",
                total_notes
            )),
        );

        // Queue embedding jobs for each note
        let mut queued = 0;
        let mut failed = 0;

        for (i, note_id) in note_ids.iter().enumerate() {
            match self
                .db
                .jobs
                .queue(Some(*note_id), JobType::Embedding, 5, None, None)
                .await
            {
                Ok(_) => queued += 1,
                Err(e) => {
                    debug!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_REEMBED_QUEUE_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "queue_embedding_job",
                        "Failed to queue embedding job"
                    );
                    failed += 1;
                }
            }

            // Update progress every 10 notes or at the end
            if (i + 1) % 10 == 0 || i + 1 == total_notes {
                let progress = 20 + ((i + 1) * 80 / total_notes) as i32;
                ctx.report_progress(
                    progress.min(99),
                    Some(&format!("Queued {}/{} embedding jobs", i + 1, total_notes)),
                );
            }
        }

        ctx.report_progress(100, Some("Bulk re-embedding jobs queued"));
        info!(
            total_notes = total_notes,
            queued = queued,
            failed = failed,
            embedding_set = ?embedding_set_slug,
            duration_ms = start.elapsed().as_millis() as u64,
            "Bulk re-embedding completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "notes_queued": queued,
            "notes_failed": failed,
            "total_notes": total_notes,
            "embedding_set": embedding_set_slug
        })))
    }
}

// =============================================================================
// UTILITY FUNCTIONS (ported from HOTM)
// =============================================================================

/// Clean up enhanced content to remove any accidental markers or prompt leakage.
///
/// This function aggressively removes system prompts, instructions, and markers
/// that may leak into the AI response, particularly in raw mode with thinking models.
fn clean_enhanced_content(content: &str, _original_prompt: &str) -> String {
    let mut cleaned = content.to_string();

    // Remove prompt leakage that occurs with raw mode models.
    // Only match highly specific prompt phrases — avoid generic terms like "Guidelines:"
    // that could legitimately appear in user content.
    let prompt_indicators = [
        "You are an intelligent note-taking assistant",
        "You are a formatting assistant",
        "Your task is to enhance the following note",
        "Your task is to improve the structure and readability of the following note",
        "Original Note:",
        "Output the enhanced note in clean markdown format",
        "Output the formatted note",
        "Output the revised note in clean markdown format",
        "Do not add any labels, markers, or metadata",
        // Phase 2 contextual revision markers (#494)
        "## PRIMARY CONTENT (this is the note you are revising",
        "## REFERENCE CONTEXT (supplementary only",
        "You are an intelligent note-taking assistant performing a contextual revision",
        // Type-aware revision markers (content-type-aware prompts)
        "You are a video content editor",
        "You are a film analyst",
        "You are a documentary analyst",
        "You are a meeting analyst",
        "You are an interview analyst",
        "You are an academic content analyst",
        "You are a technical writer",
        "You are an audio content analyst",
        "You are an educational content analyst",
        "Output the revised document in clean markdown format",
        "Revise the following content into a polished",
    ];

    // Remove any lines that match prompt indicators (case-insensitive)
    let lines: Vec<&str> = cleaned.lines().collect();
    let original_line_count = lines.len();
    let mut filtered_lines = Vec::new();
    let mut skip_until_content = false;
    let mut removed_count = 0;

    for line in &lines {
        let line_lower = line.to_lowercase();
        let line_trimmed = line.trim();

        // Check if this line is part of the system prompt leakage
        let is_prompt_line = prompt_indicators
            .iter()
            .any(|indicator| line_lower.contains(&indicator.to_lowercase()));

        if is_prompt_line {
            skip_until_content = true;
            removed_count += 1;
            continue;
        }

        // Skip empty lines immediately after detecting prompt
        if skip_until_content && line_trimmed.is_empty() {
            removed_count += 1;
            continue;
        }

        // Once we hit actual content, stop skipping
        if skip_until_content && !line_trimmed.is_empty() {
            skip_until_content = false;
        }

        filtered_lines.push(*line);
    }

    if removed_count > 0 {
        info!(
            removed_lines = removed_count,
            original_lines = original_line_count,
            remaining_lines = filtered_lines.len(),
            "Cleaned prompt leakage from AI revision output"
        );
    }

    cleaned = filtered_lines.join("\n");

    // Remove obvious wrapper markers at the start (but NOT generic markdown like "---")
    let start_markers = [
        "PART 1",
        "PART 2",
        "ENHANCED NOTE",
        "FORMATTED NOTE",
        "REVISED NOTE",
        "METADATA",
    ];

    for marker in &start_markers {
        if cleaned.starts_with(marker) {
            cleaned = cleaned
                .split_once('\n')
                .map(|x| x.1)
                .unwrap_or(&cleaned)
                .to_string();
        }
    }

    // Remove markdown code fence wrappers ONLY if the entire content is wrapped
    let is_fenced = (cleaned.starts_with("```markdown\n") || cleaned.starts_with("```md\n"))
        && (cleaned.ends_with("\n```") || cleaned.ends_with("```"));
    if is_fenced {
        // Strip opening fence
        let after_fence = cleaned.find('\n').map(|i| i + 1).unwrap_or(0);
        cleaned = cleaned[after_fence..].to_string();
        // Strip closing fence
        if let Some(pos) = cleaned.rfind("\n```") {
            cleaned = cleaned[..pos].to_string();
        } else if cleaned.ends_with("```") {
            cleaned = cleaned.trim_end_matches("```").to_string();
        }
    }

    // Remove leading/trailing whitespace
    cleaned = cleaned.trim().to_string();

    // Log warning if cleaning removed most of the content
    let content_len = content.trim().len();
    let cleaned_len = cleaned.len();
    if content_len > 0 && cleaned_len == 0 {
        warn!(
            original_bytes = content_len,
            "AI output was entirely removed by content cleaning — \
             model may have echoed the prompt instead of generating a revision"
        );
    } else if content_len > 100 && cleaned_len < content_len / 4 {
        warn!(
            original_bytes = content_len,
            cleaned_bytes = cleaned_len,
            "Content cleaning removed >75% of AI output — possible over-filtering"
        );
    }

    cleaned
}

/// Handler for refreshing embedding sets.
///
/// For manual sets: finds members that are missing embeddings for this set
/// and queues individual Embedding jobs with `embedding_set_id` in the payload.
pub struct RefreshEmbeddingSetHandler {
    db: Database,
}

impl RefreshEmbeddingSetHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for RefreshEmbeddingSetHandler {
    fn job_type(&self) -> JobType {
        JobType::RefreshEmbeddingSet
    }

    #[instrument(
        skip(self, ctx),
        fields(
            subsystem = "jobs",
            component = "refresh_embedding_set",
            op = "execute"
        )
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();

        let set_slug = match ctx
            .payload()
            .and_then(|p| p.get("set_slug"))
            .and_then(|v| v.as_str())
        {
            Some(s) => s.to_string(),
            None => return JobResult::Failed("No set_slug in payload".into()),
        };

        ctx.report_progress(10, Some("Looking up embedding set..."));

        let set = match self.db.embedding_sets.get_by_slug(&set_slug).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                return refresh_embedding_set_job_failure(
                    "embedding set not found",
                    "lookup_set_not_found",
                )
            }
            Err(e) => return refresh_embedding_set_job_failure(e, "lookup_set"),
        };

        ctx.report_progress(20, Some("Finding members missing embeddings..."));

        // Find members that don't have embeddings for this set
        let missing_note_ids: Vec<uuid::Uuid> = match sqlx::query_scalar(
            r#"
            SELECT m.note_id
            FROM embedding_set_member m
            LEFT JOIN embedding e ON e.note_id = m.note_id AND e.embedding_set_id = m.embedding_set_id
            WHERE m.embedding_set_id = $1 AND e.id IS NULL
            "#,
        )
        .bind(set.id)
        .fetch_all(&self.db.pool)
        .await
        {
            Ok(ids) => ids,
            Err(e) => return refresh_embedding_set_job_failure(e, "find_missing_embeddings"),
        };

        ctx.report_progress(50, Some("Queuing embedding jobs..."));

        let mut queued = 0;
        for note_id in &missing_note_ids {
            let payload = serde_json::json!({ "embedding_set_id": set.id.to_string() });
            match self
                .db
                .jobs
                .queue(
                    Some(*note_id),
                    JobType::Embedding,
                    JobType::Embedding.default_priority(),
                    Some(payload),
                    None,
                )
                .await
            {
                Ok(_) => queued += 1,
                Err(e) => warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_REEMBED_QUEUE_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "queue_embedding_set_refresh_job",
                    "Failed to queue embedding job"
                ),
            }
        }

        // Update set status
        let _ = sqlx::query(
            "UPDATE embedding_set SET last_refresh_at = NOW(), index_status = 'building', updated_at = NOW() WHERE id = $1",
        )
        .bind(set.id)
        .execute(&self.db.pool)
        .await;

        ctx.report_progress(100, Some("Refresh complete"));
        info!(
            set_slug = %set_slug,
            missing = missing_note_ids.len(),
            queued = queued,
            duration_ms = start.elapsed().as_millis() as u64,
            "Embedding set refresh completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "set_slug": set_slug,
            "missing_count": missing_note_ids.len(),
            "jobs_queued": queued
        })))
    }
}

/// Handler for EXIF metadata extraction jobs.
///
/// Extracts EXIF metadata (GPS, camera, datetime) from image attachments and
/// creates provenance records (location, device, file provenance) to populate
/// the spatial-temporal search pipeline.
pub struct ExifExtractionHandler {
    db: Database,
}

impl ExifExtractionHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for ExifExtractionHandler {
    fn job_type(&self) -> JobType {
        JobType::ExifExtraction
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "exif_extraction", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        // Schema context for multi-memory archive support (Issue #426)
        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(5, Some("Resolving attachments..."));

        // Get the file storage backend
        let file_storage = match self.db.file_storage.as_ref() {
            Some(fs) => fs,
            None => return JobResult::Failed("File storage not configured".into()),
        };

        // Get attachment_id from payload, or list attachments for the note
        let attachment_id: uuid::Uuid = if let Some(id_str) = ctx
            .payload()
            .and_then(|p| p.get("attachment_id"))
            .and_then(|v| v.as_str())
        {
            match id_str.parse() {
                Ok(id) => id,
                Err(e) => return attachment_processing_job_failure(e, "parse_attachment_id"),
            }
        } else {
            // No explicit attachment_id — find image attachments for this note (schema-aware)
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return attachment_processing_job_failure(e, "list_attachments_begin_tx"),
            };
            let attachments = match file_storage.list_by_note_tx(&mut tx, note_id).await {
                Ok(a) => a,
                Err(e) => return attachment_processing_job_failure(e, "list_attachments"),
            };
            if let Err(e) = tx.commit().await {
                return attachment_processing_job_failure(e, "list_attachments_commit");
            }
            match attachments.into_iter().find(|a| {
                a.content_type.starts_with("image/")
                    && !matches!(
                        a.status,
                        AttachmentStatus::Failed | AttachmentStatus::Quarantined
                    )
            }) {
                Some(a) => a.id,
                None => {
                    info!(note_id = %note_id, "No image attachments found for EXIF extraction");
                    return JobResult::Success(Some(serde_json::json!({
                        "status": "skipped",
                        "reason": "No image attachments found"
                    })));
                }
            }
        };

        ctx.report_progress(10, Some("Downloading attachment data..."));

        // Update status to Processing (schema-aware)
        {
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return attachment_processing_job_failure(e, "mark_processing_begin_tx"),
            };
            if let Err(e) = file_storage
                .update_status_tx(&mut tx, attachment_id, AttachmentStatus::Processing, None)
                .await
            {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "mark_attachment_processing",
                    "Failed to update attachment status to Processing"
                );
            }
            if let Err(e) = tx.commit().await {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "commit_attachment_processing_status",
                    "Failed to commit status update"
                );
            }
        }

        // Download the attachment bytes (schema-aware)
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return attachment_processing_job_failure(e, "download_begin_tx"),
        };
        let download_result = file_storage.download_file_tx(&mut tx, attachment_id).await;
        if let Err(e) = tx.commit().await {
            return attachment_processing_job_failure(e, "download_commit");
        }
        let (data, content_type, filename) = match download_result {
            Ok(result) => result,
            Err(e) => {
                // Mark as failed (schema-aware)
                if let Ok(mut tx) = schema_ctx.begin_tx().await {
                    let _ = file_storage
                        .update_status_tx(
                            &mut tx,
                            attachment_id,
                            AttachmentStatus::Failed,
                            Some(ATTACHMENT_PROCESSING_JOB_FAILURE),
                        )
                        .await;
                    let _ = tx.commit().await;
                }
                return attachment_processing_job_failure(e, "download_attachment");
            }
        };

        if !content_type.starts_with("image/") {
            info!(
                attachment_id_present = true,
                content_type_len = diagnostic_len(&content_type),
                detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "skip_exif_non_image_attachment",
                "Attachment is not an image, skipping EXIF extraction"
            );
            return JobResult::Success(Some(serde_json::json!({
                "status": "skipped",
                "reason": "not_image"
            })));
        }

        ctx.report_progress(30, Some("Extracting EXIF metadata..."));

        // Extract EXIF data from the image bytes
        let exif_data = match extract_exif_metadata(&data) {
            Some(data) => data,
            None => {
                info!(
                    attachment_id_present = true,
                    filename_len = diagnostic_len(&filename),
                    detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "skip_exif_no_metadata",
                    "No EXIF data found in image"
                );
                // Still mark as completed — no EXIF is a valid outcome (schema-aware)
                if let Ok(mut tx) = schema_ctx.begin_tx().await {
                    if let Err(e) = file_storage
                        .update_status_tx(&mut tx, attachment_id, AttachmentStatus::Completed, None)
                        .await
                    {
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                            operation = "mark_attachment_completed_no_exif",
                            "Failed to update attachment status to Completed"
                        );
                    }
                    let _ = tx.commit().await;
                }
                return JobResult::Success(Some(serde_json::json!({
                    "status": "completed",
                    "exif_found": false,
                    "filename": filename
                })));
            }
        };

        ctx.report_progress(50, Some("Creating provenance records..."));

        let exif = match exif_data.get("exif") {
            Some(e) => e,
            None => {
                return JobResult::Success(Some(serde_json::json!({
                    "status": "completed",
                    "exif_found": false,
                    "filename": filename
                })));
            }
        };

        // Create provenance records in a schema-scoped transaction
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return attachment_processing_job_failure(e, "provenance_begin_tx"),
        };

        let mut location_id: Option<uuid::Uuid> = None;
        let mut device_id: Option<uuid::Uuid> = None;
        // Create provenance location from GPS data
        if let Some(gps) = exif.get("gps") {
            if let (Some(lat), Some(lon)) = (
                gps.get("latitude").and_then(|v| v.as_f64()),
                gps.get("longitude").and_then(|v| v.as_f64()),
            ) {
                let altitude = gps
                    .get("altitude")
                    .and_then(|v| v.as_f64())
                    .map(|a| a as f32);
                let req = CreateProvLocationRequest {
                    latitude: lat,
                    longitude: lon,
                    altitude_m: altitude,
                    horizontal_accuracy_m: None,
                    vertical_accuracy_m: None,
                    heading_degrees: None,
                    speed_mps: None,
                    named_location_id: None,
                    source: "gps_exif".to_string(),
                    confidence: "high".to_string(),
                };
                match self
                    .db
                    .memory_search
                    .create_prov_location_tx(&mut tx, &req)
                    .await
                {
                    Ok(id) => {
                        info!(location_id = %id, lat, lon, "Created provenance location from EXIF GPS");
                        location_id = Some(id);
                    }
                    Err(e) => {
                        warn!(
                            error_len = diagnostic_len(&e),
                            detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                            operation = "create_exif_gps_location",
                            "Failed to create provenance location from EXIF GPS"
                        );
                    }
                }
            }
        }

        ctx.report_progress(60, Some("Recording device information..."));

        // Create provenance device from camera data
        if let Some(camera) = exif.get("camera") {
            let make = camera
                .get("make")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let model = camera
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let software = camera.get("software").and_then(|v| v.as_str());

            let req = CreateProvDeviceRequest {
                device_make: make.to_string(),
                device_model: model.to_string(),
                device_os: None,
                device_os_version: None,
                software: software.map(|s| s.to_string()),
                software_version: None,
                has_gps: Some(location_id.is_some()),
                has_accelerometer: None,
                sensor_metadata: Some(serde_json::json!({
                    "lens": exif.get("lens"),
                    "settings": exif.get("settings"),
                })),
                device_name: None,
            };
            match self
                .db
                .memory_search
                .create_prov_agent_device_tx(&mut tx, &req)
                .await
            {
                Ok(device) => {
                    info!(device_id = %device.id, make, model, "Created/updated provenance device from EXIF");
                    device_id = Some(device.id);
                }
                Err(e) => {
                    warn!(
                        error_len = diagnostic_len(&e),
                        detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                        operation = "create_exif_device",
                        "Failed to create provenance device from EXIF"
                    );
                }
            }
        }

        ctx.report_progress(70, Some("Processing capture time..."));

        // Parse EXIF datetime
        let capture_time = parse_exif_datetime(exif);

        ctx.report_progress(80, Some("Creating file provenance record..."));

        // Create the file provenance record linking everything together
        let event_type = if content_type.contains("video") {
            "video"
        } else {
            "photo"
        };

        let prov_req = CreateFileProvenanceRequest {
            attachment_id,
            note_id: Some(note_id),
            capture_time_start: capture_time,
            capture_time_end: capture_time,
            capture_timezone: None,
            capture_duration_seconds: None,
            time_source: if capture_time.is_some() {
                Some("exif".to_string())
            } else {
                None
            },
            time_confidence: if capture_time.is_some() {
                Some("high".to_string())
            } else {
                None
            },
            location_id,
            device_id,
            event_type: Some(event_type.to_string()),
            event_title: Some(filename.clone()),
            event_description: None,
            raw_metadata: Some(exif.clone()),
        };

        let provenance_id = match self
            .db
            .memory_search
            .create_file_provenance_tx(&mut tx, &prov_req)
            .await
        {
            Ok(id) => {
                info!(provenance_id = %id, "Created file provenance record from EXIF");
                Some(id)
            }
            Err(e) => {
                warn!(
                    error_len = diagnostic_len(&e),
                    detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                    operation = "create_exif_file_provenance",
                    "Failed to create file provenance record"
                );
                None
            }
        };

        ctx.report_progress(90, Some("Updating attachment metadata..."));

        // Persist extracted EXIF metadata on the attachment (schema-aware)
        // Store the unwrapped exif content directly (without the "exif" wrapper)
        // so fields are accessible as extracted_metadata.gps.latitude, etc.
        let metadata = prepare_attachment_metadata(&exif_data, capture_time)
            .unwrap_or_else(|| exif_data.clone());
        if let Err(e) = file_storage
            .update_extracted_content_tx(&mut tx, attachment_id, None, Some(metadata))
            .await
        {
            warn!(
                error_len = diagnostic_len(&e),
                detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "update_exif_extracted_metadata",
                "Failed to update attachment extracted metadata"
            );
        }

        // Mark attachment as completed
        if let Err(e) = file_storage
            .update_status_tx(&mut tx, attachment_id, AttachmentStatus::Completed, None)
            .await
        {
            warn!(
                error_len = diagnostic_len(&e),
                detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "mark_attachment_completed_with_exif",
                "Failed to update attachment status to Completed"
            );
        }

        // Commit all provenance and attachment updates
        if let Err(e) = tx.commit().await {
            warn!(
                error_len = diagnostic_len(&e),
                detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
                operation = "commit_exif_extraction_results",
                "Failed to commit EXIF extraction results"
            );
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        info!(
            note_id_present = true,
            attachment_id_present = true,
            filename_len = diagnostic_len(&filename),
            has_gps = location_id.is_some(),
            has_device = device_id.is_some(),
            has_capture_time = capture_time.is_some(),
            provenance_id_present = provenance_id.is_some(),
            duration_ms,
            detail = JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
            operation = "complete_exif_extraction",
            "EXIF extraction completed"
        );

        ctx.report_progress(100, Some("EXIF extraction complete"));

        JobResult::Success(Some(serde_json::json!({
            "status": "completed",
            "exif_found": true,
            "filename": filename,
            "attachment_id": attachment_id.to_string(),
            "location_id": location_id.map(|id| id.to_string()),
            "device_id": device_id.map(|id| id.to_string()),
            "provenance_id": provenance_id.map(|id| id.to_string()),
            "has_gps": location_id.is_some(),
            "has_device": device_id.is_some(),
            "has_capture_time": capture_time.is_some(),
            "duration_ms": duration_ms,
        })))
    }
}

// =============================================================================
// GRAPH MAINTENANCE HANDLER (#482)
// =============================================================================

/// Graph maintenance pipeline handler.
///
/// Runs the following steps in order:
/// 1. Edge weight normalization (#470)
/// 2. SNN scoring (#474) — prune below threshold
/// 3. PFNET sparsification (#476) — prune geometrically redundant edges
/// 4. Louvain community detection (#473) — recompute community assignments
/// 5. Save diagnostics snapshot for before/after comparison
pub struct GraphMaintenanceHandler {
    db: Database,
}

impl GraphMaintenanceHandler {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for GraphMaintenanceHandler {
    fn job_type(&self) -> JobType {
        JobType::GraphMaintenance
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "graph_maintenance", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        ctx.report_progress(5, Some("Starting graph maintenance pipeline..."));

        let schema = extract_schema(&ctx);
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        let graph_config = matric_core::defaults::GraphConfig::from_env();
        let links = matric_db::PgLinkRepository::new(self.db.pool.clone());

        // Parse optional steps from payload (default: all steps).
        let steps: Vec<String> = ctx
            .payload()
            .and_then(|p| p.get("steps"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    "normalize".to_string(),
                    "snn".to_string(),
                    "pfnet".to_string(),
                    "snapshot".to_string(),
                ]
            });

        let mut results = serde_json::Map::new();

        // Step 1: Normalization is applied during graph traversal (#470) — just log.
        if steps.iter().any(|s| s == "normalize") {
            ctx.report_progress(
                15,
                Some("Step 1/4: Edge normalization (applied at query time)"),
            );
            results.insert(
                "normalize".to_string(),
                serde_json::json!({
                    "status": "ok",
                    "note": "Normalization is applied during graph traversal via apply_edge_normalization",
                    "gamma": graph_config.normalization_gamma,
                }),
            );
        }

        // Step 2: SNN scoring.
        if steps.iter().any(|s| s == "snn") {
            ctx.report_progress(30, Some("Step 2/4: Recomputing SNN scores..."));
            let links_clone = matric_db::PgLinkRepository::new(self.db.pool.clone());
            let threshold = graph_config.snn_threshold;
            let snn_result = schema_ctx
                .query(move |tx| {
                    Box::pin(async move {
                        let n = links_clone.count_notes_tx(tx).await?;
                        let k = if n > 0 {
                            ((n as f64).log2().round() as usize)
                                .clamp(graph_config.k_min, graph_config.k_max)
                        } else {
                            graph_config.k_min
                        };
                        links_clone
                            .recompute_snn_scores_tx(tx, k, threshold, false)
                            .await
                    })
                })
                .await;

            match snn_result {
                Ok(snn) => {
                    info!(
                        updated = snn.updated,
                        pruned = snn.pruned,
                        k = snn.k_used,
                        "SNN scoring complete"
                    );
                    results.insert(
                        "snn".to_string(),
                        serde_json::to_value(&snn).unwrap_or_default(),
                    );
                }
                Err(e) => {
                    results.insert("snn".to_string(), graph_maintenance_step_failure(e, "snn"));
                }
            }
        }

        // Step 3: PFNET sparsification.
        if steps.iter().any(|s| s == "pfnet") {
            ctx.report_progress(55, Some("Step 3/4: PFNET sparsification..."));
            let links_clone = matric_db::PgLinkRepository::new(self.db.pool.clone());
            let q = graph_config.pfnet_q;
            let pfnet_result = schema_ctx
                .query(move |tx| {
                    Box::pin(async move { links_clone.pfnet_sparsify_tx(tx, q, false).await })
                })
                .await;

            match pfnet_result {
                Ok(pfnet) => {
                    info!(
                        retained = pfnet.retained,
                        pruned = pfnet.pruned,
                        retention_ratio = pfnet.retention_ratio,
                        "PFNET sparsification complete"
                    );
                    results.insert(
                        "pfnet".to_string(),
                        serde_json::to_value(&pfnet).unwrap_or_default(),
                    );
                }
                Err(e) => {
                    results.insert(
                        "pfnet".to_string(),
                        graph_maintenance_step_failure(e, "pfnet"),
                    );
                }
            }
        }

        // Step 4: Save diagnostics snapshot for before/after comparison.
        if steps.iter().any(|s| s == "snapshot") {
            ctx.report_progress(80, Some("Step 4/4: Saving diagnostics snapshot..."));
            let links_clone = matric_db::PgLinkRepository::new(self.db.pool.clone());
            let snapshot_result = schema_ctx
                .query(move |tx| {
                    Box::pin(async move {
                        let diag = links_clone.graph_diagnostics_tx(tx, 500).await?;
                        links_clone
                            .save_diagnostics_snapshot_tx(tx, "post_maintenance", &diag)
                            .await
                    })
                })
                .await;

            match snapshot_result {
                Ok(snap_id) => {
                    results.insert(
                        "snapshot".to_string(),
                        serde_json::json!({ "status": "ok", "snapshot_id": snap_id }),
                    );
                }
                Err(e) => {
                    results.insert(
                        "snapshot".to_string(),
                        graph_maintenance_step_failure(e, "snapshot"),
                    );
                }
            }
        }

        let _ = links; // suppress unused warning
        let duration_ms = start.elapsed().as_millis() as u64;
        ctx.report_progress(100, Some("Graph maintenance complete"));
        info!(duration_ms, schema, "Graph maintenance pipeline completed");

        JobResult::Success(Some(serde_json::json!({
            "schema": schema,
            "duration_ms": duration_ms,
            "steps": results,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_db::embeddings::utils::chunk_text;

    #[test]
    fn test_clean_enhanced_content_removes_system_prompt() {
        let prompt = r#"You are a formatting assistant. Your task is to improve the structure and readability of the following note.

Original Note:
Test content here

Output the formatted note."#;

        let leaked_response = r#"You are a formatting assistant. Your task is to improve the structure and readability of the following note.

# Test Content

This is the actual revised note content that should be preserved."#;

        let cleaned = clean_enhanced_content(leaked_response, prompt);

        // Should remove the system prompt and keep only the content
        assert!(!cleaned.contains("You are a formatting assistant"));
        assert!(!cleaned.contains("Your task is to improve"));
        assert!(cleaned.contains("# Test Content"));
        assert!(cleaned.contains("This is the actual revised note"));
    }

    #[test]
    fn test_clean_enhanced_content_removes_light_mode_instructions() {
        let prompt = r#"You are a formatting assistant. STRICT RULES - You MUST follow these."#;

        let leaked_response = r#"You are a formatting assistant. Your task is to improve the structure and readability of the following note WITHOUT adding any new information.

## My Note Title

This is the actual content of the note that should remain."#;

        let cleaned = clean_enhanced_content(leaked_response, prompt);

        // Should remove all instruction text
        assert!(!cleaned.contains("You are a formatting assistant"));
        assert!(!cleaned.contains("Your task is to improve"));

        // Should preserve actual content
        assert!(cleaned.contains("## My Note Title"));
        assert!(cleaned.contains("This is the actual content"));
    }

    #[test]
    fn test_clean_enhanced_content_preserves_clean_response() {
        let prompt = "Test prompt";

        let clean_response = r#"# My Enhanced Note

This is properly formatted content without any system prompt leakage.

- Point 1
- Point 2

## Section

More content here."#;

        let cleaned = clean_enhanced_content(clean_response, prompt);

        // Should preserve all content when there's no leakage
        assert_eq!(cleaned, clean_response.trim());
    }

    #[test]
    fn test_clean_enhanced_content_removes_markers() {
        let prompt = "Test prompt";

        let response_with_markers = r#"ENHANCED NOTE

# My Note

Content here."#;

        let cleaned = clean_enhanced_content(response_with_markers, prompt);

        // Should remove the marker
        assert!(!cleaned.starts_with("ENHANCED NOTE"));
        assert!(cleaned.starts_with("# My Note"));
    }

    #[test]
    fn test_clean_enhanced_content_handles_original_note_marker() {
        let prompt = r#"You are a formatting assistant.

Original Note:
Some original text

Output the formatted note."#;

        let leaked_response = r#"You are a formatting assistant.

# Formatted Version

The actual formatted content that is different from the original."#;

        let cleaned = clean_enhanced_content(leaked_response, prompt);

        // Should remove prompt instructions
        assert!(!cleaned.contains("You are a formatting assistant"));
        assert!(cleaned.contains("# Formatted Version"));
        assert!(cleaned.contains("The actual formatted content"));
    }

    #[test]
    fn test_clean_enhanced_content_case_insensitive() {
        let prompt = "Test";

        let response = r#"YOU ARE AN INTELLIGENT NOTE-TAKING ASSISTANT

# Content

Actual note content."#;

        let cleaned = clean_enhanced_content(response, prompt);

        // Should detect and remove uppercase version of prompt indicator
        assert!(!cleaned.to_lowercase().contains("you are an intelligent"));
        assert!(cleaned.contains("# Content"));
    }

    #[test]
    fn test_clean_enhanced_content_complete_light_mode_example() {
        // This simulates a realistic bug scenario from ticket #83
        let prompt = r#"You are a formatting assistant. Your task is to improve the structure and readability of the following note WITHOUT adding any new information.

Original Note:
Quick note about the meeting

STRICT RULES - You MUST follow these:
1. DO NOT add any technical details
2. DO NOT invent, expand, or elaborate on topics

What you MAY do:
- Fix grammar and spelling errors
- Add markdown headers

Output the formatted note. Do not add any labels, markers, or metadata."#;

        // Simulated AI response with prompt leakage
        let leaked_response = r#"You are a formatting assistant. Your task is to improve the structure and readability of the following note WITHOUT adding any new information.

## Meeting Notes

Quick note about the meeting discussion and action items."#;

        let cleaned = clean_enhanced_content(leaked_response, prompt);

        // The system prompt should be completely removed
        assert!(!cleaned.contains("You are a formatting assistant"));
        assert!(!cleaned.contains("Your task is to improve"));
        assert!(!cleaned.contains("WITHOUT adding any new information"));

        // Only the actual formatted content should remain
        assert!(cleaned.starts_with("## Meeting Notes"));
        assert!(cleaned.contains("Quick note about the meeting"));
    }

    #[test]
    fn test_chunk_text_basic() {
        let text = "Line 1\nLine 2\nLine 3";
        let chunks = chunk_text(text, 20);

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_empty() {
        let text = "";
        let chunks = chunk_text(text, 100);

        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_respects_max_len() {
        let text = "Short line\n".repeat(100);
        let chunks = chunk_text(&text, 50);

        // Each chunk should respect the max length
        for chunk in chunks {
            assert!(chunk.len() <= 100); // Some overhead for line breaks
        }
    }

    // =========================================================================
    // HNSW Algorithm 4 / Graph Linking Tests
    // =========================================================================

    fn make_vec(vals: &[f32]) -> pgvector::Vector {
        pgvector::Vector::from(vals.to_vec())
    }

    fn make_hit(note_id: uuid::Uuid, score: f32) -> matric_core::SearchHit {
        matric_core::SearchHit {
            note_id,
            score,
            snippet: Some(String::new()),
            title: None,
            tags: vec![],
            embedding_status: None,
        }
    }

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let v = make_vec(&[1.0, 0.0, 0.0]);
        let sim = LinkingHandler::cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        let a = make_vec(&[1.0, 0.0, 0.0]);
        let b = make_vec(&[0.0, 1.0, 0.0]);
        let sim = LinkingHandler::cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        let a = make_vec(&[1.0, 0.0]);
        let b = make_vec(&[-1.0, 0.0]);
        let sim = LinkingHandler::cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = make_vec(&[1.0, 0.0]);
        let b = make_vec(&[0.0, 0.0]);
        let sim = LinkingHandler::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_heuristic_empty_candidates() {
        let source = make_vec(&[1.0, 0.0, 0.0]);
        let result = LinkingHandler::select_neighbors_heuristic(&source, vec![], 5, false, true);
        assert!(result.is_empty());
    }

    #[test]
    fn test_heuristic_single_candidate_accepted() {
        let source = make_vec(&[1.0, 0.0, 0.0]);
        let id = uuid::Uuid::new_v4();
        let candidates = vec![(make_hit(id, 0.9), make_vec(&[0.9, 0.1, 0.0]))];

        let result =
            LinkingHandler::select_neighbors_heuristic(&source, candidates, 5, false, true);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.note_id, id);
    }

    #[test]
    fn test_heuristic_respects_m_limit() {
        let source = make_vec(&[1.0, 0.0, 0.0]);
        // Create 10 diverse candidates spread across different directions
        let candidates: Vec<_> = (0..10)
            .map(|i| {
                let angle = (i as f32) * std::f32::consts::PI / 10.0;
                let id = uuid::Uuid::new_v4();
                (
                    make_hit(id, 0.9 - (i as f32) * 0.02),
                    make_vec(&[angle.cos(), angle.sin(), 0.0]),
                )
            })
            .collect();

        let result = LinkingHandler::select_neighbors_heuristic(
            &source, candidates, 3, // Only accept 3
            false, false, // No keep_pruned
        );
        assert!(result.len() <= 3);
    }

    #[test]
    fn test_heuristic_diversity_rejects_clustered_candidates() {
        // Source at [1, 0, 0]
        let source = make_vec(&[1.0, 0.0, 0.0]);

        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();

        // Two candidates nearly identical direction from source.
        // id1 accepted first. id2 has sim(id2,id1) >> sim(id2,source),
        // so Algorithm 4 rejects it.
        let candidates = vec![
            (make_hit(id1, 0.99), make_vec(&[0.95, 0.05, 0.0])),
            (make_hit(id2, 0.98), make_vec(&[0.93, 0.07, 0.0])),
        ];

        let result = LinkingHandler::select_neighbors_heuristic(
            &source, candidates, 3, false, false, // No keep_pruned
        );

        // id1 should be accepted (first candidate, always passes)
        assert_eq!(result[0].0.note_id, id1);

        // id2 should be rejected: sim(id2, id1) ≈ 0.9998 >> sim(id2, source) ≈ 0.9546
        assert_eq!(result.len(), 1, "Clustered candidate should be rejected")
    }

    #[test]
    fn test_heuristic_keep_pruned_fills_remaining_slots() {
        let source = make_vec(&[1.0, 0.0, 0.0]);

        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();
        let id3 = uuid::Uuid::new_v4();

        // id1 accepted, id2 rejected by heuristic (same direction), id3 different
        let candidates = vec![
            (make_hit(id1, 0.99), make_vec(&[0.95, 0.05, 0.0])),
            (make_hit(id2, 0.98), make_vec(&[0.93, 0.07, 0.0])),
            (make_hit(id3, 0.80), make_vec(&[0.5, 0.5, 0.0])),
        ];

        let result_with_pruned = LinkingHandler::select_neighbors_heuristic(
            &source,
            candidates.clone(),
            5, // M=5, more than candidates available
            false,
            true, // keep_pruned = true
        );

        // With keep_pruned, all 3 should be returned (accepted + pruned)
        assert_eq!(result_with_pruned.len(), 3);

        let result_without_pruned = LinkingHandler::select_neighbors_heuristic(
            &source, candidates, 5, false, false, // keep_pruned = false
        );

        // Without keep_pruned, only diverse neighbors returned
        assert!(result_without_pruned.len() <= result_with_pruned.len());
    }

    #[test]
    fn test_heuristic_diverse_directions_accepted() {
        // Source along x-axis (unit vector)
        let source = make_vec(&[1.0, 0.0, 0.0]);

        // 15° cone: all candidates have cos(15°) ≈ 0.966 similarity to source.
        // Pairwise angles between candidates are 27-30°, so each candidate
        // is closer to source than to any accepted neighbor → all pass Algorithm 4.
        let theta = 15.0_f32.to_radians();
        let c = theta.cos(); // ≈ 0.9659
        let s = theta.sin(); // ≈ 0.2588

        let id_a = uuid::Uuid::new_v4();
        let id_b = uuid::Uuid::new_v4();
        let id_c = uuid::Uuid::new_v4();

        // Slightly decreasing scores for deterministic processing order
        let mut candidates = vec![
            (make_hit(id_a, c - 0.0001), make_vec(&[c, s, 0.0])), // +y direction
            (make_hit(id_b, c - 0.0002), make_vec(&[c, -s, 0.0])), // -y direction
            (make_hit(id_c, c - 0.0003), make_vec(&[c, 0.0, s])), // +z direction
        ];
        candidates.sort_by(|a, b| b.0.score.partial_cmp(&a.0.score).unwrap());

        let result =
            LinkingHandler::select_neighbors_heuristic(&source, candidates, 3, false, false);

        // All 3 diverse candidates should be accepted
        assert_eq!(
            result.len(),
            3,
            "All diverse candidates should pass Algorithm 4"
        );

        // Verify identities and acceptance order
        let selected: Vec<_> = result.iter().map(|(h, _)| h.note_id).collect();
        assert_eq!(selected, vec![id_a, id_b, id_c]);
    }

    #[test]
    fn test_heuristic_max_neighbors_one() {
        // With m=1, should return exactly the top candidate regardless of diversity
        let source = make_vec(&[1.0, 0.0, 0.0]);
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();

        let candidates = vec![
            (make_hit(id1, 0.95), make_vec(&[0.95, 0.31, 0.0])),
            (make_hit(id2, 0.90), make_vec(&[0.90, 0.0, 0.44])),
        ];

        let result =
            LinkingHandler::select_neighbors_heuristic(&source, candidates, 1, false, false);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.note_id, id1);
    }

    #[test]
    fn test_parse_wiki_links() {
        assert_eq!(
            LinkingHandler::parse_wiki_links("See [[My Note]] for details"),
            vec!["My Note"]
        );
        assert_eq!(
            LinkingHandler::parse_wiki_links("[[A]] and [[B]]"),
            vec!["A", "B"]
        );
        assert_eq!(
            LinkingHandler::parse_wiki_links("No links here"),
            Vec::<String>::new()
        );
        assert_eq!(
            LinkingHandler::parse_wiki_links("Empty [[]] should be filtered"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn test_chunk_for_extraction_small() {
        let small = "Short note about PostgreSQL migration.";
        let chunk_size = matric_core::defaults::EXTRACTION_CHUNK_SIZE_FALLBACK;
        let chunks = chunk_for_extraction(small, chunk_size);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], small);
    }

    #[test]
    fn test_chunk_for_extraction_large() {
        // Content larger than chunk size should be split
        let chunk_size = 3000; // Use small chunk size to force splitting
        let large = "a ".repeat(chunk_size + 1000);
        let chunks = chunk_for_extraction(&large, chunk_size);
        assert!(
            chunks.len() > 1,
            "Large content should produce multiple chunks"
        );
        // Each chunk should not exceed the max size (with some tolerance for overlap)
        for chunk in &chunks {
            assert!(
                chunk.len() <= chunk_size + 500,
                "Chunk too large: {} chars",
                chunk.len()
            );
        }
    }

    #[test]
    fn test_extraction_chunk_size_fallback() {
        // Without a backend, should return the fallback constant
        let size = extraction_chunk_size(None);
        assert_eq!(size, matric_core::defaults::EXTRACTION_CHUNK_SIZE_FALLBACK);
    }

    #[test]
    fn test_merge_json_arrays_dedup() {
        let results = vec![
            r#"["science/ml", "tool/pytorch"]"#.to_string(),
            r#"["science/ml", "tool/tensorflow"]"#.to_string(),
        ];
        let merged = merge_json_arrays(results);
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&"science/ml".to_string()));
        assert!(merged.contains(&"tool/pytorch".to_string()));
        assert!(merged.contains(&"tool/tensorflow".to_string()));
    }

    #[test]
    fn test_merge_json_arrays_empty() {
        let results = vec![r#"[]"#.to_string(), r#"[]"#.to_string()];
        let merged = merge_json_arrays(results);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_json_arrays_case_insensitive_dedup() {
        let results = vec![
            r#"["Science/ML"]"#.to_string(),
            r#"["science/ml"]"#.to_string(),
        ];
        let merged = merge_json_arrays(results);
        // Should deduplicate case-insensitively, keeping the first occurrence
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], "Science/ML");
    }

    // =========================================================================
    // Chunked Revision Tests
    // =========================================================================

    #[test]
    fn test_revision_chunk_size_fallback() {
        // revision_chunk_size requires an OllamaBackend which we can't construct
        // in unit tests without env, so test the fallback constant directly.
        assert_eq!(matric_core::defaults::REVISION_CHUNK_SIZE_FALLBACK, 40_000);
        assert_eq!(matric_core::defaults::REVISION_CHUNK_SIZE_MIN, 8_000);
        assert_eq!(matric_core::defaults::REVISION_PROMPT_OVERHEAD, 2_000);
    }

    #[test]
    fn test_revision_chunk_size_formula() {
        // Verify the formula: (native_context * 4 - overhead) / 2
        // For gpt-oss:20b (98_376 tokens):
        //   (98_376 * 4 - 2_000) / 2 = (393_504 - 2_000) / 2 = 195_752
        // Capped at 200_000, floored at 8_000 → 195_752
        let context_chars = 98_376_usize * 4;
        let available =
            context_chars.saturating_sub(matric_core::defaults::REVISION_PROMPT_OVERHEAD);
        let size = (available / 2).clamp(matric_core::defaults::REVISION_CHUNK_SIZE_MIN, 200_000);
        assert_eq!(size, 195_752);

        // For qwen3:8b (40_960 tokens):
        //   (40_960 * 4 - 2_000) / 2 = (163_840 - 2_000) / 2 = 80_920
        let context_chars = 40_960_usize * 4;
        let available =
            context_chars.saturating_sub(matric_core::defaults::REVISION_PROMPT_OVERHEAD);
        let size = (available / 2).clamp(matric_core::defaults::REVISION_CHUNK_SIZE_MIN, 200_000);
        assert_eq!(size, 80_920);

        // For gemma2:9b (8_192 tokens):
        //   (8_192 * 4 - 2_000) / 2 = (32_768 - 2_000) / 2 = 15_384
        let context_chars = 8_192_usize * 4;
        let available =
            context_chars.saturating_sub(matric_core::defaults::REVISION_PROMPT_OVERHEAD);
        let size = (available / 2).clamp(matric_core::defaults::REVISION_CHUNK_SIZE_MIN, 200_000);
        assert_eq!(size, 15_384);
    }

    #[test]
    fn test_revision_chunk_size_floor() {
        // Tiny context should be clamped to REVISION_CHUNK_SIZE_MIN
        let context_chars = 1_000_usize * 4; // 4000 chars
        let available =
            context_chars.saturating_sub(matric_core::defaults::REVISION_PROMPT_OVERHEAD);
        let size = (available / 2).clamp(matric_core::defaults::REVISION_CHUNK_SIZE_MIN, 200_000);
        assert_eq!(size, matric_core::defaults::REVISION_CHUNK_SIZE_MIN);
    }

    #[test]
    fn test_chunk_for_revision_small_content() {
        let small = "A short note about Rust programming.";
        let chunks = chunk_for_revision(small, 40_000, 0);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], small);
    }

    #[test]
    fn test_chunk_for_revision_preserves_all_content() {
        // Generate content larger than chunk size
        let chunk_size = 2_000;
        let paragraphs: Vec<String> = (0..20)
            .map(|i| format!("Paragraph {} with enough content to be meaningful. This discusses topic {} in detail with supporting evidence and examples that span multiple sentences to ensure adequate length.\n\n", i, i))
            .collect();
        let content = paragraphs.join("");
        assert!(content.len() > chunk_size);

        let chunks = chunk_for_revision(&content, chunk_size, 0);
        assert!(chunks.len() > 1, "Should produce multiple chunks");

        // All original content should be present in the concatenated chunks
        let reconstructed = chunks.join("");
        // SemanticChunker may trim whitespace at boundaries, so compare trimmed content
        let original_words: Vec<&str> = content.split_whitespace().collect();
        let reconstructed_words: Vec<&str> = reconstructed.split_whitespace().collect();
        // Every word from the original should appear in the reconstruction
        for word in &original_words {
            assert!(reconstructed_words.contains(word), "Missing word: {}", word);
        }
    }

    #[test]
    fn test_chunk_for_revision_no_overlap() {
        // With overlap=0, chunks should not have overlapping text
        let chunk_size = 3_000;
        let content = (0..50)
            .map(|i| format!("UNIQUE_MARKER_{} some filler text here.\n\n", i))
            .collect::<String>();

        let chunks = chunk_for_revision(&content, chunk_size, 0);
        if chunks.len() > 1 {
            // Count unique markers across all chunks
            let mut marker_count = 0;
            for chunk in &chunks {
                for i in 0..50 {
                    if chunk.contains(&format!("UNIQUE_MARKER_{} ", i)) {
                        marker_count += 1;
                    }
                }
            }
            // Each marker should appear exactly once (no overlap)
            assert_eq!(
                marker_count, 50,
                "Markers should not be duplicated across chunks"
            );
        }
    }

    #[test]
    fn test_chunk_video_timeline_small_content() {
        let small = "### Scene 1\nSome scene content.\n\n### Scene 2\nMore content.";
        let chunks = chunk_video_timeline(small, 100_000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], small);
    }

    #[test]
    fn test_chunk_video_timeline_scene_boundaries() {
        let header = "**Duration**: 90:00\n**Frames**: 60\n\n";
        let scenes: Vec<String> = (1..=10)
            .map(|i| {
                format!(
                    "### Scene {}\n**Timestamp**: {}:00\nThe camera shows a wide shot of the landscape. \
                     A narrator explains the history of this location in great detail, providing context \
                     and background information that spans several sentences to make this scene substantial.\n\n",
                    i, i * 9
                )
            })
            .collect();
        let content = format!("{}{}", header, scenes.join(""));

        // Use a chunk size that fits ~3 scenes per chunk
        let single_scene_size = scenes[0].len();
        let chunk_max = single_scene_size * 3 + header.len();

        let chunks = chunk_video_timeline(&content, chunk_max);
        assert!(
            chunks.len() > 1,
            "Should produce multiple chunks for 10 scenes"
        );

        // First chunk should contain the header
        assert!(
            chunks[0].contains("**Duration**:"),
            "First chunk should contain the metadata header"
        );

        // Every scene should appear in exactly one chunk
        for i in 1..=10 {
            let marker = format!("### Scene {}\n", i);
            let count = chunks.iter().filter(|c| c.contains(&marker)).count();
            assert_eq!(
                count, 1,
                "Scene {} should appear in exactly one chunk, found {}",
                i, count
            );
        }

        // No scene should be split mid-content
        for chunk in &chunks {
            if chunk.contains("### Scene ") {
                // Count scene starts in this chunk
                let scene_starts: Vec<_> = chunk.match_indices("### Scene ").collect();
                for (pos, _) in &scene_starts {
                    // Each scene marker should have content after it
                    let after = &chunk[*pos..];
                    assert!(
                        after.contains("Timestamp"),
                        "Scene marker without content — scene was split"
                    );
                }
            }
        }
    }

    #[test]
    fn test_chunk_video_timeline_preserves_header() {
        let content = "**Duration**: 120:00\n**Frames**: 90\n\n\
                       ### Scene 1\nFirst scene.\n\n\
                       ### Scene 2\nSecond scene.\n\n\
                       ### Scene 3\nThird scene.\n\n";

        // Chunk size that forces splitting
        let chunks = chunk_video_timeline(content, 60);
        assert!(chunks.len() > 1);
        // Header should be in the first chunk
        assert!(chunks[0].contains("**Duration**:"));
        assert!(chunks[0].contains("**Frames**:"));
    }

    #[test]
    fn test_chunk_video_timeline_oversized_scene() {
        // A single scene larger than the chunk budget should get its own chunk
        let large_scene = format!(
            "### Scene 1\n{}\n\n### Scene 2\nSmall scene.\n\n",
            "x".repeat(10_000)
        );
        let chunks = chunk_video_timeline(&large_scene, 5_000);
        assert!(chunks.len() >= 2, "Oversized scene should be its own chunk");
    }

    #[test]
    fn test_chunk_video_timeline_no_scenes_fallback() {
        // Content without scene markers should fall through to chunk_for_revision
        let content = "A ".repeat(5_000);
        let chunks = chunk_video_timeline(&content, 3_000);
        assert!(
            chunks.len() > 1,
            "Should fall back to generic chunking when no scenes found"
        );
    }

    #[test]
    fn test_phase2_reference_overhead_budget() {
        // Simulate Phase 2 chunk budget calculation
        let base_chunk_size = 80_000_usize; // ~qwen3:8b
        let reference_context = "- Related note snippet one\n- Related note snippet two\n";
        let reference_overhead = reference_context.len();

        let chunk_max_phase2 = base_chunk_size
            .saturating_sub(reference_overhead / 2)
            .max(matric_core::defaults::REVISION_CHUNK_SIZE_MIN);

        // Reference overhead should reduce the chunk budget
        assert!(chunk_max_phase2 < base_chunk_size);
        assert!(chunk_max_phase2 >= matric_core::defaults::REVISION_CHUNK_SIZE_MIN);
    }

    #[test]
    fn test_phase2_reference_overhead_floor() {
        // Very large reference context should still respect the minimum chunk size
        let base_chunk_size = 10_000_usize;
        let reference_overhead = 50_000; // Huge reference context

        let chunk_max_phase2 = base_chunk_size
            .saturating_sub(reference_overhead / 2)
            .max(matric_core::defaults::REVISION_CHUNK_SIZE_MIN);

        assert_eq!(
            chunk_max_phase2,
            matric_core::defaults::REVISION_CHUNK_SIZE_MIN
        );
    }

    // =========================================================================
    // Adaptive Timeout Tests
    // =========================================================================

    #[test]
    fn test_adaptive_timeout_short_content() {
        // Short content should use the base timeout
        let timeout = adaptive_timeout_secs(1_000, 120);
        assert_eq!(timeout, 120, "Short content should use base timeout");
    }

    #[test]
    fn test_adaptive_timeout_medium_content() {
        // 20K chars at 3ms/char = 60s, which is below base 120s so base wins
        let timeout = adaptive_timeout_secs(20_000, 120);
        assert_eq!(timeout, 120);
    }

    #[test]
    fn test_adaptive_timeout_large_content() {
        // 200K chars at 3ms/char = 600s
        let timeout = adaptive_timeout_secs(200_000, 120);
        assert_eq!(timeout, 600);
    }

    #[test]
    fn test_adaptive_timeout_capped_at_max() {
        // Very large content should be capped at GEN_TIMEOUT_MAX_SECS (900)
        let timeout = adaptive_timeout_secs(500_000, 120);
        assert_eq!(timeout, matric_core::defaults::GEN_TIMEOUT_MAX_SECS);
    }

    #[test]
    fn test_adaptive_timeout_respects_minimum() {
        // Even with 0 content, should return at least GEN_TIMEOUT_MIN_SECS
        let timeout = adaptive_timeout_secs(0, 30);
        assert_eq!(timeout, matric_core::defaults::GEN_TIMEOUT_MIN_SECS);
    }

    // =========================================================================
    // Type-Aware Revision Prompt Builder Tests
    // =========================================================================

    /// Build a minimal DocumentType for testing prompt generation.
    fn make_doc_type(
        name: &str,
        category: matric_core::DocumentCategory,
        required_sections: Vec<&str>,
        generation_prompt: Option<&str>,
    ) -> matric_core::DocumentType {
        matric_core::DocumentType {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            display_name: name.to_string(),
            category,
            description: None,
            file_extensions: vec![],
            mime_types: vec![],
            magic_patterns: vec![],
            filename_patterns: vec![],
            chunking_strategy: matric_core::ChunkingStrategy::Semantic,
            chunk_size_default: 512,
            chunk_overlap_default: 50,
            preserve_boundaries: true,
            chunking_config: serde_json::json!({}),
            recommended_config_id: None,
            content_types: vec![],
            tree_sitter_language: None,
            extraction_strategy: Default::default(),
            extraction_config: serde_json::json!({}),
            requires_attachment: false,
            attachment_generates_content: false,
            is_system: true,
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: None,
            agentic_config: matric_core::AgenticConfig {
                generation_prompt: generation_prompt.map(|s| s.to_string()),
                required_sections: required_sections
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
                optional_sections: vec![],
                template_id: None,
                context_requirements: Default::default(),
                validation_rules: Default::default(),
                agent_hints: Default::default(),
                revision_chunking: None,
            },
        }
    }

    #[test]
    fn test_build_prompt_with_no_doc_type() {
        let prompt = build_type_aware_prompt(
            None,
            RevisionMode::Standard,
            "Some content",
            "",
            0,
            1,
            false,
        );
        // Falls back to generic prompt with summary
        assert!(prompt.contains("intelligent note-taking assistant"));
        assert!(prompt.contains("## Summary"));
        assert!(prompt.contains("Some content"));
    }

    #[test]
    fn test_build_prompt_includes_summary_first_chunk() {
        let dt = make_doc_type(
            "meeting-recording",
            matric_core::DocumentCategory::Communication,
            vec!["Summary", "Decisions", "Action Items"],
            Some("You are a meeting analyst."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "Meeting content here",
            "",
            0, // first chunk
            3,
            false,
        );
        assert!(prompt.contains("## Summary"));
    }

    #[test]
    fn test_build_prompt_no_summary_subsequent_chunks() {
        let dt = make_doc_type(
            "meeting-recording",
            matric_core::DocumentCategory::Communication,
            vec!["Summary", "Decisions", "Action Items"],
            Some("You are a meeting analyst."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "More content",
            "",
            1, // second chunk
            3,
            false,
        );
        assert!(
            !prompt.contains("## Summary"),
            "Summary should not appear on subsequent chunks"
        );
    }

    #[test]
    fn test_build_prompt_light_mode_no_summary() {
        let dt = make_doc_type(
            "meeting-recording",
            matric_core::DocumentCategory::Communication,
            vec!["Summary", "Decisions"],
            Some("You are a meeting analyst."),
        );
        let prompt =
            build_type_aware_prompt(Some(&dt), RevisionMode::Light, "Content", "", 0, 1, false);
        // Light mode should NOT have summary and should be formatting-only
        assert!(
            !prompt.contains("## Summary"),
            "Light mode should not add summary"
        );
        assert!(prompt.contains("formatting assistant"));
        // Should NOT contain meeting analyst role
        assert!(
            !prompt.contains("meeting analyst"),
            "Light mode should not use type-specific role"
        );
    }

    #[test]
    fn test_build_prompt_meeting_sections() {
        let dt = make_doc_type(
            "meeting-recording",
            matric_core::DocumentCategory::Communication,
            vec![
                "Summary",
                "Attendees",
                "Decisions",
                "Action Items",
                "Discussion Points",
            ],
            Some("You are a meeting analyst."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "Meeting transcript content",
            "",
            0,
            1,
            false,
        );
        assert!(prompt.contains("meeting analyst"));
        assert!(prompt.contains("Decisions"));
        assert!(prompt.contains("Action Items"));
        assert!(prompt.contains("Discussion Points"));
        assert!(prompt.contains("Attendees"));
    }

    #[test]
    fn test_build_prompt_movie_sections() {
        let dt = make_doc_type(
            "movie",
            matric_core::DocumentCategory::Media,
            vec![
                "Summary",
                "Synopsis",
                "Cast & Characters",
                "Key Scenes",
                "Themes",
            ],
            Some("You are a film analyst."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "Movie timeline content",
            "",
            0,
            1,
            false,
        );
        assert!(prompt.contains("film analyst"));
        assert!(prompt.contains("Synopsis"));
        assert!(prompt.contains("Cast & Characters"));
        assert!(prompt.contains("Key Scenes"));
        assert!(prompt.contains("Themes"));
    }

    #[test]
    fn test_build_prompt_video_timeline_preserved() {
        // When no doc type but is_video_timeline, should use scene-merging prompt
        let prompt = build_type_aware_prompt(
            None,
            RevisionMode::Standard,
            "### Scene 1\n**Duration**: 5m\nDialog here",
            "",
            0,
            1,
            true,
        );
        assert!(prompt.contains("video content editor"));
        assert!(prompt.contains("SCENE MERGING"));
        assert!(prompt.contains("DIALOG INTEGRATION"));
        assert!(prompt.contains("visual PROGRESSION"));
        assert!(prompt.contains("merged scene heading with time range"));
        assert!(prompt.contains("## Summary"));
    }

    #[test]
    fn test_build_prompt_uses_generation_prompt_hint() {
        let dt = make_doc_type(
            "documentary",
            matric_core::DocumentCategory::Media,
            vec!["Summary", "Subject Overview", "Key Arguments"],
            Some("You are a documentary analyst. Produce a structured summary."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "Documentary content",
            "",
            0,
            1,
            false,
        );
        assert!(prompt.contains("documentary analyst"));
        assert!(prompt.contains("Produce a structured summary"));
    }

    #[test]
    fn test_build_prompt_educational_sections() {
        let dt = make_doc_type(
            "educational-video",
            matric_core::DocumentCategory::Media,
            vec![
                "Summary",
                "Learning Objectives",
                "Key Concepts",
                "Step-by-Step Breakdown",
            ],
            Some("You are an educational content analyst."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "Educational video content",
            "",
            0,
            1,
            false,
        );
        assert!(prompt.contains("educational content analyst"));
        assert!(prompt.contains("Learning Objectives"));
        assert!(prompt.contains("Key Concepts"));
        assert!(prompt.contains("Step-by-Step Breakdown"));
    }

    #[test]
    fn test_build_prompt_required_sections_from_agentic_config() {
        // Custom document type with unique sections
        let dt = make_doc_type(
            "custom-type",
            matric_core::DocumentCategory::Custom,
            vec!["Summary", "Alpha", "Beta", "Gamma"],
            Some("You are a custom analyst."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "Content",
            "",
            0,
            1,
            false,
        );
        // All sections except Summary should appear in the sections instruction
        assert!(prompt.contains("Alpha"));
        assert!(prompt.contains("Beta"));
        assert!(prompt.contains("Gamma"));
        // Summary is handled separately as ## Summary instruction
        assert!(prompt.contains("## Summary"));
    }

    #[test]
    fn test_build_prompt_doc_type_overrides_video_timeline_fallback() {
        // When we have a doc type, it should take priority over is_video_timeline fallback
        let dt = make_doc_type(
            "movie",
            matric_core::DocumentCategory::Media,
            vec!["Summary", "Synopsis", "Cast & Characters"],
            Some("You are a film analyst."),
        );
        let prompt = build_type_aware_prompt(
            Some(&dt),
            RevisionMode::Standard,
            "### Scene 1\n**Duration**: 5m",
            "",
            0,
            1,
            true, // is_video_timeline=true, but doc_type is present
        );
        // Should use the doc type prompt, not the generic video timeline fallback
        assert!(prompt.contains("film analyst"));
        assert!(prompt.contains("Synopsis"));
        // Doc type branch should include keyframe-merging instructions
        assert!(prompt.contains("group consecutive keyframes"));
        assert!(prompt.contains("visual progression"));
    }

    #[test]
    fn ai_generation_job_failure_uses_generic_stored_message() {
        let result = ai_generation_job_failure(
            "Cannot reach https://token:secret@provider.internal/v1 from /srv/fortemi",
            "test_generation",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, AI_GENERATION_JOB_FAILURE);
                assert!(!message.contains("token:secret"));
                assert!(!message.contains("provider.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("Cannot reach"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn model_resolution_job_failure_uses_generic_stored_message() {
        let result = model_resolution_job_failure(
            "provider resolution failed for https://token:secret@provider.internal/v1 from /srv/fortemi",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, MODEL_RESOLUTION_JOB_FAILURE);
                assert!(!message.contains("token:secret"));
                assert!(!message.contains("provider.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("provider resolution failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn job_ancillary_diagnostic_classes_are_fixed_and_redacted() {
        let raw =
            "failed at https://token:secret@provider.internal/v1 from /srv/fortemi SQLSTATE 08006";

        assert_eq!(diagnostic_len(raw), raw.chars().count());

        for detail in [
            JOB_CHUNK_MERGE_PARSE_FAILURE_DETAIL,
            JOB_AI_GENERATION_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_AI_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_AI_CONTEXTUAL_REVISION_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_PROVENANCE_WRITE_FAILURE_DETAIL,
            JOB_QUEUE_FOLLOWUP_FAILURE_DETAIL,
            JOB_REVISION_NOTE_UPDATE_FAILURE_DETAIL,
            JOB_CONTEXT_DISCOVERY_FAILURE_DETAIL,
            JOB_TITLE_ESCALATION_FAILURE_DETAIL,
            JOB_LINKING_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_PURGE_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_CONTEXT_UPDATE_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_CONCEPT_TAGGING_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_REFERENCE_EXTRACTION_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_RELATED_CONCEPT_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_METADATA_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_DOCUMENT_TYPE_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_REEMBED_QUEUE_DIAGNOSTIC_FAILURE_DETAIL,
            JOB_EXIF_DIAGNOSTIC_FAILURE_DETAIL,
        ] {
            assert!(!detail.contains("token:secret"));
            assert!(!detail.contains("provider.internal"));
            assert!(!detail.contains("/srv/fortemi"));
            assert!(!detail.contains("SQLSTATE"));
            assert!(!detail.contains("failed at"));
        }
    }

    #[test]
    fn schema_context_job_failure_message_is_generic() {
        let message = SCHEMA_CONTEXT_JOB_FAILURE.to_string();

        assert_eq!(
            message,
            "Job schema context failed. Check server logs for diagnostics."
        );
        assert!(!message.contains("tenant_secret_schema"));
        assert!(!message.contains("postgres://"));
        assert!(!message.contains("user:secret"));
        assert!(!message.contains("db.internal"));
        assert!(!message.contains("/srv/fortemi"));
        assert!(!message.contains("SQLSTATE"));
    }

    #[test]
    fn graph_maintenance_step_failure_uses_generic_stored_message() {
        let result = graph_maintenance_step_failure(
            "database error at postgresql://user:secret@db.internal/app from /srv/fortemi",
            "snn",
        );

        assert_eq!(result["status"], "failed");
        assert_eq!(result["error"], GRAPH_MAINTENANCE_STEP_FAILURE);
        let serialized = result.to_string();
        assert!(!serialized.contains("user:secret"));
        assert!(!serialized.contains("db.internal"));
        assert!(!serialized.contains("/srv/fortemi"));
        assert!(!serialized.contains("database error"));
    }

    #[test]
    fn ai_revision_job_failure_uses_generic_stored_message() {
        let result = ai_revision_job_failure(
            "revision save failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 40001",
            "save_revision",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, AI_REVISION_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("revision save failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }

        let result = ai_revision_job_failure(
            "AI revision returned empty after content cleaning (Phase 1 of contextual pipeline)",
            "empty_contextual_revision_after_cleaning",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, AI_REVISION_JOB_FAILURE);
                assert!(!message.contains("empty"));
                assert!(!message.contains("content cleaning"));
                assert!(!message.contains("Phase 1"));
                assert!(!message.contains("contextual pipeline"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn ai_contextual_revision_job_failure_uses_generic_stored_message() {
        let result = ai_contextual_revision_job_failure(
            "contextual revision save failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 40001",
            "save_contextual_revision",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, AI_CONTEXTUAL_REVISION_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("contextual revision save failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }

        let result = ai_contextual_revision_job_failure(
            "AI contextual revision returned empty after content cleaning \
             (model may have echoed the prompt instead of generating a revision)",
            "empty_contextual_revision_after_cleaning",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, AI_CONTEXTUAL_REVISION_JOB_FAILURE);
                assert!(!message.contains("empty"));
                assert!(!message.contains("content cleaning"));
                assert!(!message.contains("echoed the prompt"));
                assert!(!message.contains("generating a revision"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn embedding_job_failure_uses_generic_stored_message() {
        let result = embedding_job_failure(
            "embedding provider failed at https://token:secret@provider.internal/v1; database postgres://user:secret@db.internal/app from /srv/fortemi SQLSTATE 08006",
            "store_embeddings",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, EMBEDDING_JOB_FAILURE);
                assert!(!message.contains("token:secret"));
                assert!(!message.contains("provider.internal"));
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("embedding provider failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }

        let result = embedding_job_failure(
            "failed to fetch note from postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 42P01",
            "fetch_note",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, EMBEDDING_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("failed to fetch note"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }

        let result = embedding_job_failure(
            "commit failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 40001",
            "fetch_note_commit",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, EMBEDDING_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("commit failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn title_generation_job_failure_uses_generic_stored_message() {
        let result = title_generation_job_failure(
            "title provider failed at https://token:secret@provider.internal/v1; database postgres://user:secret@db.internal/app from /srv/fortemi SQLSTATE 23505",
            "save_title",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, TITLE_GENERATION_JOB_FAILURE);
                assert!(!message.contains("token:secret"));
                assert!(!message.contains("provider.internal"));
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("title provider failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }

        let result = title_generation_job_failure(
            "failed to fetch note from postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 42P01",
            "fetch_note",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, TITLE_GENERATION_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("failed to fetch note"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn context_update_job_failure_uses_generic_stored_message() {
        let result = context_update_job_failure(
            "context save failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 40001",
            "save_revision",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, CONTEXT_UPDATE_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("context save failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn linking_job_failure_uses_generic_stored_message() {
        let result = linking_job_failure(
            "linking note fetch failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 40001",
            "fetch_note",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, LINKING_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("linking note fetch failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn linking_step_failure_uses_generic_stored_message() {
        let message = linking_step_failure(
            "Schema tx failed: postgres://user:secret@db.internal/app SQLSTATE 40001",
            "threshold_candidate_tx",
        );

        assert_eq!(message, LINKING_JOB_FAILURE);
        assert!(!message.contains("Schema tx failed"));
        assert!(!message.contains("postgres://"));
        assert!(!message.contains("user:secret"));
        assert!(!message.contains("db.internal"));
        assert!(!message.contains("SQLSTATE"));
    }

    #[test]
    fn purge_job_failure_uses_generic_stored_message() {
        let result = purge_job_failure(
            "purge delete failed for note 018f8d6d-1111-7222-8333-c44444444444 at postgres://user:secret@db.internal/app /srv/fortemi SQLSTATE 23503",
            "delete_note",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, PURGE_JOB_FAILURE);
                assert!(!message.contains("018f8d6d"));
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("purge delete failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn concept_tagging_job_failure_uses_generic_stored_message() {
        let result = concept_tagging_job_failure(
            "concept tagging note fetch failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 42P01",
            "fetch_note",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, CONCEPT_TAGGING_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("note fetch failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn related_concept_job_failure_uses_generic_stored_message() {
        let result = related_concept_job_failure(
            "related concept lookup failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 08006",
            "fetch_concepts_begin_tx",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, RELATED_CONCEPT_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("lookup failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn reference_extraction_job_failure_uses_generic_stored_message() {
        let result = reference_extraction_job_failure(
            "reference extraction failed to fetch note from postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 42P01",
            "fetch_note",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, REFERENCE_EXTRACTION_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("fetch note"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn reembed_all_job_failure_uses_generic_stored_message() {
        let result = reembed_all_job_failure(
            "failed to list notes from postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 08006",
            "list_all_notes",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, REEMBED_ALL_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("list notes"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn refresh_embedding_set_job_failure_uses_generic_stored_message() {
        let result = refresh_embedding_set_job_failure(
            "failed to find missing embeddings for confidential-set against postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 42P01",
            "find_missing_embeddings",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, REFRESH_EMBEDDING_SET_JOB_FAILURE);
                assert!(!message.contains("confidential-set"));
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("missing embeddings"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn attachment_processing_job_failure_uses_generic_stored_message() {
        let result = attachment_processing_job_failure(
            "download failed for attachment 018f8d6d-1111-7222-8333-c44444444444 at /srv/fortemi/files with postgres://user:secret@db.internal/app SQLSTATE 58030",
            "download_attachment",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, ATTACHMENT_PROCESSING_JOB_FAILURE);
                assert!(!message.contains("018f8d6d"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("download failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn metadata_extraction_job_failure_uses_generic_stored_message() {
        let result = metadata_extraction_job_failure(
            "failed to parse metadata JSON at line 1 column 2 after contacting https://token:secret@provider.internal/v1 from /srv/fortemi",
            "parse_ai_response",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, METADATA_EXTRACTION_JOB_FAILURE);
                assert!(!message.contains("line 1"));
                assert!(!message.contains("column"));
                assert!(!message.contains("token:secret"));
                assert!(!message.contains("provider.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("failed to parse"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }

    #[test]
    fn document_type_inference_job_failure_uses_generic_stored_message() {
        let result = document_type_inference_job_failure(
            "document type lookup failed for postgres://user:secret@db.internal/app at /srv/fortemi SQLSTATE 42P01",
            "detect_document_type",
        );

        match result {
            JobResult::Failed(message) => {
                assert_eq!(message, DOCUMENT_TYPE_INFERENCE_JOB_FAILURE);
                assert!(!message.contains("postgres://"));
                assert!(!message.contains("user:secret"));
                assert!(!message.contains("db.internal"));
                assert!(!message.contains("/srv/fortemi"));
                assert!(!message.contains("SQLSTATE"));
                assert!(!message.contains("lookup failed"));
            }
            other => panic!("expected failed job result, got {other:?}"),
        }
    }
}
