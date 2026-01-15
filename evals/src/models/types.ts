/**
 * Core type definitions for the evaluation framework
 */

// ============================================================================
// Model Interfaces
// ============================================================================

export interface EmbeddingModel {
  name: string;
  dimensions: number;
  embed(text: string): Promise<number[]>;
  embedBatch(texts: string[]): Promise<number[][]>;
}

export interface GenerationModel {
  name: string;
  generate(prompt: string, options?: GenerationOptions): Promise<GenerationResult>;
}

export interface GenerationOptions {
  temperature?: number;
  maxTokens?: number;
  stopSequences?: string[];
  stream?: boolean;
}

export interface GenerationResult {
  text: string;
  tokensGenerated: number;
  timeToFirstToken?: number;
  totalTime: number;
}

// ============================================================================
// Evaluation Configuration
// ============================================================================

export interface EvaluationConfig {
  models: {
    embeddings?: string[];
    llms?: string[];
  };
  datasets: {
    embeddingTests?: string;
    revisionTests?: string;
    titleTests?: string;
    contextTests?: string;
  };
  output: {
    directory: string;
    format: 'json' | 'markdown' | 'both';
    generateCharts?: boolean;
  };
  parallel?: boolean;
  verbose?: boolean;
}

// ============================================================================
// Test Data Structures
// ============================================================================

export interface SimilarityPair {
  id: string;
  text1: string;
  text2: string;
  expectedSimilarity: 'high' | 'medium' | 'low';
  similarityScore?: number; // 0-1, optional ground truth
}

export interface RetrievalQuery {
  id: string;
  query: string;
  relevantDocs: Array<{
    docId: string;
    relevanceScore: number; // 0-1
  }>;
  corpus: Array<{
    docId: string;
    text: string;
  }>;
}

export interface RevisionTestCase {
  id: string;
  mode: 'full' | 'light' | 'none';
  input: {
    content: string;
    relatedNotes?: string[];
  };
  expected: {
    preservesConcepts: string[];
    addsStructure: boolean;
    noHallucination: boolean;
    minLength?: number;
    maxLength?: number;
  };
}

export interface TitleTestCase {
  id: string;
  content: string;
  expectedTitle: string;
  alternativeTitles?: string[];
}

// ============================================================================
// Metrics Results
// ============================================================================

export interface RetrievalMetrics {
  precisionAt5: number;
  precisionAt10: number;
  recallAt5: number;
  recallAt10: number;
  mrr: number; // Mean Reciprocal Rank
  ndcgAt10: number; // Normalized Discounted Cumulative Gain
}

export interface LatencyMetrics {
  p50: number;
  p95: number;
  p99: number;
  mean: number;
  min: number;
  max: number;
}

export interface SimilarityMetrics {
  accuracy: number; // % of correct similarity judgments
  meanAbsoluteError?: number; // if ground truth scores provided
}

// ============================================================================
// Evaluation Results
// ============================================================================

export interface EmbeddingEvalResult {
  modelName: string;
  overallScore: number;
  metrics: {
    retrieval: RetrievalMetrics;
    similarity: SimilarityMetrics;
    latency: LatencyMetrics;
    throughput: number; // embeddings per second
  };
  timestamp: string;
}

export interface LLMEvalResult {
  modelName: string;
  overallScore: number;
  dimensions: {
    revisionQuality: number;
    titleQuality: number;
    contextQuality: number;
    instructionFollowing: number;
    efficiency: number;
  };
  metrics: {
    latency: LatencyMetrics;
    tokensPerSecond: number;
  };
  timestamp: string;
}

export interface EvaluationReport {
  meta: {
    timestamp: string;
    durationMs: number;
    modelsTested: number;
    scenariosRun: number;
  };
  embeddingResults: Record<string, EmbeddingEvalResult>;
  llmResults: Record<string, LLMEvalResult>;
  recommendations: {
    bestEmbedding?: string;
    bestLLMQuality?: string;
    bestLLMBalanced?: string;
    bestLLMSpeed?: string;
  };
}

// ============================================================================
// Internal Types
// ============================================================================

export interface TimingResult {
  startTime: number;
  endTime: number;
  duration: number;
}

export interface RankedResult<T = unknown> {
  item: T;
  score: number;
  rank: number;
}
