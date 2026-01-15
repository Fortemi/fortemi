# Report Generators

This module provides report generation functionality for the evaluation framework.

## Overview

The reporters module transforms evaluation results into human-readable and machine-readable formats:

- **JSON Reporter**: Generates raw JSON output with complete evaluation data
- **Markdown Reporter**: Creates professional, formatted reports with tables and insights
- **Chart Generator**: Creates Vega-Lite visualization specifications for charts and graphs

## Usage

```typescript
import { generateJSONReport, generateMarkdownReport, generateAllCharts } from './reporters/index.js';
import type { EvaluationReport } from './models/types.js';

// After running evaluations...
const report: EvaluationReport = {
  meta: { /* ... */ },
  embeddingResults: { /* ... */ },
  llmResults: { /* ... */ },
  recommendations: { /* ... */ }
};

// Generate JSON report
const jsonReport = generateJSONReport(report);
console.log(jsonReport);

// Generate Markdown report
const markdownReport = generateMarkdownReport(report);
console.log(markdownReport);

// Generate charts
await generateAllCharts({
  embedding: Object.values(report.embeddingResults),
  llm: Object.values(report.llmResults),
}, './output/charts');
```

## JSON Reporter

**File:** `json.ts`

Generates a raw JSON report with all evaluation data, suitable for:
- Programmatic consumption
- Data pipelines
- Long-term storage
- Further analysis

**Features:**
- Pretty-printed with 2-space indentation
- Includes all metrics and metadata
- Preserves exact numerical values
- Timestamped results

**Example Output:**
```json
{
  "meta": {
    "timestamp": "2026-01-14T22:00:00Z",
    "durationMs": 5000,
    "modelsTested": 2,
    "scenariosRun": 10
  },
  "embeddingResults": {
    "nomic-embed-text": {
      "modelName": "nomic-embed-text",
      "overallScore": 78.5,
      "metrics": { /* ... */ }
    }
  }
}
```

## Markdown Reporter

**File:** `markdown.ts`

Generates a professional, human-readable Markdown report with:

### Executive Summary
- Top recommendations for best models
- High-level insights
- Key performance highlights

### Comparison Tables
- Embedding models sorted by score
- LLM models sorted by score
- All key metrics in clean, readable tables

**Example Embedding Table:**
| Model | Score | P@5 | P@10 | MRR | NDCG | Latency (p95) | Throughput |
|-------|-------|-----|------|-----|------|---------------|------------|
| nomic-embed-text | 78.5 | 82.0% | 75.0% | 88.0% | 79.0% | 65ms | 22.5/s |

**Example LLM Table:**
| Model | Score | Revision | Title | Context | Instruction | Efficiency | Latency (p95) |
|-------|-------|----------|-------|---------|-------------|------------|---------------|
| gpt-4o-mini | 85.2 | 88.5 | 82.0 | 86.5 | 90.0 | 75.0 | 850ms |

### Detailed Breakdowns
Per-model detailed results including:
- All retrieval metrics (P@5, P@10, R@5, R@10, MRR, NDCG)
- Similarity accuracy
- Complete latency statistics (P50, P95, P99, mean)
- Throughput metrics
- LLM quality dimensions

### Methodology Section
Complete documentation of:
- Scoring weights for all metrics
- Evaluation criteria
- Sub-dimension weights
- Score calculation methodology

## Chart Generator

**File:** `charts.ts`

Generates publication-quality charts using Vega-Lite specifications.

### Available Charts

#### 1. Model Comparison Bar Chart
`generateModelComparisonChart(results: LLMEvalResult[])`

- X-axis: Model names
- Y-axis: Overall score (0-100)
- Color: Model size category

#### 2. Radar Chart
`generateRadarChart(results: LLMEvalResult[])`

- Shows top 5 models
- Axes for each evaluation dimension
- Overlapping polygons for easy comparison

#### 3. Latency vs Quality Scatter Plot
`generateLatencyVsQualityChart(results: LLMEvalResult[])`

- X-axis: Average latency (ms)
- Y-axis: Quality score
- Bubble size: Model parameter count

#### 4. Retrieval Metrics Grouped Bar Chart
`generateRetrievalMetricsChart(results: EmbeddingEvalResult[])`

- Groups: P@5, P@10, R@5, R@10, MRR, NDCG@10
- One bar per embedding model

#### 5. Score Distribution Box Plot
`generateScoreDistributionChart(results: LLMEvalResult[])`

- Shows score variance across dimensions
- Box plots per model

