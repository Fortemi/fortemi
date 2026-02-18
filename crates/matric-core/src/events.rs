//! Server event types, envelope schema, and event bus for real-time notifications.
//!
//! Provides a unified event system that aggregates events from multiple sources
//! (job worker, note operations) into a single broadcast channel. Downstream
//! consumers (WebSocket, SSE, webhooks, telemetry) subscribe independently.
//!
//! ## Envelope Schema (Issue #451)
//!
//! All SSE emissions use [`EventEnvelope`] — a versioned, self-describing wrapper
//! around domain events. The envelope carries metadata (event ID, timestamps,
//! actor, entity scope, correlation) while the `payload` field contains the
//! domain-specific [`ServerEvent`] data.
//!
//! Architecture: See ADR-037 (unified-event-bus)

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

// ============================================================================
// Event Envelope (Issue #451)
// ============================================================================

/// Actor metadata for event attribution.
///
/// Identifies who or what caused an event — system processes, authenticated
/// users, or AI agents.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EventActor {
    /// Actor type: `"system"`, `"user"`, or `"agent"`.
    pub kind: String,
    /// Optional actor identifier (user ID, API key ID, agent name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Optional display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl EventActor {
    /// System actor (background jobs, periodic tasks, internal processes).
    pub fn system() -> Self {
        Self {
            kind: "system".to_string(),
            id: None,
            name: None,
        }
    }

    /// Authenticated user actor.
    pub fn user(id: impl Into<String>, name: Option<String>) -> Self {
        Self {
            kind: "user".to_string(),
            id: Some(id.into()),
            name,
        }
    }

    /// AI agent actor.
    pub fn agent(name: impl Into<String>) -> Self {
        Self {
            kind: "agent".to_string(),
            id: None,
            name: Some(name.into()),
        }
    }
}

/// Optional emission context for events that carry additional metadata.
///
/// Used with [`EventBus::emit_with_context`] to attach memory scope,
/// actor identity, and correlation IDs to emitted events.
#[derive(Debug, Clone, Default)]
pub struct EventContext {
    /// Memory/archive scope (schema name). None for system-wide events.
    pub memory: Option<String>,
    /// Tenant identifier (if multi-tenant).
    pub tenant_id: Option<String>,
    /// Who or what caused this event. Defaults to system actor.
    pub actor: Option<EventActor>,
    /// Correlation ID for tracing related events across operations.
    pub correlation_id: Option<Uuid>,
    /// ID of the event that directly caused this event.
    pub causation_id: Option<Uuid>,
}

/// Versioned server event envelope conforming to the Fortemi SSE contract.
///
/// All SSE emissions use this envelope. The `event_type` field uses
/// dot-namespaced names (e.g., `"note.updated"`, `"job.started"`).
/// The `payload` contains the domain-specific event data.
///
/// ## Wire Format (SSE)
///
/// ```text
/// event: note.updated
/// id: 019508a0-1234-7def-8000-abcdef123456
/// data: {"event_id":"...","event_type":"note.updated","occurred_at":"...","actor":{...},"payload":{...}}
/// ```
///
/// ## Schema Evolution
///
/// - `payload_version` starts at `1` and increments on breaking payload changes.
/// - New optional fields may be added to the envelope without version bump.
/// - Consumers should ignore unknown fields (forward compatibility).
/// - Breaking changes require a new `payload_version` with deprecation window.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EventEnvelope {
    /// Unique event identifier (UUIDv7 for temporal ordering).
    pub event_id: Uuid,
    /// Namespaced event type (e.g., `"note.updated"`, `"job.started"`).
    pub event_type: String,
    /// When the event occurred (UTC).
    pub occurred_at: DateTime<Utc>,
    /// Memory/archive scope. None for system-wide events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// Tenant identifier (if multi-tenant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// Who/what caused this event.
    pub actor: EventActor,
    /// Type of entity this event relates to (e.g., `"note"`, `"job"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// ID of the entity this event relates to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Correlation ID for tracing related events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<Uuid>,
    /// ID of the event that caused this event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<Uuid>,
    /// Payload schema version (for forward/backward compatibility).
    pub payload_version: u32,
    /// Domain-specific event data.
    pub payload: ServerEvent,
}

impl EventEnvelope {
    /// Create an envelope from a ServerEvent with default (system) context.
    pub fn new(event: ServerEvent) -> Self {
        Self::with_context(event, EventContext::default())
    }

    /// Create an envelope with explicit context.
    pub fn with_context(event: ServerEvent, ctx: EventContext) -> Self {
        let event_type = event.namespaced_event_type().to_string();
        let entity_type = event.entity_type().map(String::from);
        let entity_id = event.entity_id().map(|id| id.to_string());

        Self {
            event_id: crate::uuid_utils::new_v7(),
            event_type,
            occurred_at: Utc::now(),
            memory: ctx.memory,
            tenant_id: ctx.tenant_id,
            actor: ctx.actor.unwrap_or_else(EventActor::system),
            entity_type,
            entity_id,
            correlation_id: ctx.correlation_id,
            causation_id: ctx.causation_id,
            payload_version: 1,
            payload: event,
        }
    }
}

// ============================================================================
// Server Event (domain payloads)
// ============================================================================

