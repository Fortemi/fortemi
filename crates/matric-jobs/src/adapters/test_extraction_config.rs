//! Tests for configurable extraction adapter registration.
//!
//! These tests verify that extraction adapters are correctly registered based
//! on their availability (health checks) and configuration.

#[cfg(test)]
mod tests {
    use crate::adapters::{PdfTextAdapter, StructuredExtractAdapter, TextNativeAdapter};
    use crate::ExtractionRegistry;
    use matric_core::ExtractionAdapter;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_text_native_always_available() {
        let adapter = TextNativeAdapter;
        let result = adapter.health_check().await;
        assert!(
            result.is_ok(),
            "TextNativeAdapter health check should always succeed"
        );
        assert!(
            result.unwrap(),
            "TextNativeAdapter should always be available"
        );
    }

    #[tokio::test]
    async fn test_structured_extract_always_available() {
        let adapter = StructuredExtractAdapter;
        let result = adapter.health_check().await;
        assert!(
            result.is_ok(),
            "StructuredExtractAdapter health check should always succeed"
        );
        assert!(
            result.unwrap(),
            "StructuredExtractAdapter should always be available"
        );
    }

    #[tokio::test]
    async fn test_pdf_text_conditional_availability() {
        let adapter = PdfTextAdapter;
        let result = adapter.health_check().await;
        assert!(
            result.is_ok(),
            "PdfTextAdapter health check should not error"
        );
        // Result depends on whether pdftotext is installed
        // In CI it should be available, locally it may not be
    }

    #[tokio::test]
    async fn test_registry_accepts_always_available_adapters() {
        let mut registry = ExtractionRegistry::new();

        // These adapters have no external dependencies and should always register
        registry.register(Arc::new(TextNativeAdapter));
        registry.register(Arc::new(StructuredExtractAdapter));

        let strategies = registry.available_strategies();
        assert!(
            strategies.contains(&matric_core::ExtractionStrategy::TextNative),
            "TextNative should be registered"
        );
        assert!(
            strategies.contains(&matric_core::ExtractionStrategy::StructuredExtract),
            "StructuredExtract should be registered"
        );
    }

    #[tokio::test]
    async fn test_registry_conditional_registration() {
        let mut registry = ExtractionRegistry::new();

        // Only register PDF adapter if it's available
        if PdfTextAdapter.health_check().await.unwrap_or(false) {
            registry.register(Arc::new(PdfTextAdapter));
        }

        // Registry should have at least the always-available adapters
        // PDF may or may not be present depending on environment
        let strategies = registry.available_strategies();
        assert!(
            !strategies.is_empty(),
            "Registry should have at least some strategies"
        );
    }

    #[tokio::test]
    async fn test_health_check_does_not_panic() {
        // Ensure all adapters handle health_check gracefully
        let adapters: Vec<Box<dyn ExtractionAdapter>> = vec![
            Box::new(TextNativeAdapter),
            Box::new(StructuredExtractAdapter),
            Box::new(PdfTextAdapter),
        ];

        for adapter in adapters {
            let result = adapter.health_check().await;
            assert!(
                result.is_ok(),
                "Health check for {} should not return an error",
                adapter.name()
            );
        }
    }

    #[test]
    fn test_adapter_names() {
        assert_eq!(TextNativeAdapter.name(), "text_native");
        assert_eq!(StructuredExtractAdapter.name(), "structured_extract");
        assert_eq!(PdfTextAdapter.name(), "pdf_text");
    }

    #[test]
    fn test_adapter_strategies() {
        assert_eq!(
            TextNativeAdapter.strategy(),
            matric_core::ExtractionStrategy::TextNative
        );
        assert_eq!(
            StructuredExtractAdapter.strategy(),
            matric_core::ExtractionStrategy::StructuredExtract
        );
        assert_eq!(
            PdfTextAdapter.strategy(),
            matric_core::ExtractionStrategy::PdfText
        );
    }
}
