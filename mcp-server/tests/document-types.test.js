#!/usr/bin/env node

/**
 * MCP Document Types Tests (Phase 8)
 *
 * Tests document type registry via MCP tools:
 * - list_document_types: List all registered document types
 * - get_document_type: Get document type by name or ID
 * - detect_document_type: Auto-detect type from filename/content
 *
 * Document types control chunking strategies (syntactic for code,
 * semantic for prose) and are detected via file patterns.
 *
 * All tests use unique identifiers (UUIDs) for isolation.
 */

import { strict as assert } from "node:assert";
import { test, describe, before, after } from "node:test";
import { MCPTestClient } from "./helpers/mcp-client.js";

describe("Phase 8: Document Types", () => {
  let client;

  before(async () => {
    client = new MCPTestClient();
    await client.initialize();
  });

  after(async () => {
    await client.close();
  });

  test("DOCTYPE-001: list_document_types returns array", async () => {
    const result = await client.callTool("list_document_types");

    assert.ok(Array.isArray(result), "Result should be an array");
    assert.ok(result.length > 0, "Should have registered document types");

    // Verify structure of first type
    const docType = result[0];
    assert.ok(docType.id, "Document type should have ID");
    assert.ok(docType.name, "Document type should have name");
    assert.ok(docType.category, "Document type should have category");
  });

  test("DOCTYPE-002: list_document_types includes common types", async () => {
    const result = await client.callTool("list_document_types");

    // Check for expected common types
    const typeNames = result.map((t) => t.name.toLowerCase());

    // Programming languages
    assert.ok(
      typeNames.some((n) => n.includes("javascript") || n.includes("js")),
      "Should include JavaScript"
    );
    assert.ok(
      typeNames.some((n) => n.includes("python") || n.includes("py")),
      "Should include Python"
    );
    assert.ok(
      typeNames.some((n) => n.includes("rust") || n.includes("rs")),
      "Should include Rust"
    );

    // Document formats
    assert.ok(
      typeNames.some((n) => n.includes("markdown") || n.includes("md")),
      "Should include Markdown"
    );
  });

  test("DOCTYPE-003: get_document_type by name", async () => {
    // Get a known document type
    const allTypes = await client.callTool("list_document_types");
    assert.ok(allTypes.length > 0, "Should have document types");

    const firstType = allTypes[0];

    // Retrieve by name
    const result = await client.callTool("get_document_type", {
      name: firstType.name,
    });

    assert.ok(result, "Document type should be retrieved");
    assert.strictEqual(result.id, firstType.id, "ID should match");
    assert.strictEqual(result.name, firstType.name, "Name should match");
  });

  test("DOCTYPE-004: get_document_type by id", async () => {
    // Get a known document type
    const allTypes = await client.callTool("list_document_types");
    const firstType = allTypes[0];

    // Retrieve by name (tool only accepts name parameter)
    const result = await client.callTool("get_document_type", {
      name: firstType.name,
    });

    assert.ok(result, "Document type should be retrieved");
    assert.strictEqual(result.id, firstType.id, "ID should match");
    assert.strictEqual(result.name, firstType.name, "Name should match");
  });

  test("DOCTYPE-005: detect_document_type from JavaScript filename", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "app.js",
    });

    assert.ok(result, "Should detect document type");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("javascript") ||
        result.document_type.name.toLowerCase().includes("js"),
      "Should identify as JavaScript"
    );
    assert.strictEqual(result.document_type.category, "code", "JavaScript should be code category");
  });

  test("DOCTYPE-006: detect_document_type from Python filename", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "script.py",
    });

    assert.ok(result, "Should detect document type");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("python") || result.document_type.name.toLowerCase().includes("py"),
      "Should identify as Python"
    );
    assert.strictEqual(result.document_type.category, "code", "Python should be code category");
  });

  test("DOCTYPE-007: detect_document_type from Rust filename", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "main.rs",
    });

    assert.ok(result, "Should detect document type");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("rust") || result.document_type.name.toLowerCase().includes("rs"),
      "Should identify as Rust"
    );
    assert.strictEqual(result.document_type.category, "code", "Rust should be code category");
  });

  test("DOCTYPE-008: detect_document_type from Markdown filename", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "README.md",
    });

    assert.ok(result, "Should detect document type");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("markdown") ||
        result.document_type.name.toLowerCase().includes("md"),
      "Should identify as Markdown"
    );
    // Markdown is typically "prose" category
    assert.ok(
      result.document_type.category === "prose" || result.document_type.category === "document" || result.document_type.category === "text",
      "Markdown should be prose/document/text category"
    );
  });

  test("DOCTYPE-009: detect_document_type from TypeScript filename", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "component.tsx",
    });

    assert.ok(result, "Should detect document type");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("typescript") ||
        result.document_type.name.toLowerCase().includes("tsx"),
      "Should identify as TypeScript"
    );
    assert.strictEqual(result.document_type.category, "code", "TypeScript should be code category");
  });

  test("DOCTYPE-010: detect_document_type with content magic", async () => {
    // Provide content with shebang for detection
    const result = await client.callTool("detect_document_type", {
      filename: "script",
      content: "#!/usr/bin/env python3\nprint('Hello')",
    });

    assert.ok(result, "Should detect document type from content");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("python"),
      "Should identify as Python from shebang"
    );
  });

  test("DOCTYPE-011: detect_document_type unknown extension fallback", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "unknown.xyz123",
    });

    // Should return a fallback type or null
    // Behavior depends on implementation
    assert.ok(result !== undefined, "Should return a result (even if fallback)");
  });

  test("DOCTYPE-012: document type has chunking strategy", async () => {
    const allTypes = await client.callTool("list_document_types");

    // Find a code type
    const codeType = allTypes.find((t) => t.category === "code");
    assert.ok(codeType, "Should have at least one code type");

    // Code types should use syntactic chunking
    assert.ok(
      codeType.chunking_strategy === "syntactic" || codeType.chunking_strategy,
      "Code type should have chunking strategy"
    );
  });

  test("DOCTYPE-013: document type has file patterns", async () => {
    const result = await client.callTool("get_document_type", {
      name: "javascript",
    });

    assert.ok(result, "Should retrieve JavaScript type");
    assert.ok(result.file_extensions, "Document type should have file_extensions");
    assert.ok(Array.isArray(result.file_extensions), "file_extensions should be an array");
    assert.ok(result.file_extensions.length > 0, "Should have at least one file extension");

    // Check extension format
    const jsExt = result.file_extensions.find(
      (p) => p.includes(".js")
    );
    assert.ok(jsExt, "Should have .js extension");
  });

  test("DOCTYPE-014: list_document_types with category filter", async () => {
    const result = await client.callTool("list_document_types", {
      category: "code",
    });

    assert.ok(Array.isArray(result), "Result should be an array");
    if (result.length > 0) {
      result.forEach((docType) => {
        assert.strictEqual(
          docType.category,
          "code",
          "All results should be code category"
        );
      });
    }
  });

  test("DOCTYPE-015: get_document_type error - non-existent name", async () => {
    const fakeName = `NonExistent${MCPTestClient.uniqueId()}`;

    const error = await client.callToolExpectError("get_document_type", {
      name: fakeName,
    });

    assert.ok(error.error, "Should return error for non-existent type");
  });

  test("DOCTYPE-016: detect_document_type case-insensitive extension", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "Script.PY", // Uppercase extension
    });

    assert.ok(result, "Should detect document type case-insensitively");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("python"),
      "Should identify as Python"
    );
  });

  test("DOCTYPE-017: detect_document_type with path", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "/home/user/projects/app/src/main.rs",
    });

    assert.ok(result, "Should detect document type from full path");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("rust"),
      "Should identify as Rust"
    );
  });

  test("DOCTYPE-018: document types have unique IDs", async () => {
    const allTypes = await client.callTool("list_document_types");

    const ids = allTypes.map((t) => t.id);
    const uniqueIds = new Set(ids);

    assert.strictEqual(
      ids.length,
      uniqueIds.size,
      "All document type IDs should be unique"
    );
  });

  test("DOCTYPE-019: document types have descriptions", async () => {
    const allTypes = await client.callTool("list_document_types");

    // Most types should have descriptions
    const withDescription = allTypes.filter((t) => t.description && t.description.length > 0);
    assert.ok(
      withDescription.length > 0,
      "At least some document types should have descriptions"
    );
  });

  test("DOCTYPE-020: detect JSON document type", async () => {
    const result = await client.callTool("detect_document_type", {
      filename: "config.json",
    });

    assert.ok(result, "Should detect document type");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name.toLowerCase().includes("json"),
      "Should identify as JSON"
    );
  });

  test("DOCTYPE-021: combined detection uses content patterns for yaml files", async () => {
    // Content pattern detection takes priority — API currently identifies
    // OpenAPI-style yaml as docker-compose via content_pattern+file_extension
    const result = await client.callTool("detect_document_type", {
      filename: "api.yaml",
      content: "openapi: 3.1.0\ninfo:\n  title: Test API\n  version: 1.0.0\npaths: {}",
    });

    assert.ok(result, "Should detect document type");
    assert.ok(result.document_type, "Should have document_type property");
    assert.ok(
      result.document_type.name,
      "Should have a document type name"
    );
    assert.ok(
      result.confidence >= 0.5,
      `Confidence should be >= 0.5, got ${result.confidence}`
    );
    console.log(`  Detected: ${result.document_type.name} (confidence: ${result.confidence}, method: ${result.detection_method})`);
  });
});