### Generate All Charts

```typescript
import { generateAllCharts } from './reporters/charts.js';

await generateAllCharts({
  embedding: embeddingResults,
  llm: llmResults,
}, './output/charts');
```

This creates JSON files for each chart:
- `model-comparison.json`
- `radar-chart.json`
- `latency-vs-quality.json`
- `retrieval-metrics.json`
- `score-distribution.json`

### Chart Options

```typescript
interface ChartGenerationOptions {
  width?: number;   // Default: 600
  height?: number;  // Default: 400
  theme?: 'light' | 'dark';  // Not yet implemented
}
```

### Viewing Charts

**Using Vega Editor:**
1. Open https://vega.github.io/editor/
2. Select "Vega-Lite" mode
3. Paste JSON spec from generated file
4. View interactive chart

**Render to SVG:**
```typescript
import { renderChartsToSVG } from './reporters/charts.js';

await renderChartsToSVG(results, './output/charts');
```

Requires optional dependencies:
```bash
npm install vega vega-lite
```

### Helper Functions

- `extractParameterCount(modelName)` - Extract param count from model name
- `getModelSizeCategory(modelName)` - Categorize by size (micro/tiny/small/medium/large)
- `getCategoryColor(category)` - Get hex color for category

## File Structure

```
src/reporters/
├── index.ts         # Main exports
├── json.ts          # JSON reporter implementation
├── markdown.ts      # Markdown reporter implementation
├── charts.ts        # Chart generator implementation
├── json.test.ts     # JSON reporter tests
├── markdown.test.ts # Markdown reporter tests
├── charts.test.ts   # Chart generator tests
├── example.ts       # Usage example
└── README.md        # This file
```

## Testing

Run the test suite:

```bash
npm test -- src/reporters
```

View test coverage:

```bash
npm run test:coverage -- src/reporters
```

Run the example:

```bash
npx tsx src/reporters/example.ts
```

## Design Principles

1. **Data Integrity**: JSON reporter preserves all data exactly as provided
2. **Readability**: Markdown reporter prioritizes clarity and actionability
3. **Visual Clarity**: Charts use consistent color schemes and layouts
4. **Consistency**: All reporters use the same input format
5. **Extensibility**: Easy to add new report formats or chart types
6. **Testability**: Comprehensive test coverage ensures reliability
7. **Graceful Degradation**: Charts fall back to JSON when vega unavailable

## Adding New Report Formats

To add a new report format:

1. Create a new file: `src/reporters/{format}.ts`
2. Implement the generator function: `generate{Format}Report(results: EvaluationReport): string`
3. Create tests: `src/reporters/{format}.test.ts`
4. Export from `src/reporters/index.ts`
5. Update this README

Example:

```typescript
// csv.ts
export function generateCSVReport(results: EvaluationReport): string {
  // Implementation
}

// csv.test.ts
describe('generateCSVReport', () => {
  // Tests
});

// index.ts
export { generateCSVReport } from './csv.js';
```

## Adding New Chart Types

To add a new chart:

1. Add function to `charts.ts`:
   ```typescript
   export function generateNewChart(results: SomeResult[]): VegaLiteSpec {
     // Chart spec generation
   }
   ```

2. Add tests to `charts.test.ts`

3. Update `generateAllCharts()` to include new chart

4. Export from `index.ts`

## Integration

These reporters integrate with:
- **Evaluation Runner** (`src/runner.ts`): Generates reports after evaluation runs
- **CLI** (`src/index.ts`): Provides report generation commands
- **CI/CD**: Can be automated for continuous model evaluation

## Performance

All reporters are highly performant:
- JSON generation: O(1) - simple serialization
- Markdown generation: O(n log n) - dominated by model sorting
- Chart generation: O(n) - linear in number of data points
- Memory efficient: processes data in streaming fashion

## Color Scheme (Charts)

Consistent color palette for model sizes:
- **Micro** (< 2B): Orange (#f59e0b)
- **Tiny** (2-5B): Green (#10b981)
- **Small** (5-12B): Blue (#3b82f6)
- **Medium** (12-25B): Purple (#8b5cf6)
- **Large** (≥ 25B): Red (#ef4444)

## See Also

- [Types Documentation](../models/types.ts) - Evaluation result types
- [Weights Configuration](../scoring/weights.ts) - Scoring methodology
- [Evaluation Runner](../runner.ts) - Running evaluations
- [Vega-Lite Documentation](https://vega.github.io/vega-lite/) - Chart specifications
