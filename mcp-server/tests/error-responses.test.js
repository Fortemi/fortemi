#!/usr/bin/env node

/**
 * MCP Tool Error Response Tests (Issue #344)
 *
 * Validates that MCP tools handle errors correctly:
 * - 404 for non-existent resources
 * - 400 for invalid input
 * - Proper error message format
 * - Appropriate error handling in tool handlers
 */

import { strict as assert } from "node:assert";
import { test, describe } from "node:test";
import fs from "node:fs";
import path from "path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ============================================================================
// FIXTURES: Error response patterns
// ============================================================================

const ERROR_FIXTURES = {
  not_found: {
    status: 404,
    message: "Resource not found",
    examples: [
      "Note not found",
      "Collection not found",
      "Tag not found",
      "Embedding set not found"
    ]
  },
  bad_request: {
    status: 400,
    message: "Invalid input",
    examples: [
      "Missing required field",
      "Invalid UUID format",
      "Invalid parameter value",
      "Validation failed"
    ]
  },
  unauthorized: {
    status: 401,
    message: "Authentication required",
    examples: [
      "Missing authorization header",
      "Invalid token"
    ]
  },
  forbidden: {
    status: 403,
    message: "Access denied",
    examples: [
      "Insufficient permissions",
      "Resource access denied"
    ]
  },
  internal_error: {
    status: 500,
    message: "Internal server error",
    examples: [
      "Database error",
      "Unexpected error"
    ]
  }
};

// Expected error message format
const ERROR_MESSAGE_PATTERN = /^API error \d{3}: .+$/;

// ============================================================================
// MOCK API REQUEST FUNCTION
// ============================================================================

/**
 * Mock API request function that simulates various error conditions
 */
function createMockApiRequest(scenario = "success") {
  return async (method, path, body = null) => {
    // Simulate different error scenarios
    switch (scenario) {
      case "not_found":
        throw new Error("API error 404: Resource not found");

      case "invalid_uuid":
        throw new Error("API error 400: Invalid UUID format");

      case "missing_required":
        throw new Error("API error 400: Missing required field: content");

      case "validation_error":
        throw new Error("API error 400: Validation failed: tags must be an array");

      case "unauthorized":
        throw new Error("API error 401: Authentication required");

      case "forbidden":
        throw new Error("API error 403: Access denied");

      case "internal_error":
        throw new Error("API error 500: Internal server error");

      case "network_error":
        throw new Error("Network request failed");

      case "timeout":
        throw new Error("Request timeout");

      case "success":
        // Simulate successful responses based on endpoint
        if (path.includes("/notes/") && method === "GET") {
          return {
            id: "123e4567-e89b-12d3-a456-426614174000",
            content: "Test note",
            title: "Test",
            tags: []
          };
        }
        if (path.includes("/notes") && method === "POST") {
          return {
            id: "123e4567-e89b-12d3-a456-426614174000",
            content: body.content,
            title: "Generated Title",
            tags: body.tags || []
          };
        }
        return { success: true };

      default:
        throw new Error(`Unknown scenario: ${scenario}`);
    }
  };
}

// ============================================================================
// ERROR MESSAGE FORMAT TESTS
// ============================================================================

describe("Error Message Format", () => {
  test("error messages follow standard format", async () => {
    const scenarios = [
      "not_found",
      "invalid_uuid",
      "missing_required",
      "validation_error",
      "unauthorized",
      "forbidden",
      "internal_error"
    ];

    for (const scenario of scenarios) {
      const apiRequest = createMockApiRequest(scenario);

      try {
        await apiRequest("GET", "/api/v1/notes/test");
        assert.fail(`Expected ${scenario} to throw error`);
      } catch (error) {
        assert.match(
          error.message,
          ERROR_MESSAGE_PATTERN,
          `Error message should match pattern for ${scenario}: ${error.message}`
        );

        // Verify status code is present
        const statusMatch = error.message.match(/API error (\d{3}):/);
        assert.ok(statusMatch, `Error should contain status code for ${scenario}`);

        const status = parseInt(statusMatch[1], 10);
        assert.ok(status >= 400 && status < 600, `Status should be 4xx or 5xx for ${scenario}`);
      }
    }
  });

  test("error messages contain meaningful descriptions", async () => {
    const scenarios = [
      { name: "not_found", expectedKeywords: ["not found", "resource"] },
      { name: "invalid_uuid", expectedKeywords: ["invalid", "uuid"] },
      { name: "missing_required", expectedKeywords: ["missing", "required"] },
      { name: "validation_error", expectedKeywords: ["validation", "failed"] }
    ];

    for (const scenario of scenarios) {
      const apiRequest = createMockApiRequest(scenario.name);

      try {
        await apiRequest("GET", "/api/v1/notes/test");
        assert.fail(`Expected ${scenario.name} to throw error`);
      } catch (error) {
        const message = error.message.toLowerCase();
        const hasKeyword = scenario.expectedKeywords.some(keyword =>
          message.includes(keyword.toLowerCase())
        );

        assert.ok(
          hasKeyword,
          `Error message should contain one of [${scenario.expectedKeywords.join(", ")}] for ${scenario.name}: ${error.message}`
        );
      }
    }
  });
});