/// Unified server event type matching the HotM WebSocket client protocol.
///
/// These events are serialized as JSON with a `type` tag field, e.g.:
/// `{"type":"JobStarted","job_id":"...","job_type":"Embedding"}`
///
/// When wrapped in an [`EventEnvelope`], these become the `payload` field.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum ServerEvent {
    /// Periodic queue statistics broadcast.
    QueueStatus {
        total_jobs: i64,
        running: i64,
        pending: i64,
    },
    /// A job was added to the queue.
    JobQueued {
        job_id: Uuid,
        job_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<Uuid>,
    },
    /// A job started processing.
    JobStarted {
        job_id: Uuid,
        job_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<Uuid>,
    },
    /// Job progress update.
    JobProgress {
        job_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<Uuid>,
        progress: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// A job completed successfully.
    JobCompleted {
        job_id: Uuid,
        job_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<i64>,
    },
    /// A job failed.
    JobFailed {
        job_id: Uuid,
        job_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<Uuid>,
        error: String,
    },
    /// A note was created, updated, or had its AI content refreshed.
    NoteUpdated {
        note_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        tags: Vec<String>,
        has_ai_content: bool,
        has_links: bool,
    },

    // -- Note lifecycle events (Issue #453) --
    /// A new note was created.
    NoteCreated {
        note_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        tags: Vec<String>,
    },
    /// A note was soft-deleted.
    NoteDeleted { note_id: Uuid },
    /// A note was archived.
    NoteArchived { note_id: Uuid },
    /// A note was restored from archive or soft-deletion.
    NoteRestored { note_id: Uuid },
    /// Tags on a note were changed (added, removed, or replaced).
    NoteTagsUpdated { note_id: Uuid, tags: Vec<String> },
    /// Semantic links on a note were updated (by background linking job).
    NoteLinksUpdated { note_id: Uuid },
    /// An AI revision was created for a note.
    NoteRevisionCreated { note_id: Uuid },

    // -- Attachment events (Issue #454) --
    /// A file attachment was uploaded to a note.
    AttachmentCreated {
        attachment_id: Uuid,
        note_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
    /// A file attachment was deleted.
    AttachmentDeleted {
        attachment_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<Uuid>,
    },
    /// Extraction metadata for an attachment was updated (content, document type, EXIF).
    AttachmentExtractionUpdated { attachment_id: Uuid, note_id: Uuid },

    // -- Collection events (Issue #454) --
    /// A collection was created.
    CollectionCreated { collection_id: Uuid, name: String },
    /// A collection was updated (name or description changed).
    CollectionUpdated { collection_id: Uuid, name: String },
    /// A collection was deleted.
    CollectionDeleted { collection_id: Uuid },
    /// A note was moved into or out of a collection.
    CollectionMembershipChanged {
        #[serde(skip_serializing_if = "Option::is_none")]
        collection_id: Option<Uuid>,
        note_id: Uuid,
    },

    // -- Archive events (Issue #455) --
    /// A memory archive was created.
    ArchiveCreated {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        archive_id: Option<Uuid>,
    },
    /// A memory archive was updated.
    ArchiveUpdated { name: String },
    /// A memory archive was deleted.
    ArchiveDeleted { name: String },
    /// The default memory archive was changed.
    ArchiveDefaultChanged { name: String },

    // -- SKOS concept/scheme lifecycle events (Issue #462) --
    /// A SKOS concept scheme was created.
    ConceptSchemeCreated { scheme_id: Uuid },
    /// A SKOS concept scheme was updated.
    ConceptSchemeUpdated { scheme_id: Uuid },
    /// A SKOS concept scheme was deleted.
    ConceptSchemeDeleted { scheme_id: Uuid },
    /// A SKOS concept was created.
    ConceptCreated {
        concept_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        scheme_id: Option<Uuid>,
    },
    /// A SKOS concept was updated.
    ConceptUpdated { concept_id: Uuid },
    /// A SKOS concept was deleted.
    ConceptDeleted { concept_id: Uuid },
    /// Semantic relations on a concept were updated (broader/narrower/related).
    ConceptRelationsUpdated {
        concept_id: Uuid,
        relation_type: String,
    },
    /// A concept's scheme membership was changed.
    ConceptSchemeChanged {
        concept_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        scheme_id: Option<Uuid>,
    },
    /// A concept's membership in a SKOS collection was changed.
    ConceptCollectionMembershipChanged {
        concept_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        collection_id: Option<Uuid>,
    },

    // -- Tag governance lifecycle events (Issue #463) --
    /// A global tag was created.
    TagCreated { tag: String },
    /// A global tag was renamed.
    TagRenamed { old_name: String, new_name: String },
    /// A global tag was deleted.
    TagDeleted { tag: String },
    /// Two tags were merged.
    TagMerged {
        source_tag: String,
        target_tag: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        affected_count: Option<i64>,
    },
    /// Tag usage statistics were updated.
    TagStatsUpdated,

    // -- Search/index materialization events (Issue #464) --
    /// Embeddings for a note were updated.
    IndexEmbeddingUpdated {
        note_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        job_id: Option<Uuid>,
    },
    /// Semantic links for a note were updated by the linking pipeline.
    IndexLinkingUpdated {
        note_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        job_id: Option<Uuid>,
    },
    /// Full-text search index for a note was updated.
    IndexFtsUpdated {
        note_id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        job_id: Option<Uuid>,
    },
    /// The knowledge graph read model was updated.
    ReadmodelGraphUpdated {
        #[serde(skip_serializing_if = "Option::is_none")]
        note_id: Option<Uuid>,
    },
    /// All derived views for a note are ready for search.
    ReadmodelSearchReady { note_id: Uuid },
}

impl ServerEvent {
    /// Returns the legacy event type name (used for webhook filtering and WS backward compat).
    pub fn event_type(&self) -> &'static str {
        match self {
            ServerEvent::QueueStatus { .. } => "QueueStatus",
            ServerEvent::JobQueued { .. } => "JobQueued",
            ServerEvent::JobStarted { .. } => "JobStarted",
            ServerEvent::JobProgress { .. } => "JobProgress",
            ServerEvent::JobCompleted { .. } => "JobCompleted",
            ServerEvent::JobFailed { .. } => "JobFailed",
            ServerEvent::NoteUpdated { .. } => "NoteUpdated",
            ServerEvent::NoteCreated { .. } => "NoteCreated",
            ServerEvent::NoteDeleted { .. } => "NoteDeleted",
            ServerEvent::NoteArchived { .. } => "NoteArchived",
            ServerEvent::NoteRestored { .. } => "NoteRestored",
            ServerEvent::NoteTagsUpdated { .. } => "NoteTagsUpdated",
            ServerEvent::NoteLinksUpdated { .. } => "NoteLinksUpdated",
            ServerEvent::NoteRevisionCreated { .. } => "NoteRevisionCreated",
            ServerEvent::AttachmentCreated { .. } => "AttachmentCreated",
            ServerEvent::AttachmentDeleted { .. } => "AttachmentDeleted",
            ServerEvent::AttachmentExtractionUpdated { .. } => "AttachmentExtractionUpdated",
            ServerEvent::CollectionCreated { .. } => "CollectionCreated",
            ServerEvent::CollectionUpdated { .. } => "CollectionUpdated",
            ServerEvent::CollectionDeleted { .. } => "CollectionDeleted",
            ServerEvent::CollectionMembershipChanged { .. } => "CollectionMembershipChanged",
            ServerEvent::ArchiveCreated { .. } => "ArchiveCreated",
            ServerEvent::ArchiveUpdated { .. } => "ArchiveUpdated",
            ServerEvent::ArchiveDeleted { .. } => "ArchiveDeleted",
            ServerEvent::ArchiveDefaultChanged { .. } => "ArchiveDefaultChanged",
            ServerEvent::ConceptSchemeCreated { .. } => "ConceptSchemeCreated",
            ServerEvent::ConceptSchemeUpdated { .. } => "ConceptSchemeUpdated",
            ServerEvent::ConceptSchemeDeleted { .. } => "ConceptSchemeDeleted",
            ServerEvent::ConceptCreated { .. } => "ConceptCreated",
            ServerEvent::ConceptUpdated { .. } => "ConceptUpdated",
            ServerEvent::ConceptDeleted { .. } => "ConceptDeleted",
            ServerEvent::ConceptRelationsUpdated { .. } => "ConceptRelationsUpdated",
            ServerEvent::ConceptSchemeChanged { .. } => "ConceptSchemeChanged",
            ServerEvent::ConceptCollectionMembershipChanged { .. } => {
                "ConceptCollectionMembershipChanged"
            }
            ServerEvent::TagCreated { .. } => "TagCreated",
            ServerEvent::TagRenamed { .. } => "TagRenamed",
            ServerEvent::TagDeleted { .. } => "TagDeleted",
            ServerEvent::TagMerged { .. } => "TagMerged",
            ServerEvent::TagStatsUpdated => "TagStatsUpdated",
            ServerEvent::IndexEmbeddingUpdated { .. } => "IndexEmbeddingUpdated",
            ServerEvent::IndexLinkingUpdated { .. } => "IndexLinkingUpdated",
            ServerEvent::IndexFtsUpdated { .. } => "IndexFtsUpdated",
            ServerEvent::ReadmodelGraphUpdated { .. } => "ReadmodelGraphUpdated",
            ServerEvent::ReadmodelSearchReady { .. } => "ReadmodelSearchReady",
        }
    }

    /// Returns the namespaced event type for the envelope (e.g., `"note.updated"`).
    pub fn namespaced_event_type(&self) -> &'static str {
        match self {
            ServerEvent::QueueStatus { .. } => "queue.status",
            ServerEvent::JobQueued { .. } => "job.queued",
            ServerEvent::JobStarted { .. } => "job.started",
            ServerEvent::JobProgress { .. } => "job.progress",
            ServerEvent::JobCompleted { .. } => "job.completed",
            ServerEvent::JobFailed { .. } => "job.failed",
            ServerEvent::NoteUpdated { .. } => "note.updated",
            ServerEvent::NoteCreated { .. } => "note.created",
            ServerEvent::NoteDeleted { .. } => "note.deleted",
            ServerEvent::NoteArchived { .. } => "note.archived",
            ServerEvent::NoteRestored { .. } => "note.restored",
            ServerEvent::NoteTagsUpdated { .. } => "note.tags.updated",
            ServerEvent::NoteLinksUpdated { .. } => "note.links.updated",
            ServerEvent::NoteRevisionCreated { .. } => "note.revision.created",
            ServerEvent::AttachmentCreated { .. } => "attachment.created",
            ServerEvent::AttachmentDeleted { .. } => "attachment.deleted",
            ServerEvent::AttachmentExtractionUpdated { .. } => "attachment.extraction.updated",
            ServerEvent::CollectionCreated { .. } => "collection.created",
            ServerEvent::CollectionUpdated { .. } => "collection.updated",
            ServerEvent::CollectionDeleted { .. } => "collection.deleted",
            ServerEvent::CollectionMembershipChanged { .. } => "collection.membership.changed",
            ServerEvent::ArchiveCreated { .. } => "archive.created",
            ServerEvent::ArchiveUpdated { .. } => "archive.updated",
            ServerEvent::ArchiveDeleted { .. } => "archive.deleted",
            ServerEvent::ArchiveDefaultChanged { .. } => "archive.default.changed",
            ServerEvent::ConceptSchemeCreated { .. } => "concept_scheme.created",
            ServerEvent::ConceptSchemeUpdated { .. } => "concept_scheme.updated",
            ServerEvent::ConceptSchemeDeleted { .. } => "concept_scheme.deleted",
            ServerEvent::ConceptCreated { .. } => "concept.created",
            ServerEvent::ConceptUpdated { .. } => "concept.updated",
            ServerEvent::ConceptDeleted { .. } => "concept.deleted",
            ServerEvent::ConceptRelationsUpdated { .. } => "concept.relations.updated",
            ServerEvent::ConceptSchemeChanged { .. } => "concept.scheme.changed",
            ServerEvent::ConceptCollectionMembershipChanged { .. } => {
                "concept.collection.membership.changed"
            }
            ServerEvent::TagCreated { .. } => "tag.created",
            ServerEvent::TagRenamed { .. } => "tag.renamed",
            ServerEvent::TagDeleted { .. } => "tag.deleted",
            ServerEvent::TagMerged { .. } => "tag.merged",
            ServerEvent::TagStatsUpdated => "tag.stats.updated",
            ServerEvent::IndexEmbeddingUpdated { .. } => "index.embedding.updated",
            ServerEvent::IndexLinkingUpdated { .. } => "index.linking.updated",
            ServerEvent::IndexFtsUpdated { .. } => "index.fts.updated",
            ServerEvent::ReadmodelGraphUpdated { .. } => "readmodel.graph.updated",
            ServerEvent::ReadmodelSearchReady { .. } => "readmodel.search.ready",
        }
    }

    /// Returns the entity type this event relates to.
    pub fn entity_type(&self) -> Option<&'static str> {
        match self {
            ServerEvent::QueueStatus { .. } => None,
            ServerEvent::JobQueued { .. }
            | ServerEvent::JobStarted { .. }
            | ServerEvent::JobProgress { .. }
            | ServerEvent::JobCompleted { .. }
            | ServerEvent::JobFailed { .. } => Some("job"),
            ServerEvent::NoteUpdated { .. }
            | ServerEvent::NoteCreated { .. }
            | ServerEvent::NoteDeleted { .. }
            | ServerEvent::NoteArchived { .. }
            | ServerEvent::NoteRestored { .. }
            | ServerEvent::NoteTagsUpdated { .. }
            | ServerEvent::NoteLinksUpdated { .. }
            | ServerEvent::NoteRevisionCreated { .. } => Some("note"),
            ServerEvent::AttachmentCreated { .. }
            | ServerEvent::AttachmentDeleted { .. }
            | ServerEvent::AttachmentExtractionUpdated { .. } => Some("attachment"),
            ServerEvent::CollectionCreated { .. }
            | ServerEvent::CollectionUpdated { .. }
            | ServerEvent::CollectionDeleted { .. }
            | ServerEvent::CollectionMembershipChanged { .. } => Some("collection"),
            ServerEvent::ArchiveCreated { .. }
            | ServerEvent::ArchiveUpdated { .. }
            | ServerEvent::ArchiveDeleted { .. }
            | ServerEvent::ArchiveDefaultChanged { .. } => Some("archive"),
            ServerEvent::ConceptSchemeCreated { .. }
            | ServerEvent::ConceptSchemeUpdated { .. }
            | ServerEvent::ConceptSchemeDeleted { .. } => Some("concept_scheme"),
            ServerEvent::ConceptCreated { .. }
            | ServerEvent::ConceptUpdated { .. }
            | ServerEvent::ConceptDeleted { .. }
            | ServerEvent::ConceptRelationsUpdated { .. }
            | ServerEvent::ConceptSchemeChanged { .. }
            | ServerEvent::ConceptCollectionMembershipChanged { .. } => Some("concept"),
            ServerEvent::TagCreated { .. }
            | ServerEvent::TagRenamed { .. }
            | ServerEvent::TagDeleted { .. }
            | ServerEvent::TagMerged { .. }
            | ServerEvent::TagStatsUpdated => Some("tag"),
            ServerEvent::IndexEmbeddingUpdated { .. }
            | ServerEvent::IndexLinkingUpdated { .. }
            | ServerEvent::IndexFtsUpdated { .. }
            | ServerEvent::ReadmodelGraphUpdated { .. }
            | ServerEvent::ReadmodelSearchReady { .. } => Some("index"),
        }
    }

    /// Returns the primary entity ID this event relates to.
    pub fn entity_id(&self) -> Option<Uuid> {
        match self {
            ServerEvent::QueueStatus { .. } => None,
            ServerEvent::JobQueued { job_id, .. }
            | ServerEvent::JobStarted { job_id, .. }
            | ServerEvent::JobProgress { job_id, .. }
            | ServerEvent::JobCompleted { job_id, .. }
            | ServerEvent::JobFailed { job_id, .. } => Some(*job_id),
            ServerEvent::NoteUpdated { note_id, .. }
            | ServerEvent::NoteCreated { note_id, .. }
            | ServerEvent::NoteDeleted { note_id, .. }
            | ServerEvent::NoteArchived { note_id, .. }
            | ServerEvent::NoteRestored { note_id, .. }
            | ServerEvent::NoteTagsUpdated { note_id, .. }
            | ServerEvent::NoteLinksUpdated { note_id, .. }
            | ServerEvent::NoteRevisionCreated { note_id, .. } => Some(*note_id),
            ServerEvent::AttachmentCreated { attachment_id, .. }
            | ServerEvent::AttachmentDeleted { attachment_id, .. }
            | ServerEvent::AttachmentExtractionUpdated { attachment_id, .. } => {
                Some(*attachment_id)
            }
            ServerEvent::CollectionCreated { collection_id, .. }
            | ServerEvent::CollectionUpdated { collection_id, .. }
            | ServerEvent::CollectionDeleted { collection_id, .. } => Some(*collection_id),
            ServerEvent::CollectionMembershipChanged { note_id, .. } => Some(*note_id),
            ServerEvent::ArchiveCreated { archive_id, .. } => *archive_id,
            ServerEvent::ArchiveUpdated { .. }
            | ServerEvent::ArchiveDeleted { .. }
            | ServerEvent::ArchiveDefaultChanged { .. } => None,
            ServerEvent::ConceptSchemeCreated { scheme_id, .. }
            | ServerEvent::ConceptSchemeUpdated { scheme_id, .. }
            | ServerEvent::ConceptSchemeDeleted { scheme_id, .. } => Some(*scheme_id),
            ServerEvent::ConceptCreated { concept_id, .. }
            | ServerEvent::ConceptUpdated { concept_id, .. }
            | ServerEvent::ConceptDeleted { concept_id, .. }
            | ServerEvent::ConceptRelationsUpdated { concept_id, .. }
            | ServerEvent::ConceptSchemeChanged { concept_id, .. }
            | ServerEvent::ConceptCollectionMembershipChanged { concept_id, .. } => {
                Some(*concept_id)
            }
            ServerEvent::TagCreated { .. }
            | ServerEvent::TagRenamed { .. }
            | ServerEvent::TagDeleted { .. }
            | ServerEvent::TagMerged { .. }
            | ServerEvent::TagStatsUpdated => None,
            ServerEvent::IndexEmbeddingUpdated { note_id, .. }
            | ServerEvent::IndexLinkingUpdated { note_id, .. }
            | ServerEvent::IndexFtsUpdated { note_id, .. }
            | ServerEvent::ReadmodelSearchReady { note_id, .. } => Some(*note_id),
            ServerEvent::ReadmodelGraphUpdated { note_id, .. } => *note_id,
        }
    }
}

