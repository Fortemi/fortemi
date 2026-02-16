//! Job handlers for background processing.
//!
//! Ported from HOTM's enhanced NLP pipeline for contextual note enhancement.
//! Supports multiple revision modes to control AI enhancement aggressiveness.

use std::time::Instant;

use async_trait::async_trait;
use tracing::{debug, info, instrument, warn};

use matric_core::{
    AttachmentStatus, CreateFileProvenanceRequest, CreateProvDeviceRequest,
    CreateProvLocationRequest, DocumentTypeRepository, EmbeddingBackend, EmbeddingRepository,
    GenerationBackend, JobRepository, JobType, LinkRepository, NoteRepository, ProvRelation,
    RevisionMode, SearchHit,
};
use matric_db::{Chunker, ChunkerConfig, Database, SchemaContext, SemanticChunker};
use matric_inference::OllamaBackend;
use matric_jobs::adapters::exif::{
    extract_exif_metadata, parse_exif_datetime, prepare_attachment_metadata,
};
use matric_jobs::{JobContext, JobHandler, JobResult};

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
}

impl AiRevisionHandler {
    pub fn new(db: Database, backend: OllamaBackend) -> Self {
        Self { db, backend }
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
                Some(matric_core::GenerationBackend::model_name(&self.backend)),
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

        let revised = match self.backend.generate(&prompt).await {
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

        ctx.report_progress(10, Some("Fetching note..."));

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

        // Use revised content if available, otherwise original
        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            return JobResult::Success(Some(serde_json::json!({"chunks": 0})));
        }

        ctx.report_progress(30, Some("Chunking content..."));

        // Resolve chunking config from database with priority chain:
        // 1. Note's document type (if assigned)
        // 2. Default embedding config (global fallback)
        // 3. ChunkerConfig::default() (hardcoded fallback)
        let chunker_config = if let Some(doc_type_id) = note.note.document_type_id {
            // Try to fetch document type configuration
            if let Ok(Some(doc_type)) = self.db.document_types.get(doc_type_id).await {
                let max = doc_type.chunk_size_default as usize;
                ChunkerConfig {
                    max_chunk_size: max,
                    min_chunk_size: (max / 10).max(50),
                    overlap: doc_type.chunk_overlap_default as usize,
                }
            } else if let Ok(Some(config)) = self.db.embedding_sets.get_default_config().await {
                // Document type not found, fall back to default embedding config
                let max = config.chunk_size as usize;
                ChunkerConfig {
                    max_chunk_size: max,
                    min_chunk_size: (max / 10).max(50),
                    overlap: config.chunk_overlap as usize,
                }
            } else {
                // No document type or default config, use hardcoded defaults
                ChunkerConfig::default()
            }
        } else if let Ok(Some(config)) = self.db.embedding_sets.get_default_config().await {
            // No document type assigned, use default embedding config
            let max = config.chunk_size as usize;
            ChunkerConfig {
                max_chunk_size: max,
                min_chunk_size: (max / 10).max(50),
                overlap: config.chunk_overlap as usize,
            }
        } else {
            // No document type and no default config, use hardcoded defaults
            ChunkerConfig::default()
        };

        let chunker = SemanticChunker::new(chunker_config);
        let semantic_chunks = chunker.chunk(content);
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
        let embedding_set_id = ctx
            .payload()
            .and_then(|p| p.get("embedding_set_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok());

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

/// Handler for title generation jobs - uses related notes for context.
pub struct TitleGenerationHandler {
    db: Database,
    backend: OllamaBackend,
}

impl TitleGenerationHandler {
    pub fn new(db: Database, backend: OllamaBackend) -> Self {
        Self { db, backend }
    }

    /// Get related notes for title context.
    ///
    /// Returns up to MAX_CONTEXT_NOTES results, respecting Miller's Law (7±2).
    async fn get_related_notes(&self, note_id: uuid::Uuid, content: &str) -> Vec<SearchHit> {
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
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
        };

        ctx.report_progress(20, Some("Fetching note..."));

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

        ctx.report_progress(40, Some("Finding related notes..."));

        // Get related notes for context (ported from HOTM)
        let related_notes = self.get_related_notes(note_id, content).await;

        // Build related context (limit to MAX_PROMPT_SNIPPETS per Miller's Law)
        let mut related_context = String::new();
        if !related_notes.is_empty() {
            related_context.push_str("Related concepts from your knowledge base:\n");
            for hit in related_notes.iter().take(MAX_PROMPT_SNIPPETS) {
                if let Some(snippet) = &hit.snippet {
                    let preview: String = snippet
                        .chars()
                        .take(matric_core::defaults::PREVIEW_LABEL)
                        .collect();
                    related_context.push_str(&format!("- {}\n", preview));
                }
            }
            related_context.push('\n');
        }

        ctx.report_progress(60, Some("Generating contextual title..."));

        let content_preview: String = content
            .chars()
            .take(matric_core::defaults::PREVIEW_EMBEDDING)
            .collect();

        // Enhanced title prompt (ported from HOTM)
        let prompt = format!(
            r#"You are an expert at creating concise, descriptive titles for notes and documents. Your task is to generate a clear, informative title that captures the essence of the content and its place in the broader knowledge base.

Content to title:
{}

{}Guidelines for the title:
1. Keep it between 3-8 words
2. Be specific and descriptive
3. Capture the main concept or purpose
4. Consider the context of related notes
5. Use natural, readable language
6. Avoid generic words like "Note", "Document", "Text"
7. Focus on the actual subject matter

Examples of good titles:
- "Machine Learning Model Deployment Pipeline"
- "React State Management Patterns"
- "Database Index Optimization Strategies"
- "Team Meeting Notes - Project Alpha"
- "Python Data Processing Workflow"

Generate only the title, no quotes, no explanations."#,
            content_preview, related_context
        );

        let title = match self.backend.generate(&prompt).await {
            Ok(t) => {
                let cleaned = t
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .replace('\n', " ");
                // Take first 80 chars max
                cleaned
                    .chars()
                    .take(matric_core::defaults::TITLE_MAX_LENGTH)
                    .collect::<String>()
                    .trim()
                    .to_string()
            }
            Err(e) => return JobResult::Failed(format!("Title generation failed: {}", e)),
        };

        if title.is_empty() || title.len() < matric_core::defaults::TITLE_MIN_LENGTH {
            return JobResult::Failed("Invalid title generated".into());
        }

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

        info!(
            note_id = %note_id,
            title = %title,
            duration_ms = start.elapsed().as_millis() as u64,
            "Title generated"
        );

        ctx.report_progress(100, Some("Title generation completed"));

        JobResult::Success(Some(serde_json::json!({
            "title": title,
            "related_notes_used": related_notes.len()
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
}

impl ContextUpdateHandler {
    pub fn new(db: Database, backend: OllamaBackend) -> Self {
        Self { db, backend }
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
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
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

        let updated_content = match self.backend.generate(&prompt).await {
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
}

impl ConceptTaggingHandler {
    pub fn new(db: Database, backend: OllamaBackend) -> Self {
        Self { db, backend }
    }

    /// Queue a Linking job as Phase 2 of the NLP pipeline (#420).
    /// Called on all exit paths to ensure linking runs even if tagging
    /// produces no tags (embedding-only linking still works).
    async fn queue_linking_job(&self, note_id: uuid::Uuid, schema: &str) {
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
                JobType::Linking,
                JobType::Linking.default_priority(),
                payload,
            )
            .await
        {
            warn!(%note_id, error = %e, "Failed to queue phase-2 linking job");
        }
    }

    /// Get or create a concept by preferred label.
    async fn get_or_create_concept(
        &self,
        label: &str,
        scheme_id: uuid::Uuid,
        schema_ctx: &SchemaContext,
    ) -> Option<uuid::Uuid> {
        // First, search for existing concept with this label
        let results = {
            let mut tx = schema_ctx.begin_tx().await.ok()?;
            let r = self
                .db
                .skos
                .search_labels_tx(&mut tx, label, 5)
                .await
                .ok()?;
            tx.commit().await.ok();
            r
        };

        // Check for exact match (case-insensitive)
        let label_lower = label.to_lowercase();
        for concept in &results {
            if let Some(pref) = &concept.pref_label {
                if pref.to_lowercase() == label_lower {
                    return Some(concept.concept.id);
                }
            }
        }

        // No exact match found - create new concept
        let req = matric_core::CreateConceptRequest {
            scheme_id,
            notation: None, // Auto-generated
            pref_label: label.to_string(),
            language: "en".to_string(),
            status: matric_core::TagStatus::Candidate,
            facet_type: None,
            facet_source: None,
            facet_domain: None,
            facet_scope: None,
            definition: Some("Auto-created by AI concept tagging".to_string()),
            scope_note: None,
            broader_ids: vec![],
            related_ids: vec![],
            alt_labels: vec![],
        };

        let mut tx = schema_ctx.begin_tx().await.ok()?;
        let id = self.db.skos.create_concept_tx(&mut tx, req).await.ok()?;
        tx.commit().await.ok();
        Some(id)
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
        let schema_ctx = match schema_context(&self.db, schema) {
            Ok(ctx) => ctx,
            Err(e) => return e,
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
        tx.commit().await.ok();

        // Use revised content if available, otherwise original
        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            self.queue_linking_job(note_id, schema).await;
            return JobResult::Success(Some(
                serde_json::json!({"concepts": 0, "reason": "empty_content"}),
            ));
        }

        ctx.report_progress(20, Some("Getting default concept scheme..."));

        // Get or use default scheme
        let mut tx = match schema_ctx.begin_tx().await {
            Ok(t) => t,
            Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
        };
        let scheme_id = match self.db.skos.list_schemes_tx(&mut tx, false).await {
            Ok(schemes) if !schemes.is_empty() => {
                let id = schemes[0].id;
                tx.commit().await.ok();
                id
            }
            Ok(_) => {
                // Create default scheme if none exists
                let req = matric_core::CreateConceptSchemeRequest {
                    notation: "default".to_string(),
                    title: "Default Concept Scheme".to_string(),
                    uri: None,
                    description: Some(
                        "Auto-created default scheme for AI-generated concepts".to_string(),
                    ),
                    creator: None,
                    publisher: None,
                    rights: None,
                    version: None,
                };
                match self.db.skos.create_scheme_tx(&mut tx, req).await {
                    Ok(id) => {
                        tx.commit().await.ok();
                        id
                    }
                    Err(e) => {
                        return JobResult::Failed(format!("Failed to create default scheme: {}", e))
                    }
                }
            }
            Err(e) => return JobResult::Failed(format!("Failed to get schemes: {}", e)),
        };

        ctx.report_progress(30, Some("Analyzing content for concepts..."));

        // Take content preview for analysis
        let content_preview: String = content
            .chars()
            .take(matric_core::defaults::PREVIEW_TAGGING)
            .collect();

        // Generate concept suggestions using AI
        let prompt = format!(
            r#"You are a knowledge organization specialist. Analyze the following note content and suggest 3-7 specific SKOS-style concept labels that describe its main topics.

Content:
{}

Guidelines:
1. Use specific, descriptive terms (not generic words like "note", "important", "todo")
2. Use noun phrases or compound terms (e.g., "machine learning", "database optimization", "rust programming")
3. Focus on the actual subject matter and key concepts
4. Keep labels concise (1-4 words each)
5. Order by relevance (most relevant first)

Output ONLY a JSON array of concept labels, nothing else. Example:
["machine learning", "neural networks", "python programming", "data preprocessing"]"#,
            content_preview
        );

        let ai_response = match self.backend.generate(&prompt).await {
            Ok(r) => r.trim().to_string(),
            Err(e) => {
                // Still queue linking even if AI tagging fails — linking works with pure embeddings
                self.queue_linking_job(note_id, schema).await;
                return JobResult::Failed(format!("AI generation failed: {}", e));
            }
        };

        ctx.report_progress(50, Some("Parsing concept suggestions..."));

        // Parse the AI response as JSON array
        let concept_labels: Vec<String> = match serde_json::from_str(&ai_response) {
            Ok(labels) => labels,
            Err(_) => {
                // Try to extract labels if response isn't clean JSON
                let cleaned = ai_response
                    .trim()
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();
                match serde_json::from_str(cleaned) {
                    Ok(labels) => labels,
                    Err(e) => {
                        warn!(error = %e, response = %ai_response, "Failed to parse AI concept suggestions");
                        self.queue_linking_job(note_id, schema).await;
                        return JobResult::Failed(format!("Failed to parse AI response: {}", e));
                    }
                }
            }
        };

        if concept_labels.is_empty() {
            self.queue_linking_job(note_id, schema).await;
            return JobResult::Success(Some(serde_json::json!({
                "concepts": 0,
                "reason": "no_concepts_suggested"
            })));
        }

        ctx.report_progress(60, Some("Creating/matching concepts..."));

        // Get or create concepts and tag the note
        let mut tagged_count = 0;
        let total = concept_labels.len();

        for (i, label) in concept_labels.iter().enumerate() {
            // Skip empty or too-short labels
            if label.trim().len() < 2 {
                continue;
            }

            let is_primary = i == 0; // First concept is primary
            let relevance = 1.0_f32 - (i as f32 * matric_core::defaults::RELEVANCE_DECAY_FACTOR); // Decreasing relevance

            // Check if concept already exists by searching labels
            let concept_id = match self
                .get_or_create_concept(label.trim(), scheme_id, &schema_ctx)
                .await
            {
                Some(id) => id,
                None => {
                    warn!(label = %label, "Failed to get or create concept");
                    continue;
                }
            };

            // Tag the note with this concept
            let tag_req = matric_core::TagNoteRequest {
                note_id,
                concept_id,
                source: "ai_auto".to_string(),
                confidence: Some(matric_core::defaults::AI_TAGGING_CONFIDENCE),
                relevance_score: relevance,
                is_primary,
                created_by: None,
            };

            let mut tx = match schema_ctx.begin_tx().await {
                Ok(t) => t,
                Err(e) => return JobResult::Failed(format!("Schema tx failed: {}", e)),
            };
            let result = self.db.skos.tag_note_tx(&mut tx, tag_req).await;
            tx.commit().await.ok();
            if let Err(e) = result {
                debug!(error = %e, concept_id = %concept_id, "Failed to tag note (may already exist)");
            } else {
                tagged_count += 1;
            }

            // Update progress
            let progress = 60 + ((i + 1) * 30 / total) as i32;
            ctx.report_progress(progress, Some(&format!("Tagged with: {}", label)));
        }

        ctx.report_progress(95, Some("Queuing phase-2 linking job..."));

        // Queue Linking as Phase 2 of the NLP pipeline (#420).
        // Tags now exist, so the linker can blend SKOS tag overlap with embedding similarity.
        self.queue_linking_job(note_id, schema).await;

        ctx.report_progress(100, Some("Concept tagging complete"));
        info!(
            note_id = %note_id,
            result_count = tagged_count,
            concepts_suggested = concept_labels.len(),
            duration_ms = start.elapsed().as_millis() as u64,
            "Concept tagging completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "concepts_tagged": tagged_count,
            "concepts_suggested": concept_labels.len(),
            "labels": concept_labels
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
                .queue(Some(*note_id), JobType::Embedding, 5, None)
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
            // No explicit attachment_id — find image attachments for this note
            let attachments = match file_storage.list_by_note(note_id).await {
                Ok(a) => a,
                Err(e) => return JobResult::Failed(format!("Failed to list attachments: {}", e)),
            };
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

        // Update status to Processing
        if let Err(e) = file_storage
            .update_status(attachment_id, AttachmentStatus::Processing, None)
            .await
        {
            warn!(attachment_id = %attachment_id, error = %e, "Failed to update attachment status to Processing");
        }

        // Download the attachment bytes
        let (data, content_type, filename) = match file_storage.download_file(attachment_id).await {
            Ok(result) => result,
            Err(e) => {
                let _ = file_storage
                    .update_status(
                        attachment_id,
                        AttachmentStatus::Failed,
                        Some(&e.to_string()),
                    )
                    .await;
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
                // Still mark as completed — no EXIF is a valid outcome
                if let Err(e) = file_storage
                    .update_status(attachment_id, AttachmentStatus::Completed, None)
                    .await
                {
                    warn!(error = %e, "Failed to update attachment status to Completed");
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
                match self.db.memory_search.create_prov_location(&req).await {
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
            match self.db.memory_search.create_prov_agent_device(&req).await {
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
            .create_file_provenance(&prov_req)
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

        // Persist extracted EXIF metadata on the attachment
        // Store the unwrapped exif content directly (without the "exif" wrapper)
        // so fields are accessible as extracted_metadata.gps.latitude, etc.
        let metadata = prepare_attachment_metadata(&exif_data, capture_time)
            .unwrap_or_else(|| exif_data.clone());
        if let Err(e) = file_storage
            .update_extracted_content(attachment_id, None, Some(metadata))
            .await
        {
            warn!(error = %e, "Failed to update attachment extracted metadata");
        }

        // Mark attachment as completed
        if let Err(e) = file_storage
            .update_status(attachment_id, AttachmentStatus::Completed, None)
            .await
        {
            warn!(error = %e, "Failed to update attachment status to Completed");
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
}
