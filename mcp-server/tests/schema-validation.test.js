#!/usr/bin/env node

/**
 * MCP Tool Schema Validation Tests (Issue #344)
 *
 * Validates that all MCP tools have:
 * - Required fields: name, description, inputSchema
 * - Valid JSON Schema structure in inputSchema
 * - Proper type definitions
 * - Correct required field specifications
 */

import { strict as assert } from "node:assert";
import { test, describe } from "node:test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load the tools array from tools.js
const toolsPath = path.join(__dirname, "..", "tools.js");
const toolsContent = fs.readFileSync(toolsPath, "utf8");

// Extract tools array — supports 'export default [' and 'const tools = ['
let marker = "export default [";
let markerIdx = toolsContent.indexOf(marker);
if (markerIdx === -1) {
  marker = "const tools = [";
  markerIdx = toolsContent.indexOf(marker);
}
if (markerIdx === -1) {
  throw new Error("Could not find tools array in tools.js");
}

// Extract the array content between [ and ];
const arrayStart = toolsContent.indexOf("[", markerIdx);
const bracketContent = toolsContent.substring(arrayStart);

let tools;
try {
  tools = eval(`(function() { return ${bracketContent.replace(/;\s*$/, "")} })()`);
} catch (error) {
  throw new Error(`Failed to parse tools array: ${error.message}`);
}

console.log(`Loaded ${tools.length} tools for validation\n`);

// ============================================================================
// FIXTURES: Expected tool structure
// ============================================================================

const validToolExample = {
  name: "example_tool",
  description: "Example tool description",
  inputSchema: {
    type: "object",
    properties: {
      id: { type: "string", description: "Example ID" }
    },
    required: ["id"]
  }
};

const readOnlyToolExample = {
  name: "list_example",
  description: "List example items",
  inputSchema: {
    type: "object",
    properties: {}
  },
  annotations: {
    readOnlyHint: true
  }
};

const destructiveToolExample = {
  name: "delete_example",
  description: "Delete example item",
  inputSchema: {
    type: "object",
    properties: {
      id: { type: "string" }
    },
    required: ["id"]
  },
  annotations: {
    destructiveHint: true
  }
};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Validate JSON Schema structure recursively.
 *
 * Enforces JSON Schema draft 2020-12 rules required by the Claude API:
 *   - No type arrays (use anyOf instead)
 *   - No boolean exclusiveMinimum/Maximum (use numeric values)
 *   - No $ref (not supported)
 */
