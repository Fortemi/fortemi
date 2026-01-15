/**
 * Tests for chart generation functionality
 * Following TDD: write tests first, then implementation
 */

import { describe, it, expect, beforeEach } from '@jest/globals';
import type { EmbeddingEvalResult, LLMEvalResult } from '../models/types.js';
import {
  generateModelComparisonChart,
  generateRadarChart,
  generateLatencyVsQualityChart,
  generateRetrievalMetricsChart,
  generateScoreDistributionChart,
  generateAllCharts,
  getModelSizeCategory,
  getCategoryColor,
  extractParameterCount,
  renderChartsToSVG,
} from './charts.js';
import * as fs from 'fs';
import * as path from 'path';

// ============================================================================
// Test Data
// ============================================================================

const mockEmbeddingResults: EmbeddingEvalResult[] = [
  {
    modelName: 'nomic-embed-text',
    overallScore: 78.5,
    metrics: {
      retrieval: {
        precisionAt5: 0.82,
        precisionAt10: 0.78,
        recallAt5: 0.65,
        recallAt10: 0.75,
        mrr: 0.88,
        ndcgAt10: 0.79,
      },
      similarity: {
        accuracy: 0.85,
      },
      latency: {
        p50: 35,
        p95: 120,
        p99: 180,
        mean: 45,
        min: 20,
        max: 200,
      },
      throughput: 22,
    },
    timestamp: '2025-01-14T12:00:00Z',
  },
  {
    modelName: 'mxbai-embed-large',
    overallScore: 82.3,
    metrics: {
      retrieval: {
        precisionAt5: 0.85,
        precisionAt10: 0.81,
        recallAt5: 0.70,
        recallAt10: 0.80,
        mrr: 0.90,
        ndcgAt10: 0.83,
      },
      similarity: {
        accuracy: 0.88,
      },
      latency: {
        p50: 45,
        p95: 150,
        p99: 220,
        mean: 55,
        min: 25,
        max: 250,
      },
      throughput: 18,
    },
    timestamp: '2025-01-14T12:00:00Z',
  },
];

const mockLLMResults: LLMEvalResult[] = [
  {
    modelName: 'qwen2.5:14b',
    overallScore: 82.3,
    dimensions: {
      revisionQuality: 85.0,
      titleQuality: 78.5,
      contextQuality: 80.0,
      instructionFollowing: 90.0,
      efficiency: 75.0,
    },
    metrics: {
      latency: {
        p50: 1200,
        p95: 2500,
        p99: 3500,
        mean: 1400,
        min: 800,
        max: 4000,
      },
      tokensPerSecond: 25,
    },
    timestamp: '2025-01-14T12:00:00Z',
  },
  {
    modelName: 'qwen2.5:7b',
    overallScore: 76.8,
    dimensions: {
      revisionQuality: 78.0,
      titleQuality: 75.0,
      contextQuality: 76.0,
      instructionFollowing: 85.0,
      efficiency: 82.0,
    },
    metrics: {
      latency: {
        p50: 800,
        p95: 1600,
        p99: 2200,
        mean: 950,
        min: 500,
        max: 2500,
      },
      tokensPerSecond: 40,
    },
    timestamp: '2025-01-14T12:00:00Z',
  },
  {
    modelName: 'qwen2.5:32b',
    overallScore: 88.5,
    dimensions: {
      revisionQuality: 92.0,
      titleQuality: 85.0,
      contextQuality: 88.0,
      instructionFollowing: 92.0,
      efficiency: 68.0,
    },
    metrics: {
      latency: {
        p50: 2000,
        p95: 4000,
        p99: 5500,
        mean: 2300,
        min: 1500,
        max: 6000,
      },
      tokensPerSecond: 15,
    },
    timestamp: '2025-01-14T12:00:00Z',
  },
  {
    modelName: 'smollm2:1.7b',
    overallScore: 62.5,
    dimensions: {
      revisionQuality: 60.0,
      titleQuality: 65.0,
      contextQuality: 62.0,
      instructionFollowing: 70.0,
      efficiency: 90.0,
    },
    metrics: {
      latency: {
        p50: 400,
        p95: 800,
        p99: 1100,
        mean: 480,
        min: 200,
        max: 1200,
      },
      tokensPerSecond: 80,
    },
    timestamp: '2025-01-14T12:00:00Z',
  },
  {
    modelName: 'nemotron-mini:4b',
    overallScore: 68.2,
    dimensions: {
      revisionQuality: 66.0,
      titleQuality: 70.0,
      contextQuality: 68.0,
      instructionFollowing: 75.0,
      efficiency: 85.0,
    },
    metrics: {
      latency: {
        p50: 600,
        p95: 1200,
        p99: 1600,
        mean: 700,
        min: 350,
        max: 1800,
      },
      tokensPerSecond: 60,
    },
    timestamp: '2025-01-14T12:00:00Z',
  },
];

