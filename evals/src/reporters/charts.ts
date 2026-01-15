/**
 * Chart generation using Vega-Lite specifications
 * Generates SVG charts for evaluation results visualization
 */

import type { EmbeddingEvalResult, LLMEvalResult } from '../models/types.js';
import * as fs from 'fs';
import * as path from 'path';

// ============================================================================
// Type Definitions
// ============================================================================

export type ModelSizeCategory = 'micro' | 'tiny' | 'small' | 'medium' | 'large';

export interface VegaLiteSpec {
  $schema: string;
  description?: string;
  title?: string | Record<string, any>;
  width?: number;
  height?: number;
  data: {
    values: any[];
  };
  mark: string | Record<string, any>;
  encoding?: Record<string, any>;
  layer?: any[];
  config?: Record<string, any>;
}

export interface ChartGenerationOptions {
  width?: number;
  height?: number;
  theme?: 'light' | 'dark';
}

// ============================================================================
// Helper Functions
// ============================================================================

/**
 * Extract parameter count from model name
 * Examples: "qwen2.5:14b" -> 14, "llama3.1:8b" -> 8
 */
export function extractParameterCount(modelName: string): number {
  const match = modelName.match(/(\d+\.?\d*)b/i);
  if (match) {
    return parseFloat(match[1]);
  }
  return 7; // Default to 7B if can't extract
}

/**
 * Categorize model by size based on parameter count
 * Follows PLAN.md model categories
 */
export function getModelSizeCategory(modelName: string): ModelSizeCategory {
  const params = extractParameterCount(modelName);

  if (params < 2) return 'micro';
  if (params < 5) return 'tiny';
  if (params < 12) return 'small';
  if (params < 25) return 'medium';
  return 'large';
}

/**
 * Get color scheme for model size categories
 */
export function getCategoryColor(category: ModelSizeCategory): string {
  const colors: Record<ModelSizeCategory, string> = {
    micro: '#f59e0b',
    tiny: '#10b981',
    small: '#3b82f6',
    medium: '#8b5cf6',
    large: '#ef4444',
  };
  return colors[category];
}

/**
 * Create base Vega-Lite spec with common settings
 */
function createBaseSpec(
  title: string,
  options: ChartGenerationOptions = {}
): Partial<VegaLiteSpec> {
  return {
    $schema: 'https://vega.github.io/schema/vega-lite/v5.json',
    title,
    width: options.width || 600,
    height: options.height || 400,
    config: {
      axis: {
        labelFontSize: 11,
        titleFontSize: 13,
      },
      legend: {
        labelFontSize: 11,
        titleFontSize: 12,
      },
    },
  };
}

// ============================================================================
// Chart Generation Functions
// ============================================================================

/**
 * Generate model comparison bar chart
 * X: Models, Y: Overall Score, Color: Model size category
 */
export function generateModelComparisonChart(
  results: LLMEvalResult[],
  options: ChartGenerationOptions = {}
): VegaLiteSpec {
  const data = results.map((result) => ({
    model: result.modelName,
    score: result.overallScore,
    category: getModelSizeCategory(result.modelName),
  }));

  return {
    ...createBaseSpec('Model Comparison: Overall Scores', options),
    data: { values: data },
    mark: 'bar',
    encoding: {
      x: {
        field: 'model',
        type: 'nominal',
        axis: { labelAngle: -45, title: 'Model' },
        sort: '-y',
      },
      y: {
        field: 'score',
        type: 'quantitative',
        axis: { title: 'Overall Score (0-100)' },
        scale: { domain: [0, 100] },
      },
      color: {
        field: 'category',
        type: 'nominal',
        legend: { title: 'Model Size' },
        scale: {
          domain: ['micro', 'tiny', 'small', 'medium', 'large'],
          range: ['#f59e0b', '#10b981', '#3b82f6', '#8b5cf6', '#ef4444'],
        },
      },
      tooltip: [
        { field: 'model', type: 'nominal', title: 'Model' },
        { field: 'score', type: 'quantitative', title: 'Score', format: '.1f' },
        { field: 'category', type: 'nominal', title: 'Category' },
      ],
    },
  } as VegaLiteSpec;
}