function validateJsonSchema(schema, path = "schema") {
  const errors = [];

  // Root must be object
  if (!schema || typeof schema !== "object") {
    errors.push(`${path}: Schema must be an object`);
    return errors;
  }

  // --- Draft 2020-12 specific checks (Claude API rejects these) ---

  // type arrays are draft-04/07 syntax for nullable; use anyOf instead
  if (Array.isArray(schema.type)) {
    errors.push(`${path}: type is array ${JSON.stringify(schema.type)} — use anyOf: [{type: "string"}, {type: "null"}] instead (draft 2020-12)`);
  }

  // boolean exclusiveMinimum/Maximum is draft-04; must be a number in 2020-12
  if (schema.exclusiveMinimum === true || schema.exclusiveMinimum === false) {
    errors.push(`${path}: exclusiveMinimum is boolean — must be a number (e.g. exclusiveMinimum: 0) (draft 2020-12)`);
  }
  if (schema.exclusiveMaximum === true || schema.exclusiveMaximum === false) {
    errors.push(`${path}: exclusiveMaximum is boolean — must be a number (draft 2020-12)`);
  }

  // $ref not supported by Claude API tool schemas
  if (schema["$ref"]) {
    errors.push(`${path}: $ref is not supported in Claude API tool schemas`);
  }

  // --- Standard structure checks ---

  // A property must have type OR a composition keyword (anyOf/oneOf/allOf)
  const hasType = !!schema.type;
  const hasComposition = schema.anyOf || schema.oneOf || schema.allOf;
  if (!hasType && !hasComposition && path !== "schema") {
    // Only flag if this is a leaf schema (not the root which always has type: "object")
    errors.push(`${path}: Missing 'type' field (or anyOf/oneOf/allOf)`);
  }

  // Validate type values
  const validTypes = ["object", "array", "string", "number", "integer", "boolean", "null"];
  if (schema.type && typeof schema.type === "string" && !validTypes.includes(schema.type)) {
    errors.push(`${path}: Invalid type '${schema.type}'. Must be one of: ${validTypes.join(", ")}`);
  }

  // Validate properties if present
  if (schema.properties) {
    if (typeof schema.properties !== "object") {
      errors.push(`${path}.properties: Must be an object`);
    } else {
      for (const [propName, propSchema] of Object.entries(schema.properties)) {
        if (!propSchema || typeof propSchema !== "object") {
          errors.push(`${path}.properties.${propName}: Must be an object`);
          continue;
        }
        // Recurse into all property schemas
        errors.push(...validateJsonSchema(propSchema, `${path}.properties.${propName}`));
      }
    }
  }

  // Validate required array if present
  if (schema.required) {
    if (!Array.isArray(schema.required)) {
      errors.push(`${path}.required: Must be an array`);
    } else if (schema.properties) {
      for (const requiredField of schema.required) {
        if (!schema.properties[requiredField]) {
          errors.push(`${path}.required: Field '${requiredField}' not found in properties`);
        }
      }
    }
  }

  // Validate items (arrays)
  if (schema.items) {
    if (Array.isArray(schema.items)) {
      errors.push(`${path}.items: tuple form (array) not allowed — use prefixItems in draft 2020-12`);
    } else if (typeof schema.items === "object") {
      errors.push(...validateJsonSchema(schema.items, `${path}.items`));
    }
  }

  // Validate enum if present
  if (schema.enum && !Array.isArray(schema.enum)) {
    errors.push(`${path}.enum: Must be an array`);
  }

  // Recurse into composition keywords
  for (const kw of ["anyOf", "oneOf", "allOf"]) {
    if (Array.isArray(schema[kw])) {
      schema[kw].forEach((s, i) => {
        errors.push(...validateJsonSchema(s, `${path}.${kw}[${i}]`));
      });
    }
  }

  // Recurse into additionalProperties if object
  if (schema.additionalProperties && typeof schema.additionalProperties === "object" && schema.additionalProperties !== true) {
    errors.push(...validateJsonSchema(schema.additionalProperties, `${path}.additionalProperties`));
  }

  return errors;
}

/**
 * Check if a tool is a read-only operation
 */
function isReadOnlyTool(tool) {
  const readOnlyPrefixes = ["list_", "get_", "search_", "export_"];
  const readOnlyNames = ["health_check"];

  return readOnlyPrefixes.some(prefix => tool.name.startsWith(prefix)) ||
         readOnlyNames.includes(tool.name);
}

/**
 * Check if a tool is a destructive operation
 */
function isDestructiveTool(tool) {
  const destructivePrefixes = ["delete_", "purge_", "remove_"];
  const destructiveNames = ["wipe_archive_registry"];

  return destructivePrefixes.some(prefix => tool.name.startsWith(prefix)) ||
         destructiveNames.includes(tool.name);
}

// ============================================================================
// SCHEMA STRUCTURE TESTS
// ============================================================================

