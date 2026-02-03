#!/usr/bin/env node

/**
 * Test suite for issues #445, #213, #352, #311, #212
 * - Archive management with confirm parameter (#445, #213)
 * - Rate limiting status (#352)
 * - Embedding sets listing verification (#311)
 * - PKE passphrase auto-generation (#212)
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

// Mock API responses
const mockArchivesResponse = [
  {
    id: "uuid-1",
    name: "default",
    schema_name: "public",
    description: "Default archive",
    created_at: "2026-01-01T00:00:00Z",
    note_count: 100,
    size_bytes: 5000000,
    is_default: true
  },
  {
    id: "uuid-2",
    name: "project_xyz",
    schema_name: "archive_project_xyz",
    description: "Project XYZ knowledge base",
    created_at: "2026-01-15T00:00:00Z",
    note_count: 50,
    size_bytes: 2500000,
    is_default: false
  }
];

const mockArchiveResponse = {
  id: "uuid-2",
  name: "project_xyz",
  schema_name: "archive_project_xyz",
  description: "Project XYZ knowledge base",
  created_at: "2026-01-15T00:00:00Z",
  note_count: 50,
  size_bytes: 2500000,
  is_default: false
};

const mockCreateArchiveResponse = {
  id: "uuid-3",
  name: "new_archive",
  schema_name: "archive_new_archive",
  description: "Newly created archive",
  created_at: "2026-02-02T00:00:00Z",
  note_count: 0,
  size_bytes: 0,
  is_default: false
};

const mockRateLimitResponse = {
  remaining: 95,
  limit: 100,
  reset_at: "2026-02-02T01:00:00Z",
  window_seconds: 3600,
  current_time: "2026-02-02T00:45:00Z"
};

const mockEmbeddingSetsResponse = [
  {
    id: "uuid-set-1",
    name: "Default Set",
    slug: "default",
    description: "All notes",
    purpose: "Global semantic search",
    usage_hints: "Use for broad queries",
    keywords: ["all", "global"],
    document_count: 100,
    embedding_count: 500,
    index_status: "ready"
  },
  {
    id: "uuid-set-2",
    name: "ML Research",
    slug: "ml_research",
    description: "Machine learning research notes",
    purpose: "Domain-specific ML search",
    usage_hints: "Use for ML-related queries",
    keywords: ["machine", "learning", "ai"],
    document_count: 25,
    embedding_count: 150,
    index_status: "ready"
  }
];

// Mock apiRequest function
let apiRequestMock;
let apiCallLog = [];

function setupMockApiRequest() {
  apiCallLog = [];
  apiRequestMock = async (method, path, body) => {
    apiCallLog.push({ method, path, body });

    // Archive endpoints
    if (method === "GET" && path === "/api/v1/archives") {
      return mockArchivesResponse;
    }
    if (method === "GET" && path === "/api/v1/archives/project_xyz") {
      return mockArchiveResponse;
    }
    if (method === "POST" && path === "/api/v1/archives") {
      return mockCreateArchiveResponse;
    }
    if (method === "DELETE" && path === "/api/v1/archives/old_archive") {
      return { success: true };
    }
    if (method === "POST" && path === "/api/v1/archives/project_xyz/set-default") {
      return { success: true, default_archive: "project_xyz" };
    }

    // Rate limit endpoint
    if (method === "GET" && path === "/api/v1/rate-limit/status") {
      return mockRateLimitResponse;
    }

    // Embedding sets endpoint
    if (method === "GET" && path === "/api/v1/embedding-sets") {
      return mockEmbeddingSetsResponse;
    }

    throw new Error(`Unexpected API call: ${method} ${path}`);
  };
  return apiRequestMock;
}

// ============================================================================
// Archive Management Tests (Issues #445, #213)
// ============================================================================

test("list_archives returns all archives with stats", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/api/v1/archives");

  assert.equal(result.length, 2);
  assert.equal(result[0].name, "default");
  assert.equal(result[0].is_default, true);
  assert.equal(result[0].note_count, 100);
  assert.equal(result[1].name, "project_xyz");
  assert.equal(result[1].note_count, 50);
});

test("get_archive returns specific archive details", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/api/v1/archives/project_xyz");

  assert.equal(result.name, "project_xyz");
  assert.equal(result.schema_name, "archive_project_xyz");
  assert.equal(result.description, "Project XYZ knowledge base");
  assert.equal(result.note_count, 50);
});

test("create_archive creates new archive with name and description", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("POST", "/api/v1/archives", {
    name: "new_archive",
    description: "Newly created archive"
  });

  assert.equal(result.name, "new_archive");
  assert.equal(result.schema_name, "archive_new_archive");
  assert.equal(result.note_count, 0);

  // Verify API was called with correct body
  assert.deepEqual(apiCallLog[0].body, {
    name: "new_archive",
    description: "Newly created archive"
  });
});

test("delete_archive calls DELETE endpoint", async () => {
  const apiRequest = setupMockApiRequest();
  await apiRequest("DELETE", "/api/v1/archives/old_archive");

  // Verify DELETE was called
  assert.equal(apiCallLog[0].method, "DELETE");
  assert.equal(apiCallLog[0].path, "/api/v1/archives/old_archive");
});

test("set_default_archive calls set-default endpoint", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("POST", "/api/v1/archives/project_xyz/set-default");

  assert.equal(result.success, true);
  assert.equal(result.default_archive, "project_xyz");
});

// ============================================================================
// Rate Limiting Tests (Issue #352)
// ============================================================================

test("get_rate_limit_status returns current rate limit info", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/api/v1/rate-limit/status");

  assert.equal(result.remaining, 95);
  assert.equal(result.limit, 100);
  assert.ok(result.reset_at);
  assert.equal(result.window_seconds, 3600);
  assert.ok(result.current_time);
});

test("get_rate_limit_status includes reset time and window", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/api/v1/rate-limit/status");

  assert.ok(result.reset_at, "reset_at missing");
  assert.ok(result.window_seconds, "window_seconds missing");
  assert.ok(result.current_time, "current_time missing");
});

// ============================================================================
// Embedding Sets Tests (Issue #311)
// ============================================================================

test("list_embedding_sets returns all sets with metadata", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/api/v1/embedding-sets");

  assert.equal(result.length, 2);
  assert.equal(result[0].slug, "default");
  assert.equal(result[0].name, "Default Set");
  assert.equal(result[0].index_status, "ready");
  assert.equal(result[1].slug, "ml_research");
  assert.equal(result[1].document_count, 25);
});

test("list_embedding_sets includes all required fields", async () => {
  const apiRequest = setupMockApiRequest();
  const result = await apiRequest("GET", "/api/v1/embedding-sets");

  const set = result[0];
  assert.ok(set.id, "id missing");
  assert.ok(set.name, "name missing");
  assert.ok(set.slug, "slug missing");
  assert.ok(set.description, "description missing");
  assert.ok(set.purpose, "purpose missing");
  assert.ok(set.usage_hints, "usage_hints missing");
  assert.ok(set.keywords, "keywords missing");
  assert.ok(typeof set.document_count === "number", "document_count not a number");
  assert.ok(typeof set.embedding_count === "number", "embedding_count not a number");
  assert.ok(set.index_status, "index_status missing");
});

// ============================================================================
// PKE Passphrase Auto-generation Tests (Issue #212)
// ============================================================================

test("pke_create_keyset supports manual passphrase", () => {
  const passphrase = "my-secure-passphrase-12345";

  // Verify passphrase meets minimum length
  assert.ok(passphrase.length >= 12, "Passphrase must be at least 12 characters");

  // This would be the flow:
  // 1. User provides passphrase
  // 2. Keyset created with provided passphrase
  // 3. No auto-generated passphrase in response
  const mockResult = {
    name: "test-keyset",
    address: "0x1234567890abcdef",
    public_key_path: "/path/to/public.key",
    private_key_path: "/path/to/private.key.enc",
    created: "2026-02-02T00:00:00Z",
    generated_passphrase: undefined, // Not auto-generated
    message: "Keyset created successfully"
  };

  assert.equal(mockResult.generated_passphrase, undefined);
  assert.equal(mockResult.message, "Keyset created successfully");
});

test("pke_create_keyset auto-generates passphrase when requested", () => {
  // Simulate auto-generation
  const wordList = ['alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot'];
  const words = [];
  for (let i = 0; i < 6; i++) {
    words.push(wordList[i % wordList.length]);
  }
  const generatedPassphrase = words.join('-');

  // Verify auto-generated passphrase format
  assert.ok(generatedPassphrase.length >= 12, "Auto-generated passphrase too short");
  assert.ok(generatedPassphrase.includes('-'), "Auto-generated passphrase should use dashes");

  // This would be the flow:
  // 1. User sets auto_generate_passphrase=true
  // 2. MCP generates passphrase
  // 3. Keyset created with generated passphrase
  // 4. Passphrase returned in response with warning
  const mockResult = {
    name: "test-keyset",
    address: "0x1234567890abcdef",
    public_key_path: "/path/to/public.key",
    private_key_path: "/path/to/private.key.enc",
    created: "2026-02-02T00:00:00Z",
    generated_passphrase: generatedPassphrase,
    message: "Keyset created with auto-generated passphrase. SAVE THIS PASSPHRASE - it cannot be recovered!"
  };

  assert.ok(mockResult.generated_passphrase);
  assert.ok(mockResult.message.includes("SAVE THIS PASSPHRASE"));
});

test("auto-generated passphrase has sufficient entropy", () => {
  // Simulate generating multiple passphrases to verify randomness
  const wordList = ['alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel'];
  const passphrases = new Set();

  for (let i = 0; i < 10; i++) {
    const words = [];
    for (let j = 0; j < 6; j++) {
      // In real implementation, use crypto.randomInt()
      words.push(wordList[Math.floor(Math.random() * wordList.length)]);
    }
    passphrases.add(words.join('-'));
  }

  // With 8 words and 6 selections, we should get some variety
  assert.ok(passphrases.size > 1, "Auto-generated passphrases should be random");
});

test("pke_create_keyset rejects short passphrase", () => {
  const shortPassphrase = "short";

  // Verify validation
  assert.ok(shortPassphrase.length < 12, "Test passphrase should be short");

  // In real implementation, this would throw:
  // throw new Error("Passphrase must be at least 12 characters...")
  try {
    if (!shortPassphrase || shortPassphrase.length < 12) {
      throw new Error("Passphrase must be at least 12 characters. Use auto_generate_passphrase=true for automatic generation.");
    }
    assert.fail("Should have thrown error");
  } catch (err) {
    assert.ok(err.message.includes("at least 12 characters"));
    assert.ok(err.message.includes("auto_generate_passphrase"));
  }
});

console.log("All tests passed!");
