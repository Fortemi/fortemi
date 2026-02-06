//! Extraction adapter registry for dispatching file processing.

use std::collections::HashMap;
use std::sync::Arc;

use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
use serde_json::Value as JsonValue;

/// Registry mapping extraction strategies to their adapter implementations.
pub struct ExtractionRegistry {
    adapters: HashMap<ExtractionStrategy, Arc<dyn ExtractionAdapter>>,
}

impl ExtractionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Register an adapter. Replaces any existing adapter for the same strategy.
    pub fn register(&mut self, adapter: Arc<dyn ExtractionAdapter>) {
        self.adapters.insert(adapter.strategy(), adapter);
    }

    /// Extract content using the adapter registered for the given strategy.
    pub async fn extract(
        &self,
        strategy: ExtractionStrategy,
        data: &[u8],
        filename: &str,
        mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        let adapter = self.adapters.get(&strategy).ok_or_else(|| {
            matric_core::Error::Internal(format!(
                "No extraction adapter registered for strategy: {:?}",
                strategy
            ))
        })?;
        adapter.extract(data, filename, mime_type, config).await
    }

    /// List all strategies that have registered adapters.
    pub fn available_strategies(&self) -> Vec<ExtractionStrategy> {
        self.adapters.keys().copied().collect()
    }

    /// Check if an adapter is registered for the given strategy.
    pub fn has_adapter(&self, strategy: ExtractionStrategy) -> bool {
        self.adapters.contains_key(&strategy)
    }

    /// Run health checks on all registered adapters.
    pub async fn health_check_all(&self) -> HashMap<ExtractionStrategy, bool> {
        let mut results = HashMap::new();
        for (strategy, adapter) in &self.adapters {
            let healthy = adapter.health_check().await.unwrap_or(false);
            results.insert(*strategy, healthy);
        }
        results
    }
}

impl Default for ExtractionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::TextNativeAdapter;

    #[test]
    fn test_registry_new_is_empty() {
        let registry = ExtractionRegistry::new();
        assert!(registry.available_strategies().is_empty());
        assert!(!registry.has_adapter(ExtractionStrategy::TextNative));
    }

    #[test]
    fn test_registry_register_and_lookup() {
        let mut registry = ExtractionRegistry::new();
        let adapter = Arc::new(TextNativeAdapter);
        registry.register(adapter);
        assert!(registry.has_adapter(ExtractionStrategy::TextNative));
        assert!(!registry.has_adapter(ExtractionStrategy::PdfText));
        assert_eq!(registry.available_strategies().len(), 1);
    }

    #[tokio::test]
    async fn test_registry_extract_missing_adapter() {
        let registry = ExtractionRegistry::new();
        let result = registry
            .extract(
                ExtractionStrategy::PdfText,
                b"data",
                "test.pdf",
                "application/pdf",
                &serde_json::json!({}),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_extract_with_adapter() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));

        let result = registry
            .extract(
                ExtractionStrategy::TextNative,
                b"hello world",
                "test.txt",
                "text/plain",
                &serde_json::json!({}),
            )
            .await;
        assert!(result.is_ok());
        let extraction = result.unwrap();
        assert_eq!(extraction.extracted_text.as_deref(), Some("hello world"));
    }

    #[tokio::test]
    async fn test_registry_health_check_all() {
        let mut registry = ExtractionRegistry::new();
        registry.register(Arc::new(TextNativeAdapter));

        let results = registry.health_check_all().await;
        assert_eq!(results.len(), 1);
        assert!(results[&ExtractionStrategy::TextNative]);
    }
}
