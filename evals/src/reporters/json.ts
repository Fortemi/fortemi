/**
 * JSON reporter for evaluation results
 * Generates raw JSON output with all metrics and metadata
 */

import type { EvaluationReport } from '../models/types.js';

/**
 * Generate a JSON report from evaluation results
 *
 * @param results - The complete evaluation report
 * @returns Pretty-printed JSON string with 2-space indentation
 */
export function generateJSONReport(results: EvaluationReport): string {
  return JSON.stringify(results, null, 2);
}