// ============================================================================
// Helper Function Tests
// ============================================================================

describe('extractParameterCount', () => {
  it('should extract parameter count from model names', () => {
    expect(extractParameterCount('qwen2.5:14b')).toBe(14);
    expect(extractParameterCount('llama3.1:8b')).toBe(8);
    expect(extractParameterCount('qwen2.5:32b')).toBe(32);
    expect(extractParameterCount('smollm2:1.7b')).toBe(1.7);
  });

  it('should handle uppercase B', () => {
    expect(extractParameterCount('MODEL:7B')).toBe(7);
  });

  it('should default to 7 for unknown format', () => {
    expect(extractParameterCount('unknown-model')).toBe(7);
    expect(extractParameterCount('model-name')).toBe(7);
  });
});

describe('getModelSizeCategory', () => {
  it('should categorize micro models correctly', () => {
    expect(getModelSizeCategory('smollm2:1.7b')).toBe('micro');
    expect(getModelSizeCategory('model:0.5b')).toBe('micro');
  });

  it('should categorize tiny models correctly', () => {
    expect(getModelSizeCategory('nemotron-mini:4b')).toBe('tiny');
    expect(getModelSizeCategory('granite4:3b')).toBe('tiny');
  });

  it('should categorize small models correctly', () => {
    expect(getModelSizeCategory('qwen2.5:7b')).toBe('small');
    expect(getModelSizeCategory('llama3.1:8b')).toBe('small');
    expect(getModelSizeCategory('mistral:latest')).toBe('small');
  });

  it('should categorize medium models correctly', () => {
    expect(getModelSizeCategory('qwen2.5:14b')).toBe('medium');
    expect(getModelSizeCategory('deepseek-r1:14b')).toBe('medium');
    expect(getModelSizeCategory('gpt-oss:20b')).toBe('medium');
  });

  it('should categorize large models correctly', () => {
    expect(getModelSizeCategory('qwen2.5:32b')).toBe('large');
    expect(getModelSizeCategory('model:40b')).toBe('large');
  });

  it('should default to small for unknown models', () => {
    expect(getModelSizeCategory('unknown-model')).toBe('small');
  });
});

describe('getCategoryColor', () => {
  it('should return correct colors for all categories', () => {
    expect(getCategoryColor('micro')).toBe('#f59e0b');
    expect(getCategoryColor('tiny')).toBe('#10b981');
    expect(getCategoryColor('small')).toBe('#3b82f6');
    expect(getCategoryColor('medium')).toBe('#8b5cf6');
    expect(getCategoryColor('large')).toBe('#ef4444');
  });
});

// ============================================================================
// Chart Generation Tests
// ============================================================================

describe('generateModelComparisonChart', () => {
  it('should generate valid Vega-Lite spec for LLM comparison', () => {
    const spec = generateModelComparisonChart(mockLLMResults);

    expect(spec).toBeDefined();
    expect(spec.$schema).toContain('vega-lite');
    expect(spec.mark).toBeDefined();
    expect(spec.encoding).toBeDefined();
    expect(spec.data).toBeDefined();
    expect(spec.data.values).toHaveLength(5);
  });

  it('should include model names and scores in data', () => {
    const spec = generateModelComparisonChart(mockLLMResults);
    const data = spec.data.values;

    expect(data[0]).toHaveProperty('model');
    expect(data[0]).toHaveProperty('score');
    expect(data[0]).toHaveProperty('category');
  });

  it('should color-code by model size category', () => {
    const spec = generateModelComparisonChart(mockLLMResults);

    expect(spec.encoding).toBeDefined();
    if (spec.encoding) {
      expect(spec.encoding.color).toBeDefined();
      expect(spec.encoding.color.field).toBe('category');
    }
  });

  it('should handle empty results', () => {
    const spec = generateModelComparisonChart([]);
    expect(spec.data.values).toHaveLength(0);
  });

  it('should have proper chart dimensions', () => {
    const spec = generateModelComparisonChart(mockLLMResults);
    expect(spec.width).toBe(600);
    expect(spec.height).toBe(400);
  });

  it('should respect custom dimensions', () => {
    const spec = generateModelComparisonChart(mockLLMResults, {
      width: 800,
      height: 600,
    });
    expect(spec.width).toBe(800);
    expect(spec.height).toBe(600);
  });
});

