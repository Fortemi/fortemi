//! Standards-shaped real-time call abstractions.
//!
//! This module follows ADR-RTP-001: core real-time code is expressed in
//! RTP/SIP/codec terms, while provider-specific wire formats are translated at
//! adapter boundaries.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use matric_core::{Error, Result};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Boxed media stream used by adapter implementations.
pub type MediaFrameStream = Pin<Box<dyn Stream<Item = MediaFrame> + Send>>;

/// Boxed control event stream used by adapter implementations.
pub type CallControlEventStream = Pin<Box<dyn Stream<Item = CallControlEvent> + Send>>;

/// Wire-format-agnostic media frame, conceptually equivalent to an RTP packet
/// without network framing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFrame {
    /// IANA-aligned codec descriptor for the payload.
    pub codec: Codec,
    /// RTP timestamp semantics in codec-rate ticks.
    pub timestamp_rtp: u32,
    /// Monotonic per-session media sequence.
    pub sequence: u32,
    /// RTP marker bit, often used to mark the start of a talkspurt.
    pub marker: bool,
    /// Raw codec payload bytes.
    pub payload: Vec<u8>,
}

/// Codec identification aligned with VoIP/IANA media names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Codec {
    /// G.711 PCMU / mu-law.
    PcmuG711 { sample_rate: u32 },
    /// G.711 PCMA / A-law.
    PcmaG711 { sample_rate: u32 },
    /// Opus audio.
    Opus { sample_rate: u32, channels: u8 },
    /// Linear PCM.
    L16 { sample_rate: u32, channels: u8 },
    /// RFC 4733 telephone-event payload such as DTMF.
    Telephone { event_code: u8 },
}

/// SIP-style call lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallState {
    Ringing,
    EarlyMedia,
    Active,
    OnHold,
    Ended { reason: EndReason },
}

/// Standards-shaped call end reasons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndReason {
    NormalHangup,
    Dropped,
    Failed,
    Cancelled,
}

/// Control-plane events emitted by real-time call adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallControlEvent {
    CallStarted {
        provider: String,
        provider_call_id: String,
    },
    StateChanged {
        state: CallState,
    },
    DtmfDigit {
        digit: char,
    },
    RecordingAvailable {
        url: String,
    },
    Dropped {
        reason: String,
    },
    Custom {
        event_type: String,
        payload: serde_json::Value,
    },
}

/// Provider adapter contract.
///
/// Per ADR-RTP-001, adapters translate provider-specific signaling and media
/// into this standards-shaped surface before the rest of Fortemi sees it.
#[async_trait]
pub trait CallTransport: Send + Sync {
    /// Stable adapter name such as `mock`, `twilio`, `livekit`, or `sip-direct`.
    fn adapter_name(&self) -> &str;

    /// Provider-specific opaque call identifier.
    fn provider_call_id(&self) -> &str;

    /// Inbound media frames from the provider.
    fn frames(&mut self) -> MediaFrameStream;

    /// Outbound media frames toward the provider.
    async fn send_frame(&mut self, _frame: MediaFrame) -> Result<()> {
        Err(Error::InvalidInput(
            "adapter does not support outbound media frames".to_string(),
        ))
    }

    /// Control events such as state transitions, DTMF, and dropped calls.
    fn control_events(&mut self) -> CallControlEventStream;

    /// Initiate call teardown.
    async fn end_call(&mut self, reason: EndReason) -> Result<()>;
}

/// Active call session tracked by [`CallSessionManager`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveCallSession {
    pub call_id: Uuid,
    pub provider: String,
    pub provider_call_id: String,
    pub state: CallState,
    pub started_at: DateTime<Utc>,
}

/// In-memory registry for active real-time sessions.
#[derive(Debug, Clone, Default)]
pub struct CallSessionManager {
    sessions: Arc<RwLock<HashMap<Uuid, ActiveCallSession>>>,
    provider_index: Arc<RwLock<HashMap<(String, String), Uuid>>>,
}

