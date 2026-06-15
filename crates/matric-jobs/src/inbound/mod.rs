//! Inbound external event source connectors (#833, Phase D).
//!
//! A plug-in framework that pulls events from upstream technical sources and
//! normalizes them into the shared `event_outbox`, reusing the existing fan-out
//! pipeline. Concrete connectors land in follow-ups: Redis Stream (#834), SSE
//! (#835), Kafka (#836). This module provides the contract, registry, lifecycle
//! supervisor, DLQ wiring, and per-connector metrics.

pub mod metrics;
pub mod registry;
pub mod source;
pub mod supervisor;

pub use metrics::InboundMetrics;
pub use registry::{SourceBuilder, SourceRegistry};
pub use source::{
    InMemorySource, InboundError, InboundEvent, InboundEventSource, InboundResult, Offset,
};
pub use supervisor::InboundSupervisor;
