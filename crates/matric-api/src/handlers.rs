//! Job handlers for background processing.
//!
//! Ported from HOTM's enhanced NLP pipeline for contextual note enhancement.
//! Supports multiple revision modes to control AI enhancement aggressiveness.

use async_trait::async_trait;
use tracing::{debug, info, warn};

use matric_core::{
    EmbeddingBackend, EmbeddingRepository, GenerationBackend, JobType, LinkRepository,
    NoteRepository, RevisionMode, SearchHit,
};
use matric_db::Database;
use matric_inference::OllamaBackend;
use matric_jobs::{JobContext, JobHandler, JobResult};

/// Handler for AI revision jobs - enhanced with context from related notes.
pub struct AiRevisionHandler {
    db: Database,
    backend: OllamaBackend,
}

impl AiRevisionHandler {
    pub fn new(db: Database, backend: OllamaBackend) -> Self {
        Self { db, backend }
    }

    /// Get related notes for contextual enhancement (similarity > 50%)
    async fn get_related_notes(&self, note_id: uuid::Uuid, content: &str) -> Vec<SearchHit> {
        // Generate embedding for the content to find related notes
        let chunks = vec![content.chars().take(500).collect::<String>()];
        let vectors = match self.backend.embed_texts(&chunks).await {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let query_vec = match vectors.into_iter().next() {
            Some(v) => v,
            None => return vec![],
        };

        // Find similar notes
        match self.db.embeddings.find_similar(&query_vec, 10, true).await {
            Ok(hits) => hits
                .into_iter()
                .filter(|hit| hit.score > 0.5 && hit.note_id != note_id)
                .take(5)
                .collect(),
            Err(_) => vec![],
        }
    }

    /// Build context string from related notes
    fn build_related_context(&self, related_notes: &[SearchHit]) -> String {
        if related_notes.is_empty() {
            return String::new();
        }

        let mut context = String::from("Related concepts from the knowledge base:\n");
        for hit in related_notes.iter().take(3) {
            if let Some(snippet) = &hit.snippet {
                let preview: String = snippet.chars().take(150).collect();
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

    async fn execute(&self, ctx: JobContext) -> JobResult {
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

        // Build prompt based on revision mode
        let (prompt, related_count) = match revision_mode {
            RevisionMode::Full => {
                ctx.report_progress(20, Some("Finding related notes for context..."));
                let related_notes = self.get_related_notes(note_id, original_content).await;
                let related_context = self.build_related_context(&related_notes);
                let count = related_notes.len();

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
                (prompt, count)
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
                (prompt, 0)
            }
            RevisionMode::None => unreachable!(), // Already handled above
        };

        let revised = match self.backend.generate(&prompt).await {
            Ok(r) => clean_enhanced_content(r.trim()),
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

        ctx.report_progress(100, Some("Revision complete"));
        info!(
            note_id = %note_id,
            mode = ?revision_mode,
            related_count = related_count,
            "AI revision completed in {:?} mode",
            revision_mode
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

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

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

        // Better chunking - by lines up to max length (ported from HOTM)
        let chunks = chunk_text(content, 1000);
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
        info!(note_id = %note_id, chunks = chunk_count, "Embeddings generated");

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

    /// Get related notes for title context
    async fn get_related_notes(&self, note_id: uuid::Uuid, content: &str) -> Vec<SearchHit> {
        let chunks = vec![content.chars().take(500).collect::<String>()];
        let vectors = match self.backend.embed_texts(&chunks).await {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let query_vec = match vectors.into_iter().next() {
            Some(v) => v,
            None => return vec![],
        };

        match self.db.embeddings.find_similar(&query_vec, 10, true).await {
            Ok(hits) => hits
                .into_iter()
                .filter(|hit| hit.score > 0.5 && hit.note_id != note_id)
                .take(5)
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

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

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

        // Build related context
        let mut related_context = String::new();
        if !related_notes.is_empty() {
            related_context.push_str("Related concepts from your knowledge base:\n");
            for hit in related_notes.iter().take(3) {
                if let Some(snippet) = &hit.snippet {
                    let preview: String = snippet.chars().take(100).collect();
                    related_context.push_str(&format!("- {}\n", preview));
                }
            }
            related_context.push('\n');
        }

        ctx.report_progress(60, Some("Generating contextual title..."));

        let content_preview: String = content.chars().take(500).collect();

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
                    .take(80)
                    .collect::<String>()
                    .trim()
                    .to_string()
            }
            Err(e) => return JobResult::Failed(format!("Title generation failed: {}", e)),
        };

        if title.is_empty() || title.len() < 3 {
            return JobResult::Failed("Invalid title generated".into());
        }

        ctx.report_progress(80, Some("Saving title..."));

        // Save the title
        if let Err(e) = self.db.notes.update_title(note_id, &title).await {
            return JobResult::Failed(format!("Failed to save title: {}", e));
        }

        info!(note_id = %note_id, title = %title, "Title generated and saved");

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
}

#[async_trait]
impl JobHandler for LinkingHandler {
    fn job_type(&self) -> JobType {
        JobType::Linking
    }

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        ctx.report_progress(20, Some("Finding embeddings..."));

        // Get embeddings for this note
        let embeddings = match self.db.embeddings.get_for_note(note_id).await {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "No embeddings for note, skipping linking");
                return JobResult::Success(Some(serde_json::json!({"links_created": 0})));
            }
        };

        if embeddings.is_empty() {
            return JobResult::Success(Some(serde_json::json!({"links_created": 0})));
        }

        ctx.report_progress(40, Some("Searching for similar notes..."));

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

        ctx.report_progress(60, Some("Creating bidirectional links..."));

        let mut created = 0;
        for hit in similar {
            // Skip self and low scores (threshold 0.7 for semantic links)
            if hit.note_id == note_id || hit.score < 0.7 {
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
        info!(note_id = %note_id, links = created, "Created {} bidirectional links", created);

        JobResult::Success(Some(serde_json::json!({
            "links_created": created
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

    async fn execute(&self, ctx: JobContext) -> JobResult {
        let note_id = match ctx.note_id() {
            Some(id) => id,
            None => return JobResult::Failed("No note_id provided".into()),
        };

        ctx.report_progress(20, Some("Finding linked notes..."));

        // Get outgoing semantic links with high scores
        let links = match self.db.links.get_outgoing(note_id).await {
            Ok(l) => l
                .into_iter()
                .filter(|l| l.score > 0.75)
                .take(3)
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
                        linked_note.revised.content.chars().take(200).collect()
                    } else {
                        linked_note.original.content.chars().take(200).collect()
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
            Ok(c) => clean_enhanced_content(c.trim()),
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
        info!(note_id = %note_id, links = links.len(), "Added Related Context section");

        JobResult::Success(Some(serde_json::json!({
            "updated": true,
            "links_referenced": links.len()
        })))
    }
}

// =============================================================================
// UTILITY FUNCTIONS (ported from HOTM)
// =============================================================================

/// Clean up enhanced content to remove any accidental markers
fn clean_enhanced_content(content: &str) -> String {
    let mut cleaned = content.to_string();

    // Remove common markers that might slip through
    let markers = [
        "PART 1",
        "PART 2",
        "ENHANCED NOTE",
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

    cleaned.trim().to_string()
}

/// Chunk text into smaller pieces for embedding (line-aware)
fn chunk_text(text: &str, max_len: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for line in text.lines() {
        if current_chunk.len() + line.len() > max_len && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }
        current_chunk.push_str(line);
        current_chunk.push('\n');
    }

    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}