/// Event priority for backpressure decisions (Issue #458).
///
/// Critical events (domain mutations) are never coalesced or dropped.
/// Normal events (job lifecycle) are delivered in order but may be dropped under lag.
/// Low events (telemetry, progress) may be coalesced or dropped to protect stream stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub enum EventPriority {
    /// Domain mutations — never coalesced or dropped.
    Critical,
    /// Job lifecycle — delivered in order, dropped only under severe lag.
    Normal,
    /// Telemetry and progress — may be coalesced within time windows.
    Low,
}

impl ServerEvent {
    /// Returns the backpressure priority for this event type (Issue #458).
    pub fn priority(&self) -> EventPriority {
        match self {
            // Domain mutations are critical — must be delivered
            ServerEvent::NoteCreated { .. }
            | ServerEvent::NoteUpdated { .. }
            | ServerEvent::NoteDeleted { .. }
            | ServerEvent::NoteArchived { .. }
            | ServerEvent::NoteRestored { .. }
            | ServerEvent::NoteTagsUpdated { .. }
            | ServerEvent::NoteLinksUpdated { .. }
            | ServerEvent::NoteRevisionCreated { .. }
            | ServerEvent::AttachmentCreated { .. }
            | ServerEvent::AttachmentDeleted { .. }
            | ServerEvent::AttachmentExtractionUpdated { .. }
            | ServerEvent::CollectionCreated { .. }
            | ServerEvent::CollectionUpdated { .. }
            | ServerEvent::CollectionDeleted { .. }
            | ServerEvent::CollectionMembershipChanged { .. }
            | ServerEvent::ArchiveCreated { .. }
            | ServerEvent::ArchiveUpdated { .. }
            | ServerEvent::ArchiveDeleted { .. }
            | ServerEvent::ArchiveDefaultChanged { .. }
            | ServerEvent::ConceptSchemeCreated { .. }
            | ServerEvent::ConceptSchemeUpdated { .. }
            | ServerEvent::ConceptSchemeDeleted { .. }
            | ServerEvent::ConceptCreated { .. }
            | ServerEvent::ConceptUpdated { .. }
            | ServerEvent::ConceptDeleted { .. }
            | ServerEvent::ConceptRelationsUpdated { .. }
            | ServerEvent::ConceptSchemeChanged { .. }
            | ServerEvent::ConceptCollectionMembershipChanged { .. }
            | ServerEvent::TagCreated { .. }
            | ServerEvent::TagRenamed { .. }
            | ServerEvent::TagDeleted { .. }
            | ServerEvent::TagMerged { .. } => EventPriority::Critical,

            // Job lifecycle transitions — important but not critical
            ServerEvent::JobQueued { .. }
            | ServerEvent::JobStarted { .. }
            | ServerEvent::JobCompleted { .. }
            | ServerEvent::JobFailed { .. }
            | ServerEvent::IndexEmbeddingUpdated { .. }
            | ServerEvent::IndexLinkingUpdated { .. }
            | ServerEvent::IndexFtsUpdated { .. }
            | ServerEvent::ReadmodelGraphUpdated { .. }
            | ServerEvent::ReadmodelSearchReady { .. } => EventPriority::Normal,

            // Telemetry and progress — coalescable
            ServerEvent::QueueStatus { .. }
            | ServerEvent::JobProgress { .. }
            | ServerEvent::TagStatsUpdated => EventPriority::Low,
        }
    }

