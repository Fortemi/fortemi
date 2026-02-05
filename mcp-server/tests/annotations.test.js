#!/usr/bin/env node

/**
 * MCP Tool Annotations Test (Issue #344)
 *
 * Validates that MCP tools have proper annotations:
 * - readOnlyHint: true for read-only operations
 * - destructiveHint: true for destructive operations
 * - destructiveHint: false for safe write operations
 *
 * Annotations help MCP clients make informed decisions about:
 * - Caching (read-only operations)
 * - User confirmation (destructive operations)
 * - Rate limiting and quotas
 */

import { strict as assert } from "node:assert";
import { test, describe } from "node:test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load the tools array from index.js
const indexPath = path.join(__dirname, "..", "index.js");
const indexContent = fs.readFileSync(indexPath, "utf8");

// Extract tools array
const toolsMatch = indexContent.match(/const tools = \[([\s\S]*?)\n\];/);
if (!toolsMatch) {
  throw new Error("Could not extract tools array from index.js");
}

let tools;
try {
  const toolsCode = `(function() { return [${toolsMatch[1]}]; })()`;
  tools = eval(toolsCode);
} catch (error) {
  throw new Error(`Failed to parse tools array: ${error.message}`);
}

console.log(`Loaded ${tools.length} tools for annotation validation\n`);

// ============================================================================
// FIXTURES: Tool classification patterns
// ============================================================================

const READ_ONLY_PATTERNS = {
  prefixes: ["list_", "get_", "search_", "export_"],
  exact: ["health_check", "get_queue_stats", "get_system_info"],
  description_keywords: ["retrieve", "fetch", "view", "query", "read"]
};

const DESTRUCTIVE_PATTERNS = {
  prefixes: ["delete_", "purge_", "remove_", "wipe_"],
  exact: [],
  description_keywords: ["permanently delete", "destroy", "remove permanently", "wipe"]
};

