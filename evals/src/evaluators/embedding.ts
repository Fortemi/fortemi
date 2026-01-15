/**
 * Embedding model evaluator
 *
 * Evaluates embedding models across:
 * - Similarity/dissimilarity detection
 * - Retrieval quality (Precision@K, Recall@K, MRR, NDCG)
 * - Latency and throughput
 */

import { readFile } from 'fs/promises';
import { join } from 'path';
import type {
  EmbeddingModel,
  EmbeddingEvalResult,
  SimilarityPair,
  RetrievalMetrics,
  SimilarityMetrics,
  LatencyMetrics,
} from '../models/types.js';
import { cosineSimilarity, calculateSimilarityAccuracy } from '../metrics/similarity.js';
import {
  calculatePrecisionAtK,
  calculateRecallAtK,
  calculateMRR,
  calculateNDCG,
} from '../metrics/retrieval.js';
import { LatencyTracker } from '../metrics/latency.js';

/**
 * Dataset structure for embedding evaluation
 */
export interface EmbeddingDataset {
  similarityPairs: SimilarityPair[];
  dissimilarityPairs: SimilarityPair[];
  retrievalQueries: Array<{
    id: string;
    query: string;
    documents: Array<{
      id: string;
      content: string;
      relevance: number;
    }>;
  }>;
}

/**
 * Load embedding test datasets from directory
 */
export async function loadEmbeddingDatasets(datasetPath: string): Promise<EmbeddingDataset> {
  const [similarityPairsRaw, dissimilarityPairsRaw, retrievalQueriesRaw] = await Promise.all([
    readFile(join(datasetPath, 'similarity_pairs.json'), 'utf-8').then(JSON.parse),
    readFile(join(datasetPath, 'dissimilarity_pairs.json'), 'utf-8').then(JSON.parse),
    readFile(join(datasetPath, 'retrieval_queries.json'), 'utf-8').then(JSON.parse),
  ]);

  // Transform schema from snake_case to camelCase
  const similarityPairs = similarityPairsRaw.map((pair: any) => ({
    id: String(pair.id),
    text1: pair.text1,
    text2: pair.text2,
    expectedSimilarity: pair.expected_similarity,
    similarityScore: pair.similarity_score,
  }));

  const dissimilarityPairs = dissimilarityPairsRaw.map((pair: any) => ({
    id: String(pair.id),
    text1: pair.text1,
    text2: pair.text2,
    expectedSimilarity: pair.expected_similarity,
    similarityScore: pair.similarity_score,
  }));

  const retrievalQueries = retrievalQueriesRaw.map((query: any) => ({
    id: String(query.id),
    query: query.query,
    documents: query.documents.map((doc: any) => ({
      id: doc.id,
      content: doc.content,
      relevance: doc.relevance,
    })),
  }));

  return {
    similarityPairs,
    dissimilarityPairs,
    retrievalQueries,
  };
}

/**
 * Evaluate an embedding model across all test scenarios
 */
export async function evaluateEmbeddingModel(
  model: EmbeddingModel,
  dataset: EmbeddingDataset
): Promise<EmbeddingEvalResult> {
  const latencyTracker = new LatencyTracker();
  let totalEmbeddings = 0;
  const startTime = Date.now();

  // Helper to track latency for each embedding call
  const timedEmbed = async (text: string): Promise<number[]> => {
    const timer = latencyTracker.start();
    const embedding = await model.embed(text);
    timer.end();
    latencyTracker.recordFromTimer(timer);
    totalEmbeddings++;
    return embedding;
  };

  // 1. Evaluate similarity metrics
  const similarityMetrics = await evaluateSimilarity(
    model,
    [...dataset.similarityPairs, ...dataset.dissimilarityPairs],
    timedEmbed
  );

  // 2. Evaluate retrieval metrics
  const retrievalMetrics = await evaluateRetrieval(
    model,
    dataset.retrievalQueries,
    timedEmbed
  );

  // 3. Calculate latency and throughput
  const totalTime = Date.now() - startTime;
  const latencyMetrics = latencyTracker.getMeasurements().length > 0
    ? latencyTracker.getMetrics()
    : {
        p50: 0,
        p95: 0,
        p99: 0,
        mean: 0,
        min: 0,
        max: 0,
      };

  const throughput = totalTime > 0 ? (totalEmbeddings / totalTime) * 1000 : 0;

  // 4. Calculate overall score (per PLAN.md weights)
  const overallScore = calculateOverallScore(
    retrievalMetrics,
    similarityMetrics,
    latencyMetrics,
    throughput
  );

  return {
    modelName: model.name,
    overallScore,
    metrics: {
      retrieval: retrievalMetrics,
      similarity: similarityMetrics,
      latency: latencyMetrics,
      throughput,
    },
    timestamp: new Date().toISOString(),
  };
}

/**
 * Evaluate similarity accuracy
 */