/**
 * Generate radar/spider chart for top 5 models
 * Shows all evaluation dimensions for comparison
 */
export function generateRadarChart(
  results: LLMEvalResult[],
  options: ChartGenerationOptions = {}
): VegaLiteSpec {
  // Sort by overall score and take top 5
  const topModels = results
    .sort((a, b) => b.overallScore - a.overallScore)
    .slice(0, 5);

  // Transform data to long format for radar chart
  const data: any[] = [];
  topModels.forEach((result) => {
    Object.entries(result.dimensions).forEach(([dimension, value]) => {
      data.push({
        model: result.modelName,
        dimension,
        score: value,
      });
    });
  });

  return {
    ...createBaseSpec('Performance Radar: Top 5 Models', options),
    data: { values: data },
    layer: [
      {
        mark: { type: 'line', point: true },
        encoding: {
          theta: {
            field: 'dimension',
            type: 'nominal',
            stack: null,
            scale: { domain: null },
          },
          radius: {
            field: 'score',
            type: 'quantitative',
            scale: { domain: [0, 100], zero: true },
          },
          color: {
            field: 'model',
            type: 'nominal',
            legend: { title: 'Model' },
          },
          tooltip: [
            { field: 'model', type: 'nominal' },
            { field: 'dimension', type: 'nominal' },
            { field: 'score', type: 'quantitative', format: '.1f' },
          ],
        },
      },
      {
        mark: { type: 'point', size: 50, filled: true },
        encoding: {
          theta: {
            field: 'dimension',
            type: 'nominal',
            stack: null,
          },
          radius: {
            field: 'score',
            type: 'quantitative',
            scale: { domain: [0, 100], zero: true },
          },
          color: {
            field: 'model',
            type: 'nominal',
          },
        },
      },
    ],
  } as VegaLiteSpec;
}

/**
 * Generate latency vs quality scatter plot
 * X: Average latency, Y: Quality score, Size: Model parameters
 */
export function generateLatencyVsQualityChart(
  results: LLMEvalResult[],
  options: ChartGenerationOptions = {}
): VegaLiteSpec {
  const data = results.map((result) => ({
    model: result.modelName,
    latency: result.metrics.latency.mean,
    quality: result.overallScore,
    parameters: extractParameterCount(result.modelName),
    category: getModelSizeCategory(result.modelName),
  }));

  return {
    ...createBaseSpec('Latency vs Quality Tradeoff', options),
    data: { values: data },
    mark: { type: 'point', filled: true },
    encoding: {
      x: {
        field: 'latency',
        type: 'quantitative',
        axis: { title: 'Average Latency (ms)' },
        scale: { zero: false },
      },
      y: {
        field: 'quality',
        type: 'quantitative',
        axis: { title: 'Overall Quality Score (0-100)' },
        scale: { domain: [0, 100] },
      },
      size: {
        field: 'parameters',
        type: 'quantitative',
        legend: { title: 'Parameters (B)' },
        scale: { range: [100, 1000] },
      },
      color: {
        field: 'category',
        type: 'nominal',
        legend: { title: 'Model Size' },
        scale: {
          domain: ['micro', 'tiny', 'small', 'medium', 'large'],
          range: ['#f59e0b', '#10b981', '#3b82f6', '#8b5cf6', '#ef4444'],
        },
      },
      tooltip: [
        { field: 'model', type: 'nominal', title: 'Model' },
        {
          field: 'quality',
          type: 'quantitative',
          title: 'Quality',
          format: '.1f',
        },
        {
          field: 'latency',
          type: 'quantitative',
          title: 'Latency (ms)',
          format: '.0f',
        },
        {
          field: 'parameters',
          type: 'quantitative',
          title: 'Parameters (B)',
          format: '.1f',
        },
      ],
    },
  } as VegaLiteSpec;
}