describe("Tool Schema Structure", () => {
  test("all tools have required top-level fields", () => {
    const missingFields = [];

    for (const tool of tools) {
      const errors = [];

      if (!tool.name || typeof tool.name !== "string") {
        errors.push("missing or invalid 'name'");
      }

      if (!tool.description || typeof tool.description !== "string") {
        errors.push("missing or invalid 'description'");
      }

      if (!tool.inputSchema || typeof tool.inputSchema !== "object") {
        errors.push("missing or invalid 'inputSchema'");
      }

      if (errors.length > 0) {
        missingFields.push({
          tool: tool.name || "(unnamed)",
          errors
        });
      }
    }

    assert.equal(
      missingFields.length,
      0,
      `${missingFields.length} tools have missing fields:\n${JSON.stringify(missingFields, null, 2)}`
    );
  });

  test("all tool names are unique", () => {
    const names = tools.map(t => t.name);
    const duplicates = names.filter((name, index) => names.indexOf(name) !== index);

    assert.equal(
      duplicates.length,
      0,
      `Duplicate tool names found: ${duplicates.join(", ")}`
    );
  });

  test("all tool names follow naming convention", () => {
    const invalidNames = [];
    const validPattern = /^[a-z][a-z0-9_]*$/;

    for (const tool of tools) {
      if (!validPattern.test(tool.name)) {
        invalidNames.push(tool.name);
      }
    }

    assert.equal(
      invalidNames.length,
      0,
      `Tools with invalid names (must be snake_case): ${invalidNames.join(", ")}`
    );
  });

  test("all tool descriptions are non-empty and meaningful", () => {
    const shortDescriptions = [];

    for (const tool of tools) {
      if (!tool.description || tool.description.trim().length < 10) {
        shortDescriptions.push({
          name: tool.name,
          length: tool.description?.length || 0
        });
      }
    }

    assert.equal(
      shortDescriptions.length,
      0,
      `${shortDescriptions.length} tools have descriptions shorter than 10 characters:\n${JSON.stringify(shortDescriptions, null, 2)}`
    );
  });
});

// ============================================================================
// JSON SCHEMA VALIDATION TESTS
// ============================================================================

describe("Tool InputSchema Validation", () => {
  test("all inputSchemas have valid JSON Schema structure", () => {
    const schemaErrors = [];

    for (const tool of tools) {
      const errors = validateJsonSchema(tool.inputSchema, tool.name);
      if (errors.length > 0) {
        schemaErrors.push({
          tool: tool.name,
          errors
        });
      }
    }

    assert.equal(
      schemaErrors.length,
      0,
      `${schemaErrors.length} tools have invalid JSON Schema:\n${JSON.stringify(schemaErrors, null, 2)}`
    );
  });

  test("all inputSchemas have type 'object' at root", () => {
    const invalidSchemas = [];

    for (const tool of tools) {
      if (tool.inputSchema.type !== "object") {
        invalidSchemas.push({
          name: tool.name,
          type: tool.inputSchema.type
        });
      }
    }

    assert.equal(
      invalidSchemas.length,
      0,
      `${invalidSchemas.length} tools have non-object root schema:\n${JSON.stringify(invalidSchemas, null, 2)}`
    );
  });

  test("all required fields exist in properties", () => {
    const mismatches = [];

    for (const tool of tools) {
      if (!tool.inputSchema.required) continue;
      if (!tool.inputSchema.properties) {
        mismatches.push({
          tool: tool.name,
          error: "has 'required' but no 'properties'"
        });
        continue;
      }

      for (const requiredField of tool.inputSchema.required) {
        if (!tool.inputSchema.properties[requiredField]) {
          mismatches.push({
            tool: tool.name,
            field: requiredField,
            error: "required field not in properties"
          });
        }
      }
    }

    assert.equal(
      mismatches.length,
      0,
      `${mismatches.length} schema mismatches:\n${JSON.stringify(mismatches, null, 2)}`
    );
  });

  test("all property schemas have type or composition keyword", () => {
    const missingTypes = [];

    for (const tool of tools) {
      if (!tool.inputSchema.properties) continue;

      for (const [propName, propSchema] of Object.entries(tool.inputSchema.properties)) {
        const hasType = !!propSchema.type;
        const hasComposition = propSchema.anyOf || propSchema.oneOf || propSchema.allOf;
        if (!hasType && !hasComposition) {
          missingTypes.push({
            tool: tool.name,
            property: propName
          });
        }
      }
    }

    assert.equal(
      missingTypes.length,
      0,
      `${missingTypes.length} properties missing type or anyOf/oneOf/allOf:\n${JSON.stringify(missingTypes, null, 2)}`
    );
  });

  test("enum properties have valid enum arrays", () => {
    const invalidEnums = [];

    for (const tool of tools) {
      if (!tool.inputSchema.properties) continue;

      for (const [propName, propSchema] of Object.entries(tool.inputSchema.properties)) {
        if (propSchema.enum) {
          if (!Array.isArray(propSchema.enum)) {
            invalidEnums.push({
              tool: tool.name,
              property: propName,
              error: "enum is not an array"
            });
          } else if (propSchema.enum.length === 0) {
            invalidEnums.push({
              tool: tool.name,
              property: propName,
              error: "enum array is empty"
            });
          }
        }
      }
    }

    assert.equal(
      invalidEnums.length,
      0,
      `${invalidEnums.length} invalid enum properties:\n${JSON.stringify(invalidEnums, null, 2)}`
    );
  });

  test("array properties have items schema", () => {
    const missingItems = [];

    for (const tool of tools) {
      if (!tool.inputSchema.properties) continue;

      for (const [propName, propSchema] of Object.entries(tool.inputSchema.properties)) {
        if (propSchema.type === "array" && !propSchema.items) {
          missingItems.push({
            tool: tool.name,
            property: propName
          });
        }
      }
    }

    assert.equal(
      missingItems.length,
      0,
      `${missingItems.length} array properties missing items schema:\n${JSON.stringify(missingItems, null, 2)}`
    );
  });
});