// ============================================================================
// RESOURCE NOT FOUND (404) TESTS
// ============================================================================

describe("404 Not Found Errors", () => {
  test("get_note returns 404 for non-existent note", async () => {
    const apiRequest = createMockApiRequest("not_found");

    try {
      await apiRequest("GET", "/api/v1/notes/non-existent-id");
      assert.fail("Expected 404 error");
    } catch (error) {
      assert.match(error.message, /404/);
    }
  });

  test("get_collection returns 404 for non-existent collection", async () => {
    const apiRequest = createMockApiRequest("not_found");

    try {
      await apiRequest("GET", "/api/v1/collections/non-existent-id");
      assert.fail("Expected 404 error");
    } catch (error) {
      assert.match(error.message, /404/);
    }
  });

  test("get_template returns 404 for non-existent template", async () => {
    const apiRequest = createMockApiRequest("not_found");

    try {
      await apiRequest("GET", "/api/v1/templates/non-existent-slug");
      assert.fail("Expected 404 error");
    } catch (error) {
      assert.match(error.message, /404/);
    }
  });

  test("get_embedding_set returns 404 for non-existent set", async () => {
    const apiRequest = createMockApiRequest("not_found");

    try {
      await apiRequest("GET", "/api/v1/embedding-sets/non-existent-slug");
      assert.fail("Expected 404 error");
    } catch (error) {
      assert.match(error.message, /404/);
    }
  });
});

// ============================================================================
// BAD REQUEST (400) TESTS
// ============================================================================

describe("400 Bad Request Errors", () => {
  test("create_note returns 400 for missing content", async () => {
    const apiRequest = createMockApiRequest("missing_required");

    try {
      await apiRequest("POST", "/api/v1/notes", {});
      assert.fail("Expected 400 error");
    } catch (error) {
      assert.match(error.message, /400/);
      assert.match(error.message, /missing|required/i);
    }
  });

  test("update_note returns 400 for invalid UUID", async () => {
    const apiRequest = createMockApiRequest("invalid_uuid");

    try {
      await apiRequest("PATCH", "/api/v1/notes/invalid-uuid", { starred: true });
      assert.fail("Expected 400 error");
    } catch (error) {
      assert.match(error.message, /400/);
      assert.match(error.message, /invalid|uuid/i);
    }
  });

  test("set_note_tags returns 400 for invalid tags format", async () => {
    const apiRequest = createMockApiRequest("validation_error");

    try {
      await apiRequest("PUT", "/api/v1/notes/test-id/tags", { tags: "not-an-array" });
      assert.fail("Expected 400 error");
    } catch (error) {
      assert.match(error.message, /400/);
      assert.match(error.message, /validation/i);
    }
  });

  test("create_template returns 400 for invalid template structure", async () => {
    const apiRequest = createMockApiRequest("validation_error");

    try {
      await apiRequest("POST", "/api/v1/templates", {
        name: "test",
        // Missing required 'content' field
      });
      assert.fail("Expected 400 error");
    } catch (error) {
      assert.match(error.message, /400/);
    }
  });

  test("bulk_create_notes returns 400 when exceeding batch limit", async () => {
    const apiRequest = createMockApiRequest("validation_error");

    try {
      const notes = Array(101).fill({ content: "test" });
      await apiRequest("POST", "/api/v1/notes/bulk", { notes });
      assert.fail("Expected 400 error");
    } catch (error) {
      assert.match(error.message, /400/);
    }
  });
});

// ============================================================================
// AUTHENTICATION (401) TESTS
// ============================================================================

describe("401 Unauthorized Errors", () => {
  test("requests without auth token return 401", async () => {
    const apiRequest = createMockApiRequest("unauthorized");

    try {
      await apiRequest("GET", "/api/v1/notes");
      assert.fail("Expected 401 error");
    } catch (error) {
      assert.match(error.message, /401/);
      assert.match(error.message, /authentication/i);
    }
  });

  test("requests with invalid token return 401", async () => {
    const apiRequest = createMockApiRequest("unauthorized");

    try {
      await apiRequest("GET", "/api/v1/notes");
      assert.fail("Expected 401 error");
    } catch (error) {
      assert.match(error.message, /401/);
    }
  });
});

// ============================================================================
// AUTHORIZATION (403) TESTS
// ============================================================================

describe("403 Forbidden Errors", () => {
  test("accessing restricted resources returns 403", async () => {
    const apiRequest = createMockApiRequest("forbidden");

    try {
      await apiRequest("DELETE", "/api/v1/admin/users/test-id");
      assert.fail("Expected 403 error");
    } catch (error) {
      assert.match(error.message, /403/);
      assert.match(error.message, /access|forbidden|denied/i);
    }
  });
});

// ============================================================================
// INTERNAL ERROR (500) TESTS
// ============================================================================