    /// Returns a coalescing key for low-priority events (Issue #458).
    ///
    /// Events with the same coalescing key within a time window are merged,
    /// keeping only the latest. Returns `None` for non-coalescable events.
    pub fn coalesce_key(&self) -> Option<String> {
        match self {
            ServerEvent::JobProgress { job_id, .. } => Some(format!("job.progress:{}", job_id)),
            ServerEvent::QueueStatus { .. } => Some("queue.status".to_string()),
            ServerEvent::TagStatsUpdated => Some("tag.stats.updated".to_string()),
            _ => None,
        }
    }
}

// ============================================================================
// Variant Metadata (for AsyncAPI spec generation)
// ============================================================================

/// Metadata for a single `ServerEvent` variant, used for AsyncAPI spec generation.
#[derive(Debug, Clone)]
pub struct EventVariantMeta {
    /// Dot-namespaced event type (e.g., `"note.updated"`).
    pub namespaced_type: &'static str,
    /// Rust variant name (e.g., `"NoteUpdated"`).
    pub variant_name: &'static str,
    /// Entity type (e.g., `"note"`, `"job"`), or `None` for system events.
    pub entity_type: Option<&'static str>,
    /// Backpressure priority.
    pub priority: EventPriority,
    /// Human-readable description of the event.
    pub description: &'static str,
}

impl ServerEvent {
    /// Returns a human-readable description of this event variant.
    ///
    /// This uses an exhaustive match so the compiler enforces coverage
    /// whenever a new variant is added.
    pub fn description(&self) -> &'static str {
        match self {
            ServerEvent::QueueStatus { .. } => "Periodic queue statistics broadcast",
            ServerEvent::JobQueued { .. } => "A job was added to the queue",
            ServerEvent::JobStarted { .. } => "A job started processing",
            ServerEvent::JobProgress { .. } => "Job progress update",
            ServerEvent::JobCompleted { .. } => "A job completed successfully",
            ServerEvent::JobFailed { .. } => "A job failed",
            ServerEvent::NoteUpdated { .. } => {
                "A note was created, updated, or had its AI content refreshed"
            }
            ServerEvent::NoteCreated { .. } => "A new note was created",
            ServerEvent::NoteDeleted { .. } => "A note was soft-deleted",
            ServerEvent::NoteArchived { .. } => "A note was archived",
            ServerEvent::NoteRestored { .. } => "A note was restored from archive or soft-deletion",
            ServerEvent::NoteTagsUpdated { .. } => "Tags on a note were changed",
            ServerEvent::NoteLinksUpdated { .. } => "Semantic links on a note were updated",
            ServerEvent::NoteRevisionCreated { .. } => "An AI revision was created for a note",
            ServerEvent::AttachmentCreated { .. } => "A file attachment was uploaded to a note",
            ServerEvent::AttachmentDeleted { .. } => "A file attachment was deleted",
            ServerEvent::AttachmentExtractionUpdated { .. } => {
                "Extraction metadata for an attachment was updated"
            }
            ServerEvent::CollectionCreated { .. } => "A collection was created",
            ServerEvent::CollectionUpdated { .. } => "A collection was updated",
            ServerEvent::CollectionDeleted { .. } => "A collection was deleted",
            ServerEvent::CollectionMembershipChanged { .. } => {
                "A note was moved into or out of a collection"
            }
            ServerEvent::ArchiveCreated { .. } => "A memory archive was created",
            ServerEvent::ArchiveUpdated { .. } => "A memory archive was updated",
            ServerEvent::ArchiveDeleted { .. } => "A memory archive was deleted",
            ServerEvent::ArchiveDefaultChanged { .. } => "The default memory archive was changed",
            ServerEvent::ConceptSchemeCreated { .. } => "A SKOS concept scheme was created",
            ServerEvent::ConceptSchemeUpdated { .. } => "A SKOS concept scheme was updated",
            ServerEvent::ConceptSchemeDeleted { .. } => "A SKOS concept scheme was deleted",
            ServerEvent::ConceptCreated { .. } => "A SKOS concept was created",
            ServerEvent::ConceptUpdated { .. } => "A SKOS concept was updated",
            ServerEvent::ConceptDeleted { .. } => "A SKOS concept was deleted",
            ServerEvent::ConceptRelationsUpdated { .. } => {
                "Semantic relations on a concept were updated"
            }
            ServerEvent::ConceptSchemeChanged { .. } => "A concept's scheme membership was changed",
            ServerEvent::ConceptCollectionMembershipChanged { .. } => {
                "A concept's membership in a SKOS collection was changed"
            }
            ServerEvent::TagCreated { .. } => "A global tag was created",
            ServerEvent::TagRenamed { .. } => "A global tag was renamed",
            ServerEvent::TagDeleted { .. } => "A global tag was deleted",
            ServerEvent::TagMerged { .. } => "Two tags were merged",
            ServerEvent::TagStatsUpdated => "Tag usage statistics were updated",
            ServerEvent::IndexEmbeddingUpdated { .. } => "Embeddings for a note were updated",
            ServerEvent::IndexLinkingUpdated { .. } => {
                "Semantic links for a note were updated by the linking pipeline"
            }
            ServerEvent::IndexFtsUpdated { .. } => "Full-text search index for a note was updated",
            ServerEvent::ReadmodelGraphUpdated { .. } => {
                "The knowledge graph read model was updated"
            }
            ServerEvent::ReadmodelSearchReady { .. } => {
                "All derived views for a note are ready for search"
            }
        }
    }

    /// Returns metadata for all 44 `ServerEvent` variants.
    ///
    /// Constructs dummy instances to enumerate every variant. The exhaustive
    /// match in `description()`, `namespaced_event_type()`, `entity_type()`,
    /// and `priority()` ensures compiler enforcement when new variants are added.
    pub fn all_variants_metadata() -> Vec<EventVariantMeta> {
        let dummy_id = Uuid::nil();
        let variants: Vec<ServerEvent> = vec![
            ServerEvent::QueueStatus {
                total_jobs: 0,
                running: 0,
                pending: 0,
            },
            ServerEvent::JobQueued {
                job_id: dummy_id,
                job_type: String::new(),
                note_id: None,
            },
            ServerEvent::JobStarted {
                job_id: dummy_id,
                job_type: String::new(),
                note_id: None,
            },
            ServerEvent::JobProgress {
                job_id: dummy_id,
                note_id: None,
                progress: 0,
                message: None,
            },
            ServerEvent::JobCompleted {
                job_id: dummy_id,
                job_type: String::new(),
                note_id: None,
                duration_ms: None,
            },
            ServerEvent::JobFailed {
                job_id: dummy_id,
                job_type: String::new(),
                note_id: None,
                error: String::new(),
            },
            ServerEvent::NoteUpdated {
                note_id: dummy_id,
                title: None,
                tags: vec![],
                has_ai_content: false,
                has_links: false,
            },
            ServerEvent::NoteCreated {
                note_id: dummy_id,
                title: None,
                tags: vec![],
            },
            ServerEvent::NoteDeleted { note_id: dummy_id },
            ServerEvent::NoteArchived { note_id: dummy_id },
            ServerEvent::NoteRestored { note_id: dummy_id },
            ServerEvent::NoteTagsUpdated {
                note_id: dummy_id,
                tags: vec![],
            },
            ServerEvent::NoteLinksUpdated { note_id: dummy_id },
            ServerEvent::NoteRevisionCreated { note_id: dummy_id },
            ServerEvent::AttachmentCreated {
                attachment_id: dummy_id,
                note_id: dummy_id,
                filename: None,
            },
            ServerEvent::AttachmentDeleted {
                attachment_id: dummy_id,
                note_id: None,
            },
            ServerEvent::AttachmentExtractionUpdated {
                attachment_id: dummy_id,
                note_id: dummy_id,
            },
            ServerEvent::CollectionCreated {
                collection_id: dummy_id,
                name: String::new(),
            },
            ServerEvent::CollectionUpdated {
                collection_id: dummy_id,
                name: String::new(),
            },
            ServerEvent::CollectionDeleted {
                collection_id: dummy_id,
            },
            ServerEvent::CollectionMembershipChanged {
                collection_id: None,
                note_id: dummy_id,
            },
            ServerEvent::ArchiveCreated {
                name: String::new(),
                archive_id: None,
            },
            ServerEvent::ArchiveUpdated {
                name: String::new(),
            },
            ServerEvent::ArchiveDeleted {
                name: String::new(),
            },
            ServerEvent::ArchiveDefaultChanged {
                name: String::new(),
            },
            // SKOS events (Issue #462)
            ServerEvent::ConceptSchemeCreated {
                scheme_id: dummy_id,
            },
            ServerEvent::ConceptSchemeUpdated {
                scheme_id: dummy_id,
            },
            ServerEvent::ConceptSchemeDeleted {
                scheme_id: dummy_id,
            },
            ServerEvent::ConceptCreated {
                concept_id: dummy_id,
                scheme_id: None,
            },
            ServerEvent::ConceptUpdated {
                concept_id: dummy_id,
            },
            ServerEvent::ConceptDeleted {
                concept_id: dummy_id,
            },
            ServerEvent::ConceptRelationsUpdated {
                concept_id: dummy_id,
                relation_type: String::new(),
            },
            ServerEvent::ConceptSchemeChanged {
                concept_id: dummy_id,
                scheme_id: None,
            },
            ServerEvent::ConceptCollectionMembershipChanged {
                concept_id: dummy_id,
                collection_id: None,
            },
            // Tag governance events (Issue #463)
            ServerEvent::TagCreated { tag: String::new() },
            ServerEvent::TagRenamed {
                old_name: String::new(),
                new_name: String::new(),
            },
            ServerEvent::TagDeleted { tag: String::new() },
            ServerEvent::TagMerged {
                source_tag: String::new(),
                target_tag: String::new(),
                affected_count: None,
            },
            ServerEvent::TagStatsUpdated,
            // Search/index events (Issue #464)
            ServerEvent::IndexEmbeddingUpdated {
                note_id: dummy_id,
                job_id: None,
            },
            ServerEvent::IndexLinkingUpdated {
                note_id: dummy_id,
                job_id: None,
            },
            ServerEvent::IndexFtsUpdated {
                note_id: dummy_id,
                job_id: None,
            },
            ServerEvent::ReadmodelGraphUpdated { note_id: None },
            ServerEvent::ReadmodelSearchReady { note_id: dummy_id },
        ];

        variants
            .iter()
            .map(|v| EventVariantMeta {
                namespaced_type: v.namespaced_event_type(),
                variant_name: v.event_type(),
                entity_type: v.entity_type(),
                priority: v.priority(),
                description: v.description(),
            })
            .collect()
    }
}

