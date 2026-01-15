/**
 * Configurable weight constants for scoring system
 * Based on PLAN.md evaluation criteria
 */

// ============================================================================
// Embedding Model Weights (Section 1.1 of PLAN.md)
// ============================================================================

export const EMBEDDING_WEIGHTS = {
  precisionAt5: 0.20,
  recallAt10: 0.15,
  mrr: 0.20,
  ndcgAt10: 0.20,
  semanticAccuracy: 0.15,
  latency: 0.05,
  throughput: 0.05,
} as const;

// ============================================================================
// LLM Dimension Weights (Section 1.2 of PLAN.md)
// ============================================================================

export const LLM_DIMENSION_WEIGHTS = {
  revisionQuality: 0.40,
  titleQuality: 0.20,
  contextQuality: 0.20,
  instructionFollowing: 0.10,
  efficiency: 0.10,
} as const;

// Revision Quality Sub-Weights (Section 1.2.A)
export const REVISION_QUALITY_WEIGHTS = {
  informationPreservation: 0.25,
  structureEnhancement: 0.20,
  noHallucination: 0.30,
  contextualIntegration: 0.15,
  readability: 0.10,
} as const;

// Title Generation Quality Sub-Weights (Section 1.2.B)
export const TITLE_QUALITY_WEIGHTS = {
  relevance: 0.35,
  conciseness: 0.25,
  uniqueness: 0.20,
  formatCompliance: 0.20,
} as const;

// Context Understanding Sub-Weights (Section 1.2.C)
export const CONTEXT_QUALITY_WEIGHTS = {
  summaryAccuracy: 0.40,
  relationshipClarity: 0.30,
  brevity: 0.30,
} as const;

// Instruction Following Sub-Weights (Section 1.2.D)
export const INSTRUCTION_FOLLOWING_WEIGHTS = {
  modeCompliance: 0.50,
  formatAdherence: 0.30,
  constraintRespect: 0.20,
} as const;

// Efficiency Sub-Weights (Section 1.2.E)
export const EFFICIENCY_WEIGHTS = {
  latencyTTFT: 0.30,
  latencyTotal: 0.30,
  tokenEfficiency: 0.40,
} as const;

// ============================================================================
// Normalization Parameters
// ============================================================================

/**
 * Latency normalization parameters
 * LatencyScore = max(0, 100 - (p95_latency_ms / 10))
 */
export const LATENCY_NORMALIZATION = {
  divisor: 10,
  maxScore: 100,
} as const;

/**
 * Throughput normalization parameters
 * ThroughputScore = min(100, embeddings_per_sec × 5)
 */
export const THROUGHPUT_NORMALIZATION = {
  multiplier: 5,
  maxScore: 100,
} as const;

/**
 * Similarity score thresholds for categorical judgments
 */
export const SIMILARITY_THRESHOLDS = {
  high: 0.7,
  medium: 0.4,
} as const;

// ============================================================================
// Validation
// ============================================================================

/**
 * Validate that weights sum to 1.0 (within floating point tolerance)
 */
function validateWeights(weights: Record<string, number>, name: string): void {
  const sum = Object.values(weights).reduce((acc, w) => acc + w, 0);
  const tolerance = 0.001;

  if (Math.abs(sum - 1.0) > tolerance) {
    throw new Error(
      `${name} weights must sum to 1.0, got ${sum.toFixed(3)}`
    );
  }
}

// Validate all weight configurations at module load time
validateWeights(EMBEDDING_WEIGHTS, 'EMBEDDING_WEIGHTS');
validateWeights(LLM_DIMENSION_WEIGHTS, 'LLM_DIMENSION_WEIGHTS');
validateWeights(REVISION_QUALITY_WEIGHTS, 'REVISION_QUALITY_WEIGHTS');
validateWeights(TITLE_QUALITY_WEIGHTS, 'TITLE_QUALITY_WEIGHTS');
validateWeights(CONTEXT_QUALITY_WEIGHTS, 'CONTEXT_QUALITY_WEIGHTS');
validateWeights(INSTRUCTION_FOLLOWING_WEIGHTS, 'INSTRUCTION_FOLLOWING_WEIGHTS');
validateWeights(EFFICIENCY_WEIGHTS, 'EFFICIENCY_WEIGHTS');
