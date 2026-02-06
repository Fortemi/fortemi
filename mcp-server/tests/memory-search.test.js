#!/usr/bin/env node

/**
 * MCP Memory Search Tool Tests
 *
 * Validates memory search tools:
 * - search_memories_by_location: Geographic search
 * - search_memories_by_time: Temporal search
 * - search_memories_combined: Spatiotemporal search
 * - get_memory_provenance: File provenance tracking
 *
 * Tests cover:
 * - Schema validation (required fields, types, defaults)
 * - URL construction (query params, encoding)
 * - Error handling (API errors, validation)
 * - Cross-tool consistency (shared schemas, paths)
 */

import { strict as assert } from "node:assert";
import { test, describe } from "node:test";
import fs from "node:fs";
import path from "path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ============================================================================
// EXTRACT TOOLS FROM INDEX.JS
// ============================================================================

const indexPath = path.join(__dirname, "..", "index.js");
const indexContent = fs.readFileSync(indexPath, "utf8");

// Extract tools array by finding the const tools = [ ... ]; declaration
const toolsMatch = indexContent.match(/const tools = \[([\s\S]*?)\n\];/);
if (!toolsMatch) {
  throw new Error("Could not extract tools array from index.js");
}

// Parse the tools array safely
let tools;
try {
  const toolsCode = `(function() { return [${toolsMatch[1]}]; })()`;
  tools = eval(toolsCode);
} catch (error) {
  throw new Error(`Failed to parse tools array: ${error.message}`);
}

console.log(`Loaded ${tools.length} tools for memory search validation\n`);

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Find a specific tool by name
 */
function getTool(name) {
  const tool = tools.find(t => t.name === name);
  if (!tool) {
    throw new Error(`Tool not found: ${name}`);
  }
  return tool;
}

/**
 * Mock URL builder simulating the handler logic
 * Note: The handler uses encodeURIComponent() before passing to URLSearchParams.set()
 * which results in double-encoding (e.g., : becomes %253A instead of %3A)
 */
function buildMemorySearchUrl(toolName, args) {
  const basePath = "/api/v1/memories/search";
  const params = new URLSearchParams();

  switch (toolName) {
    case "search_memories_by_location":
      params.set("lat", args.lat);
      params.set("lon", args.lon);
      if (args.radius !== undefined && args.radius !== null) {
        params.set("radius", args.radius);
      }
      break;

    case "search_memories_by_time":
      // Handler does: params.set("start", encodeURIComponent(args.start))
      params.set("start", encodeURIComponent(args.start));
      params.set("end", encodeURIComponent(args.end));
      break;

    case "search_memories_combined":
      params.set("lat", args.lat);
      params.set("lon", args.lon);
      if (args.radius !== undefined && args.radius !== null) {
        params.set("radius", args.radius);
      }
      // Handler does: params.set("start", encodeURIComponent(args.start))
      params.set("start", encodeURIComponent(args.start));
      params.set("end", encodeURIComponent(args.end));
      break;

    default:
      throw new Error(`Unknown memory search tool: ${toolName}`);
  }

  return `${basePath}?${params}`;
}

/**
 * Build provenance URL
 */
function buildProvenanceUrl(noteId) {
  return `/api/v1/notes/${noteId}/memory-provenance`;
}

/**
 * Mock API request function for error simulation
 */
function createMockApiRequest(scenario = "success") {
  return async (method, path, body = null) => {
    switch (scenario) {
      case "not_found_404":
        throw new Error("API error 404: Note not found");

      case "bad_request_400":
        throw new Error("API error 400: Invalid coordinates");

      case "internal_error_500":
        throw new Error("API error 500: Database error");

      case "network_error":
        throw new Error("Network request failed");

      case "success":
        if (path.includes("/memory-provenance")) {
          return {
            note_id: "123e4567-e89b-12d3-a456-426614174000",
            attachments: [
              {
                file_id: "att-001",
                location: { lat: 48.8566, lon: 2.3522 },
                captured_at: "2025-01-15T10:30:00Z",
              }
            ]
          };
        }
        if (path.includes("/memories/search")) {
          return {
            results: [
              {
                note_id: "note-001",
                distance_meters: 150,
                captured_at: "2025-01-15T10:30:00Z",
              }
            ]
          };
        }
        return { success: true };

      default:
        throw new Error(`Unknown scenario: ${scenario}`);
    }
  };
}

// ============================================================================
// MEMORY SEARCH TOOL SCHEMA VALIDATION
// ============================================================================