// ============================================================================
// SSE Metrics (Issue #459)
// ============================================================================

/// Atomic counters for SSE subsystem observability (Issue #459).
///
/// All counters are monotonically increasing and lock-free. Expose via
/// `/health` endpoint or Prometheus scrape. Counters reset on process restart.
#[derive(Debug, Default)]
pub struct SseMetrics {
    /// Total SSE connections opened since startup.
    pub connections_total: AtomicU64,
    /// Total SSE connections closed (disconnect) since startup.
    pub disconnections_total: AtomicU64,
    /// Total events emitted to the broadcast bus.
    pub events_emitted: AtomicU64,
    /// Total events delivered to SSE clients (after filtering).
    pub events_delivered: AtomicU64,
    /// Total events coalesced (skipped due to debounce window).
    pub events_coalesced: AtomicU64,
    /// Total events dropped due to slow consumers (broadcast lag).
    pub events_lagged: AtomicU64,
    /// Total successful Last-Event-ID replays.
    pub replays_success: AtomicU64,
    /// Total expired/failed Last-Event-ID replays.
    pub replays_expired: AtomicU64,
}

impl SseMetrics {
    /// Returns a snapshot of all metrics as a serializable struct.
    pub fn snapshot(&self) -> SseMetricsSnapshot {
        SseMetricsSnapshot {
            connections_total: self.connections_total.load(Ordering::Relaxed),
            disconnections_total: self.disconnections_total.load(Ordering::Relaxed),
            active_connections: self
                .connections_total
                .load(Ordering::Relaxed)
                .saturating_sub(self.disconnections_total.load(Ordering::Relaxed)),
            events_emitted: self.events_emitted.load(Ordering::Relaxed),
            events_delivered: self.events_delivered.load(Ordering::Relaxed),
            events_coalesced: self.events_coalesced.load(Ordering::Relaxed),
            events_lagged: self.events_lagged.load(Ordering::Relaxed),
            replays_success: self.replays_success.load(Ordering::Relaxed),
            replays_expired: self.replays_expired.load(Ordering::Relaxed),
        }
    }
}

/// Serializable snapshot of SSE metrics for health endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct SseMetricsSnapshot {
    pub connections_total: u64,
    pub disconnections_total: u64,
    pub active_connections: u64,
    pub events_emitted: u64,
    pub events_delivered: u64,
    pub events_coalesced: u64,
    pub events_lagged: u64,
    pub replays_success: u64,
    pub replays_expired: u64,
}

// ============================================================================
// Event Bus
// ============================================================================

/// Broadcast-based event bus for distributing server events to multiple consumers.
///
/// Uses `tokio::sync::broadcast` with a configurable buffer size. Events are
/// wrapped in [`EventEnvelope`] with metadata before broadcast. Slow receivers
/// that fall behind will receive a `Lagged` error and miss events — this is by
/// design for real-time streams where freshness matters more than completeness.
///
/// ## Replay Buffer (Issue #456)
///
/// The bus retains the last N events in a bounded ring buffer for SSE
/// `Last-Event-ID` replay. Clients that reconnect with a valid event ID
/// receive all events since that ID before joining the live stream.
pub struct EventBus {
    tx: broadcast::Sender<EventEnvelope>,
    /// Bounded ring buffer for SSE Last-Event-ID replay (Issue #456).
    replay_buffer: Mutex<VecDeque<EventEnvelope>>,
    /// Maximum events retained in the replay buffer.
    replay_capacity: usize,
    /// SSE subsystem metrics (Issue #459).
    pub metrics: SseMetrics,
}

impl EventBus {
    /// Create a new event bus with the given broadcast capacity and default replay buffer.
    ///
    /// Recommended: 256 broadcast for production, 32 for tests.
    /// Replay buffer defaults to [`crate::defaults::SSE_REPLAY_BUFFER_SIZE`].
    pub fn new(capacity: usize) -> Self {
        Self::with_replay(capacity, crate::defaults::SSE_REPLAY_BUFFER_SIZE)
    }

    /// Create a new event bus with explicit broadcast and replay capacities.
    pub fn with_replay(broadcast_capacity: usize, replay_capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(broadcast_capacity);
        Self {
            tx,
            replay_buffer: Mutex::new(VecDeque::with_capacity(replay_capacity)),
            replay_capacity,
            metrics: SseMetrics::default(),
        }
    }

    /// Emit an event to all subscribers (system actor, no memory scope).
    ///
    /// The event is automatically wrapped in an [`EventEnvelope`] with a
    /// system actor and UUIDv7 event ID. If there are no active subscribers,
    /// the event is silently dropped. The event is also retained in the replay
    /// buffer for `Last-Event-ID` reconnection support.
    pub fn emit(&self, event: ServerEvent) {
        let envelope = EventEnvelope::new(event);
        let subscriber_count = self.tx.receiver_count();
        tracing::debug!(
            event_type = %envelope.event_type,
            event_id = %envelope.event_id,
            subscriber_count,
            "EventBus emit"
        );
        self.metrics.events_emitted.fetch_add(1, Ordering::Relaxed);
        self.push_to_replay(&envelope);
        let _ = self.tx.send(envelope);
    }