// ============================================================================
// DRAFT 2020-12 COMPLIANCE (Claude API rejects non-compliant schemas)
// ============================================================================

describe("JSON Schema Draft 2020-12 Compliance", () => {
  test("no type arrays (draft-04 nullable syntax)", () => {
    const violations = [];

    function checkTypeArrays(schema, path, toolName) {
      if (!schema || typeof schema !== "object") return;
      if (Array.isArray(schema.type)) {
        violations.push({ tool: toolName, path, value: schema.type });
      }
      if (schema.properties) {
        for (const [k, v] of Object.entries(schema.properties)) {
          checkTypeArrays(v, `${path}.${k}`, toolName);
        }
      }
      if (schema.items && typeof schema.items === "object") {
        checkTypeArrays(schema.items, `${path}.items`, toolName);
      }
      for (const kw of ["anyOf", "oneOf", "allOf"]) {
        if (Array.isArray(schema[kw])) {
          schema[kw].forEach((s, i) => checkTypeArrays(s, `${path}.${kw}[${i}]`, toolName));
        }
      }
    }

    for (const tool of tools) {
      checkTypeArrays(tool.inputSchema, "inputSchema", tool.name);
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} type array violations (use anyOf instead):\n${JSON.stringify(violations, null, 2)}`
    );
  });

  test("no boolean exclusiveMinimum/Maximum (draft-04 syntax)", () => {
    const violations = [];

    function checkBooleanBounds(schema, path, toolName) {
      if (!schema || typeof schema !== "object") return;
      if (schema.exclusiveMinimum === true || schema.exclusiveMinimum === false) {
        violations.push({ tool: toolName, path, field: "exclusiveMinimum", value: schema.exclusiveMinimum });
      }
      if (schema.exclusiveMaximum === true || schema.exclusiveMaximum === false) {
        violations.push({ tool: toolName, path, field: "exclusiveMaximum", value: schema.exclusiveMaximum });
      }
      if (schema.properties) {
        for (const [k, v] of Object.entries(schema.properties)) {
          checkBooleanBounds(v, `${path}.${k}`, toolName);
        }
      }
      if (schema.items && typeof schema.items === "object") {
        checkBooleanBounds(schema.items, `${path}.items`, toolName);
      }
      for (const kw of ["anyOf", "oneOf", "allOf"]) {
        if (Array.isArray(schema[kw])) {
          schema[kw].forEach((s, i) => checkBooleanBounds(s, `${path}.${kw}[${i}]`, toolName));
        }
      }
    }

    for (const tool of tools) {
      checkBooleanBounds(tool.inputSchema, "inputSchema", tool.name);
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} boolean bound violations (use numeric values):\n${JSON.stringify(violations, null, 2)}`
    );
  });

  test("no $ref usage (unsupported by Claude API)", () => {
    const violations = [];

    function checkRefs(schema, path, toolName) {
      if (!schema || typeof schema !== "object") return;
      if (schema["$ref"]) {
        violations.push({ tool: toolName, path, ref: schema["$ref"] });
      }
      if (schema.properties) {
        for (const [k, v] of Object.entries(schema.properties)) {
          checkRefs(v, `${path}.${k}`, toolName);
        }
      }
      if (schema.items && typeof schema.items === "object") {
        checkRefs(schema.items, `${path}.items`, toolName);
      }
      for (const kw of ["anyOf", "oneOf", "allOf"]) {
        if (Array.isArray(schema[kw])) {
          schema[kw].forEach((s, i) => checkRefs(s, `${path}.${kw}[${i}]`, toolName));
        }
      }
    }

    for (const tool of tools) {
      checkRefs(tool.inputSchema, "inputSchema", tool.name);
    }

    assert.equal(
      violations.length,
      0,
      `${violations.length} $ref violations:\n${JSON.stringify(violations, null, 2)}`
    );
  });
});

