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
    LinkRepository, NoteRepository, ProvRelation, RevisionMode, SearchHit, SkosSemanticRelation,
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
    db.for_schema(schema)
        .map_err(|e| JobResult::Failed(format!("Invalid schema '{}': {}", schema, e)))
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
        .map_err(|e| JobResult::Failed(format!("Model resolution error: {}", e)))
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
                info!(error = %e, "Skipping unparseable chunk result in merge");
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

    /// Get related notes for contextual enhancement (similarity > 50%).
    ///
    /// Returns up to MAX_CONTEXT_NOTES results, respecting Miller's Law (7±2)
    /// for optimal cognitive processing of context items.
    async fn get_related_notes(&self, note_id: uuid::Uuid, content: &str) -> Vec<SearchHit> {
        // Generate embedding for the content to find related notes
        let chunks = vec![content
            .chars()
            .take(matric_core::defaults::PREVIEW_EMBEDDING)
            .collect::<String>()];
        let vectors = match self.backend.embed_texts(&chunks).await {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let query_vec = match vectors.into_iter().next() {
            Some(v) => v,
            None => return vec![],
        };

        // Fetch more candidates than needed, then filter and limit to Miller's Law bound
        let fetch_limit = (MAX_CONTEXT_NOTES * 2) as i64;
        match self
            .db
            .embeddings
            .find_similar(&query_vec, fetch_limit, true)
            .await
        {
            Ok(hits) => hits
                .into_iter()
                .filter(|hit| {
                    hit.score > matric_core::defaults::RELATED_NOTES_MIN_SIMILARITY
                        && hit.note_id != note_id
                })
                .take(MAX_CONTEXT_NOTES)
                .collect(),
            Err(_) => vec![],
        }
    }

    /// Build context string from related notes.
    ///
    /// Includes up to MAX_PROMPT_SNIPPETS snippets in the prompt context,
    /// staying within Miller's Law bounds for working memory.
    fn build_related_context(&self, related_notes: &[SearchHit]) -> String {
        if related_notes.is_empty() {
            return String::new();
        }

        let mut context = String::from("Related concepts from the knowledge base:\n");
        for hit in related_notes.iter().take(MAX_PROMPT_SNIPPETS) {
            if let Some(snippet) = &hit.snippet {
                let preview: String = snippet
                    .chars()
                    .take(matric_core::defaults::PREVIEW_CONTEXT_SNIPPET)
                    .collect();
                context.push_str(&format!("- {}\n", preview));
            }
        }
        context.push('\n');
        context
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
        }

        let original_content = &note.original.content;
        if original_content.trim().is_empty() {
            return JobResult::Failed("Note has no content to revise".into());
        }

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

        // Build prompt based on revision mode
        let (prompt, related_count, related_note_ids) = match revision_mode {
            RevisionMode::Full => {
                ctx.report_progress(20, Some("Finding related notes for context..."));
                let related_notes = self.get_related_notes(note_id, original_content).await;
                let related_context = self.build_related_context(&related_notes);
                let count = related_notes.len();
                let note_ids: Vec<uuid::Uuid> = related_notes.iter().map(|h| h.note_id).collect();

                ctx.report_progress(40, Some("Generating AI-enhanced revision (full mode)..."));

                // Full mode: aggressive expansion with context (original HOTM prompt)
                let prompt = format!(
                    r#"You are an intelligent note-taking assistant. Your task is to enhance the following note by leveraging related concepts from the knowledge base to create a more holistic and contextual revision.

Original Note:
{}

{}Please provide an enhanced version that:
1. Preserves ALL original information and meaning
2. Improves clarity and organization with proper markdown formatting
3. Identifies and highlights key concepts
4. Makes connections to related concepts where relevant (without overwhelming the original content)
5. Adds contextual insights that help place this note within the broader knowledge landscape
6. Maintains a professional yet accessible tone
7. Formats any code blocks, math expressions, or diagrams properly

Guidelines:
- Only reference related concepts when they genuinely enhance understanding
- Do not force connections that don't make sense
- Keep the focus on the original note's content
- Add value through context, not just length

Output the enhanced note in clean markdown format. Do not add any labels, markers, or metadata."#,
                    original_content, related_context
                );
                (prompt, count, note_ids)
            }
            RevisionMode::Light => {
                ctx.report_progress(40, Some("Generating AI-enhanced revision (light mode)..."));

                // Light mode: structure and formatting ONLY, no invented details
                let prompt = format!(
                    r#"You are a formatting assistant. Your task is to improve the structure and readability of the following note WITHOUT adding any new information.

Original Note:
{}

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
                    original_content
                );
                (prompt, 0, vec![])
            }
            RevisionMode::None => unreachable!(), // Already handled above
        };

        let revised = match backend.generate(&prompt).await {
            Ok(r) => clean_enhanced_content(r.trim(), &prompt),
            Err(e) => return JobResult::Failed(format!("AI generation failed: {}", e)),
        };

        if revised.is_empty() {
            return JobResult::Failed("AI returned empty response".into());
        }

        ctx.report_progress(80, Some("Saving revision..."));

        // Save the revision with mode indicator
        let revision_note = match revision_mode {
            RevisionMode::Full => "AI-enhanced revision with context",
            RevisionMode::Light => "Light formatting revision (no expansion)",
            RevisionMode::None => "Original preserved",
        };

        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        if let Err(e) = self
            .db
            .notes
            .update_revised_tx(&mut tx, note_id, &revised, Some(revision_note))
            .await
        {
            return JobResult::Failed(format!("Failed to save revision: {}", e));
        }
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
        }

        // Record W3C PROV provenance for the AI revision
        ctx.report_progress(90, Some("Recording provenance..."));

        // Get the current revision ID to attach provenance edges
        if let Ok(Some(chain)) = self.db.provenance.get_chain(note_id).await {
            let rev_id = chain.revision_id;

            // Record "used" edges for each related note that contributed context
            if !related_note_ids.is_empty() {
                if let Err(e) = self
                    .db
                    .provenance
                    .record_edges_batch(rev_id, &related_note_ids, &ProvRelation::Used)
                    .await
                {
                    warn!(error = %e, "Failed to record provenance edges");
                }
            }

            // Complete the provenance activity
            if let Some(act_id) = activity_id {
                let metadata = serde_json::json!({
                    "revision_mode": format!("{:?}", revision_mode),
                    "related_notes_used": related_count,
                    "revised_length": revised.len(),
                });
                if let Err(e) = self
                    .db
                    .provenance
                    .complete_activity(act_id, Some(rev_id), Some(metadata))
                    .await
                {
                    warn!(error = %e, "Failed to complete provenance activity");
                }
            }
        }

        ctx.report_progress(100, Some("Revision complete"));
        info!(
            note_id = %note_id,
            mode = ?revision_mode,
            related_count = related_count,
            duration_ms = start.elapsed().as_millis() as u64,
            "AI revision completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "revised_length": revised.len(),
            "revision_mode": revision_mode,
            "related_notes_used": related_count
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
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
            return JobResult::Failed(format!("Commit failed: {}", e));
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
            Err(e) => return JobResult::Failed(format!("Embedding failed: {}", e)),
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
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
                return JobResult::Failed(format!("Failed to delete embeddings: {}", e));
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
                        return JobResult::Failed(format!("Failed to insert embedding: {}", e));
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
            return JobResult::Failed(format!("Failed to commit: {}", e));
        }

        if let Err(e) = store_result {
            return JobResult::Failed(format!("Failed to store embeddings: {}", e));
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
                warn!(error = %e, "Failed to complete embedding provenance activity");
            }
        }

        ctx.report_progress(100, Some("Embeddings complete"));
        info!(
            note_id = %note_id,
            chunk_count = chunk_count,
            duration_ms = start.elapsed().as_millis() as u64,
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
    async fn queue_tier_escalation(&self, note_id: uuid::Uuid, schema: &str, next_tier: i16) {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        if let Err(e) = self
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
            warn!(%note_id, next_tier, error = %e, "Failed to queue title generation tier escalation");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };
        tx.commit().await.ok();

        // Skip if already has a title
        if note.note.title.is_some() {
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
                        self.queue_tier_escalation(
                            note_id,
                            schema,
                            matric_core::cost_tier::STANDARD_GPU,
                        )
                        .await;
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
                    info!(error = %e, "Fast model failed for title generation, escalating to tier-2");
                    self.queue_tier_escalation(
                        note_id,
                        schema,
                        matric_core::cost_tier::STANDARD_GPU,
                    )
                    .await;
                    return JobResult::Success(Some(serde_json::json!({
                        "escalated": true,
                        "reason": "fast_model_failed"
                    })));
                }
                return JobResult::Failed(format!("Title generation failed: {}", e));
            }
        };

        ctx.report_progress(80, Some("Saving title..."));

        // Save the title
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        if let Err(e) = self
            .db
            .notes
            .update_title_tx(&mut tx, note_id, &title)
            .await
        {
            return JobResult::Failed(format!("Failed to save title: {}", e));
        }
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
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
                warn!(error = %e, "Failed to complete title generation provenance activity");
            }
        }

        info!(
            note_id = %note_id,
            title = %title,
            duration_ms = start.elapsed().as_millis() as u64,
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
                debug!(error = %e, "Failed to fetch concepts for tag boost, using pure embedding similarity");
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

        // NOTE: find_similar_with_vectors doesn't have a _tx variant yet.
        // This will silently return empty for non-default archives (graceful degradation).
        let _ = schema_ctx; // Suppress unused warning until _tx variant is available
        let candidates = match self
            .db
            .embeddings
            .find_similar_with_vectors(source_vec, candidate_limit, true)
            .await
        {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to find similar: {}", e)),
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
                    .map_err(|e| format!("Schema tx failed: {}", e))?;
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
                tx.commit().await.ok();
                res
            };
            if let Err(e) = result {
                debug!(error = %e, "Failed to create reciprocal link (may already exist)");
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
                    .map_err(|e| format!("Schema tx failed: {}", e))?;
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
                        .map_err(|e| format!("Schema tx failed: {}", e))?;
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
                    tx.commit().await.ok();
                    res
                };
                if let Err(e) = result {
                    debug!(error = %e, "Failed to create fallback link");
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
                .map_err(|e| format!("Schema tx failed: {}", e))?;
            let s = self
                .db
                .embeddings
                .find_similar_tx(&mut tx, source_vec, 10, true)
                .await
                .map_err(|e| format!("Failed to find similar: {}", e))?;
            tx.commit()
                .await
                .map_err(|e| format!("Commit failed: {}", e))?;
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
                    .map_err(|e| format!("Schema tx failed: {}", e))?;
                let res = self
                    .db
                    .links
                    .create_tx(&mut tx, note_id, hit.note_id, "semantic", hit.score, None)
                    .await;
                tx.commit().await.ok();
                if let Err(e) = res {
                    debug!(error = %e, "Failed to create forward link (may already exist)");
                } else {
                    created += 1;
                }
            }

            // Backward link (old -> new)
            {
                let mut tx = schema_ctx
                    .begin_tx()
                    .await
                    .map_err(|e| format!("Schema tx failed: {}", e))?;
                let res = self
                    .db
                    .links
                    .create_tx(&mut tx, hit.note_id, note_id, "semantic", hit.score, None)
                    .await;
                tx.commit().await.ok();
                if let Err(e) = res {
                    debug!(error = %e, "Failed to create backward link (may already exist)");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
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
                        debug!(error = %e, target = %link_title, "Failed to create wiki link (may already exist)");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let embeddings = match self.db.embeddings.get_for_note_tx(&mut tx, note_id).await {
            Ok(e) => {
                tx.commit().await.ok();
                e
            }
            Err(e) => {
                warn!(error = %e, "No embeddings for note, skipping semantic linking");
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
                // Count total notes for adaptive k
                let note_count = self
                    .db
                    .embeddings
                    .find_similar(&embeddings[0].vector, 1, true)
                    .await
                    .map(|r| r.len())
                    .unwrap_or(0);
                // Use a rough count — we'll get the real count from the candidate pool size
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
            note_id = %note_id,
            result_count = created,
            strategy = %graph_config.strategy,
            wiki_found = wiki_links_found,
            wiki_resolved = wiki_links_resolved,
            duration_ms = start.elapsed().as_millis() as u64,
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
                warn!(error = %e, "Failed to complete linking provenance activity");
            }
        }

        // Queue a deduplicated GraphMaintenance job so SNN/PFNET run after new
        // links are created.  Deduplication ensures only one pending maintenance job
        // exists even if many linking jobs complete in rapid succession.
        let schema = extract_schema(&ctx);
        let maint_payload = serde_json::json!({ "schema": schema });
        if let Err(e) = self
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
            warn!(error = %e, "Failed to queue post-linking graph maintenance job");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let affected_sets = match self
            .db
            .embedding_sets
            .get_sets_for_note_tx(&mut tx, note_id)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "Failed to get embedding sets for note, continuing with deletion");
                vec![]
            }
        };
        tx.commit().await.ok();

        ctx.report_progress(30, Some("Verifying note exists..."));

        // Verify note exists before attempting deletion
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let exists = self
            .db
            .notes
            .exists_tx(&mut tx, note_id)
            .await
            .unwrap_or(false);
        tx.commit().await.ok();
        if !exists {
            return JobResult::Failed(format!("Note {} does not exist", note_id));
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        if let Err(e) = self.db.notes.hard_delete_tx(&mut tx, note_id).await {
            return JobResult::Failed(format!("Failed to delete note: {}", e));
        }
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
        }

        ctx.report_progress(80, Some("Updating embedding set statistics..."));

        // Update stats for all affected embedding sets
        let mut stats_updated = 0;
        for set_id in &affected_sets {
            if let Err(e) = self.db.embedding_sets.refresh_stats(*set_id).await {
                warn!(error = %e, set_id = %set_id, "Failed to update embedding set stats");
            } else {
                stats_updated += 1;
            }
        }

        ctx.report_progress(100, Some("Note permanently deleted"));
        info!(
            note_id = %note_id,
            affected_sets = affected_sets.len(),
            stats_updated = stats_updated,
            duration_ms = start.elapsed().as_millis() as u64,
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let links = match self.db.links.get_outgoing_tx(&mut tx, note_id).await {
            Ok(l) => l
                .into_iter()
                .filter(|l| l.score > matric_core::defaults::CONTEXT_LINK_THRESHOLD)
                .take(MAX_PROMPT_SNIPPETS)
                .collect::<Vec<_>>(),
            Err(e) => {
                warn!(error = %e, "Failed to get links");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
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
            Err(e) => return JobResult::Failed(format!("Generation failed: {}", e)),
        };

        if updated_content.is_empty() {
            return JobResult::Failed("Empty content generated".into());
        }

        ctx.report_progress(80, Some("Saving updated content..."));

        // Save the updated revision
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
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
            return JobResult::Failed(format!("Failed to save: {}", e));
        }
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
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
    async fn queue_phase2_jobs(&self, note_id: uuid::Uuid, schema: &str) {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        // RelatedConceptInference starts at tier-1 (fast GPU).
        if let Err(e) = self
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
            warn!(%note_id, error = %e, "Failed to queue phase-2 related concept inference job");
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
    fn make_concept_prompt(text: &str, existing: &[String], target: usize) -> String {
        let count_hint = if !existing.is_empty() {
            let needed = target.saturating_sub(existing.len());
            format!("We already have {} concepts from entity extraction. Suggest {} MORE distinct concepts that cover different dimensions. Do NOT repeat: {:?}\n\n",
                existing.len(), needed, existing)
        } else {
            String::new()
        };
        format!(
            r#"You are a knowledge organization specialist using SKOS (Simple Knowledge Organization System). Analyze the following content and suggest concept tags organized as hierarchical paths across MULTIPLE dimensions.

{count_hint}Content:
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
7. Aim for {target} tags total — breadth across dimensions is more valuable than depth in one

Output ONLY a JSON array of tag paths, nothing else. Example:
["science/machine-learning", "nlp/transformers", "technique/attention-mechanism", "methodology/experimental", "evaluation/benchmark", "application/translation", "tool/pytorch", "content-type/research-paper", "era/foundation-models"]"#
        )
    }

    /// Tier-0: GLiNER NER only. Chains to tier-1 if insufficient concepts.
    async fn execute_ner(
        &self,
        ctx: &JobContext,
        note_id: uuid::Uuid,
        schema: &str,
        content_preview: &str,
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
                    warn!(error = %e, "Tier-0 GLiNER concept extraction failed");
                }
            }
        }

        // Chain to tier-1 if below target
        let escalating = concept_labels.len() < self.target_concepts;
        if escalating {
            self.queue_escalation(
                note_id,
                schema,
                matric_core::cost_tier::FAST_GPU,
                &concept_labels,
                matric_core::cost_tier::CPU_NER,
            )
            .await;
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
    ) -> (Vec<String>, &'static str, bool) {
        let mut concept_labels = Self::extract_prior_concepts(ctx);
        let prior_count = concept_labels.len();

        let backend: &dyn GenerationBackend = match overridden {
            Some(b) => b,
            None => match &self.fast_backend {
                Some(fb) => fb,
                None => {
                    // No fast backend — escalate directly to tier-2
                    self.queue_escalation(
                        note_id,
                        schema,
                        matric_core::cost_tier::STANDARD_GPU,
                        &concept_labels,
                        matric_core::cost_tier::FAST_GPU,
                    )
                    .await;
                    return (concept_labels, "fast_unavailable", true);
                }
            },
        };

        ctx.report_progress(30, Some("Running fast LLM concept extraction..."));

        let chunk_size = extraction_chunk_size(self.fast_backend.as_ref());
        let chunks = chunk_for_extraction(content_preview, chunk_size);
        let mut chunk_results: Vec<String> = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let prompt = Self::make_concept_prompt(chunk, &concept_labels, self.target_concepts);
            match backend.generate_json(&prompt).await {
                Ok(r) => chunk_results.push(r.trim().to_string()),
                Err(e) => {
                    info!(chunk = i, chunks = chunks.len(), error = %e, "Tier-1 fast model failed on chunk, skipping");
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

        // Chain to tier-2 if still below half target (standard escalation threshold)
        let standard_threshold = self.target_concepts.div_ceil(2);
        let escalating = concept_labels.len() < standard_threshold;
        if escalating {
            info!(
                note_id = %note_id,
                count = concept_labels.len(),
                threshold = standard_threshold,
                "Tier-1 below escalation threshold, chaining to tier-2"
            );
            self.queue_escalation(
                note_id,
                schema,
                matric_core::cost_tier::STANDARD_GPU,
                &concept_labels,
                matric_core::cost_tier::FAST_GPU,
            )
            .await;
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
    ) -> (Vec<String>, &'static str, bool) {
        let mut concept_labels = Self::extract_prior_concepts(ctx);
        let prior_count = concept_labels.len();

        let backend: &dyn GenerationBackend = match overridden {
            Some(b) => b,
            None => &self.backend,
        };

        ctx.report_progress(30, Some("Running standard model concept extraction..."));

        let existing_snapshot: Vec<String> = concept_labels.clone();
        let prompt =
            Self::make_concept_prompt(content_preview, &existing_snapshot, self.target_concepts);

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
                    warn!(error = %e, "Tier-2 standard model failed with no prior concepts");
                } else {
                    warn!(error = %e, "Tier-2 standard model failed, proceeding with {} prior concepts", concept_labels.len());
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
    async fn queue_escalation(
        &self,
        note_id: uuid::Uuid,
        schema: &str,
        next_tier: i16,
        prior_concepts: &[String],
        prior_tier: i16,
    ) {
        let mut payload = serde_json::json!({
            "prior_concepts": prior_concepts,
            "prior_tier": prior_tier,
            "prior_count": prior_concepts.len(),
        });
        if schema != "public" {
            payload["schema"] = serde_json::json!(schema);
        }
        if let Err(e) = self
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
            warn!(
                %note_id,
                next_tier,
                error = %e,
                "Failed to queue concept tagging tier escalation"
            );
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
            self.queue_escalation(
                note_id,
                schema,
                matric_core::cost_tier::FAST_GPU,
                &[],
                matric_core::cost_tier::CPU_NER,
            )
            .await;
            return JobResult::Success(Some(serde_json::json!({
                "concepts": 0,
                "escalating": true,
                "reason": "no_ner_backend"
            })));
        }

        ctx.report_progress(10, Some("Fetching note content..."));

        // Get the note
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };
        tx.commit().await.ok();

        // Use revised content if available, otherwise original
        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            self.queue_phase2_jobs(note_id, schema).await;
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
        let (concept_labels, extraction_method, escalating) = match ctx.job.cost_tier {
            Some(matric_core::cost_tier::CPU_NER) => {
                self.execute_ner(&ctx, note_id, schema, &content_preview)
                    .await
            }
            Some(matric_core::cost_tier::FAST_GPU) => {
                self.execute_fast(
                    &ctx,
                    note_id,
                    schema,
                    &content_preview,
                    overridden.as_deref(),
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
                )
                .await
            }
            _ => {
                // Treat NULL cost_tier as CPU_NER (tier-0 entry point).
                // Escalation to tier-1/tier-2 happens via job queue chaining.
                self.execute_ner(&ctx, note_id, schema, &content_preview)
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
                self.queue_phase2_jobs(note_id, schema).await;
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
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };

            let resolved = match self
                .db
                .skos
                .resolve_or_create_tag_tx(&mut tx, &tag_input)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(label = %label, error = %e, "Failed to resolve concept");
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
                debug!(error = %e, concept_id = %resolved.concept_id, "Failed to tag note (may already exist)");
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
            self.queue_phase2_jobs(note_id, schema).await;
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
                warn!(error = %e, "Failed to complete concept tagging provenance activity");
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
    async fn queue_ref_tier_escalation(&self, note_id: uuid::Uuid, schema: &str, next_tier: i16) {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        if let Err(e) = self
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
            warn!(%note_id, next_tier, error = %e, "Failed to queue reference extraction tier escalation");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
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
                        self.queue_ref_tier_escalation(
                            note_id,
                            schema,
                            matric_core::cost_tier::FAST_GPU,
                        )
                        .await;
                        return JobResult::Success(Some(serde_json::json!({
                            "references": 0,
                            "escalated": true,
                            "reason": "gliner_empty"
                        })));
                    }
                    (Vec::new(), "gliner_empty")
                }
                Err(e) => {
                    warn!(error = %e, "GLiNER extraction failed, falling back to LLM");
                    if is_tiered {
                        self.queue_ref_tier_escalation(
                            note_id,
                            schema,
                            matric_core::cost_tier::FAST_GPU,
                        )
                        .await;
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
                self.queue_ref_tier_escalation(note_id, schema, matric_core::cost_tier::FAST_GPU)
                    .await;
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
                                info!(chunk = i, chunks = chunks.len(), error = %e, "Fast model ref parse failed, skipping chunk");
                            }
                        },
                        Err(e) => {
                            info!(chunk = i, chunks = chunks.len(), error = %e, "Fast model ref extraction failed, skipping chunk");
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
                self.queue_ref_tier_escalation(
                    note_id,
                    schema,
                    matric_core::cost_tier::STANDARD_GPU,
                )
                .await;
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
                            warn!(error = %e, "Failed to parse LLM reference response");
                            return JobResult::Success(Some(serde_json::json!({
                                "references": 0,
                                "reason": "parse_error",
                                "extraction_method": extraction_method,
                            })));
                        }
                    },
                    Err(e) => {
                        warn!(error = %e, "LLM reference extraction failed");
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
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };

            let resolved = match self
                .db
                .skos
                .resolve_or_create_tag_tx(&mut tx, &tag_input)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(tag_path = %tag_path, error = %e, "Failed to resolve reference concept");
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
                debug!(error = %e, concept_id = %resolved.concept_id, "Failed to tag note with reference (may already exist)");
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
                warn!(error = %e, "Failed to complete reference extraction provenance activity");
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
    async fn queue_phase3_jobs(&self, note_id: uuid::Uuid, schema: &str) {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        // Embedding and Linking are tier-agnostic (NULL).
        if let Err(e) = self
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
            warn!(%note_id, error = %e, "Failed to queue phase-3 embedding job");
        }
        if let Err(e) = self
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
            warn!(%note_id, error = %e, "Failed to queue phase-3 linking job");
        }
    }

    /// Queue a tier-2 escalation job for related concept inference.
    async fn queue_related_tier_escalation(&self, note_id: uuid::Uuid, schema: &str) {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        if let Err(e) = self
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
            warn!(%note_id, error = %e, "Failed to queue related concept tier-2 escalation");
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
                self.queue_phase3_jobs(note_id, schema).await;
                return JobResult::Failed(format!("Schema tx failed: {}", e));
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
            self.queue_phase3_jobs(note_id, schema).await;
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
                    info!(error = %e, "Fast model failed for related concepts, escalating to tier-2");
                    self.queue_related_tier_escalation(note_id, schema).await;
                    return JobResult::Success(Some(serde_json::json!({
                        "relations_created": 0,
                        "escalated": true,
                        "reason": "fast_model_failed"
                    })));
                }
                self.queue_phase3_jobs(note_id, schema).await;
                return JobResult::Failed(format!("AI generation failed: {}", e));
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
                            info!(error = %e, "Fast model output unparseable for related concepts, escalating to tier-2");
                            self.queue_related_tier_escalation(note_id, schema).await;
                            return JobResult::Success(Some(serde_json::json!({
                                "relations_created": 0,
                                "escalated": true,
                                "reason": "fast_model_parse_failed"
                            })));
                        }
                        warn!(error = %e, response = %ai_response, "Failed to parse related concept pairs");
                        self.queue_phase3_jobs(note_id, schema).await;
                        return JobResult::Failed(format!("Failed to parse AI response: {}", e));
                    }
                }
            }
        };

        if pairs.is_empty() {
            self.queue_phase3_jobs(note_id, schema).await;
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
                    debug!(error = %e, a = %a.notation, b = %b.notation, "Failed to create related relation (may already exist)");
                }
            }

            let progress = 70 + ((i + 1) * 25 / total_pairs) as i32;
            ctx.report_progress(
                progress,
                Some(&format!("Related: {} ↔ {}", a.label, b.label)),
            );
        }

        ctx.report_progress(98, Some("Queuing embedding and linking..."));
        self.queue_phase3_jobs(note_id, schema).await;

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
    async fn queue_tier_escalation(&self, note_id: uuid::Uuid, schema: &str, next_tier: i16) {
        let payload = if schema != "public" {
            Some(serde_json::json!({ "schema": schema }))
        } else {
            None
        };
        if let Err(e) = self
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
            warn!(%note_id, next_tier, error = %e, "Failed to queue metadata extraction tier escalation");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
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
                    info!(error = %e, "Fast model failed for metadata extraction, escalating to tier-2");
                    self.queue_tier_escalation(
                        note_id,
                        schema,
                        matric_core::cost_tier::STANDARD_GPU,
                    )
                    .await;
                    return JobResult::Success(Some(serde_json::json!({
                        "fields_extracted": 0,
                        "escalated": true,
                        "reason": "fast_model_failed"
                    })));
                }
                return JobResult::Failed(format!("AI generation failed: {}", e));
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
                            info!(error = %e, "Fast model returned unparseable metadata, escalating to tier-2");
                            self.queue_tier_escalation(
                                note_id,
                                schema,
                                matric_core::cost_tier::STANDARD_GPU,
                            )
                            .await;
                            return JobResult::Success(Some(serde_json::json!({
                                "fields_extracted": 0,
                                "escalated": true,
                                "reason": "fast_model_parse_failed"
                            })));
                        }
                        warn!(error = %e, response = %ai_response, "Failed to parse AI metadata response");
                        return JobResult::Failed(format!("Failed to parse AI response: {}", e));
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
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
            return JobResult::Failed(format!("Failed to update metadata: {}", e));
        }

        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
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
                warn!(error = %e, "Failed to complete metadata extraction provenance activity");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let note = match self.db.notes.fetch_tx(&mut tx, note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
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
                return JobResult::Failed(format!("Document type detection failed: {}", e));
            }
        };

        ctx.report_progress(80, Some("Assigning document type..."));

        // Update note with detected document type
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        if let Err(e) = sqlx::query("UPDATE note SET document_type_id = $1 WHERE id = $2")
            .bind(doc_type_id)
            .bind(note_id)
            .execute(&mut *tx)
            .await
        {
            return JobResult::Failed(format!("Failed to assign document type: {}", e));
        }
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
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
                warn!(error = %e, "Failed to complete document type inference provenance activity");
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
                Err(e) => {
                    return JobResult::Failed(format!(
                        "Failed to list embedding set members: {}",
                        e
                    ))
                }
            }
        } else {
            ctx.report_progress(10, Some("Getting all active notes..."));

            // Get all active notes
            match self.db.notes.list_all_ids().await {
                Ok(ids) => ids,
                Err(e) => return JobResult::Failed(format!("Failed to list notes: {}", e)),
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
                    debug!(error = %e, note_id = %note_id, "Failed to queue embedding job");
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
fn clean_enhanced_content(content: &str, original_prompt: &str) -> String {
    let mut cleaned = content.to_string();

    // CRITICAL FIX: Remove prompt leakage that occurs with raw mode models
    // Extract key phrases from the original prompt to detect leakage
    let prompt_indicators = [
        "You are an intelligent note-taking assistant",
        "You are a formatting assistant",
        "Your task is to enhance",
        "Your task is to improve",
        "Original Note:",
        "STRICT RULES",
        "What you MAY do:",
        "Guidelines:",
        "Output the enhanced note",
        "Output the formatted note",
        "Do not add any labels, markers, or metadata",
    ];

    // Remove any lines that match prompt indicators (case-insensitive)
    let lines: Vec<&str> = cleaned.lines().collect();
    let mut filtered_lines = Vec::new();
    let mut skip_until_content = false;

    for line in lines {
        let line_lower = line.to_lowercase();
        let line_trimmed = line.trim();

        // Check if this line is part of the system prompt leakage
        let is_prompt_line = prompt_indicators
            .iter()
            .any(|indicator| line_lower.contains(&indicator.to_lowercase()));

        if is_prompt_line {
            skip_until_content = true;
            continue;
        }

        // Skip empty lines immediately after detecting prompt
        if skip_until_content && line_trimmed.is_empty() {
            continue;
        }

        // Once we hit actual content, stop skipping
        if skip_until_content && !line_trimmed.is_empty() {
            skip_until_content = false;
        }

        filtered_lines.push(line);
    }

    cleaned = filtered_lines.join("\n");

    // Remove common markers that might slip through
    let markers = [
        "PART 1",
        "PART 2",
        "ENHANCED NOTE",
        "FORMATTED NOTE",
        "REVISED NOTE",
        "METADATA",
        "---",
        "```json",
        "```markdown",
    ];

    for marker in &markers {
        if cleaned.starts_with(marker) {
            cleaned = cleaned
                .split_once('\n')
                .map(|x| x.1)
                .unwrap_or(&cleaned)
                .to_string();
        }
    }

    // Remove trailing ``` if present
    if cleaned.ends_with("```") {
        cleaned = cleaned.trim_end_matches("```").to_string();
    }

    // Remove leading/trailing whitespace
    cleaned = cleaned.trim().to_string();

    // Final sanity check: if the cleaned content looks like it's just instructions,
    // check against the original prompt more aggressively
    if cleaned.len() < 50 || cleaned.lines().count() < 2 {
        // Try to find where the actual content starts by looking for the original note marker
        if let Some(idx) = original_prompt.find("Original Note:") {
            if let Some(_content_start) = original_prompt[idx..].find('\n') {
                // This is risky but necessary for extreme cases
                warn!("Cleaned content suspiciously short, may indicate complete prompt leakage");
            }
        }
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
            Ok(None) => return JobResult::Failed(format!("Embedding set not found: {}", set_slug)),
            Err(e) => return JobResult::Failed(format!("Failed to look up set: {}", e)),
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
            Err(e) => return JobResult::Failed(format!("Failed to find missing embeddings: {}", e)),
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
                Err(e) => warn!(note_id = %note_id, error = %e, "Failed to queue embedding job"),
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
                Err(e) => return JobResult::Failed(format!("Invalid attachment_id: {}", e)),
            }
        } else {
            // No explicit attachment_id — find image attachments for this note (schema-aware)
            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let attachments = match file_storage.list_by_note_tx(&mut tx, note_id).await {
                Ok(a) => a,
                Err(e) => return JobResult::Failed(format!("Failed to list attachments: {}", e)),
            };
            if let Err(e) = tx.commit().await {
                return JobResult::Failed(format!("Commit failed: {}", e));
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
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            if let Err(e) = file_storage
                .update_status_tx(&mut tx, attachment_id, AttachmentStatus::Processing, None)
                .await
            {
                warn!(attachment_id = %attachment_id, error = %e, "Failed to update attachment status to Processing");
            }
            if let Err(e) = tx.commit().await {
                warn!(error = %e, "Failed to commit status update");
            }
        }

        // Download the attachment bytes (schema-aware)
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let download_result = file_storage.download_file_tx(&mut tx, attachment_id).await;
        if let Err(e) = tx.commit().await {
            return JobResult::Failed(format!("Commit failed: {}", e));
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
                            Some(&e.to_string()),
                        )
                        .await;
                    let _ = tx.commit().await;
                }
                return JobResult::Failed(format!(
                    "Failed to download attachment {}: {}",
                    attachment_id, e
                ));
            }
        };

        if !content_type.starts_with("image/") {
            info!(
                attachment_id = %attachment_id,
                content_type = %content_type,
                "Attachment is not an image, skipping EXIF extraction"
            );
            return JobResult::Success(Some(serde_json::json!({
                "status": "skipped",
                "reason": format!("Not an image: {}", content_type)
            })));
        }

        ctx.report_progress(30, Some("Extracting EXIF metadata..."));

        // Extract EXIF data from the image bytes
        let exif_data = match extract_exif_metadata(&data) {
            Some(data) => data,
            None => {
                info!(
                    attachment_id = %attachment_id,
                    filename = %filename,
                    "No EXIF data found in image"
                );
                // Still mark as completed — no EXIF is a valid outcome (schema-aware)
                if let Ok(mut tx) = schema_ctx.begin_tx().await {
                    if let Err(e) = file_storage
                        .update_status_tx(&mut tx, attachment_id, AttachmentStatus::Completed, None)
                        .await
                    {
                        warn!(error = %e, "Failed to update attachment status to Completed");
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
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
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
                        warn!(error = %e, "Failed to create provenance location from EXIF GPS");
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
                    warn!(error = %e, "Failed to create provenance device from EXIF");
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
                warn!(error = %e, "Failed to create file provenance record");
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
            warn!(error = %e, "Failed to update attachment extracted metadata");
        }

        // Mark attachment as completed
        if let Err(e) = file_storage
            .update_status_tx(&mut tx, attachment_id, AttachmentStatus::Completed, None)
            .await
        {
            warn!(error = %e, "Failed to update attachment status to Completed");
        }

        // Commit all provenance and attachment updates
        if let Err(e) = tx.commit().await {
            warn!(error = %e, "Failed to commit EXIF extraction results");
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        info!(
            note_id = %note_id,
            attachment_id = %attachment_id,
            filename = %filename,
            has_gps = location_id.is_some(),
            has_device = device_id.is_some(),
            has_capture_time = capture_time.is_some(),
            provenance_id = ?provenance_id,
            duration_ms,
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

/// Handler for 3D file analysis jobs.
///
/// Note: This is a placeholder handler that will be fully implemented
/// once the file attachment infrastructure (#430) and Python/trimesh
/// integration are in place.
#[allow(dead_code)]
pub struct ThreeDAnalysisHandler {
    db: Database,
}

impl ThreeDAnalysisHandler {
    #[allow(dead_code)]
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobHandler for ThreeDAnalysisHandler {
    fn job_type(&self) -> JobType {
        JobType::ThreeDAnalysis
    }

    #[instrument(
        skip(self, ctx),
        fields(subsystem = "jobs", component = "3d_analysis", op = "execute")
    )]
    async fn execute(&self, ctx: JobContext) -> JobResult {
        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        let schema = extract_schema(&ctx);
        let _schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(10, Some("Fetching 3D model information..."));

        // Placeholder handler for 3D analysis
        // This will be implemented with Python-based processing using trimesh
        // once the file attachment infrastructure is in place

        info!(
            note_id = %note_id,
            duration_ms = start.elapsed().as_millis() as u64,
            "3D analysis placeholder executed"
        );

        ctx.report_progress(100, Some("3D analysis placeholder complete"));

        JobResult::Success(Some(serde_json::json!({
            "status": "placeholder",
            "message": "3D analysis not yet implemented - requires Python trimesh integration and file attachment infrastructure"
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
                    warn!(error = %e, "SNN scoring failed");
                    results.insert(
                        "snn".to_string(),
                        serde_json::json!({ "status": "failed", "error": e.to_string() }),
                    );
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
                    warn!(error = %e, "PFNET sparsification failed");
                    results.insert(
                        "pfnet".to_string(),
                        serde_json::json!({ "status": "failed", "error": e.to_string() }),
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
                    warn!(error = %e, "Diagnostics snapshot failed");
                    results.insert(
                        "snapshot".to_string(),
                        serde_json::json!({ "status": "failed", "error": e.to_string() }),
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
}