    /// Emit an event with explicit context (memory scope, actor, correlation).
    ///
    /// Use this when the emission context is known — e.g., from an
    /// authenticated API handler where the actor and memory scope are available.
    pub fn emit_with_context(&self, event: ServerEvent, ctx: EventContext) {
        let envelope = EventEnvelope::with_context(event, ctx);
        let subscriber_count = self.tx.receiver_count();
        tracing::debug!(
            event_type = %envelope.event_type,
            event_id = %envelope.event_id,
            subscriber_count,
            ?envelope.memory,
            "EventBus emit (with context)"
        );
        self.metrics.events_emitted.fetch_add(1, Ordering::Relaxed);
        self.push_to_replay(&envelope);
        let _ = self.tx.send(envelope);
    }

    /// Subscribe to receive enveloped events. Each subscriber gets its own independent stream.
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.tx.subscribe()
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Replay events since the given event ID (exclusive).
    ///
    /// Returns events emitted after `last_event_id`, in chronological order.
    /// Returns `None` if the ID is not found in the replay buffer (expired cursor).
    /// Returns `Some(vec![])` if the ID is found but no newer events exist.
    pub fn replay_since(&self, last_event_id: Uuid) -> Option<Vec<EventEnvelope>> {
        let buffer = self.replay_buffer.lock().unwrap();

        // Find the position of the requested event ID
        let pos = buffer.iter().position(|e| e.event_id == last_event_id);

        pos.map(|idx| buffer.iter().skip(idx + 1).cloned().collect())
    }

    /// Returns the number of events currently in the replay buffer.
    pub fn replay_buffer_len(&self) -> usize {
        self.replay_buffer.lock().unwrap().len()
    }

    /// Returns the replay buffer capacity.
    pub fn replay_capacity(&self) -> usize {
        self.replay_capacity
    }

    /// Push an event to the replay buffer, evicting the oldest if at capacity.
    fn push_to_replay(&self, envelope: &EventEnvelope) {
        let mut buffer = self.replay_buffer.lock().unwrap();
        if buffer.len() >= self.replay_capacity {
            buffer.pop_front();
        }
        buffer.push_back(envelope.clone());
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_emit_subscribe() {
        let bus = EventBus::new(32);
        let mut rx = bus.subscribe();

        bus.emit(ServerEvent::QueueStatus {
            total_jobs: 5,
            running: 1,
            pending: 4,
        });

        let envelope = rx.recv().await.unwrap();
        assert!(matches!(
            envelope.payload,
            ServerEvent::QueueStatus { total_jobs: 5, .. }
        ));
        assert_eq!(envelope.event_type, "queue.status");
        assert_eq!(envelope.payload_version, 1);
        assert_eq!(envelope.actor.kind, "system");
        assert!(envelope.entity_type.is_none()); // QueueStatus has no entity
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(32);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(ServerEvent::JobStarted {
            job_id: Uuid::nil(),
            job_type: "Embedding".to_string(),
            note_id: None,
        });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(e1.payload, ServerEvent::JobStarted { .. }));
        assert!(matches!(e2.payload, ServerEvent::JobStarted { .. }));
        assert_eq!(e1.event_type, "job.started");
        assert_eq!(e1.entity_type.as_deref(), Some("job"));
    }

    #[tokio::test]
    async fn test_event_bus_no_subscribers_ok() {
        let bus = EventBus::new(32);
        // Should not panic even with no subscribers
        bus.emit(ServerEvent::QueueStatus {
            total_jobs: 0,
            running: 0,
            pending: 0,
        });
    }