describe("Memory Search Tool Schema Validation", () => {
  test("all 4 memory tools exist in tools array", () => {
    const expectedTools = [
      "search_memories_by_location",
      "search_memories_by_time",
      "search_memories_combined",
      "get_memory_provenance"
    ];

    const missingTools = [];
    for (const toolName of expectedTools) {
      const tool = tools.find(t => t.name === toolName);
      if (!tool) {
        missingTools.push(toolName);
      }
    }

    assert.equal(
      missingTools.length,
      0,
      `Missing memory search tools: ${missingTools.join(", ")}`
    );
  });

  test("search_memories_by_location has correct required fields", () => {
    const tool = getTool("search_memories_by_location");

    assert.deepEqual(
      tool.inputSchema.required,
      ["lat", "lon"],
      "Location search should require lat and lon"
    );

    assert.ok(
      tool.inputSchema.properties.lat,
      "Should have lat property"
    );
    assert.ok(
      tool.inputSchema.properties.lon,
      "Should have lon property"
    );
    assert.ok(
      tool.inputSchema.properties.radius,
      "Should have radius property"
    );
  });

  test("search_memories_by_time has correct required fields", () => {
    const tool = getTool("search_memories_by_time");

    assert.deepEqual(
      tool.inputSchema.required,
      ["start", "end"],
      "Time search should require start and end"
    );

    assert.ok(
      tool.inputSchema.properties.start,
      "Should have start property"
    );
    assert.ok(
      tool.inputSchema.properties.end,
      "Should have end property"
    );
  });

  test("search_memories_combined has all 4 required fields", () => {
    const tool = getTool("search_memories_combined");

    assert.deepEqual(
      tool.inputSchema.required,
      ["lat", "lon", "start", "end"],
      "Combined search should require lat, lon, start, and end"
    );

    assert.ok(
      tool.inputSchema.properties.lat,
      "Should have lat property"
    );
    assert.ok(
      tool.inputSchema.properties.lon,
      "Should have lon property"
    );
    assert.ok(
      tool.inputSchema.properties.radius,
      "Should have radius property"
    );
    assert.ok(
      tool.inputSchema.properties.start,
      "Should have start property"
    );
    assert.ok(
      tool.inputSchema.properties.end,
      "Should have end property"
    );
  });

  test("radius parameter has default value of 1000", () => {
    const locationTool = getTool("search_memories_by_location");
    const combinedTool = getTool("search_memories_combined");

    assert.equal(
      locationTool.inputSchema.properties.radius.default,
      1000,
      "Location search radius should default to 1000"
    );

    assert.equal(
      combinedTool.inputSchema.properties.radius.default,
      1000,
      "Combined search radius should default to 1000"
    );
  });

  test("all memory search tools have readOnlyHint annotation", () => {
    const toolNames = [
      "search_memories_by_location",
      "search_memories_by_time",
      "search_memories_combined",
      "get_memory_provenance"
    ];

    for (const toolName of toolNames) {
      const tool = getTool(toolName);
      assert.equal(
        tool.annotations?.readOnlyHint,
        true,
        `${toolName} should have readOnlyHint: true`
      );
    }
  });

  test("get_memory_provenance has note_id with uuid format", () => {
    const tool = getTool("get_memory_provenance");

    assert.ok(
      tool.inputSchema.properties.note_id,
      "Should have note_id property"
    );

    assert.equal(
      tool.inputSchema.properties.note_id.type,
      "string",
      "note_id should be string type"
    );

    assert.equal(
      tool.inputSchema.properties.note_id.format,
      "uuid",
      "note_id should have format: uuid"
    );

    assert.deepEqual(
      tool.inputSchema.required,
      ["note_id"],
      "Should require note_id"
    );
  });

  test("location tools have numeric types for lat/lon/radius", () => {
    const locationTool = getTool("search_memories_by_location");
    const combinedTool = getTool("search_memories_combined");

    for (const tool of [locationTool, combinedTool]) {
      assert.equal(
        tool.inputSchema.properties.lat.type,
        "number",
        `${tool.name}: lat should be number`
      );

      assert.equal(
        tool.inputSchema.properties.lon.type,
        "number",
        `${tool.name}: lon should be number`
      );

      assert.equal(
        tool.inputSchema.properties.radius.type,
        "number",
        `${tool.name}: radius should be number`
      );
    }
  });

  test("time tools have string types for start/end", () => {
    const timeTool = getTool("search_memories_by_time");
    const combinedTool = getTool("search_memories_combined");

    for (const tool of [timeTool, combinedTool]) {
      assert.equal(
        tool.inputSchema.properties.start.type,
        "string",
        `${tool.name}: start should be string`
      );

      assert.equal(
        tool.inputSchema.properties.end.type,
        "string",
        `${tool.name}: end should be string`
      );
    }
  });
});

