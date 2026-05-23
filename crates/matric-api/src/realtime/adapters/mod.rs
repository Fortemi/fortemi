//! Provider-specific realtime adapter boundaries.

#[cfg(any(test, feature = "mock-rtp"))]
pub mod mock;
pub mod twilio;

#[cfg(any(test, feature = "mock-rtp"))]
pub use mock::{MockAdapter, MockAdapterBuilder};