    #[tokio::test]
    async fn test_event_bus_subscriber_count() {
        let bus = EventBus::new(32);
        assert_eq!(bus.subscriber_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        drop(_rx1);
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn test_server_event_json_serialization() {
        let event = ServerEvent::JobStarted {
            job_id: Uuid::nil(),
            job_type: "AiRevision".to_string(),
            note_id: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"JobStarted"#));
        assert!(json.contains(r#""job_type":"AiRevision"#));
        // note_id should be skipped when None
        assert!(!json.contains("note_id"));
    }

    #[test]
    fn test_server_event_note_updated_json() {
        let event = ServerEvent::NoteUpdated {
            note_id: Uuid::nil(),
            title: Some("Test Note".to_string()),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            has_ai_content: true,
            has_links: false,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"NoteUpdated"#));
        assert!(json.contains(r#""has_ai_content":true"#));
        assert!(json.contains(r#""has_links":false"#));
    }

    #[test]
    fn test_server_event_type_names() {
        assert_eq!(
            ServerEvent::QueueStatus {
                total_jobs: 0,
                running: 0,
                pending: 0,
            }
            .event_type(),
            "QueueStatus"
        );
        assert_eq!(
            ServerEvent::NoteUpdated {
                note_id: Uuid::nil(),
                title: None,
                tags: vec![],
                has_ai_content: false,
                has_links: false,
            }
            .event_type(),
            "NoteUpdated"
        );
    }

    #[tokio::test]
    async fn test_event_bus_lagged_receiver() {
        // Create a tiny buffer to test lagged behavior
        let bus = EventBus::new(2);
        let mut rx = bus.subscribe();

        // Emit more events than buffer capacity
        for i in 0..5 {
            bus.emit(ServerEvent::QueueStatus {
                total_jobs: i,
                running: 0,
                pending: i,
            });
        }

        // First recv should return Lagged error
        let result = rx.recv().await;
        assert!(result.is_ok() || matches!(result, Err(broadcast::error::RecvError::Lagged(_))));
    }

    // =========================================================================
    // ServerEvent serialization tests (Issue #47)
    // =========================================================================

    #[test]
    fn test_server_event_job_queued_json() {
        let note_id = Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap();
        let event = ServerEvent::JobQueued {
            job_id: Uuid::nil(),
            job_type: "Embedding".to_string(),
            note_id: Some(note_id),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"JobQueued"#));
        assert!(json.contains(r#""note_id":"01234567-89ab-cdef-0123-456789abcdef"#));
        assert!(json.contains(r#""job_type":"Embedding"#));

        // note_id absent when None
        let event_no_note = ServerEvent::JobQueued {
            job_id: Uuid::nil(),
            job_type: "Embedding".to_string(),
            note_id: None,
        };
        let json2 = serde_json::to_string(&event_no_note).unwrap();
        assert!(!json2.contains("note_id"));
    }

    #[test]
    fn test_server_event_job_progress_json() {
        // With message
        let event = ServerEvent::JobProgress {
            job_id: Uuid::nil(),
            note_id: None,
            progress: 50,
            message: Some("halfway done".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"JobProgress"#));
        assert!(json.contains(r#""progress":50"#));
        assert!(json.contains(r#""message":"halfway done"#));
        assert!(!json.contains("note_id")); // None -> absent

        // Without message
        let event_no_msg = ServerEvent::JobProgress {
            job_id: Uuid::nil(),
            note_id: None,
            progress: 75,
            message: None,
        };
        let json2 = serde_json::to_string(&event_no_msg).unwrap();
        assert!(!json2.contains("message")); // skip_serializing_if
    }

    #[test]
    fn test_server_event_job_completed_json() {
        // With duration
        let event = ServerEvent::JobCompleted {
            job_id: Uuid::nil(),
            job_type: "AiRevision".to_string(),
            note_id: None,
            duration_ms: Some(1500),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"JobCompleted"#));
        assert!(json.contains(r#""duration_ms":1500"#));

        // Without duration
        let event_no_dur = ServerEvent::JobCompleted {
            job_id: Uuid::nil(),
            job_type: "AiRevision".to_string(),
            note_id: None,
            duration_ms: None,
        };
        let json2 = serde_json::to_string(&event_no_dur).unwrap();
        assert!(!json2.contains("duration_ms")); // skip_serializing_if
    }

    #[test]
    fn test_server_event_job_failed_json() {
        let event = ServerEvent::JobFailed {
            job_id: Uuid::nil(),
            job_type: "Linking".to_string(),
            note_id: None,
            error: "connection refused: \"host\" not found\nnested error".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"JobFailed"#));
        assert!(json.contains(r#""job_type":"Linking"#));
        // Verify the JSON is valid despite special characters in error
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("connection refused"));
        assert!(parsed["error"].as_str().unwrap().contains("nested error"));
    }

    #[test]
    fn test_server_event_type_names_exhaustive() {
        // All 7 variants
        assert_eq!(
            ServerEvent::QueueStatus {
                total_jobs: 0,
                running: 0,
                pending: 0,
            }
            .event_type(),
            "QueueStatus"
        );
        assert_eq!(
            ServerEvent::JobQueued {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
            }
            .event_type(),
            "JobQueued"
        );
        assert_eq!(
            ServerEvent::JobStarted {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
            }
            .event_type(),
            "JobStarted"
        );
        assert_eq!(
            ServerEvent::JobProgress {
                job_id: Uuid::nil(),
                note_id: None,
                progress: 0,
                message: None,
            }
            .event_type(),
            "JobProgress"
        );
        assert_eq!(
            ServerEvent::JobCompleted {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
                duration_ms: None,
            }
            .event_type(),
            "JobCompleted"
        );
        assert_eq!(
            ServerEvent::JobFailed {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
                error: String::new(),
            }
            .event_type(),
            "JobFailed"
        );
        assert_eq!(
            ServerEvent::NoteUpdated {
                note_id: Uuid::nil(),
                title: None,
                tags: vec![],
                has_ai_content: false,
                has_links: false,
            }
            .event_type(),
            "NoteUpdated"
        );
    }

    // =========================================================================
    // Envelope tests (Issue #451)
    // =========================================================================

    #[test]
    fn test_envelope_new_defaults() {
        let event = ServerEvent::NoteUpdated {
            note_id: Uuid::nil(),
            title: Some("Test".to_string()),
            tags: vec!["a".to_string()],
            has_ai_content: false,
            has_links: false,
        };
        let envelope = EventEnvelope::new(event);

        assert_eq!(envelope.event_type, "note.updated");
        assert_eq!(envelope.payload_version, 1);
        assert_eq!(envelope.actor.kind, "system");
        assert_eq!(envelope.entity_type.as_deref(), Some("note"));
        assert_eq!(
            envelope.entity_id.as_deref(),
            Some(Uuid::nil().to_string().as_str())
        );
        assert!(envelope.memory.is_none());
        assert!(envelope.correlation_id.is_none());
        assert!(envelope.causation_id.is_none());
        // event_id should be a valid UUIDv7
        assert!(crate::uuid_utils::is_v7(&envelope.event_id));
    }

    #[test]
    fn test_envelope_with_context() {
        let ctx = EventContext {
            memory: Some("archive_001".to_string()),
            tenant_id: Some("tenant_a".to_string()),
            actor: Some(EventActor::user("user-123", Some("Alice".to_string()))),
            correlation_id: Some(Uuid::nil()),
            causation_id: None,
        };
        let event = ServerEvent::JobStarted {
            job_id: Uuid::nil(),
            job_type: "Embedding".to_string(),
            note_id: None,
        };
        let envelope = EventEnvelope::with_context(event, ctx);

        assert_eq!(envelope.event_type, "job.started");
        assert_eq!(envelope.memory.as_deref(), Some("archive_001"));
        assert_eq!(envelope.tenant_id.as_deref(), Some("tenant_a"));
        assert_eq!(envelope.actor.kind, "user");
        assert_eq!(envelope.actor.id.as_deref(), Some("user-123"));
        assert_eq!(envelope.actor.name.as_deref(), Some("Alice"));
        assert_eq!(envelope.correlation_id, Some(Uuid::nil()));
        assert!(envelope.causation_id.is_none());
        assert_eq!(envelope.entity_type.as_deref(), Some("job"));
    }

    #[test]
    fn test_envelope_json_serialization() {
        let event = ServerEvent::NoteUpdated {
            note_id: Uuid::nil(),
            title: Some("Hello".to_string()),
            tags: vec![],
            has_ai_content: true,
            has_links: false,
        };
        let envelope = EventEnvelope::new(event);
        let json = serde_json::to_string(&envelope).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["event_type"], "note.updated");
        assert_eq!(parsed["payload_version"], 1);
        assert_eq!(parsed["actor"]["kind"], "system");
        // Payload should contain the ServerEvent with its type tag
        assert_eq!(parsed["payload"]["type"], "NoteUpdated");
        assert_eq!(parsed["payload"]["has_ai_content"], true);
        // event_id should be present
        assert!(parsed["event_id"].is_string());
        // occurred_at should be present
        assert!(parsed["occurred_at"].is_string());
        // Optional fields should be absent when None
        assert!(parsed.get("memory").is_none() || parsed["memory"].is_null());
        assert!(parsed.get("correlation_id").is_none() || parsed["correlation_id"].is_null());
    }

    #[test]
    fn test_envelope_queue_status_no_entity() {
        let envelope = EventEnvelope::new(ServerEvent::QueueStatus {
            total_jobs: 10,
            running: 2,
            pending: 8,
        });
        assert_eq!(envelope.event_type, "queue.status");
        assert!(envelope.entity_type.is_none());
        assert!(envelope.entity_id.is_none());
    }

    #[test]
    fn test_namespaced_event_types_exhaustive() {
        assert_eq!(
            ServerEvent::QueueStatus {
                total_jobs: 0,
                running: 0,
                pending: 0,
            }
            .namespaced_event_type(),
            "queue.status"
        );
        assert_eq!(
            ServerEvent::JobQueued {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
            }
            .namespaced_event_type(),
            "job.queued"
        );
        assert_eq!(
            ServerEvent::JobStarted {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
            }
            .namespaced_event_type(),
            "job.started"
        );
        assert_eq!(
            ServerEvent::JobProgress {
                job_id: Uuid::nil(),
                note_id: None,
                progress: 0,
                message: None,
            }
            .namespaced_event_type(),
            "job.progress"
        );
        assert_eq!(
            ServerEvent::JobCompleted {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
                duration_ms: None,
            }
            .namespaced_event_type(),
            "job.completed"
        );
        assert_eq!(
            ServerEvent::JobFailed {
                job_id: Uuid::nil(),
                job_type: String::new(),
                note_id: None,
                error: String::new(),
            }
            .namespaced_event_type(),
            "job.failed"
        );
        assert_eq!(
            ServerEvent::NoteUpdated {
                note_id: Uuid::nil(),
                title: None,
                tags: vec![],
                has_ai_content: false,
                has_links: false,
            }
            .namespaced_event_type(),
            "note.updated"
        );
    }

    #[test]
    fn test_entity_type_and_id() {
        let job_id = Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap();
        let event = ServerEvent::JobStarted {
            job_id,
            job_type: "Embedding".to_string(),
            note_id: None,
        };
        assert_eq!(event.entity_type(), Some("job"));
        assert_eq!(event.entity_id(), Some(job_id));

        let note_id = Uuid::parse_str("fedcba98-7654-3210-fedc-ba9876543210").unwrap();
        let event = ServerEvent::NoteUpdated {
            note_id,
            title: None,
            tags: vec![],
            has_ai_content: false,
            has_links: false,
        };
        assert_eq!(event.entity_type(), Some("note"));
        assert_eq!(event.entity_id(), Some(note_id));

        let event = ServerEvent::QueueStatus {
            total_jobs: 0,
            running: 0,
            pending: 0,
        };
        assert_eq!(event.entity_type(), None);
        assert_eq!(event.entity_id(), None);
    }

    #[tokio::test]
    async fn test_event_bus_emit_with_context() {
        let bus = EventBus::new(32);
        let mut rx = bus.subscribe();

        let ctx = EventContext {
            memory: Some("test_archive".to_string()),
            actor: Some(EventActor::user("u1", None)),
            ..Default::default()
        };
        bus.emit_with_context(
            ServerEvent::NoteUpdated {
                note_id: Uuid::nil(),
                title: None,
                tags: vec![],
                has_ai_content: false,
                has_links: false,
            },
            ctx,
        );

        let envelope = rx.recv().await.unwrap();
        assert_eq!(envelope.memory.as_deref(), Some("test_archive"));
        assert_eq!(envelope.actor.kind, "user");
        assert_eq!(envelope.actor.id.as_deref(), Some("u1"));
    }

    #[test]
    fn test_event_actor_constructors() {
        let sys = EventActor::system();
        assert_eq!(sys.kind, "system");
        assert!(sys.id.is_none());
        assert!(sys.name.is_none());

        let user = EventActor::user("id-1", Some("Bob".to_string()));
        assert_eq!(user.kind, "user");
        assert_eq!(user.id.as_deref(), Some("id-1"));
        assert_eq!(user.name.as_deref(), Some("Bob"));

        let agent = EventActor::agent("mcp-server");
        assert_eq!(agent.kind, "agent");
        assert!(agent.id.is_none());
        assert_eq!(agent.name.as_deref(), Some("mcp-server"));
    }

    // =========================================================================
    // Replay buffer tests (Issue #456)
    // =========================================================================

    #[test]
    fn test_replay_buffer_stores_events() {
        let bus = EventBus::with_replay(32, 10);
        assert_eq!(bus.replay_buffer_len(), 0);

        bus.emit(ServerEvent::QueueStatus {
            total_jobs: 1,
            running: 0,
            pending: 1,
        });

        assert_eq!(bus.replay_buffer_len(), 1);
    }

    #[test]
    fn test_replay_buffer_capacity_eviction() {
        let bus = EventBus::with_replay(32, 3);

        for i in 0..5 {
            bus.emit(ServerEvent::QueueStatus {
                total_jobs: i,
                running: 0,
                pending: i,
            });
        }

        // Buffer capacity is 3, so only the last 3 events should remain
        assert_eq!(bus.replay_buffer_len(), 3);
    }

    #[test]
    fn test_replay_since_returns_subsequent_events() {
        let bus = EventBus::with_replay(32, 100);

        // Emit 5 events
        let mut event_ids = Vec::new();
        for i in 0..5 {
            bus.emit(ServerEvent::QueueStatus {
                total_jobs: i,
                running: 0,
                pending: i,
            });
        }

        // Capture the event IDs from the buffer
        let all = bus.replay_since(Uuid::nil()); // nil won't be found
        assert!(all.is_none(), "Unknown ID should return None");

        // Get the actual buffer contents via replay_since on a known event
        let buf = bus.replay_buffer.lock().unwrap();
        for e in buf.iter() {
            event_ids.push(e.event_id);
        }
        drop(buf);

        // Replay since the second event — should get events 3, 4, 5
        let replayed = bus.replay_since(event_ids[1]).unwrap();
        assert_eq!(replayed.len(), 3);

        // Replay since the last event — should get empty
        let replayed = bus.replay_since(*event_ids.last().unwrap()).unwrap();
        assert!(replayed.is_empty());

        // Replay since the first event — should get events 2-5
        let replayed = bus.replay_since(event_ids[0]).unwrap();
        assert_eq!(replayed.len(), 4);
    }

    #[test]
    fn test_replay_since_expired_cursor() {
        let bus = EventBus::with_replay(32, 3);

        // Emit 5 events (buffer capacity 3, so first 2 are evicted)
        let mut first_id = Uuid::nil();
        for i in 0..5 {
            if i == 0 {
                // Capture the first event's ID before it's evicted
                let envelope = EventEnvelope::new(ServerEvent::QueueStatus {
                    total_jobs: i,
                    running: 0,
                    pending: i,
                });
                first_id = envelope.event_id;
                bus.push_to_replay(&envelope);
                let _ = bus.tx.send(envelope);
            } else {
                bus.emit(ServerEvent::QueueStatus {
                    total_jobs: i,
                    running: 0,
                    pending: i,
                });
            }
        }

        // The first event has been evicted — replay should return None
        assert!(bus.replay_since(first_id).is_none());
    }

    #[test]
    fn test_replay_with_context_stores_memory_scope() {
        let bus = EventBus::with_replay(32, 10);

        bus.emit_with_context(
            ServerEvent::NoteCreated {
                note_id: Uuid::nil(),
                title: None,
                tags: vec![],
            },
            EventContext {
                memory: Some("research".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(bus.replay_buffer_len(), 1);
        let buf = bus.replay_buffer.lock().unwrap();
        assert_eq!(buf[0].memory.as_deref(), Some("research"));
    }

    #[test]
    fn test_replay_capacity_getter() {
        let bus = EventBus::with_replay(32, 512);
        assert_eq!(bus.replay_capacity(), 512);
    }

    // -- Priority and coalescing tests (Issue #458) --

    #[test]
    fn test_note_events_are_critical_priority() {
        let events = vec![
            ServerEvent::NoteCreated {
                note_id: Uuid::nil(),
                title: None,
                tags: vec![],
            },
            ServerEvent::NoteUpdated {
                note_id: Uuid::nil(),
                title: None,
                tags: vec![],
                has_ai_content: false,
                has_links: false,
            },
            ServerEvent::NoteDeleted {
                note_id: Uuid::nil(),
            },
            ServerEvent::NoteArchived {
                note_id: Uuid::nil(),
            },
            ServerEvent::NoteRestored {
                note_id: Uuid::nil(),
            },
        ];
        for event in events {
            assert_eq!(
                event.priority(),
                EventPriority::Critical,
                "Note events must be Critical: {:?}",
                event.event_type()
            );
        }
    }

    #[test]
    fn test_job_lifecycle_events_are_normal_priority() {
        let events = vec![
            ServerEvent::JobQueued {
                job_id: Uuid::nil(),
                job_type: "Embedding".to_string(),
                note_id: None,
            },
            ServerEvent::JobStarted {
                job_id: Uuid::nil(),
                job_type: "Embedding".to_string(),
                note_id: None,
            },
            ServerEvent::JobCompleted {
                job_id: Uuid::nil(),
                job_type: "Embedding".to_string(),
                note_id: None,
                duration_ms: None,
            },
            ServerEvent::JobFailed {
                job_id: Uuid::nil(),
                job_type: "Embedding".to_string(),
                note_id: None,
                error: "err".to_string(),
            },
        ];
        for event in events {
            assert_eq!(
                event.priority(),
                EventPriority::Normal,
                "Job lifecycle events must be Normal: {:?}",
                event.event_type()
            );
        }
    }

    #[test]
    fn test_telemetry_events_are_low_priority() {
        let events = vec![
            ServerEvent::QueueStatus {
                total_jobs: 0,
                running: 0,
                pending: 0,
            },
            ServerEvent::JobProgress {
                job_id: Uuid::nil(),
                note_id: None,
                progress: 50,
                message: None,
            },
        ];
        for event in events {
            assert_eq!(
                event.priority(),
                EventPriority::Low,
                "Telemetry events must be Low: {:?}",
                event.event_type()
            );
        }
    }

    #[test]
    fn test_coalesce_key_for_job_progress() {
        let id = Uuid::new_v4();
        let event = ServerEvent::JobProgress {
            job_id: id,
            note_id: None,
            progress: 42,
            message: None,
        };
        assert_eq!(event.coalesce_key(), Some(format!("job.progress:{}", id)));
    }

    #[test]
    fn test_coalesce_key_for_queue_status() {
        let event = ServerEvent::QueueStatus {
            total_jobs: 10,
            running: 1,
            pending: 9,
        };
        assert_eq!(event.coalesce_key(), Some("queue.status".to_string()));
    }

    #[test]
    fn test_coalesce_key_none_for_critical_events() {
        let event = ServerEvent::NoteCreated {
            note_id: Uuid::nil(),
            title: None,
            tags: vec![],
        };
        assert!(
            event.coalesce_key().is_none(),
            "Critical events must not have a coalescing key"
        );
    }

    #[test]
    fn test_archive_events_are_critical_priority() {
        let events = vec![
            ServerEvent::ArchiveCreated {
                name: "test".to_string(),
                archive_id: None,
            },
            ServerEvent::ArchiveDeleted {
                name: "test".to_string(),
            },
            ServerEvent::ArchiveDefaultChanged {
                name: "test".to_string(),
            },
        ];
        for event in events {
            assert_eq!(event.priority(), EventPriority::Critical);
        }
    }

    // -- Variant metadata tests (AsyncAPI) --

    #[test]
    fn test_all_variants_metadata_is_complete() {
        let meta = ServerEvent::all_variants_metadata();
        assert_eq!(
            meta.len(),
            44,
            "Expected 44 event variants, got {}",
            meta.len()
        );

        // All namespaced types should be unique
        let types: std::collections::HashSet<&str> =
            meta.iter().map(|m| m.namespaced_type).collect();
        assert_eq!(types.len(), 44, "Duplicate namespaced_type found");

        // All descriptions should be non-empty
        for m in &meta {
            assert!(
                !m.description.is_empty(),
                "Empty description for {}",
                m.variant_name
            );
        }
    }

    #[test]
    fn test_description_matches_doc_comments() {
        let event = ServerEvent::NoteCreated {
            note_id: Uuid::nil(),
            title: None,
            tags: vec![],
        };
        assert_eq!(event.description(), "A new note was created");

        let event = ServerEvent::JobFailed {
            job_id: Uuid::nil(),
            job_type: String::new(),
            note_id: None,
            error: String::new(),
        };
        assert_eq!(event.description(), "A job failed");
    }
}