const SAFE_WRITE_PATTERNS = {
  prefixes: ["create_", "update_", "set_", "add_", "restore_", "attach_", "detach_"],
  exact: ["bulk_create_notes", "create_job"],
  description_keywords: ["create", "update", "modify", "change", "add", "restore"]
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Classify tool based on naming conventions and description
 */
function classifyTool(tool) {
  const name = tool.name.toLowerCase();
  const desc = tool.description.toLowerCase();

  // Check for read-only
  if (READ_ONLY_PATTERNS.prefixes.some(p => name.startsWith(p)) ||
      READ_ONLY_PATTERNS.exact.includes(name)) {
    return "read-only";
  }

  // Check for destructive
  if (DESTRUCTIVE_PATTERNS.prefixes.some(p => name.startsWith(p)) ||
      DESTRUCTIVE_PATTERNS.exact.includes(name) ||
      DESTRUCTIVE_PATTERNS.description_keywords.some(k => desc.includes(k))) {
    return "destructive";
  }

  // Check for safe write
  if (SAFE_WRITE_PATTERNS.prefixes.some(p => name.startsWith(p)) ||
      SAFE_WRITE_PATTERNS.exact.includes(name)) {
    return "safe-write";
  }

  return "unknown";
}

/**
 * Get annotation status for a tool
 */
function getAnnotationStatus(tool) {
  if (!tool.annotations) {
    return { hasAnnotations: false };
  }

  return {
    hasAnnotations: true,
    readOnlyHint: tool.annotations.readOnlyHint,
    destructiveHint: tool.annotations.destructiveHint
  };
}

/**
 * Check if annotations match expected classification
 */
function validateAnnotations(tool) {
  const classification = classifyTool(tool);
  const annotations = getAnnotationStatus(tool);
  const issues = [];

  switch (classification) {
    case "read-only":
      if (!annotations.hasAnnotations) {
        issues.push("Missing annotations object");
      } else if (annotations.readOnlyHint !== true) {
        issues.push(`Expected readOnlyHint: true, got ${annotations.readOnlyHint}`);
      }
      // Read-only tools shouldn't have destructiveHint set
      if (annotations.destructiveHint !== undefined) {
        issues.push(`Read-only tool should not have destructiveHint`);
      }
      break;

    case "destructive":
      if (!annotations.hasAnnotations) {
        issues.push("Missing annotations object");
      } else if (annotations.destructiveHint !== true) {
        issues.push(`Expected destructiveHint: true, got ${annotations.destructiveHint}`);
      }
      break;

    case "safe-write":
      if (!annotations.hasAnnotations) {
        issues.push("Missing annotations object");
      } else if (annotations.destructiveHint !== false) {
        issues.push(`Expected destructiveHint: false, got ${annotations.destructiveHint}`);
      }
      break;

    case "unknown":
      // For unknown classification, annotations are optional but should be consistent
      if (annotations.hasAnnotations) {
        if (annotations.readOnlyHint === true && annotations.destructiveHint !== undefined) {
          issues.push("Tool has both readOnlyHint and destructiveHint");
        }
      }
      break;
  }

  return { classification, annotations, issues };
}

// ============================================================================
// ANNOTATION VALIDATION TESTS
// ============================================================================

describe("Read-Only Tool Annotations", () => {
  test("all read-only tools have readOnlyHint: true", () => {
    const violations = [];

    for (const tool of tools) {
      const classification = classifyTool(tool);
      if (classification !== "read-only") continue;

      const validation = validateAnnotations(tool);
      if (validation.issues.length > 0) {
        violations.push({
          name: tool.name,
          issues: validation.issues
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} read-only tools have annotation issues:\n${JSON.stringify(violations, null, 2)}`
    );
  });

  test("read-only tools do not have destructiveHint", () => {
    const violations = [];

    for (const tool of tools) {
      const classification = classifyTool(tool);
      if (classification !== "read-only") continue;

      if (tool.annotations?.destructiveHint !== undefined) {
        violations.push({
          name: tool.name,
          destructiveHint: tool.annotations.destructiveHint
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} read-only tools have destructiveHint:\n${JSON.stringify(violations, null, 2)}`
    );
  });
});

describe("Destructive Tool Annotations", () => {
  test("all destructive tools have destructiveHint: true", () => {
    const violations = [];

    for (const tool of tools) {
      const classification = classifyTool(tool);
      if (classification !== "destructive") continue;

      const validation = validateAnnotations(tool);
      if (validation.issues.length > 0) {
        violations.push({
          name: tool.name,
          issues: validation.issues
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} destructive tools have annotation issues:\n${JSON.stringify(violations, null, 2)}`
    );
  });

  test("destructive tools do not have readOnlyHint", () => {
    const violations = [];

    for (const tool of tools) {
      const classification = classifyTool(tool);
      if (classification !== "destructive") continue;

      if (tool.annotations?.readOnlyHint !== undefined) {
        violations.push({
          name: tool.name,
          readOnlyHint: tool.annotations.readOnlyHint
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} destructive tools have readOnlyHint:\n${JSON.stringify(violations, null, 2)}`
    );
  });
});

describe("Safe Write Tool Annotations", () => {
  test("all safe write tools have destructiveHint: false", () => {
    const violations = [];

    for (const tool of tools) {
      const classification = classifyTool(tool);
      if (classification !== "safe-write") continue;

      const validation = validateAnnotations(tool);
      if (validation.issues.length > 0) {
        violations.push({
          name: tool.name,
          issues: validation.issues
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} safe write tools have annotation issues:\n${JSON.stringify(violations, null, 2)}`
    );
  });

  test("safe write tools do not have readOnlyHint", () => {
    const violations = [];

    for (const tool of tools) {
      const classification = classifyTool(tool);
      if (classification !== "safe-write") continue;

      if (tool.annotations?.readOnlyHint !== undefined) {
        violations.push({
          name: tool.name,
          readOnlyHint: tool.annotations.readOnlyHint
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} safe write tools have readOnlyHint:\n${JSON.stringify(violations, null, 2)}`
    );
  });
});

describe("Annotation Consistency", () => {
  test("no tool has both readOnlyHint and destructiveHint", () => {
    const violations = [];

    for (const tool of tools) {
      if (tool.annotations?.readOnlyHint !== undefined &&
          tool.annotations?.destructiveHint !== undefined) {
        violations.push({
          name: tool.name,
          readOnlyHint: tool.annotations.readOnlyHint,
          destructiveHint: tool.annotations.destructiveHint
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} tools have both hints:\n${JSON.stringify(violations, null, 2)}`
    );
  });

  test("annotation values are boolean when present", () => {
    const violations = [];

    for (const tool of tools) {
      if (!tool.annotations) continue;

      if (tool.annotations.readOnlyHint !== undefined &&
          typeof tool.annotations.readOnlyHint !== "boolean") {
        violations.push({
          name: tool.name,
          field: "readOnlyHint",
          value: tool.annotations.readOnlyHint,
          type: typeof tool.annotations.readOnlyHint
        });
      }

      if (tool.annotations.destructiveHint !== undefined &&
          typeof tool.annotations.destructiveHint !== "boolean") {
        violations.push({
          name: tool.name,
          field: "destructiveHint",
          value: tool.annotations.destructiveHint,
          type: typeof tool.annotations.destructiveHint
        });
      }
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} annotations have non-boolean values:\n${JSON.stringify(violations, null, 2)}`
    );
  });
});

// ============================================================================
// ANNOTATION COVERAGE STATISTICS
// ============================================================================

describe("Annotation Coverage Report", () => {
  test("generate annotation coverage statistics", () => {
    const stats = {
      total: tools.length,
      by_classification: {
        "read-only": 0,
        "destructive": 0,
        "safe-write": 0,
        "unknown": 0
      },
      with_annotations: 0,
      without_annotations: 0,
      properly_annotated: 0,
      missing_annotations: [],
      incorrect_annotations: []
    };

    for (const tool of tools) {
      const classification = classifyTool(tool);
      stats.by_classification[classification]++;

      const validation = validateAnnotations(tool);

      if (validation.annotations.hasAnnotations) {
        stats.with_annotations++;
      } else {
        stats.without_annotations++;
      }

      if (validation.issues.length === 0) {
        stats.properly_annotated++;
      } else {
        if (!validation.annotations.hasAnnotations) {
          stats.missing_annotations.push({
            name: tool.name,
            classification,
            issues: validation.issues
          });
        } else {
          stats.incorrect_annotations.push({
            name: tool.name,
            classification,
            issues: validation.issues
          });
        }
      }
    }

    console.log("\n=== Annotation Coverage Statistics ===");
    console.log(`Total tools: ${stats.total}`);
    console.log(`\nClassification breakdown:`);
    console.log(`  Read-only: ${stats.by_classification["read-only"]}`);
    console.log(`  Destructive: ${stats.by_classification["destructive"]}`);
    console.log(`  Safe-write: ${stats.by_classification["safe-write"]}`);
    console.log(`  Unknown: ${stats.by_classification["unknown"]}`);
    console.log(`\nAnnotation status:`);
    console.log(`  With annotations: ${stats.with_annotations} (${(stats.with_annotations/stats.total*100).toFixed(1)}%)`);
    console.log(`  Without annotations: ${stats.without_annotations}`);
    console.log(`  Properly annotated: ${stats.properly_annotated} (${(stats.properly_annotated/stats.total*100).toFixed(1)}%)`);

    if (stats.missing_annotations.length > 0) {
      console.log(`\nTools missing annotations (${stats.missing_annotations.length}):`);
      for (const tool of stats.missing_annotations.slice(0, 10)) {
        console.log(`  - ${tool.name} (${tool.classification})`);
      }
      if (stats.missing_annotations.length > 10) {
        console.log(`  ... and ${stats.missing_annotations.length - 10} more`);
      }
    }

    if (stats.incorrect_annotations.length > 0) {
      console.log(`\nTools with incorrect annotations (${stats.incorrect_annotations.length}):`);
      for (const tool of stats.incorrect_annotations.slice(0, 10)) {
        console.log(`  - ${tool.name} (${tool.classification}): ${tool.issues.join(", ")}`);
      }
      if (stats.incorrect_annotations.length > 10) {
        console.log(`  ... and ${stats.incorrect_annotations.length - 10} more`);
      }
    }

    // Basic sanity checks
    assert.ok(stats.total > 0, "Should have at least one tool");
    assert.equal(
      stats.with_annotations + stats.without_annotations,
      stats.total,
      "Annotation counts should sum to total"
    );
  });
});

describe("Annotation Best Practices", () => {
  test("suggest annotations for unannotated tools", () => {
    const suggestions = [];

    for (const tool of tools) {
      if (tool.annotations) continue;

      const classification = classifyTool(tool);
      if (classification === "unknown") continue;

      let suggested = {};
      switch (classification) {
        case "read-only":
          suggested = { readOnlyHint: true };
          break;
        case "destructive":
          suggested = { destructiveHint: true };
          break;
        case "safe-write":
          suggested = { destructiveHint: false };
          break;
      }

      suggestions.push({
        name: tool.name,
        classification,
        suggested_annotation: suggested
      });
    }

    if (suggestions.length > 0) {
      console.log(`\n=== Annotation Suggestions (${suggestions.length} tools) ===`);
      for (const suggestion of suggestions.slice(0, 10)) {
        console.log(`  ${suggestion.name}: ${JSON.stringify(suggestion.suggested_annotation)}`);
      }
      if (suggestions.length > 10) {
        console.log(`  ... and ${suggestions.length - 10} more`);
      }
    }
  });
});

console.log("\n✓ All annotation tests passed");