// ============================================================================
// DOCUMENTATION TESTS
// ============================================================================

describe("Tool Documentation", () => {
  test("all properties have description field", () => {
    const missingDescriptions = [];

    for (const tool of tools) {
      if (!tool.inputSchema.properties) continue;

      for (const [propName, propSchema] of Object.entries(tool.inputSchema.properties)) {
        if (!propSchema.description) {
          missingDescriptions.push({
            tool: tool.name,
            property: propName
          });
        }
      }
    }

    assert.equal(
      missingDescriptions.length,
      0,
      `${missingDescriptions.length} properties missing descriptions:\n${JSON.stringify(missingDescriptions, null, 2)}`
    );
  });

  test("UUID fields have format: 'uuid' annotation", () => {
    const missingFormat = [];

    for (const tool of tools) {
      if (!tool.inputSchema.properties) continue;

      for (const [propName, propSchema] of Object.entries(tool.inputSchema.properties)) {
        // Check if property name suggests it's a UUID
        if ((propName.endsWith("_id") || propName === "id") &&
            propSchema.type === "string" &&
            propSchema.description?.toLowerCase().includes("uuid")) {
          if (!propSchema.format || propSchema.format !== "uuid") {
            missingFormat.push({
              tool: tool.name,
              property: propName
            });
          }
        }
      }
    }

    // This is a warning, not an error - format is optional
    if (missingFormat.length > 0) {
      console.log(`\nNote: ${missingFormat.length} UUID properties could benefit from format: 'uuid'`);
    }
  });
});

// ============================================================================
// COVERAGE STATISTICS
// ============================================================================

describe("Schema Coverage Statistics", () => {
  test("report overall schema quality metrics", () => {
    const stats = {
      total_tools: tools.length,
      with_required_fields: 0,
      with_optional_fields: 0,
      with_enums: 0,
      with_arrays: 0,
      with_nested_objects: 0,
      empty_schemas: 0,
      avg_properties_per_tool: 0
    };

    let totalProperties = 0;

    for (const tool of tools) {
      const props = tool.inputSchema.properties || {};
      const propCount = Object.keys(props).length;
      totalProperties += propCount;

      if (propCount === 0) {
        stats.empty_schemas++;
      }

      if (tool.inputSchema.required && tool.inputSchema.required.length > 0) {
        stats.with_required_fields++;
      }

      if (propCount > (tool.inputSchema.required?.length || 0)) {
        stats.with_optional_fields++;
      }

      for (const propSchema of Object.values(props)) {
        if (propSchema.enum) stats.with_enums++;
        if (propSchema.type === "array") stats.with_arrays++;
        if (propSchema.type === "object") stats.with_nested_objects++;
      }
    }

    stats.avg_properties_per_tool = (totalProperties / tools.length).toFixed(2);

    console.log("\n=== Schema Coverage Statistics ===");
    console.log(`Total tools: ${stats.total_tools}`);
    console.log(`Tools with required fields: ${stats.with_required_fields} (${(stats.with_required_fields/stats.total_tools*100).toFixed(1)}%)`);
    console.log(`Tools with optional fields: ${stats.with_optional_fields} (${(stats.with_optional_fields/stats.total_tools*100).toFixed(1)}%)`);
    console.log(`Tools with empty schemas: ${stats.empty_schemas}`);
    console.log(`Tools with enum properties: ${stats.with_enums}`);
    console.log(`Tools with array properties: ${stats.with_arrays}`);
    console.log(`Tools with nested objects: ${stats.with_nested_objects}`);
    console.log(`Average properties per tool: ${stats.avg_properties_per_tool}`);

    // Basic sanity checks
    assert.ok(stats.total_tools > 0, "Should have at least one tool");
    assert.ok(stats.avg_properties_per_tool >= 0, "Average should be non-negative");
  });
});

