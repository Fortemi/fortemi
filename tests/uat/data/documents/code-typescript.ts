/**
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