/**
 * Generate grouped bar chart for retrieval metrics
 * Groups: P@5, P@10, R@5, R@10, MRR, NDCG
 * One bar per embedding model
 */
export function generateRetrievalMetricsChart(
  results: EmbeddingEvalResult[],
  options: ChartGenerationOptions = {}
): VegaLiteSpec {
  const data: any[] = [];

  results.forEach((result) => {
    const metrics = result.metrics.retrieval;
    data.push(
      { model: result.modelName, metric: 'P@5', value: metrics.precisionAt5 * 100 },
      { model: result.modelName, metric: 'P@10', value: metrics.precisionAt10 * 100 },
      { model: result.modelName, metric: 'R@5', value: metrics.recallAt5 * 100 },
      { model: result.modelName, metric: 'R@10', value: metrics.recallAt10 * 100 },
      { model: result.modelName, metric: 'MRR', value: metrics.mrr * 100 },
      { model: result.modelName, metric: 'NDCG@10', value: metrics.ndcgAt10 * 100 }
    );
  });

  return {
    ...createBaseSpec('Retrieval Metrics Comparison', options),
    data: { values: data },
    mark: 'bar',
    encoding: {
      x: {
        field: 'metric',
        type: 'nominal',
        axis: { title: 'Metric', labelAngle: 0 },
      },
      y: {
        field: 'value',
        type: 'quantitative',
        axis: { title: 'Score (%)' },
        scale: { domain: [0, 100] },
      },
      color: {
        field: 'model',
        type: 'nominal',
        legend: { title: 'Embedding Model' },
      },
      xOffset: {
        field: 'model',
        type: 'nominal',
      },
      tooltip: [
        { field: 'model', type: 'nominal', title: 'Model' },
        { field: 'metric', type: 'nominal', title: 'Metric' },
        { field: 'value', type: 'quantitative', title: 'Score', format: '.1f' },
      ],
    },
  } as VegaLiteSpec;
}

/**
 * Generate score distribution box plot
 * Shows variance across different dimensions per model
 */
export function generateScoreDistributionChart(
  results: LLMEvalResult[],
  options: ChartGenerationOptions = {}
): VegaLiteSpec {
  // Transform dimension scores to long format
  const data: any[] = [];

  results.forEach((result) => {
    Object.entries(result.dimensions).forEach(([dimension, value]) => {
      data.push({
        model: result.modelName,
        dimension,
        score: value,
      });
    });
  });

  return {
    ...createBaseSpec('Score Distribution by Model', options),
    data: { values: data },
    mark: { type: 'boxplot', extent: 'min-max' },
    encoding: {
      x: {
        field: 'model',
        type: 'nominal',
        axis: { title: 'Model', labelAngle: -45 },
      },
      y: {
        field: 'score',
        type: 'quantitative',
        axis: { title: 'Dimension Scores (0-100)' },
        scale: { domain: [0, 100] },
      },
      color: {
        field: 'model',
        type: 'nominal',
        legend: null,
      },
      tooltip: [
        { field: 'model', type: 'nominal', title: 'Model' },
        { field: 'score', type: 'quantitative', title: 'Score', format: '.1f' },
      ],
    },
  } as VegaLiteSpec;
}

// ============================================================================
// Batch Chart Generation
// ============================================================================

export interface ChartResults {
  embedding: EmbeddingEvalResult[];
  llm: LLMEvalResult[];
}

/**
 * Generate all charts and save to output directory
 * Saves Vega-Lite JSON specs that can be rendered elsewhere
 */