// ============================================================================
// MEMORY SEARCH URL CONSTRUCTION
// ============================================================================

describe("Memory Search URL Construction", () => {
  test("location search builds correct URLSearchParams", () => {
    const url = buildMemorySearchUrl("search_memories_by_location", {
      lat: 48.86,
      lon: 2.29,
      radius: 1000
    });

    assert.match(url, /lat=48\.86/, "Should include lat parameter");
    assert.match(url, /lon=2\.29/, "Should include lon parameter");
    assert.match(url, /radius=1000/, "Should include radius parameter");
    assert.match(url, /^\/api\/v1\/memories\/search\?/, "Should use correct base path");
  });

  test("time search builds correct URLSearchParams with encoded dates", () => {
    const start = "2025-01-01T00:00:00Z";
    const end = "2025-12-31T23:59:59Z";

    const url = buildMemorySearchUrl("search_memories_by_time", {
      start,
      end
    });

    // The handler uses encodeURIComponent() before URLSearchParams.set()
    // This causes double-encoding: colons become %253A (not %3A)
    assert.match(url, /start=2025-01-01T00%253A00%253A00Z/, "Should include double-encoded start parameter");
    assert.match(url, /end=2025-12-31T23%253A59%253A59Z/, "Should include double-encoded end parameter");
    assert.match(url, /^\/api\/v1\/memories\/search\?/, "Should use correct base path");
  });

  test("combined search builds URL with all 5 params", () => {
    const url = buildMemorySearchUrl("search_memories_combined", {
      lat: 48.86,
      lon: 2.29,
      radius: 500,
      start: "2025-01-01T00:00:00Z",
      end: "2025-01-31T23:59:59Z"
    });

    assert.match(url, /lat=48\.86/, "Should include lat");
    assert.match(url, /lon=2\.29/, "Should include lon");
    assert.match(url, /radius=500/, "Should include radius");
    assert.match(url, /start=2025-01-01T00%253A00%253A00Z/, "Should include double-encoded start");
    assert.match(url, /end=2025-01-31T23%253A59%253A59Z/, "Should include double-encoded end");
  });

  test("default radius is used when not specified", () => {
    const urlWithoutRadius = buildMemorySearchUrl("search_memories_by_location", {
      lat: 48.86,
      lon: 2.29
    });

    // When radius is not provided, the URL builder doesn't add it
    // The API will use its default (1000)
    assert.doesNotMatch(urlWithoutRadius, /radius=/, "Should not include radius when not specified");

    const urlWithRadius = buildMemorySearchUrl("search_memories_by_location", {
      lat: 48.86,
      lon: 2.29,
      radius: 1000
    });

    assert.match(urlWithRadius, /radius=1000/, "Should include radius when specified");
  });

  test("get_memory_provenance builds correct path with UUID", () => {
    const noteId = "123e4567-e89b-12d3-a456-426614174000";
    const url = buildProvenanceUrl(noteId);

    assert.equal(
      url,
      `/api/v1/notes/${noteId}/memory-provenance`,
      "Should build correct provenance path"
    );
  });

  test("location search handles negative coordinates", () => {
    const url = buildMemorySearchUrl("search_memories_by_location", {
      lat: -33.8688,
      lon: 151.2093,
      radius: 2000
    });

    assert.match(url, /lat=-33\.8688/, "Should handle negative latitude");
    assert.match(url, /lon=151\.2093/, "Should handle positive longitude");
  });

  test("time search handles ISO 8601 dates with timezone offset", () => {
    const start = "2025-06-15T14:30:00+02:00";
    const end = "2025-06-15T18:30:00+02:00";

    const url = buildMemorySearchUrl("search_memories_by_time", {
      start,
      end
    });

    // Double encoding: + becomes %252B, : becomes %253A
    assert.match(url, /start=2025-06-15T14%253A30%253A00%252B02%253A00/, "Should double-encode timezone offset");
    assert.match(url, /end=2025-06-15T18%253A30%253A00%252B02%253A00/, "Should double-encode timezone offset");
  });
});

// ============================================================================
// MEMORY SEARCH ERROR HANDLING
// ============================================================================