async function evaluateSimilarity(
  model: EmbeddingModel,
  pairs: SimilarityPair[],
  timedEmbed: (text: string) => Promise<number[]>
): Promise<SimilarityMetrics> {
  if (pairs.length === 0) {
    return { accuracy: 0 };
  }

  const predictions = await Promise.all(
    pairs.map(async (pair) => {
      const [emb1, emb2] = await Promise.all([
        timedEmbed(pair.text1),
        timedEmbed(pair.text2),
      ]);

      const actualSimilarity = cosineSimilarity(emb1, emb2);

      return {
        actualSimilarity,
        expected: pair.expectedSimilarity,
      };
    })
  );

  const accuracy = calculateSimilarityAccuracy(predictions);

  // Calculate MAE if ground truth scores are provided
  const groundTruthPairs = pairs.filter(p => p.similarityScore !== undefined);
  let meanAbsoluteError: number | undefined;

  if (groundTruthPairs.length > 0) {
    const errors = await Promise.all(
      groundTruthPairs.map(async (pair) => {
        const [emb1, emb2] = await Promise.all([
          model.embed(pair.text1),
          model.embed(pair.text2),
        ]);

        const predicted = cosineSimilarity(emb1, emb2);
        const actual = pair.similarityScore!;

        return Math.abs(predicted - actual);
      })
    );

    meanAbsoluteError = errors.reduce((sum, err) => sum + err, 0) / errors.length;
  }

  return {
    accuracy,
    meanAbsoluteError,
  };
}

/**
 * Evaluate retrieval metrics
 */
async function evaluateRetrieval(
  _model: EmbeddingModel,
  queries: EmbeddingDataset['retrievalQueries'],
  timedEmbed: (text: string) => Promise<number[]>
): Promise<RetrievalMetrics> {
  if (queries.length === 0) {
    return {
      precisionAt5: 0,
      precisionAt10: 0,
      recallAt5: 0,
      recallAt10: 0,
      mrr: 0,
      ndcgAt10: 0,
    };
  }

  const queryResults = await Promise.all(
    queries.map(async (queryData) => {
      // Embed query
      const queryEmbedding = await timedEmbed(queryData.query);

      // Embed all documents in corpus
      const docEmbeddings = await Promise.all(
        queryData.documents.map(async (doc) => ({
          id: doc.id,
          embedding: await timedEmbed(doc.content),
          relevance: doc.relevance,
        }))
      );

      // Calculate similarities and rank documents
      const rankedDocs = docEmbeddings
        .map((doc) => ({
          docId: doc.id,
          similarity: cosineSimilarity(queryEmbedding, doc.embedding),
          relevance: doc.relevance,
        }))
        .sort((a, b) => b.similarity - a.similarity);

      // Extract relevant document IDs
      const relevantDocIds = new Set(
        queryData.documents
          .filter((doc) => doc.relevance >= 2) // Relevance >= 2 is considered relevant
          .map((doc) => doc.id)
      );

      // Create relevance score map for NDCG
      const relevanceScores = new Map(
        queryData.documents.map((doc) => [doc.id, doc.relevance])
      );

      const rankedDocIds = rankedDocs.map((doc) => doc.docId);

      return {
        ranked: rankedDocIds,
        relevant: relevantDocIds,
        relevanceScores,
      };
    })
  );

  // Calculate average metrics across all queries
  const precisionAt5 =
    queryResults.reduce(
      (sum, result) => sum + calculatePrecisionAtK(result.ranked, result.relevant, 5),
      0
    ) / queryResults.length;

  const precisionAt10 =
    queryResults.reduce(
      (sum, result) => sum + calculatePrecisionAtK(result.ranked, result.relevant, 10),
      0
    ) / queryResults.length;

  const recallAt5 =
    queryResults.reduce(
      (sum, result) => sum + calculateRecallAtK(result.ranked, result.relevant, 5),
      0
    ) / queryResults.length;

  const recallAt10 =
    queryResults.reduce(
      (sum, result) => sum + calculateRecallAtK(result.ranked, result.relevant, 10),
      0
    ) / queryResults.length;

  const mrr = calculateMRR(queryResults);

  const ndcgAt10 =
    queryResults.reduce(
      (sum, result) => sum + calculateNDCG(result.ranked, result.relevanceScores, 10),
      0
    ) / queryResults.length;

  return {
    precisionAt5,
    precisionAt10,
    recallAt5,
    recallAt10,
    mrr,
    ndcgAt10,
  };
}

/**
 * Calculate overall score using weighted metrics (per PLAN.md)
 *
 * Weights:
 * - Precision@5: 20%
 * - Recall@10: 15%
 * - MRR: 20%
 * - NDCG@10: 20%
 * - Semantic Accuracy: 15%
 * - Latency: 5%
 * - Throughput: 5%
 */
function calculateOverallScore(
  retrieval: RetrievalMetrics,
  similarity: SimilarityMetrics,
  latency: LatencyMetrics,
  throughput: number
): number {
  // Normalize retrieval metrics (already 0-1, convert to 0-100)
  const precisionScore = retrieval.precisionAt5 * 100;
  const recallScore = retrieval.recallAt10 * 100;
  const mrrScore = retrieval.mrr * 100;
  const ndcgScore = retrieval.ndcgAt10 * 100;
  const semanticScore = similarity.accuracy * 100;

  // Latency score: max(0, 100 - (p95_latency_ms / 10))
  // Lower latency = higher score
  const latencyScore = Math.max(0, 100 - (latency.p95 / 10));

  // Throughput score: min(100, throughput * 5)
  // Higher throughput = higher score
  const throughputScore = Math.min(100, throughput * 5);

  // Weighted average
  const overallScore =
    precisionScore * 0.20 +
    recallScore * 0.15 +
    mrrScore * 0.20 +
    ndcgScore * 0.20 +
    semanticScore * 0.15 +
    latencyScore * 0.05 +
    throughputScore * 0.05;

  return Math.round(overallScore * 100) / 100; // Round to 2 decimal places
}
