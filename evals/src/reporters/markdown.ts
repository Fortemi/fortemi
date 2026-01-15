/**
 * Markdown reporter for evaluation results
 * Generates a professional, human-readable report
 */

import type { EvaluationReport, EmbeddingEvalResult, LLMEvalResult } from '../models/types.js';
import {
  EMBEDDING_WEIGHTS,
  LLM_DIMENSION_WEIGHTS,
  REVISION_QUALITY_WEIGHTS,
  TITLE_QUALITY_WEIGHTS,
  CONTEXT_QUALITY_WEIGHTS,
  INSTRUCTION_FOLLOWING_WEIGHTS,
  EFFICIENCY_WEIGHTS,
} from '../scoring/weights.js';

/**
 * Generate a Markdown report from evaluation results
 *
 * @param results - The complete evaluation report
 * @returns Formatted markdown document
 */
export function generateMarkdownReport(results: EvaluationReport): string {
  const sections: string[] = [];

  // Header
  sections.push('# Model Evaluation Report\n');
  sections.push(generateMetadata(results));
  sections.push(generateExecutiveSummary(results));

  // Embedding Models Section
  if (Object.keys(results.embeddingResults).length > 0) {
    sections.push(generateEmbeddingSection(results));
  } else {
    sections.push('## Embedding Models\n');
    sections.push('No embedding models were evaluated.\n');
  }

  // LLM Models Section
  if (Object.keys(results.llmResults).length > 0) {
    sections.push(generateLLMSection(results));
  } else {
    sections.push('## LLM Models\n');
    sections.push('No LLM models were evaluated.\n');
  }

  // Methodology
  sections.push(generateMethodology());

  return sections.join('\n');
}

/**
 * Generate metadata section
 */
function generateMetadata(results: EvaluationReport): string {
  const date = new Date(results.meta.timestamp).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });

  const durationSec = (results.meta.durationMs / 1000).toFixed(1);

  return `
**Generated:** ${date}
**Duration:** ${durationSec}s
**Models Tested:** ${results.meta.modelsTested}
**Scenarios Run:** ${results.meta.scenariosRun}
`;
}

/**
 * Generate executive summary with recommendations
 */
function generateExecutiveSummary(results: EvaluationReport): string {
  const lines: string[] = ['\n## Executive Summary\n'];

  // Recommendations
  if (results.recommendations.bestEmbedding) {
    lines.push(`**Recommended Embedding Model:** ${results.recommendations.bestEmbedding}`);
  }

  if (results.recommendations.bestLLMQuality) {
    lines.push(`**Best LLM for Quality:** ${results.recommendations.bestLLMQuality}`);
  }

  if (results.recommendations.bestLLMBalanced) {
    lines.push(`**Best Balanced LLM:** ${results.recommendations.bestLLMBalanced}`);
  }

  if (results.recommendations.bestLLMSpeed) {
    lines.push(`**Fastest LLM:** ${results.recommendations.bestLLMSpeed}`);
  }

  lines.push(''); // Empty line

  // Top-level insights
  const embeddingModels = Object.values(results.embeddingResults);
  const llmModels = Object.values(results.llmResults);

  if (embeddingModels.length > 0) {
    const topEmbedding = embeddingModels.reduce((a, b) =>
      a.overallScore > b.overallScore ? a : b
    );
    lines.push(
      `The top embedding model achieved an overall score of **${topEmbedding.overallScore.toFixed(1)}**, ` +
      `with strong performance in retrieval accuracy and semantic understanding.`
    );
  }

  if (llmModels.length > 0) {
    const topLLM = llmModels.reduce((a, b) =>
      a.overallScore > b.overallScore ? a : b
    );
    lines.push(
      `The top LLM model achieved an overall score of **${topLLM.overallScore.toFixed(1)}**, ` +
      `excelling in revision quality and instruction following.`
    );
  }

  return lines.join('\n');
}

/**
 * Generate embedding models section
 */
function generateEmbeddingSection(results: EvaluationReport): string {
  const sections: string[] = ['\n## Embedding Models\n'];

  // Comparison table
  sections.push(generateEmbeddingComparisonTable(results));

  // Detailed breakdowns
  sections.push('\n### Detailed Results\n');

  // Sort by score descending
  const models = Object.values(results.embeddingResults).sort(
    (a, b) => b.overallScore - a.overallScore
  );

  for (const model of models) {
    sections.push(generateEmbeddingDetails(model));
  }

  return sections.join('\n');
}

/**
 * Generate embedding comparison table
 */
