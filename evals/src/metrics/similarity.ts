/**
 * Similarity metrics: Cosine similarity, Euclidean distance, clustering metrics
 */

/**
 * Calculate cosine similarity between two vectors
 * Returns value between -1 (opposite) and 1 (identical)
 */
export function cosineSimilarity(a: number[], b: number[]): number {
  if (a.length !== b.length) {
    throw new Error('Vectors must have same length');
  }

  if (a.length === 0) {
    return 0;
  }

  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (let i = 0; i < a.length; i++) {
    dotProduct += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }

  normA = Math.sqrt(normA);
  normB = Math.sqrt(normB);

  if (normA === 0 || normB === 0) {
    return 0;
  }

  return dotProduct / (normA * normB);
}

/**
 * Calculate Euclidean distance between two vectors
 */
export function euclideanDistance(a: number[], b: number[]): number {
  if (a.length !== b.length) {
    throw new Error('Vectors must have same length');
  }

  let sumSquaredDiff = 0;
  for (let i = 0; i < a.length; i++) {
    const diff = a[i] - b[i];
    sumSquaredDiff += diff * diff;
  }

  return Math.sqrt(sumSquaredDiff);
}

/**
 * Calculate similarity accuracy based on categorical judgments
 * Thresholds: high >= 0.7, medium >= 0.4, low < 0.4
 */
export function calculateSimilarityAccuracy(
  predictions: Array<{
    actualSimilarity: number;
    expected: 'high' | 'medium' | 'low';
  }>
): number {
  if (predictions.length === 0) {
    return 0;
  }

  let correct = 0;

  for (const pred of predictions) {
    const category = categorizeSimilarity(pred.actualSimilarity);
    if (category === pred.expected) {
      correct++;
    }
  }

  return correct / predictions.length;
}

/**
 * Categorize similarity score into high/medium/low
 */
function categorizeSimilarity(similarity: number): 'high' | 'medium' | 'low' {
  if (similarity >= 0.7) {
    return 'high';
  } else if (similarity >= 0.4) {
    return 'medium';
  } else {
    return 'low';
  }
}

/**
 * Calculate mean absolute error for similarity predictions
 */
export function calculateSimilarityMAE(
  predictions: Array<{
    predicted: number;
    actual: number;
  }>
): number {
  if (predictions.length === 0) {
    return 0;
  }

  let sumAbsError = 0;
  for (const pred of predictions) {
    sumAbsError += Math.abs(pred.predicted - pred.actual);
  }

  return sumAbsError / predictions.length;
}