export async function generateAllCharts(
  results: ChartResults,
  outputDir: string,
  options: ChartGenerationOptions = {}
): Promise<void> {
  // Create output directory if it doesn't exist
  if (!fs.existsSync(outputDir)) {
    fs.mkdirSync(outputDir, { recursive: true });
  }

  const charts: Array<{ name: string; spec: VegaLiteSpec }> = [];

  // Generate LLM charts if data available
  if (results.llm && results.llm.length > 0) {
    charts.push({
      name: 'model-comparison',
      spec: generateModelComparisonChart(results.llm, options),
    });

    charts.push({
      name: 'radar-chart',
      spec: generateRadarChart(results.llm, options),
    });

    charts.push({
      name: 'latency-vs-quality',
      spec: generateLatencyVsQualityChart(results.llm, options),
    });

    charts.push({
      name: 'score-distribution',
      spec: generateScoreDistributionChart(results.llm, options),
    });
  }

  // Generate embedding charts if data available
  if (results.embedding && results.embedding.length > 0) {
    charts.push({
      name: 'retrieval-metrics',
      spec: generateRetrievalMetricsChart(results.embedding, options),
    });
  }

  // Write all charts to disk as JSON specs
  for (const { name, spec } of charts) {
    const filePath = path.join(outputDir, `${name}.json`);
    fs.writeFileSync(filePath, JSON.stringify(spec, null, 2), 'utf-8');
  }

  console.log(`Generated ${charts.length} chart specifications in ${outputDir}`);
}

/**
 * Attempt to render charts to SVG using vega if available
 * Falls back to JSON specs if rendering not available
 */
export async function renderChartsToSVG(
  results: ChartResults,
  outputDir: string,
  options: ChartGenerationOptions = {}
): Promise<void> {
  try {
    // Try to import vega for rendering (dynamic import to handle optional dependency)
    // @ts-expect-error - vega is an optional dependency
    const vegaModule = await import('vega').catch(() => null);
    const vegaLiteModule = await import('vega-lite').catch(() => null);

    if (!vegaModule || !vegaLiteModule) {
      console.warn('Vega modules not available, falling back to JSON specs');
      await generateAllCharts(results, outputDir, options);
      return;
    }

    const vega = vegaModule;
    const vegaLite = vegaLiteModule;

    // Create output directory
    if (!fs.existsSync(outputDir)) {
      fs.mkdirSync(outputDir, { recursive: true });
    }

    const charts: Array<{ name: string; spec: VegaLiteSpec }> = [];

    // Generate charts (same as generateAllCharts)
    if (results.llm && results.llm.length > 0) {
      charts.push({
        name: 'model-comparison',
        spec: generateModelComparisonChart(results.llm, options),
      });
      charts.push({
        name: 'radar-chart',
        spec: generateRadarChart(results.llm, options),
      });
      charts.push({
        name: 'latency-vs-quality',
        spec: generateLatencyVsQualityChart(results.llm, options),
      });
      charts.push({
        name: 'score-distribution',
        spec: generateScoreDistributionChart(results.llm, options),
      });
    }

    if (results.embedding && results.embedding.length > 0) {
      charts.push({
        name: 'retrieval-metrics',
        spec: generateRetrievalMetricsChart(results.embedding, options),
      });
    }

    // Render each chart to SVG
    for (const { name, spec } of charts) {
      try {
        // Compile Vega-Lite to Vega (using any type to avoid type issues)
        const vegaSpec = (vegaLite as any).compile(spec as any).spec;

        // Create Vega view
        const view = new (vega as any).View((vega as any).parse(vegaSpec), {
          renderer: 'none',
        });

        // Render to SVG
        const svg = await view.toSVG();

        // Save SVG
        const svgPath = path.join(outputDir, `${name}.svg`);
        fs.writeFileSync(svgPath, svg, 'utf-8');

        // Also save JSON spec
        const jsonPath = path.join(outputDir, `${name}.json`);
        fs.writeFileSync(jsonPath, JSON.stringify(spec, null, 2), 'utf-8');
      } catch (error) {
        console.warn(`Failed to render ${name} to SVG:`, error);
        // Fall back to saving JSON spec only
        const jsonPath = path.join(outputDir, `${name}.json`);
        fs.writeFileSync(jsonPath, JSON.stringify(spec, null, 2), 'utf-8');
      }
    }

    console.log(`Rendered ${charts.length} charts to ${outputDir}`);
  } catch (error) {
    console.warn('Vega rendering not available, falling back to JSON specs:', error);
    // Fall back to JSON-only generation
    await generateAllCharts(results, outputDir, options);
  }
}