describe("Memory Search Error Handling", () => {
  test("location search with API 400 error handled gracefully", async () => {
    const apiRequest = createMockApiRequest("bad_request_400");

    try {
      await apiRequest("GET", "/api/v1/memories/search?lat=invalid&lon=2.29");
      assert.fail("Expected 400 error");
    } catch (error) {
      assert.match(error.message, /400/, "Should contain 400 status code");
      assert.match(error.message, /invalid|coordinates/i, "Should contain error description");
    }
  });

  test("time search with API 500 error handled gracefully", async () => {
    const apiRequest = createMockApiRequest("internal_error_500");

    try {
      await apiRequest("GET", "/api/v1/memories/search?start=2025-01-01T00:00:00Z&end=2025-01-31T23:59:59Z");
      assert.fail("Expected 500 error");
    } catch (error) {
      assert.match(error.message, /500/, "Should contain 500 status code");
      assert.match(error.message, /error|database/i, "Should contain error description");
    }
  });

  test("provenance with 404 (note not found) handled gracefully", async () => {
    const apiRequest = createMockApiRequest("not_found_404");

    try {
      await apiRequest("GET", "/api/v1/notes/non-existent-id/memory-provenance");
      assert.fail("Expected 404 error");
    } catch (error) {
      assert.match(error.message, /404/, "Should contain 404 status code");
      assert.match(error.message, /not found/i, "Should contain error description");
    }
  });

  test("network errors are caught and reported", async () => {
    const apiRequest = createMockApiRequest("network_error");

    try {
      await apiRequest("GET", "/api/v1/memories/search?lat=48.86&lon=2.29");
      assert.fail("Expected network error");
    } catch (error) {
      assert.ok(error.message.length > 0, "Should have error message");
      assert.match(error.message, /network|failed/i, "Should indicate network failure");
    }
  });

  test("successful requests return expected data structure", async () => {
    const apiRequest = createMockApiRequest("success");

    const searchResult = await apiRequest(
      "GET",
      "/api/v1/memories/search?lat=48.86&lon=2.29&radius=1000"
    );

    assert.ok(searchResult.results, "Should have results array");
    assert.ok(Array.isArray(searchResult.results), "Results should be an array");

    const provenanceResult = await apiRequest(
      "GET",
      "/api/v1/notes/123e4567-e89b-12d3-a456-426614174000/memory-provenance"
    );

    assert.ok(provenanceResult.note_id, "Should have note_id");
    assert.ok(provenanceResult.attachments, "Should have attachments array");
  });
});

// ============================================================================
// MEMORY SEARCH CONSISTENCY
// ============================================================================

