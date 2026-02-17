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

use chrono::{DateTime, Utc};
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
            ServerEvent::NoteUpdated { .. } => Some("note"),
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
            ServerEvent::NoteUpdated { note_id, .. } => Some(*note_id),
        }
    }
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
pub struct EventBus {
    tx: broadcast::Sender<EventEnvelope>,
}

impl EventBus {
    /// Create a new event bus with the given buffer capacity.
    ///
    /// Recommended: 256 for production, 32 for tests.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Emit an event to all subscribers (system actor, no memory scope).
    ///
    /// The event is automatically wrapped in an [`EventEnvelope`] with a
    /// system actor and UUIDv7 event ID. If there are no active subscribers,
    /// the event is silently dropped.
    pub fn emit(&self, event: ServerEvent) {
        let envelope = EventEnvelope::new(event);
        let subscriber_count = self.tx.receiver_count();
        tracing::debug!(
            event_type = %envelope.event_type,
            event_id = %envelope.event_id,
            subscriber_count,
            "EventBus emit"
        );
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
}