function generateEmbeddingComparisonTable(results: EvaluationReport): string {
  const models = Object.values(results.embeddingResults).sort(
    (a, b) => b.overallScore - a.overallScore
  );

  const lines: string[] = [
    '| Model | Score | P@5 | P@10 | MRR | NDCG | Latency (p95) | Throughput |',
    '|-------|-------|-----|------|-----|------|---------------|------------|',
  ];

  for (const model of models) {
    const m = model.metrics;
    lines.push(
      `| ${model.modelName} | ` +
      `${model.overallScore.toFixed(1)} | ` +
      `${(m.retrieval.precisionAt5 * 100).toFixed(1)}% | ` +
      `${(m.retrieval.precisionAt10 * 100).toFixed(1)}% | ` +
      `${(m.retrieval.mrr * 100).toFixed(1)}% | ` +
      `${(m.retrieval.ndcgAt10 * 100).toFixed(1)}% | ` +
      `${m.latency.p95.toFixed(0)}ms | ` +
      `${m.throughput.toFixed(1)}/s |`
    );
  }

  return lines.join('\n');
}

/**
 * Generate detailed breakdown for one embedding model
 */
function generateEmbeddingDetails(model: EmbeddingEvalResult): string {
  const m = model.metrics;

  return `
#### ${model.modelName}

**Overall Score:** ${model.overallScore.toFixed(1)}

**Retrieval Performance:**
- Precision@5: ${(m.retrieval.precisionAt5 * 100).toFixed(1)}%
- Precision@10: ${(m.retrieval.precisionAt10 * 100).toFixed(1)}%
- Recall@5: ${(m.retrieval.recallAt5 * 100).toFixed(1)}%
- Recall@10: ${(m.retrieval.recallAt10 * 100).toFixed(1)}%
- MRR: ${(m.retrieval.mrr * 100).toFixed(1)}%
- NDCG@10: ${(m.retrieval.ndcgAt10 * 100).toFixed(1)}%

**Similarity Accuracy:** ${(m.similarity.accuracy * 100).toFixed(1)}%${
  m.similarity.meanAbsoluteError
    ? ` (MAE: ${m.similarity.meanAbsoluteError.toFixed(3)})`
    : ''
}

**Latency:**
- P50: ${m.latency.p50.toFixed(0)}ms
- P95: ${m.latency.p95.toFixed(0)}ms
- P99: ${m.latency.p99.toFixed(0)}ms
- Mean: ${m.latency.mean.toFixed(0)}ms

**Throughput:** ${m.throughput.toFixed(1)} embeddings/sec
`;
}

/**
 * Generate LLM models section
 */
function generateLLMSection(results: EvaluationReport): string {
  const sections: string[] = ['\n## LLM Models\n'];

  // Comparison table
  sections.push(generateLLMComparisonTable(results));

  // Detailed breakdowns
  sections.push('\n### Detailed Results\n');

  // Sort by score descending
  const models = Object.values(results.llmResults).sort(
    (a, b) => b.overallScore - a.overallScore
  );

  for (const model of models) {
    sections.push(generateLLMDetails(model));
  }

  return sections.join('\n');
}

/**
 * Generate LLM comparison table
 */
function generateLLMComparisonTable(results: EvaluationReport): string {
  const models = Object.values(results.llmResults).sort(
    (a, b) => b.overallScore - a.overallScore
  );

  const lines: string[] = [
    '| Model | Score | Revision | Title | Context | Instruction | Efficiency | Latency (p95) |',
    '|-------|-------|----------|-------|---------|-------------|------------|---------------|',
  ];

  for (const model of models) {
    const d = model.dimensions;
    lines.push(
      `| ${model.modelName} | ` +
      `${model.overallScore.toFixed(1)} | ` +
      `${d.revisionQuality.toFixed(1)} | ` +
      `${d.titleQuality.toFixed(1)} | ` +
      `${d.contextQuality.toFixed(1)} | ` +
      `${d.instructionFollowing.toFixed(1)} | ` +
      `${d.efficiency.toFixed(1)} | ` +
      `${model.metrics.latency.p95.toFixed(0)}ms |`
    );
  }

  return lines.join('\n');
}

/**
 * Generate detailed breakdown for one LLM model
 */
function generateLLMDetails(model: LLMEvalResult): string {
  const d = model.dimensions;
  const m = model.metrics;

  return `
#### ${model.modelName}

**Overall Score:** ${model.overallScore.toFixed(1)}

**Quality Dimensions:**
- Revision Quality: ${d.revisionQuality.toFixed(1)}
- Title Quality: ${d.titleQuality.toFixed(1)}
- Context Quality: ${d.contextQuality.toFixed(1)}
- Instruction Following: ${d.instructionFollowing.toFixed(1)}
- Efficiency: ${d.efficiency.toFixed(1)}

**Latency:**
- P50: ${m.latency.p50.toFixed(0)}ms
- P95: ${m.latency.p95.toFixed(0)}ms
- P99: ${m.latency.p99.toFixed(0)}ms
- Mean: ${m.latency.mean.toFixed(0)}ms

**Throughput:** ${m.tokensPerSecond.toFixed(1)} tokens/sec
`;
}