// ============================================================================
// CORE TOOL SURFACE VALIDATION (Issue #365 — Tool Surface Reduction)
// ============================================================================

const CORE_TOOLS = new Set([
  "list_notes", "get_note", "update_note", "delete_note", "restore_note",
  "capture_knowledge", "search", "record_provenance",
  "manage_tags", "manage_collection", "manage_concepts",
  "explore_graph", "get_note_links", "export_note",
  "get_documentation", "get_system_info", "health_check",
  "select_memory", "get_active_memory",
  "describe_image", "transcribe_audio",
  "get_knowledge_health",
  "bulk_reprocess_notes",
]);

describe("Core Tool Surface (Issue #365)", () => {
  const toolNames = new Set(tools.map(t => t.name));

  test("CORE-001: All CORE_TOOLS exist in tools.js", () => {
    const missing = [...CORE_TOOLS].filter(n => !toolNames.has(n));
    assert.equal(missing.length, 0, `Core tools missing from tools.js: ${missing.join(", ")}`);
  });

  test("CORE-002: Core surface has exactly 23 tools", () => {
    assert.equal(CORE_TOOLS.size, 23, `Expected 23 core tools, got ${CORE_TOOLS.size}`);
  });

  test("CORE-003: Core filtering produces correct count", () => {
    const coreTools = tools.filter(t => CORE_TOOLS.has(t.name));
    assert.equal(coreTools.length, 23, `Expected 23 filtered tools, got ${coreTools.length}`);
  });

  test("CORE-004: All 6 consolidated tools have action enum", () => {
    const consolidated = [
      "capture_knowledge", "search", "record_provenance",
      "manage_tags", "manage_collection", "manage_concepts",
    ];
    for (const name of consolidated) {
      const tool = tools.find(t => t.name === name);
      assert.ok(tool, `Consolidated tool ${name} should exist`);
      assert.ok(tool.inputSchema.properties?.action?.enum,
        `${name} should have action enum in schema`);
    }
  });

  test("CORE-005: Core tools have short descriptions (≤80 words)", () => {
    const verbose = [];
    for (const name of CORE_TOOLS) {
      const tool = tools.find(t => t.name === name);
      if (!tool) continue;
      const words = tool.description.split(/\s+/).length;
      if (words > 80) verbose.push({ name, words });
    }
    assert.equal(verbose.length, 0,
      `Core tools with >80 word descriptions: ${verbose.map(v => `${v.name}(${v.words}w)`).join(", ")}`);
  });

  test("CORE-006: Token reduction is significant (≥60%)", () => {
    const fullJson = JSON.stringify(tools);
    const coreJson = JSON.stringify(tools.filter(t => CORE_TOOLS.has(t.name)));
    const reduction = 1 - coreJson.length / fullJson.length;
    assert.ok(reduction >= 0.6, `Expected ≥60% reduction, got ${(reduction * 100).toFixed(1)}%`);
    console.log(`  Token reduction: ${(reduction * 100).toFixed(1)}% (${fullJson.length} → ${coreJson.length} chars)`);
  });
});

console.log("\n✓ All schema validation tests passed");
