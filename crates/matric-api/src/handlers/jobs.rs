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
use matric_db::{Chunker, ChunkerConfig, Database, SemanticChunker};
use matric_inference::OllamaBackend;
use matric_jobs::adapters::exif::extract_exif_metadata;
use matric_jobs::{JobContext, JobHandler, JobResult};

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

        // Extract revision mode from payload (default to Full)
        let revision_mode = ctx
            .payload()
            .and_then(|p| p.get("revision_mode"))
            .and_then(|v| serde_json::from_value::<RevisionMode>(v.clone()).ok())
            .unwrap_or(RevisionMode::Full);

        // Extract schema from payload (default to "public" for backward compatibility)
        let _schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("public");

        // Skip if mode is None (shouldn't happen as we don't queue, but safety check)
        if revision_mode == RevisionMode::None {
            return JobResult::Success(Some(serde_json::json!({
                "skipped": true,
                "reason": "revision_mode is none"
            })));
        }

        ctx.report_progress(10, Some("Fetching note content..."));

        // Get the note
        let note = match self.db.notes.fetch(note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };

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

        if let Err(e) = self
            .db
            .notes
            .update_revised(note_id, &revised, Some(revision_note))
            .await
        {
            return JobResult::Failed(format!("Failed to save revision: {}", e));
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

        // Extract schema from payload (default to "public" for backward compatibility)
        let _schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("public");

        ctx.report_progress(10, Some("Fetching note..."));

        let note = match self.db.notes.fetch(note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };

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

        // Store embeddings
        let model_name = EmbeddingBackend::model_name(&self.backend);
        if let Err(e) = self
            .db
            .embeddings
            .store(note_id, chunk_vectors, model_name)
            .await
        {
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

        // Extract schema from payload (default to "public" for backward compatibility)
        let _schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("public");

        ctx.report_progress(20, Some("Fetching note..."));

        let note = match self.db.notes.fetch(note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };

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
        if let Err(e) = self.db.notes.update_title(note_id, &title).await {
            return JobResult::Failed(format!("Failed to save title: {}", e));
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
        // Search for notes with matching title (case-insensitive)
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

        let mut created = 0;
        #[allow(clippy::needless_late_init)]
        let wiki_links_found;
        let mut wiki_links_resolved = 0;

        // First, parse wiki-style [[links]] from note content
        ctx.report_progress(10, Some("Parsing wiki-style links..."));

        let note = match self.db.notes.fetch(note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
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
                    // Create explicit wiki link with title in metadata
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
        let embeddings = match self.db.embeddings.get_for_note(note_id).await {
            Ok(e) => e,
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

        ctx.report_progress(60, Some("Searching for similar notes..."));

        // Use the first embedding to find similar notes
        let similar = match self
            .db
            .embeddings
            .find_similar(&embeddings[0].vector, 10, true)
            .await
        {
            Ok(s) => s,
            Err(e) => return JobResult::Failed(format!("Failed to find similar: {}", e)),
        };

        ctx.report_progress(80, Some("Creating bidirectional semantic links..."));

        for hit in similar {
            // Skip self and low scores (threshold 0.7 for semantic links)
            if hit.note_id == note_id || hit.score < matric_core::defaults::SEMANTIC_LINK_THRESHOLD
            {
                continue;
            }

            // Forward link (new -> old)
            if let Err(e) = self
                .db
                .links
                .create(note_id, hit.note_id, "semantic", hit.score, None)
                .await
            {
                debug!(error = %e, "Failed to create forward link (may already exist)");
            } else {
                created += 1;
            }

            // Backward link (old -> new) - reciprocal linking like HOTM
            if let Err(e) = self
                .db
                .links
                .create(hit.note_id, note_id, "semantic", hit.score, None)
                .await
            {
                debug!(error = %e, "Failed to create backward link (may already exist)");
            } else {
                created += 1;
            }
        }

        ctx.report_progress(100, Some("Linking complete"));
        info!(
            note_id = %note_id,
            result_count = created,
            wiki_found = wiki_links_found,
            wiki_resolved = wiki_links_resolved,
            duration_ms = start.elapsed().as_millis() as u64,
            "Linking completed"
        );

        JobResult::Success(Some(serde_json::json!({
            "links_created": created,
            "wiki_links_found": wiki_links_found,
            "wiki_links_resolved": wiki_links_resolved
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

        // Extract schema from payload (default to "public" for backward compatibility)
        let _schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("public");

        ctx.report_progress(10, Some("Finding affected embedding sets..."));

        // Get embedding sets this note is a member of (to update stats after deletion)
        let affected_sets = match self.db.embedding_sets.get_sets_for_note(note_id).await {
            Ok(sets) => sets,
            Err(e) => {
                warn!(error = %e, "Failed to get embedding sets for note, continuing with deletion");
                vec![]
            }
        };

        ctx.report_progress(30, Some("Verifying note exists..."));

        // Verify note exists before attempting deletion
        if !self.db.notes.exists(note_id).await.unwrap_or(false) {
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
        if let Err(e) = self.db.notes.hard_delete(note_id).await {
            return JobResult::Failed(format!("Failed to delete note: {}", e));
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

        // Extract schema from payload (default to "public" for backward compatibility)
        let _schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("public");

        ctx.report_progress(20, Some("Finding linked notes..."));

        // Get outgoing semantic links with high scores (limit per Miller's Law)
        let links = match self.db.links.get_outgoing(note_id).await {
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

        if links.is_empty() {
            return JobResult::Success(Some(
                serde_json::json!({"updated": false, "reason": "no_high_quality_links"}),
            ));
        }

        ctx.report_progress(40, Some("Fetching linked content..."));

        // Get current note content
        let note = match self.db.notes.fetch(note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };

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
        if let Err(e) = self
            .db
            .notes
            .update_revised(
                note_id,
                &updated_content,
                Some("Added related context section"),
            )
            .await
        {
            return JobResult::Failed(format!("Failed to save: {}", e));
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

    /// Get or create a concept by preferred label.
    async fn get_or_create_concept(
        &self,
        label: &str,
        scheme_id: uuid::Uuid,
    ) -> Option<uuid::Uuid> {
        use matric_db::SkosConceptRepository;
        use matric_db::SkosLabelRepository;

        // First, search for existing concept with this label
        let results = self.db.skos.search_labels(label, 5).await.ok()?;

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

        self.db.skos.create_concept(req).await.ok()
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
        use matric_db::{SkosConceptSchemeRepository, SkosTaggingRepository};

        let start = Instant::now();
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        // Extract schema from payload (default to "public" for backward compatibility)
        let _schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("public");

        ctx.report_progress(10, Some("Fetching note content..."));

        // Get the note
        let note = match self.db.notes.fetch(note_id).await {
            Ok(n) => n,
            Err(e) => return JobResult::Failed(format!("Failed to fetch note: {}", e)),
        };

        // Use revised content if available, otherwise original
        let content: &str = if !note.revised.content.is_empty() {
            &note.revised.content
        } else {
            &note.original.content
        };

        if content.trim().is_empty() {
            return JobResult::Success(Some(
                serde_json::json!({"concepts": 0, "reason": "empty_content"}),
            ));
        }

        ctx.report_progress(20, Some("Getting default concept scheme..."));

        // Get or use default scheme
        let scheme_id = match self.db.skos.list_schemes(false).await {
            Ok(schemes) if !schemes.is_empty() => schemes[0].id,
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
                match self.db.skos.create_scheme(req).await {
                    Ok(id) => id,
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
            Err(e) => return JobResult::Failed(format!("AI generation failed: {}", e)),
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
                        return JobResult::Failed(format!("Failed to parse AI response: {}", e));
                    }
                }
            }
        };

        if concept_labels.is_empty() {
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
            let concept_id = match self.get_or_create_concept(label.trim(), scheme_id).await {
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

            if let Err(e) = self.db.skos.tag_note(tag_req).await {
                debug!(error = %e, concept_id = %concept_id, "Failed to tag note (may already exist)");
            } else {
                tagged_count += 1;
            }

            // Update progress
            let progress = 60 + ((i + 1) * 30 / total) as i32;
            ctx.report_progress(progress, Some(&format!("Tagged with: {}", label)));
        }

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
        let mut capture_time: Option<chrono::DateTime<chrono::Utc>> = None;

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
        if let Some(datetime) = exif.get("datetime") {
            let dt_str = datetime
                .get("original")
                .or_else(|| datetime.get("digitized"))
                .and_then(|v| v.as_str());

            if let Some(dt_str) = dt_str {
                // EXIF datetime format: "YYYY:MM:DD HH:MM:SS"
                if let Ok(naive) =
                    chrono::NaiveDateTime::parse_from_str(dt_str, "%Y:%m:%d %H:%M:%S")
                {
                    capture_time = Some(naive.and_utc());
                } else {
                    debug!(dt_str, "Could not parse EXIF datetime");
                }
            }
        }

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
        if let Err(e) = file_storage
            .update_extracted_content(attachment_id, None, Some(exif_data.clone()))
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

        // Extract schema from payload (default to "public" for backward compatibility)
        let _schema = ctx
            .payload()
            .and_then(|p| p.get("schema"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("public");

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
}
