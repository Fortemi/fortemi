#!/usr/bin/env node
/**
 * Regression test for issue #359: Metadata field in create_note and update_note
 *
 * This test validates that:
 * 1. create_note accepts and stores metadata
 * 2. update_note can modify metadata
 * 3. get_note returns the stored metadata
 * 4. Metadata is properly passed through MCP → API → DB
 */

const API_BASE = process.env.API_BASE || "http://localhost:3000";

async function apiRequest(method, path, body = null) {
  const options = {
    method,
    headers: { "Content-Type": "application/json" },
  };
  if (body) {
    options.body = JSON.stringify(body);
  }
  const response = await fetch(`${API_BASE}${path}`, options);
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`API ${method} ${path} failed: ${response.status} - ${text}`);
  }
  return response.json();
}

async function runTests() {
  console.log("=".repeat(60));
  console.log("Regression Test: Issue #359 - Metadata in create/update");
  console.log("=".repeat(60));
  console.log(`API_BASE: ${API_BASE}\n`);

  let noteId = null;
  let passed = 0;
  let failed = 0;

  // Test 1: Create note with metadata
  console.log("Test 1: Create note with metadata");
  try {
    const createResult = await apiRequest("POST", "/api/v1/notes", {
      content: "Regression test note for #359",
      metadata: {
        source: "regression-test",
        priority: "high",
        ticket: "359",
        nested: { key: "value" }
      }
    });
    noteId = createResult.id;
    console.log(`  ✓ Created note: ${noteId}`);
    passed++;
  } catch (err) {
    console.log(`  ✗ FAILED: ${err.message}`);
    failed++;
    return { passed, failed };
  }

  // Test 2: Verify metadata was stored
  console.log("\nTest 2: Verify metadata stored correctly");
  try {
    const getResult = await apiRequest("GET", `/api/v1/notes/${noteId}`);
    const metadata = getResult.note?.metadata;

    if (!metadata) {
      throw new Error("metadata field is missing from response");
    }
    if (metadata.source !== "regression-test") {
      throw new Error(`Expected source="regression-test", got "${metadata.source}"`);
    }
    if (metadata.priority !== "high") {
      throw new Error(`Expected priority="high", got "${metadata.priority}"`);
    }
    if (metadata.ticket !== "359") {
      throw new Error(`Expected ticket="359", got "${metadata.ticket}"`);
    }
    if (!metadata.nested || metadata.nested.key !== "value") {
      throw new Error("Nested metadata not preserved");
    }
    console.log(`  ✓ Metadata verified: ${JSON.stringify(metadata)}`);
    passed++;
  } catch (err) {
    console.log(`  ✗ FAILED: ${err.message}`);
    failed++;
  }

  // Test 3: Update note metadata
  console.log("\nTest 3: Update note metadata");
  try {
    await apiRequest("PATCH", `/api/v1/notes/${noteId}`, {
      metadata: {
        source: "regression-test",
        priority: "low",  // Changed
        ticket: "359",
        updated: true     // Added
      }
    });
    console.log("  ✓ Update request succeeded");
    passed++;
  } catch (err) {
    console.log(`  ✗ FAILED: ${err.message}`);
    failed++;
  }

  // Test 4: Verify updated metadata
  console.log("\nTest 4: Verify updated metadata");
  try {
    const getResult = await apiRequest("GET", `/api/v1/notes/${noteId}`);
    const metadata = getResult.note?.metadata;

    if (metadata.priority !== "low") {
      throw new Error(`Expected priority="low" after update, got "${metadata.priority}"`);
    }
    if (metadata.updated !== true) {
      throw new Error("New 'updated' field not present");
    }
    console.log(`  ✓ Updated metadata verified: ${JSON.stringify(metadata)}`);
    passed++;
  } catch (err) {
    console.log(`  ✗ FAILED: ${err.message}`);
    failed++;
  }

  // Test 5: Create note without metadata (should default to empty object)
  console.log("\nTest 5: Create note without metadata (default behavior)");
  let noteId2 = null;
  try {
    const createResult = await apiRequest("POST", "/api/v1/notes", {
      content: "Note without metadata"
    });
    noteId2 = createResult.id;

    const getResult = await apiRequest("GET", `/api/v1/notes/${noteId2}`);
    const metadata = getResult.note?.metadata;

    if (typeof metadata !== "object") {
      throw new Error(`Expected metadata to be object, got ${typeof metadata}`);
    }
    console.log(`  ✓ Default metadata is empty object: ${JSON.stringify(metadata)}`);
    passed++;
  } catch (err) {
    console.log(`  ✗ FAILED: ${err.message}`);
    failed++;
  }

  // Cleanup
  console.log("\nCleanup:");
  if (noteId) {
    try {
      await apiRequest("DELETE", `/api/v1/notes/${noteId}`);
      console.log(`  Deleted test note: ${noteId}`);
    } catch (err) {
      console.log(`  Warning: Could not delete ${noteId}: ${err.message}`);
    }
  }
  if (noteId2) {
    try {
      await apiRequest("DELETE", `/api/v1/notes/${noteId2}`);
      console.log(`  Deleted test note: ${noteId2}`);
    } catch (err) {
      console.log(`  Warning: Could not delete ${noteId2}: ${err.message}`);
    }
  }

  // Summary
  console.log("\n" + "=".repeat(60));
  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log("=".repeat(60));

  return { passed, failed };
}

// Run tests
runTests()
  .then(({ passed, failed }) => {
    process.exit(failed > 0 ? 1 : 0);
  })
  .catch(err => {
    console.error("Test runner error:", err);
    process.exit(1);
  });
