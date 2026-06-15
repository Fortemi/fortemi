//! Connector registry: maps an inbound-source `kind` to a builder that
//! constructs a connector from its JSON config (#833). Concrete connectors
//! (#834 `redis-stream`, #835 `sse`, #836 `kafka`) register their builder here;
//! the supervisor uses it to instantiate enabled sources from the DB.

use serde_json::Value;
use std::collections::HashMap;

use super::source::{InboundError, InboundEventSource, InboundResult};

/// Builds a connector instance for a registered source: `(name, config)`.
pub type SourceBuilder =
    Box<dyn Fn(&str, &Value) -> InboundResult<Box<dyn InboundEventSource>> + Send + Sync>;

/// Registry of connector builders keyed by `kind`.
#[derive(Default)]
pub struct SourceRegistry {
    builders: HashMap<String, SourceBuilder>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a builder for `kind`. Replaces any existing builder.
    pub fn register(&mut self, kind: impl Into<String>, builder: SourceBuilder) {
        self.builders.insert(kind.into(), builder);
    }

    /// Build a connector for a registered source. Errors when `kind` is unknown.
    pub fn build(
        &self,
        kind: &str,
        name: &str,
        config: &Value,
    ) -> InboundResult<Box<dyn InboundEventSource>> {
        let builder = self.builders.get(kind).ok_or_else(|| {
            InboundError::Transient(format!("no connector registered for kind '{kind}'"))
        })?;
        builder(name, config)
    }

    /// Registered connector kinds (diagnostics).
    pub fn kinds(&self) -> Vec<String> {
        let mut k: Vec<String> = self.builders.keys().cloned().collect();
        k.sort();
        k
    }

    pub fn is_empty(&self) -> bool {
        self.builders.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::source::{InMemorySource, InboundEvent};
    use super::*;
    use serde_json::json;

    #[test]
    fn register_build_and_unknown_kind() {
        let mut reg = SourceRegistry::new();
        reg.register(
            "memory",
            Box::new(|name, _config| {
                Ok(Box::new(InMemorySource::new(
                    name,
                    vec![InboundEvent::new("e.v1", json!({}), "0-0")],
                )) as Box<dyn InboundEventSource>)
            }),
        );
        assert_eq!(reg.kinds(), vec!["memory".to_string()]);
        assert!(reg.build("memory", "m1", &json!({})).is_ok());
        assert!(reg.build("redis-stream", "r1", &json!({})).is_err());
    }
}
