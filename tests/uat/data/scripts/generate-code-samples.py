#!/usr/bin/env python3
"""Generate code samples for document type detection testing."""

from pathlib import Path


PYTHON_SAMPLE = '''"""Sample Python module for testing code chunking."""

from typing import List, Optional
import json


class DataProcessor:
    """Processes data with various transformations."""

    def __init__(self, config: dict):
        self.config = config
        self.cache = {}

    def process(self, data: List[dict]) -> List[dict]:
        """Process a list of data items."""
        return [self._transform(item) for item in data]

    def _transform(self, item: dict) -> dict:
        """Transform a single item."""
        if not isinstance(item, dict):
            raise ValueError("Item must be a dictionary")

        # Apply transformations
        transformed = {
            "id": item.get("id"),
            "processed": True,
            "original": item
        }

        # Cache result
        if "id" in item:
            self.cache[item["id"]] = transformed

        return transformed


def main():
    """Main entry point."""
    config = {"mode": "strict", "validate": True}
    processor = DataProcessor(config)

    test_data = [
        {"id": 1, "value": "test1"},
        {"id": 2, "value": "test2"}
    ]

    result = processor.process(test_data)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
'''

RUST_SAMPLE = '''//! Sample Rust module for testing code chunking.

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
'''

JAVASCRIPT_SAMPLE = '''/**
 * Sample JavaScript module for testing code chunking
 * @module DataProcessor
 */

import fs from 'fs/promises';

/**
 * Data processor class
 */
export class DataProcessor {
  constructor(config) {
    this.config = config;
    this.cache = new Map();
  }

  /**
   * Process data asynchronously
   * @param {Array} data - Input data
   * @returns {Promise<Array>} Processed data
   */
  async process(data) {
    const results = await Promise.all(
      data.map(item => this.transform(item))
    );
    return results;
  }

  async transform(item) {
    const cached = this.cache.get(item.id);
    if (cached) return cached;

    const result = {
      ...item,
      processed: true,
      timestamp: Date.now()
    };

    this.cache.set(item.id, result);
    return result;
  }

  clearCache() {
    this.cache.clear();
  }
}

/**
 * Utility function
 */
export const loadConfig = async (path) => {
  const content = await fs.readFile(path, 'utf-8');
  return JSON.parse(content);
};

/**
 * Validate configuration object
 * @param {Object} config - Configuration to validate
 * @returns {boolean} True if valid
 */
export function validateConfig(config) {
  if (!config || typeof config !== 'object') {
    return false;
  }

  const requiredKeys = ['mode', 'threshold'];
  return requiredKeys.every(key => key in config);
}

// Default export
export default DataProcessor;
'''

TYPESCRIPT_SAMPLE = '''/**
 * Sample TypeScript module for testing code chunking
 */

interface ProcessorConfig {
  mode: 'strict' | 'lenient';
  threshold: number;
}

interface DataItem {
  id: string;
  value: unknown;
  timestamp?: Date;
}

/**
 * Generic data processor
 */
export class DataProcessor<T extends DataItem> {
  private config: ProcessorConfig;
  private cache: Map<string, T>;

  constructor(config: ProcessorConfig) {
    this.config = config;
    this.cache = new Map();
  }

  /**
   * Process items with type safety
   */
  async process(items: T[]): Promise<T[]> {
    return Promise.all(items.map(item => this.transform(item)));
  }

  private async transform(item: T): Promise<T> {
    const cached = this.cache.get(item.id);
    if (cached) return cached;

    const result = {
      ...item,
      timestamp: new Date()
    } as T;

    this.cache.set(item.id, result);
    return result;
  }

  clearCache(): void {
    this.cache.clear();
  }

  getCacheSize(): number {
    return this.cache.size;
  }
}

/**
 * Type-safe config loader
 */
export async function loadConfig(path: string): Promise<ProcessorConfig> {
  const fs = await import('fs/promises');
  const content = await fs.readFile(path, 'utf-8');
  return JSON.parse(content) as ProcessorConfig;
}

/**
 * Type guard for ProcessorConfig
 */
export function isProcessorConfig(obj: unknown): obj is ProcessorConfig {
  if (!obj || typeof obj !== 'object') return false;

  const config = obj as Partial<ProcessorConfig>;
  return (
    (config.mode === 'strict' || config.mode === 'lenient') &&
    typeof config.threshold === 'number'
  );
}
'''

def main():
    script_dir = Path(__file__).parent
    data_dir = script_dir.parent
    documents_dir = data_dir / "documents"
    documents_dir.mkdir(parents=True, exist_ok=True)

    print("Generating code samples...")

    files = {
        "code-python.py": PYTHON_SAMPLE,
        "code-rust.rs": RUST_SAMPLE,
        "code-javascript.js": JAVASCRIPT_SAMPLE,
        "code-typescript.ts": TYPESCRIPT_SAMPLE,
    }

    for filename, content in files.items():
        filepath = documents_dir / filename
        filepath.write_text(content)
        print(f"  ✓ Created {filename}")

    print("")
    print(f"✓ Generated {len(files)} code sample files")
    print("  Languages: Python, Rust, JavaScript, TypeScript")


if __name__ == "__main__":
    main()