impl CallSessionManager {
    /// Create an empty session manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Start tracking a call and return its internal `call_id`.
    pub async fn start_session(&self, provider: &str, provider_call_id: &str) -> Result<Uuid> {
        if provider.trim().is_empty() {
            return Err(Error::InvalidInput("provider is required".to_string()));
        }
        if provider_call_id.trim().is_empty() {
            return Err(Error::InvalidInput(
                "provider_call_id is required".to_string(),
            ));
        }

        let key = (provider.to_string(), provider_call_id.to_string());
        if let Some(call_id) = self.provider_index.read().await.get(&key).copied() {
            return Ok(call_id);
        }

        let call_id = matric_core::new_v7();
        let session = ActiveCallSession {
            call_id,
            provider: provider.to_string(),
            provider_call_id: provider_call_id.to_string(),
            state: CallState::Ringing,
            started_at: Utc::now(),
        };

        self.sessions.write().await.insert(call_id, session);
        self.provider_index.write().await.insert(key, call_id);
        Ok(call_id)
    }

    /// Look up the internal call ID for a provider call ID.
    pub async fn lookup_call_id(&self, provider: &str, provider_call_id: &str) -> Option<Uuid> {
        self.provider_index
            .read()
            .await
            .get(&(provider.to_string(), provider_call_id.to_string()))
            .copied()
    }

    /// Get an active session by internal ID.
    pub async fn get_session(&self, call_id: Uuid) -> Option<ActiveCallSession> {
        self.sessions.read().await.get(&call_id).cloned()
    }

    /// Update a session state.
    pub async fn update_state(&self, call_id: Uuid, state: CallState) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&call_id)
            .ok_or_else(|| Error::NotFound(format!("call session {call_id}")))?;
        session.state = state;
        Ok(())
    }

    /// End a session and remove provider lookup state.
    pub async fn end_session(&self, call_id: Uuid, reason: EndReason) -> Result<ActiveCallSession> {
        let mut sessions = self.sessions.write().await;
        let mut session = sessions
            .remove(&call_id)
            .ok_or_else(|| Error::NotFound(format!("call session {call_id}")))?;
        session.state = CallState::Ended { reason };
        self.provider_index
            .write()
            .await
            .remove(&(session.provider.clone(), session.provider_call_id.clone()));
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_frame_round_trips_through_clone_and_eq() {
        let frame = MediaFrame {
            codec: Codec::Opus {
                sample_rate: 48_000,
                channels: 2,
            },
            timestamp_rtp: 42,
            sequence: 7,
            marker: true,
            payload: vec![1, 2, 3],
        };

        assert_eq!(frame.clone(), frame);
    }

    #[test]
    fn control_event_custom_payload_round_trips_through_clone_and_eq() {
        let event = CallControlEvent::Custom {
            event_type: "adapter.capability".to_string(),
            payload: serde_json::json!({"feature": "recording"}),
        };

        assert_eq!(event.clone(), event);
    }

    #[tokio::test]
    async fn call_session_manager_tracks_start_lookup_update_and_end() {
        let manager = CallSessionManager::new();

        let call_id = manager.start_session("mock", "provider-1").await.unwrap();
        assert_eq!(
            manager.lookup_call_id("mock", "provider-1").await,
            Some(call_id)
        );

        manager
            .update_state(call_id, CallState::Active)
            .await
            .unwrap();
        assert_eq!(
            manager.get_session(call_id).await.unwrap().state,
            CallState::Active
        );

        let ended = manager
            .end_session(call_id, EndReason::NormalHangup)
            .await
            .unwrap();
        assert_eq!(
            ended.state,
            CallState::Ended {
                reason: EndReason::NormalHangup
            }
        );
        assert_eq!(manager.lookup_call_id("mock", "provider-1").await, None);
    }
}
