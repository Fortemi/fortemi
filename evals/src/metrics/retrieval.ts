/**
 * Retrieval metrics: Precision@K, Recall@K, MRR, NDCG
 */

/**
 * Calculate Precision@K
 * Precision@K = (# of relevant items in top-K) / K
 */
export function calculatePrecisionAtK(
  rankedResults: string[],
  relevantDocs: Set<string>,
  k: number
): number {
  if (k <= 0 || rankedResults.length === 0) {
    return 0;
  }

  const topK = rankedResults.slice(0, k);
  const relevantInTopK = topK.filter((doc) => relevantDocs.has(doc)).length;

  return relevantInTopK / topK.length;
}

/**
 * Calculate Recall@K
 * Recall@K = (# of relevant items in top-K) / (total # of relevant items)
 */
export function calculateRecallAtK(
  rankedResults: string[],
  relevantDocs: Set<string>,
  k: number
): number {
  if (relevantDocs.size === 0) {
    return 0;
  }

  const topK = rankedResults.slice(0, k);
  const relevantInTopK = topK.filter((doc) => relevantDocs.has(doc)).length;

  return relevantInTopK / relevantDocs.size;
}

/**
 * Calculate Mean Reciprocal Rank (MRR)
 * MRR = average of (1 / rank of first relevant result) across queries
 */
export function calculateMRR(
  queries: Array<{
    ranked: string[];
    relevant: Set<string>;
  }>
): number {
  if (queries.length === 0) {
    return 0;
  }

  let sumReciprocalRank = 0;

  for (const query of queries) {
    const firstRelevantRank = query.ranked.findIndex((doc) =>
      query.relevant.has(doc)
    );

    if (firstRelevantRank !== -1) {
      // Ranks are 1-indexed, so add 1
      sumReciprocalRank += 1 / (firstRelevantRank + 1);
    }
    // If no relevant result found, contributes 0
  }

  return sumReciprocalRank / queries.length;
}

/**
 * Calculate Normalized Discounted Cumulative Gain (NDCG@K)
 * NDCG = DCG / IDCG where:
 * - DCG = sum of (relevance / log2(position + 1)) for actual ranking
 * - IDCG = DCG for ideal ranking (sorted by relevance)
 */
export function calculateNDCG(
  rankedResults: string[],
  relevanceScores: Map<string, number>,
  k: number
): number {
  if (k <= 0 || rankedResults.length === 0) {
    return 0;
  }

  // Calculate DCG for actual ranking
  const dcg = calculateDCG(rankedResults, relevanceScores, k);

  // Calculate IDCG (ideal DCG)
  const idealRanking = Array.from(relevanceScores.entries())
    .sort((a, b) => b[1] - a[1]) // Sort by relevance score descending
    .map(([docId]) => docId);

  const idcg = calculateDCG(idealRanking, relevanceScores, k);

  if (idcg === 0) {
    return 0;
  }

  return dcg / idcg;
}

/**
 * Calculate Discounted Cumulative Gain (DCG@K)
 * DCG = sum of (relevance / log2(position + 1)) for positions 1 to K
 */
function calculateDCG(
  rankedResults: string[],
  relevanceScores: Map<string, number>,
  k: number
): number {
  const topK = rankedResults.slice(0, k);

  let dcg = 0;
  for (let i = 0; i < topK.length; i++) {
    const docId = topK[i];
    const relevance = relevanceScores.get(docId) ?? 0;

    // Position is 1-indexed, so i+1
    // Discount factor is log2(position + 1)
    dcg += relevance / Math.log2(i + 2);
  }

  return dcg;
}