/**
 * Generate methodology section
 */
function generateMethodology(): string {
  return `
## Methodology

### Embedding Model Evaluation

Embedding models are scored using a weighted combination of:

- **Precision@5** (${(EMBEDDING_WEIGHTS.precisionAt5 * 100).toFixed(0)}%): Accuracy of top 5 results
- **Recall@10** (${(EMBEDDING_WEIGHTS.recallAt10 * 100).toFixed(0)}%): Coverage of relevant docs in top 10
- **MRR** (${(EMBEDDING_WEIGHTS.mrr * 100).toFixed(0)}%): Mean Reciprocal Rank
- **NDCG@10** (${(EMBEDDING_WEIGHTS.ndcgAt10 * 100).toFixed(0)}%): Normalized Discounted Cumulative Gain
- **Semantic Accuracy** (${(EMBEDDING_WEIGHTS.semanticAccuracy * 100).toFixed(0)}%): Similarity judgment accuracy
- **Latency** (${(EMBEDDING_WEIGHTS.latency * 100).toFixed(0)}%): Response time (P95)
- **Throughput** (${(EMBEDDING_WEIGHTS.throughput * 100).toFixed(0)}%): Embeddings per second

### LLM Model Evaluation

LLM models are evaluated across five dimensions:

**1. Revision Quality (${(LLM_DIMENSION_WEIGHTS.revisionQuality * 100).toFixed(0)}%)**
- Information Preservation (${(REVISION_QUALITY_WEIGHTS.informationPreservation * 100).toFixed(0)}%)
- Structure Enhancement (${(REVISION_QUALITY_WEIGHTS.structureEnhancement * 100).toFixed(0)}%)
- No Hallucination (${(REVISION_QUALITY_WEIGHTS.noHallucination * 100).toFixed(0)}%)
- Contextual Integration (${(REVISION_QUALITY_WEIGHTS.contextualIntegration * 100).toFixed(0)}%)
- Readability (${(REVISION_QUALITY_WEIGHTS.readability * 100).toFixed(0)}%)

**2. Title Quality (${(LLM_DIMENSION_WEIGHTS.titleQuality * 100).toFixed(0)}%)**
- Relevance (${(TITLE_QUALITY_WEIGHTS.relevance * 100).toFixed(0)}%)
- Conciseness (${(TITLE_QUALITY_WEIGHTS.conciseness * 100).toFixed(0)}%)
- Uniqueness (${(TITLE_QUALITY_WEIGHTS.uniqueness * 100).toFixed(0)}%)
- Format Compliance (${(TITLE_QUALITY_WEIGHTS.formatCompliance * 100).toFixed(0)}%)

**3. Context Quality (${(LLM_DIMENSION_WEIGHTS.contextQuality * 100).toFixed(0)}%)**
- Summary Accuracy (${(CONTEXT_QUALITY_WEIGHTS.summaryAccuracy * 100).toFixed(0)}%)
- Relationship Clarity (${(CONTEXT_QUALITY_WEIGHTS.relationshipClarity * 100).toFixed(0)}%)
- Brevity (${(CONTEXT_QUALITY_WEIGHTS.brevity * 100).toFixed(0)}%)

**4. Instruction Following (${(LLM_DIMENSION_WEIGHTS.instructionFollowing * 100).toFixed(0)}%)**
- Mode Compliance (${(INSTRUCTION_FOLLOWING_WEIGHTS.modeCompliance * 100).toFixed(0)}%)
- Format Adherence (${(INSTRUCTION_FOLLOWING_WEIGHTS.formatAdherence * 100).toFixed(0)}%)
- Constraint Respect (${(INSTRUCTION_FOLLOWING_WEIGHTS.constraintRespect * 100).toFixed(0)}%)

**5. Efficiency (${(LLM_DIMENSION_WEIGHTS.efficiency * 100).toFixed(0)}%)**
- Latency (TTFT) (${(EFFICIENCY_WEIGHTS.latencyTTFT * 100).toFixed(0)}%)
- Latency (Total) (${(EFFICIENCY_WEIGHTS.latencyTotal * 100).toFixed(0)}%)
- Token Efficiency (${(EFFICIENCY_WEIGHTS.tokenEfficiency * 100).toFixed(0)}%)

### Score Calculation

All scores are normalized to a 0-100 scale. The overall score is computed as a weighted sum of the individual metrics, ensuring consistency across different evaluation runs.
`;
}