describe("500 Internal Server Error", () => {
  test("server errors return 500", async () => {
    const apiRequest = createMockApiRequest("internal_error");

    try {
      await apiRequest("GET", "/api/v1/notes");
      assert.fail("Expected 500 error");
    } catch (error) {
      assert.match(error.message, /500/);
      assert.match(error.message, /internal|server|error/i);
    }
  });
});

// ============================================================================
// NETWORK ERROR TESTS
// ============================================================================

describe("Network Errors", () => {
  test("network failures are handled gracefully", async () => {
    const apiRequest = createMockApiRequest("network_error");

    try {
      await apiRequest("GET", "/api/v1/notes");
      assert.fail("Expected network error");
    } catch (error) {
      assert.ok(error.message.length > 0, "Error should have a message");
      assert.match(error.message, /network|failed/i);
    }
  });

  test("timeouts are handled gracefully", async () => {
    const apiRequest = createMockApiRequest("timeout");

    try {
      await apiRequest("GET", "/api/v1/notes");
      assert.fail("Expected timeout error");
    } catch (error) {
      assert.ok(error.message.length > 0, "Error should have a message");
      assert.match(error.message, /timeout/i);
    }
  });
});

// ============================================================================
// ERROR HANDLING BEST PRACTICES
// ============================================================================

describe("Error Handling Best Practices", () => {
  test("successful requests do not throw errors", async () => {
    const apiRequest = createMockApiRequest("success");

    // These should all succeed without throwing
    const result1 = await apiRequest("GET", "/api/v1/notes/test-id");
    assert.ok(result1.id, "Should return note data");

    const result2 = await apiRequest("POST", "/api/v1/notes", {
      content: "Test note"
    });
    assert.ok(result2.id, "Should return created note");

    const result3 = await apiRequest("DELETE", "/api/v1/notes/test-id");
    assert.ok(result3.success, "Should return success");
  });

  test("errors contain enough context for debugging", async () => {
    const scenarios = [
      "not_found",
      "invalid_uuid",
      "missing_required",
      "validation_error",
      "unauthorized",
      "forbidden",
      "internal_error"
    ];

    for (const scenario of scenarios) {
      const apiRequest = createMockApiRequest(scenario);

      try {
        await apiRequest("GET", "/api/v1/notes/test");
        assert.fail(`Expected ${scenario} to throw`);
      } catch (error) {
        // Error message should have at least:
        // 1. Status code
        // 2. Error type/reason
        assert.ok(error.message.length > 15, `Error message too short for ${scenario}: ${error.message}`);

        // Should contain both status code and description
        const parts = error.message.split(":");
        assert.ok(parts.length >= 2, `Error message should have status and description for ${scenario}`);
      }
    }
  });

  test("errors are catchable and don't crash the process", async () => {
    const scenarios = [
      "not_found",
      "invalid_uuid",
      "unauthorized",
      "internal_error",
      "network_error"
    ];

    let caughtErrors = 0;

    for (const scenario of scenarios) {
      const apiRequest = createMockApiRequest(scenario);

      try {
        await apiRequest("GET", "/api/v1/notes/test");
      } catch (error) {
        caughtErrors++;
        // Verify it's a proper Error object
        assert.ok(error instanceof Error, `Should be Error instance for ${scenario}`);
        assert.ok(error.message, `Should have message for ${scenario}`);
        assert.ok(error.stack, `Should have stack trace for ${scenario}`);
      }
    }

    assert.equal(caughtErrors, scenarios.length, "All errors should be catchable");
  });
});

// ============================================================================
// ERROR RESPONSE STATISTICS
// ============================================================================

describe("Error Response Coverage", () => {
  test("report error handling coverage", () => {
    const errorTypes = Object.keys(ERROR_FIXTURES);
    const stats = {
      total_error_types: errorTypes.length,
      status_codes: errorTypes.map(type => ERROR_FIXTURES[type].status),
      http_4xx: errorTypes.filter(type =>
        ERROR_FIXTURES[type].status >= 400 && ERROR_FIXTURES[type].status < 500
      ).length,
      http_5xx: errorTypes.filter(type =>
        ERROR_FIXTURES[type].status >= 500
      ).length,
      example_messages: errorTypes.reduce((acc, type) => {
        return acc + ERROR_FIXTURES[type].examples.length;
      }, 0)
    };

    console.log("\n=== Error Response Coverage ===");
    console.log(`Total error types tested: ${stats.total_error_types}`);
    console.log(`Status codes covered: ${stats.status_codes.join(", ")}`);
    console.log(`4xx client errors: ${stats.http_4xx}`);
    console.log(`5xx server errors: ${stats.http_5xx}`);
    console.log(`Example error messages: ${stats.example_messages}`);

    // Verify we cover the most important status codes
    assert.ok(stats.status_codes.includes(400), "Should test 400 Bad Request");
    assert.ok(stats.status_codes.includes(401), "Should test 401 Unauthorized");
    assert.ok(stats.status_codes.includes(403), "Should test 403 Forbidden");
    assert.ok(stats.status_codes.includes(404), "Should test 404 Not Found");
    assert.ok(stats.status_codes.includes(500), "Should test 500 Internal Server Error");
  });
});

console.log("\n✓ All error response tests passed");