describe('generateRadarChart', () => {
  it('should generate valid Vega-Lite spec for radar chart', () => {
    const spec = generateRadarChart(mockLLMResults);

    expect(spec).toBeDefined();
    expect(spec.$schema).toContain('vega-lite');
    expect(spec.data).toBeDefined();
  });

  it('should include top 5 models only', () => {
    const spec = generateRadarChart(mockLLMResults);
    const models = new Set(spec.data.values.map((v: any) => v.model));

    expect(models.size).toBeLessThanOrEqual(5);
  });

  it('should include all dimension axes', () => {
    const spec = generateRadarChart(mockLLMResults);
    const dimensions = new Set(spec.data.values.map((v: any) => v.dimension));

    expect(dimensions).toContain('revisionQuality');
    expect(dimensions).toContain('titleQuality');
    expect(dimensions).toContain('contextQuality');
    expect(dimensions).toContain('instructionFollowing');
    expect(dimensions).toContain('efficiency');
  });

  it('should use polar coordinates', () => {
    const spec = generateRadarChart(mockLLMResults);
    // Radar charts typically use layer or mark with theta encoding
    expect(spec.layer || spec.mark).toBeDefined();
  });

  it('should handle fewer than 5 models', () => {
    const fewResults = mockLLMResults.slice(0, 2);
    const spec = generateRadarChart(fewResults);
    const models = new Set(spec.data.values.map((v: any) => v.model));

    expect(models.size).toBe(2);
  });
});

describe('generateLatencyVsQualityChart', () => {
  it('should generate valid scatter plot spec', () => {
    const spec = generateLatencyVsQualityChart(mockLLMResults);

    expect(spec).toBeDefined();
    expect(spec.$schema).toContain('vega-lite');
    expect(spec.mark).toBeDefined();
    expect(spec.encoding).toBeDefined();
  });

  it('should map latency to x-axis and quality to y-axis', () => {
    const spec = generateLatencyVsQualityChart(mockLLMResults);

    expect(spec.encoding).toBeDefined();
    if (spec.encoding) {
      expect(spec.encoding.x).toBeDefined();
      expect(spec.encoding.y).toBeDefined();
      expect(spec.encoding.x.field).toBe('latency');
      expect(spec.encoding.y.field).toBe('quality');
    }
  });

  it('should use model size for bubble size', () => {
    const spec = generateLatencyVsQualityChart(mockLLMResults);

    expect(spec.encoding).toBeDefined();
    if (spec.encoding) {
      expect(spec.encoding.size).toBeDefined();
      expect(spec.encoding.size.field).toBe('parameters');
    }
  });

  it('should extract parameter count from model names', () => {
    const spec = generateLatencyVsQualityChart(mockLLMResults);
    const data = spec.data.values;

    expect(data[0]).toHaveProperty('parameters');
    expect(data[0].parameters).toBeGreaterThan(0);
  });
});

describe('generateRetrievalMetricsChart', () => {
  it('should generate valid grouped bar chart spec', () => {
    const spec = generateRetrievalMetricsChart(mockEmbeddingResults);

    expect(spec).toBeDefined();
    expect(spec.$schema).toContain('vega-lite');
    expect(spec.mark).toBe('bar');
    expect(spec.encoding).toBeDefined();
  });

  it('should include all retrieval metrics', () => {
    const spec = generateRetrievalMetricsChart(mockEmbeddingResults);
    const metrics = new Set(spec.data.values.map((v: any) => v.metric));

    expect(metrics).toContain('P@5');
    expect(metrics).toContain('P@10');
    expect(metrics).toContain('R@5');
    expect(metrics).toContain('R@10');
    expect(metrics).toContain('MRR');
    expect(metrics).toContain('NDCG@10');
  });

  it('should group by embedding model', () => {
    const spec = generateRetrievalMetricsChart(mockEmbeddingResults);

    expect(spec.encoding).toBeDefined();
    if (spec.encoding) {
      expect(spec.encoding.x.field).toBe('metric');
      expect(spec.encoding.color.field).toBe('model');
    }
  });

  it('should convert metrics to percentage scale', () => {
    const spec = generateRetrievalMetricsChart(mockEmbeddingResults);
    const data = spec.data.values;

    // Find a P@5 metric value
    const p5Value = data.find((v: any) => v.metric === 'P@5');
    expect(p5Value.value).toBeGreaterThan(1); // Should be in percentage
  });

  it('should handle empty results', () => {
    const spec = generateRetrievalMetricsChart([]);
    expect(spec.data.values).toHaveLength(0);
  });
});

