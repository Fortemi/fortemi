//! Sample Rust module for testing code chunking.

use std::collections::HashMap;

/// Configuration for the processor
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    pub mode: String,
    pub threshold: f64,
}

/// Main data processor
pub struct DataProcessor {
    config: ProcessorConfig,
    cache: HashMap<String, String>,
}

impl DataProcessor {
    /// Create a new processor with given config
    pub fn new(config: ProcessorConfig) -> Self {
        Self {
            config,
            cache: HashMap::new(),
        }
    }

    /// Process input data
    pub fn process(&mut self, data: &str) -> String {
        if let Some(cached) = self.cache.get(data) {
            return cached.clone();
        }

        let result = self.transform(data);
        self.cache.insert(data.to_string(), result.clone());
        result
    }

    fn transform(&self, data: &str) -> String {
        // Implementation
        data.to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor() {
        let config = ProcessorConfig {
            mode: "strict".to_string(),
            threshold: 0.5,
        };
        let mut processor = DataProcessor::new(config);
        assert_eq!(processor.process("test"), "TEST");
    }

    #[test]
    fn test_caching() {
        let config = ProcessorConfig {
            mode: "lenient".to_string(),
            threshold: 0.8,
        };
        let mut processor = DataProcessor::new(config);

        let result1 = processor.process("cached");
        let result2 = processor.process("cached");

        assert_eq!(result1, result2);
    }
}
