/**
 * Latency metrics: Timing utilities with percentiles (p50, p95, p99)
 */

import type { LatencyMetrics } from '../models/types.js';

/**
 * Calculate percentiles from an array of values
 */
export function calculatePercentiles(values: number[]): LatencyMetrics {
  if (values.length === 0) {
    throw new Error('Cannot calculate percentiles for empty array');
  }

  // Sort values ascending
  const sorted = [...values].sort((a, b) => a - b);

  return {
    p50: percentile(sorted, 0.5),
    p95: percentile(sorted, 0.95),
    p99: percentile(sorted, 0.99),
    mean: mean(sorted),
    min: sorted[0],
    max: sorted[sorted.length - 1],
  };
}

/**
 * Calculate a specific percentile from sorted array
 */
function percentile(sortedValues: number[], p: number): number {
  if (sortedValues.length === 1) {
    return sortedValues[0];
  }

  const index = p * (sortedValues.length - 1);
  const lower = Math.floor(index);
  const upper = Math.ceil(index);
  const weight = index - lower;

  return sortedValues[lower] * (1 - weight) + sortedValues[upper] * weight;
}

/**
 * Calculate mean of an array
 */
function mean(values: number[]): number {
  const sum = values.reduce((acc, val) => acc + val, 0);
  return sum / values.length;
}

/**
 * Timer for tracking individual measurements
 */
export class Timer {
  private startTime: number;
  private endTime?: number;
  private firstTokenTime?: number;

  constructor() {
    this.startTime = Date.now();
  }

  getStartTime(): number {
    return this.startTime;
  }

  markFirstToken(): void {
    if (!this.firstTokenTime) {
      this.firstTokenTime = Date.now();
    }
  }

  end(): number {
    if (!this.endTime) {
      this.endTime = Date.now();
    }
    return this.endTime - this.startTime;
  }

  getDuration(): number {
    if (!this.endTime) {
      throw new Error('Timer not ended');
    }
    return this.endTime - this.startTime;
  }

  getTimeToFirstToken(): number | undefined {
    if (!this.firstTokenTime) {
      return undefined;
    }
    return this.firstTokenTime - this.startTime;
  }
}

/**
 * Latency tracker for collecting and analyzing timing measurements
 */
export class LatencyTracker {
  private measurements: number[] = [];
  private ttftMeasurements: number[] = [];

  start(): Timer {
    return new Timer();
  }

  record(duration: number): void {
    this.measurements.push(duration);
  }

  recordFromTimer(timer: Timer): void {
    const duration = timer.getDuration();
    this.measurements.push(duration);

    const ttft = timer.getTimeToFirstToken();
    if (ttft !== undefined) {
      this.ttftMeasurements.push(ttft);
    }
  }

  getMetrics(): LatencyMetrics {
    return calculatePercentiles(this.measurements);
  }

  getTTFTMetrics(): LatencyMetrics | undefined {
    if (this.ttftMeasurements.length === 0) {
      return undefined;
    }
    return calculatePercentiles(this.ttftMeasurements);
  }

  getMeasurements(): number[] {
    return [...this.measurements];
  }

  reset(): void {
    this.measurements = [];
    this.ttftMeasurements = [];
  }
}
