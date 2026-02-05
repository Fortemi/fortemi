/**
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
