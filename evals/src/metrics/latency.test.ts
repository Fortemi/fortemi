/**
 * Tests for latency metrics
 */

import { describe, it, expect } from '@jest/globals';
import { calculatePercentiles, LatencyTracker } from './latency.js';

describe('Latency Metrics', () => {
  describe('calculatePercentiles', () => {
    it('should calculate percentiles correctly for sorted data', () => {
      const values = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
      const percentiles = calculatePercentiles(values);

      expect(percentiles.p50).toBe(5.5);
      expect(percentiles.p95).toBeCloseTo(9.5, 1);
      expect(percentiles.p99).toBeCloseTo(9.9, 1);
      expect(percentiles.mean).toBe(5.5);
      expect(percentiles.min).toBe(1);
      expect(percentiles.max).toBe(10);
    });

    it('should handle unsorted data', () => {
      const values = [10, 1, 5, 3, 8, 2, 9, 4, 7, 6];
      const percentiles = calculatePercentiles(values);

      expect(percentiles.p50).toBe(5.5);
      expect(percentiles.min).toBe(1);
      expect(percentiles.max).toBe(10);
    });

    it('should handle single value', () => {
      const values = [42];
      const percentiles = calculatePercentiles(values);

      expect(percentiles.p50).toBe(42);
      expect(percentiles.p95).toBe(42);
      expect(percentiles.p99).toBe(42);
      expect(percentiles.mean).toBe(42);
      expect(percentiles.min).toBe(42);
      expect(percentiles.max).toBe(42);
    });

    it('should handle two values', () => {
      const values = [10, 20];
      const percentiles = calculatePercentiles(values);

      expect(percentiles.p50).toBe(15);
      expect(percentiles.mean).toBe(15);
      expect(percentiles.min).toBe(10);
      expect(percentiles.max).toBe(20);
    });

    it('should throw error for empty array', () => {
      expect(() => calculatePercentiles([])).toThrow();
    });
  });

  describe('LatencyTracker', () => {
    it('should track timing correctly', () => {
      const tracker = new LatencyTracker();

      const timer1 = tracker.start();
      timer1.end();
      tracker.recordFromTimer(timer1);

      const timer2 = tracker.start();
      timer2.end();
      tracker.recordFromTimer(timer2);

      const metrics = tracker.getMetrics();

      expect(metrics.p50).toBeGreaterThanOrEqual(0);
      expect(metrics.mean).toBeGreaterThanOrEqual(0);
      expect(metrics.min).toBeGreaterThanOrEqual(0);
      expect(metrics.max).toBeGreaterThanOrEqual(0);
    });

    it('should handle multiple measurements', () => {
      const tracker = new LatencyTracker();

      // Simulate different latencies
      for (let i = 0; i < 10; i++) {
        const timer = tracker.start();
        // Simulate some work
        const start = Date.now();
        while (Date.now() - start < 1) {
          // Wait ~1ms
        }
        timer.end();
        tracker.recordFromTimer(timer);
      }

      const metrics = tracker.getMetrics();

      expect(metrics.p50).toBeGreaterThan(0);
      expect(metrics.p95).toBeGreaterThan(0);
      expect(metrics.p99).toBeGreaterThan(0);
    });

    it('should throw error when getting metrics with no measurements', () => {
      const tracker = new LatencyTracker();
      expect(() => tracker.getMetrics()).toThrow();
    });

    it('should reset measurements correctly', () => {
      const tracker = new LatencyTracker();

      const timer = tracker.start();
      timer.end();
      tracker.recordFromTimer(timer);

      tracker.reset();

      expect(() => tracker.getMetrics()).toThrow();
    });

    it('should track time to first token separately', () => {
      const tracker = new LatencyTracker();

      const timer = tracker.start();
      timer.markFirstToken();
      timer.end();
      tracker.recordFromTimer(timer);

      const metrics = tracker.getMetrics();

      expect(metrics.p50).toBeGreaterThanOrEqual(0);
    });

    it('should use record method for direct duration recording', () => {
      const tracker = new LatencyTracker();

      tracker.record(100);
      tracker.record(200);
      tracker.record(300);

      const metrics = tracker.getMetrics();

      expect(metrics.p50).toBe(200);
      expect(metrics.mean).toBe(200);
      expect(metrics.min).toBe(100);
      expect(metrics.max).toBe(300);
    });
  });
});