describe('generateScoreDistributionChart', () => {
  it('should generate valid box plot spec', () => {
    const spec = generateScoreDistributionChart(mockLLMResults);

    expect(spec).toBeDefined();
    expect(spec.$schema).toContain('vega-lite');
    expect(spec.data).toBeDefined();
  });

  it('should include model names and dimension scores', () => {
    const spec = generateScoreDistributionChart(mockLLMResults);
    const data = spec.data.values;

    expect(data.length).toBeGreaterThan(0);
    expect(data[0]).toHaveProperty('model');
    expect(data[0]).toHaveProperty('score');
  });

  it('should handle multiple dimension scores per model', () => {
    const spec = generateScoreDistributionChart(mockLLMResults);
    const data = spec.data.values;

    // Each model should have multiple dimension scores
    const modelScores = data.filter((v: any) => v.model === 'qwen2.5:14b');
    expect(modelScores.length).toBe(5); // 5 dimensions
  });

  it('should handle empty results', () => {
    const spec = generateScoreDistributionChart([]);
    expect(spec.data.values).toHaveLength(0);
  });
});

// ============================================================================
// Integration Tests
// ============================================================================

describe('generateAllCharts', () => {
  const testOutputDir = '/tmp/test-charts';

  beforeEach(() => {
    // Clean up test directory
    if (fs.existsSync(testOutputDir)) {
      fs.rmSync(testOutputDir, { recursive: true });
    }
  });

  it('should create output directory if it does not exist', async () => {
    await generateAllCharts(
      {
        embedding: mockEmbeddingResults,
        llm: mockLLMResults,
      },
      testOutputDir
    );

    expect(fs.existsSync(testOutputDir)).toBe(true);
  });

  it('should generate all chart files', async () => {
    await generateAllCharts(
      {
        embedding: mockEmbeddingResults,
        llm: mockLLMResults,
      },
      testOutputDir
    );

    const files = fs.readdirSync(testOutputDir);

    expect(files).toContain('model-comparison.json');
    expect(files).toContain('radar-chart.json');
    expect(files).toContain('latency-vs-quality.json');
    expect(files).toContain('retrieval-metrics.json');
    expect(files).toContain('score-distribution.json');
  });

  it('should write valid JSON specs', async () => {
    await generateAllCharts(
      {
        embedding: mockEmbeddingResults,
        llm: mockLLMResults,
      },
      testOutputDir
    );

    const specPath = path.join(testOutputDir, 'model-comparison.json');
    const content = fs.readFileSync(specPath, 'utf-8');
    const spec = JSON.parse(content);

    expect(spec.$schema).toContain('vega-lite');
  });

  it('should skip chart types with no data', async () => {
    await generateAllCharts(
      {
        embedding: [],
        llm: mockLLMResults,
      },
      testOutputDir
    );

    const files = fs.readdirSync(testOutputDir);

    // Should not generate retrieval metrics chart without embedding data
    expect(files).not.toContain('retrieval-metrics.json');
  });

  it('should handle only embedding data', async () => {
    await generateAllCharts(
      {
        embedding: mockEmbeddingResults,
        llm: [],
      },
      testOutputDir
    );

    const files = fs.readdirSync(testOutputDir);

    expect(files).toContain('retrieval-metrics.json');
    expect(files).not.toContain('model-comparison.json');
  });

  it('should handle empty results gracefully', async () => {
    await generateAllCharts(
      {
        embedding: [],
        llm: [],
      },
      testOutputDir
    );

    expect(fs.existsSync(testOutputDir)).toBe(true);
    const files = fs.readdirSync(testOutputDir);
    expect(files.length).toBe(0);
  });
});

describe('renderChartsToSVG', () => {
  const testOutputDir = '/tmp/test-svg-charts';

  beforeEach(() => {
    // Clean up test directory
    if (fs.existsSync(testOutputDir)) {
      fs.rmSync(testOutputDir, { recursive: true });
    }
  });

  it('should fallback to JSON specs when vega not available', async () => {
    // The function will catch the import error and fall back to JSON
    await renderChartsToSVG(
      {
        embedding: mockEmbeddingResults,
        llm: mockLLMResults,
      },
      testOutputDir
    );

    expect(fs.existsSync(testOutputDir)).toBe(true);
    const files = fs.readdirSync(testOutputDir);

    // Should have JSON files even if SVG rendering fails
    expect(files.some((f) => f.endsWith('.json'))).toBe(true);
  });

  it('should create output directory if needed', async () => {
    await renderChartsToSVG(
      {
        embedding: [],
        llm: mockLLMResults,
      },
      testOutputDir
    );

    expect(fs.existsSync(testOutputDir)).toBe(true);
  });
});
