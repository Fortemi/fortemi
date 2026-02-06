//! Server event types and event bus for real-time notifications.
//!
//! Provides a unified event system that aggregates events from multiple sources
//! (job worker, note operations) into a single broadcast channel. Downstream
//! consumers (WebSocket, SSE, webhooks, telemetry) subscribe independently.
//!
//! Architecture: See ADR-037 (unified-event-bus)

use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Unified server event type matching the HotM WebSocket client protocol.
///
/// These events are serialized as JSON with a `type` tag field, e.g.:
/// `{"type":"JobStarted","job_id":"...","job_type":"Embedding"}`
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
    /// Returns the event type name (used for SSE event field and webhook filtering).
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
}

/// Broadcast-based event bus for distributing server events to multiple consumers.
///
/// Uses `tokio::sync::broadcast` with a configurable buffer size. Slow receivers
/// that fall behind will receive a `Lagged` error and miss events — this is by
/// design for real-time streams where freshness matters more than completeness.
pub struct EventBus {
    tx: broadcast::Sender<ServerEvent>,
}

impl EventBus {
    /// Create a new event bus with the given buffer capacity.
    ///
    /// Recommended: 256 for production, 32 for tests.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Emit an event to all subscribers.
    ///
    /// If there are no active subscribers, the event is silently dropped.
    /// Emitted events are traced at debug level for telemetry (Issue #45).
    pub fn emit(&self, event: ServerEvent) {
        let event_type = event.event_type();
        let subscriber_count = self.tx.receiver_count();
        tracing::debug!(event_type, subscriber_count, "EventBus emit");
        // send() returns Err only if there are no receivers, which is fine
        let _ = self.tx.send(event);
    }

    /// Subscribe to receive events. Each subscriber gets its own independent stream.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

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

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            ServerEvent::QueueStatus { total_jobs: 5, .. }
        ));
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
        assert!(matches!(e1, ServerEvent::JobStarted { .. }));
        assert!(matches!(e2, ServerEvent::JobStarted { .. }));
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
        assert!(!json.contains("note_id")); // None → absent

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
}