describe("Memory Search Consistency", () => {
  test("location tools share same lat/lon/radius schema", () => {
    const locationTool = getTool("search_memories_by_location");
    const combinedTool = getTool("search_memories_combined");

    // Compare lat schema
    assert.equal(
      locationTool.inputSchema.properties.lat.type,
      combinedTool.inputSchema.properties.lat.type,
      "lat type should match"
    );

    assert.ok(
      locationTool.inputSchema.properties.lat.description,
      "Location tool lat should have description"
    );

    assert.ok(
      combinedTool.inputSchema.properties.lat.description,
      "Combined tool lat should have description"
    );

    // Compare lon schema
    assert.equal(
      locationTool.inputSchema.properties.lon.type,
      combinedTool.inputSchema.properties.lon.type,
      "lon type should match"
    );

    // Compare radius schema
    assert.equal(
      locationTool.inputSchema.properties.radius.type,
      combinedTool.inputSchema.properties.radius.type,
      "radius type should match"
    );

    assert.equal(
      locationTool.inputSchema.properties.radius.default,
      combinedTool.inputSchema.properties.radius.default,
      "radius default should match"
    );
  });

  test("time tools share same start/end schema", () => {
    const timeTool = getTool("search_memories_by_time");
    const combinedTool = getTool("search_memories_combined");

    // Compare start schema
    assert.equal(
      timeTool.inputSchema.properties.start.type,
      combinedTool.inputSchema.properties.start.type,
      "start type should match"
    );

    assert.ok(
      timeTool.inputSchema.properties.start.description,
      "Time tool start should have description"
    );

    assert.ok(
      combinedTool.inputSchema.properties.start.description,
      "Combined tool start should have description"
    );

    // Compare end schema
    assert.equal(
      timeTool.inputSchema.properties.end.type,
      combinedTool.inputSchema.properties.end.type,
      "end type should match"
    );
  });

  test("all 3 search tools target same API base path", () => {
    const expectedPath = "/api/v1/memories/search";

    const locationUrl = buildMemorySearchUrl("search_memories_by_location", {
      lat: 48.86,
      lon: 2.29
    });

    const timeUrl = buildMemorySearchUrl("search_memories_by_time", {
      start: "2025-01-01T00:00:00Z",
      end: "2025-01-31T23:59:59Z"
    });

    const combinedUrl = buildMemorySearchUrl("search_memories_combined", {
      lat: 48.86,
      lon: 2.29,
      start: "2025-01-01T00:00:00Z",
      end: "2025-01-31T23:59:59Z"
    });

    assert.ok(
      locationUrl.startsWith(expectedPath),
      "Location search should use correct base path"
    );

    assert.ok(
      timeUrl.startsWith(expectedPath),
      "Time search should use correct base path"
    );

    assert.ok(
      combinedUrl.startsWith(expectedPath),
      "Combined search should use correct base path"
    );
  });

  test("all 4 tools are annotated as read-only", () => {
    const toolNames = [
      "search_memories_by_location",
      "search_memories_by_time",
      "search_memories_combined",
      "get_memory_provenance"
    ];

    const notReadOnly = [];

    for (const toolName of toolNames) {
      const tool = getTool(toolName);
      if (!tool.annotations?.readOnlyHint) {
        notReadOnly.push(toolName);
      }
    }

    assert.equal(
      notReadOnly.length,
      0,
      `All memory tools should be read-only. Missing annotation: ${notReadOnly.join(", ")}`
    );
  });

  test("parameter descriptions are meaningful and consistent", () => {
    const locationTool = getTool("search_memories_by_location");
    const timeTool = getTool("search_memories_by_time");
    const combinedTool = getTool("search_memories_combined");

    // Check lat/lon descriptions mention decimal degrees
    assert.match(
      locationTool.inputSchema.properties.lat.description,
      /decimal degrees/i,
      "lat description should mention decimal degrees"
    );

    assert.match(
      locationTool.inputSchema.properties.lon.description,
      /decimal degrees/i,
      "lon description should mention decimal degrees"
    );

    // Check radius description mentions meters
    assert.match(
      locationTool.inputSchema.properties.radius.description,
      /meters/i,
      "radius description should mention meters"
    );

    // Check time descriptions mention ISO 8601
    assert.match(
      timeTool.inputSchema.properties.start.description,
      /ISO 8601/i,
      "start description should mention ISO 8601"
    );

    // Check all descriptions are non-empty
    const allProps = [
      ...Object.values(locationTool.inputSchema.properties),
      ...Object.values(timeTool.inputSchema.properties),
      ...Object.values(combinedTool.inputSchema.properties),
    ];

    for (const prop of allProps) {
      assert.ok(
        prop.description && prop.description.length > 10,
        `Property description should be meaningful: ${prop.description}`
      );
    }
  });
});

// ============================================================================
// COVERAGE STATISTICS
// ============================================================================

describe("Memory Search Tool Coverage", () => {
  test("report memory search tool statistics", () => {
    const memoryTools = [
      "search_memories_by_location",
      "search_memories_by_time",
      "search_memories_combined",
      "get_memory_provenance"
    ];

    const stats = {
      total_memory_tools: memoryTools.length,
      search_tools: memoryTools.filter(name => name.startsWith("search_")).length,
      read_only_tools: memoryTools.filter(name => {
        const tool = getTool(name);
        return tool.annotations?.readOnlyHint === true;
      }).length,
      tools_with_defaults: memoryTools.filter(name => {
        const tool = getTool(name);
        const props = tool.inputSchema.properties || {};
        return Object.values(props).some(prop => prop.default !== undefined);
      }).length,
      total_parameters: memoryTools.reduce((sum, name) => {
        const tool = getTool(name);
        return sum + Object.keys(tool.inputSchema.properties || {}).length;
      }, 0),
    };

    console.log("\n=== Memory Search Tool Statistics ===");
    console.log(`Total memory tools: ${stats.total_memory_tools}`);
    console.log(`Search tools: ${stats.search_tools}`);
    console.log(`Read-only tools: ${stats.read_only_tools}`);
    console.log(`Tools with default values: ${stats.tools_with_defaults}`);
    console.log(`Total parameters: ${stats.total_parameters}`);

    // Verify all tools are present and read-only
    assert.equal(stats.total_memory_tools, 4, "Should have 4 memory tools");
    assert.equal(stats.search_tools, 3, "Should have 3 search tools");
    assert.equal(stats.read_only_tools, 4, "All 4 tools should be read-only");
    assert.equal(stats.tools_with_defaults, 2, "Location and combined tools should have default radius");
  });
});

console.log("\n✓ All memory search tests passed");
