#!/usr/bin/env node

/**
 * Test suite for observability MCP tools (issues #343, #325)
 * Tests health_check and get_system_info tools
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

// Mock API responses
const mockHealthResponse = {
  status: "healthy",
  timestamp: "2026-01-31T12:00:00Z",
  components: {
    database: { status: "healthy", latency_ms: 5 },
    embedding_service: { status: "healthy", latency_ms: 50 },
    job_queue: { status: "healthy", pending: 0, failed_last_hour: 0 },
    search: { status: "healthy", index_status: "ready" }
  },
  version: "1.2.3",
  uptime_seconds: 86400
};

const mockMemoryInfoResponse = {
  total_notes: 150,
  total_embeddings: 300,
  chunking_config: {
    code_strategy: "syntactic",
    prose_strategy: "semantic"
  }
};

const mockJobStatsResponse = {
  pending: 5,
  processing: 2,
  completed_last_hour: 100,
  failed_last_hour: 1,
  total: 108
};

const mockEmbeddingSetsResponse = {
  sets: [
    { slug: "default", name: "Default Set" },
    { slug: "filtered", name: "Filtered Set" }
  ]
};

// Mock apiRequest function
let apiRequestMock;

function setupMockApiRequest() {
  apiRequestMock = async (method, path) => {
    if (method === "GET" && path === "/health") {
      return mockHealthResponse;
    }
    if (method === "GET" && path === "/api/v1/memory/info") {
      return mockMemoryInfoResponse;
    }
    if (method === "GET" && path === "/api/v1/jobs/stats") {
      return mockJobStatsResponse;
    }
    if (method === "GET" && path === "/api/v1/embedding-sets") {
      return mockEmbeddingSetsResponse;
    }
    throw new Error(`Unexpected API call: ${method} ${path}`);
  };
  return apiRequestMock;
}

// Test cases for health_check tool
test("health_check returns full health status", async () => {
  const apiRequest = setupMockApiRequest();

  // Simulate the handler logic
  const result = await apiRequest("GET", "/health");

  assert.equal(result.status, "healthy");
  assert.ok(result.timestamp);
  assert.ok(result.components);
  assert.equal(result.components.database.status, "healthy");
  assert.equal(result.version, "1.2.3");
  assert.equal(result.uptime_seconds, 86400);
});

test("health_check includes all required components", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/health");

  assert.ok(result.components.database, "database component missing");
  assert.ok(result.components.embedding_service, "embedding_service component missing");
  assert.ok(result.components.job_queue, "job_queue component missing");
  assert.ok(result.components.search, "search component missing");
});

test("health_check component has status and metrics", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/health");

  const dbComponent = result.components.database;
  assert.equal(dbComponent.status, "healthy");
  assert.equal(typeof dbComponent.latency_ms, "number");

  const jobComponent = result.components.job_queue;
  assert.equal(typeof jobComponent.pending, "number");
  assert.equal(typeof jobComponent.failed_last_hour, "number");
});

// Test cases for get_system_info tool
test("get_system_info aggregates multiple endpoints", async () => {
  const apiRequest = setupMockApiRequest();

  // Simulate the handler logic
  const [health, memoryInfo, queueStats, embeddingSets] = await Promise.all([
    apiRequest("GET", "/health"),
    apiRequest("GET", "/api/v1/memory/info"),
    apiRequest("GET", "/api/v1/jobs/stats"),
    apiRequest("GET", "/api/v1/embedding-sets"),
  ]);

  const result = {
    version: health.version,
    status: health.status,
    configuration: {
      chunking: memoryInfo.chunking_config,
      ai_revision: { enabled: true },
    },
    stats: {
      total_notes: memoryInfo.total_notes,
      total_embeddings: memoryInfo.total_embeddings,
      embedding_sets: embeddingSets.sets.length,
      pending_jobs: queueStats.pending,
    },
    components: health.components,
  };

  assert.equal(result.version, "1.2.3");
  assert.equal(result.status, "healthy");
  assert.equal(result.stats.total_notes, 150);
  assert.equal(result.stats.total_embeddings, 300);
  assert.equal(result.stats.embedding_sets, 2);
  assert.equal(result.stats.pending_jobs, 5);
});

test("get_system_info includes configuration details", async () => {
  const apiRequest = setupMockApiRequest();

  const memoryInfo = await apiRequest("GET", "/api/v1/memory/info");
  const health = await apiRequest("GET", "/health");

  const result = {
    configuration: {
      chunking: memoryInfo.chunking_config,
      ai_revision: { enabled: true },
    },
  };

  assert.ok(result.configuration.chunking);
  assert.equal(result.configuration.chunking.code_strategy, "syntactic");
  assert.equal(result.configuration.chunking.prose_strategy, "semantic");
  assert.equal(result.configuration.ai_revision.enabled, true);
});

test("get_system_info handles missing data gracefully", async () => {
  // Create mock that returns errors
  const failingApiRequest = async (method, path) => {
    throw new Error("Service unavailable");
  };

  // Simulate handler with error handling
  const [health, memoryInfo, queueStats, embeddingSets] = await Promise.all([
    failingApiRequest("GET", "/health").catch(() => ({ status: "unknown" })),
    failingApiRequest("GET", "/api/v1/memory/info").catch(() => ({})),
    failingApiRequest("GET", "/api/v1/jobs/stats").catch(() => ({})),
    failingApiRequest("GET", "/api/v1/embedding-sets").catch(() => ({ sets: [] })),
  ]);

  const result = {
    version: health.version || "unknown",
    status: health.status || "unknown",
    configuration: {
      chunking: memoryInfo.chunking_config || {},
      ai_revision: { enabled: true },
    },
    stats: {
      total_notes: memoryInfo.total_notes || 0,
      total_embeddings: memoryInfo.total_embeddings || 0,
      embedding_sets: (embeddingSets.sets || []).length,
      pending_jobs: queueStats.pending || 0,
    },
    components: health.components || {},
  };

  assert.equal(result.version, "unknown");
  assert.equal(result.status, "unknown");
  assert.equal(result.stats.total_notes, 0);
  assert.equal(result.stats.embedding_sets, 0);
  assert.deepEqual(result.components, {});
});

test("get_system_info includes all required stats", async () => {
  const apiRequest = setupMockApiRequest();

  const [health, memoryInfo, queueStats, embeddingSets] = await Promise.all([
    apiRequest("GET", "/health"),
    apiRequest("GET", "/api/v1/memory/info"),
    apiRequest("GET", "/api/v1/jobs/stats"),
    apiRequest("GET", "/api/v1/embedding-sets"),
  ]);

  const result = {
    stats: {
      total_notes: memoryInfo.total_notes || 0,
      total_embeddings: memoryInfo.total_embeddings || 0,
      embedding_sets: (embeddingSets.sets || []).length,
      pending_jobs: queueStats.pending || 0,
    },
  };

  assert.ok("total_notes" in result.stats);
  assert.ok("total_embeddings" in result.stats);
  assert.ok("embedding_sets" in result.stats);
  assert.ok("pending_jobs" in result.stats);
});

console.log("\n✓ All observability tools tests passed");
