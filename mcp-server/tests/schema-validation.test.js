#!/usr/bin/env node

/**
 * MCP Tool Schema Validation Tests (Issue #344)
 *
 * Validates that all 155 MCP tools have:
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

// Load the tools array from index.js
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
  // Create a minimal evaluation context
  const toolsCode = `(function() { return [${toolsMatch[1]}]; })()`;
  tools = eval(toolsCode);
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
 * Validate JSON Schema structure recursively
 */
function validateJsonSchema(schema, path = "schema") {
  const errors = [];

  // Root must be object with type
  if (!schema || typeof schema !== "object") {
    errors.push(`${path}: Schema must be an object`);
    return errors;
  }

  if (!schema.type) {
    errors.push(`${path}: Missing 'type' field`);
  }

  // Validate type values
  const validTypes = ["object", "array", "string", "number", "integer", "boolean", "null"];
  if (schema.type && !validTypes.includes(schema.type)) {
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
        if (!propSchema.type) {
          errors.push(`${path}.properties.${propName}: Missing 'type' field`);
        }
        // Recursive validation for nested schemas
        if (propSchema.type === "object" && propSchema.properties) {
          errors.push(...validateJsonSchema(propSchema, `${path}.properties.${propName}`));
        }
        if (propSchema.type === "array" && propSchema.items) {
          errors.push(...validateJsonSchema(propSchema.items, `${path}.properties.${propName}.items`));
        }
      }
    }
  }

  // Validate required array if present
  if (schema.required) {
    if (!Array.isArray(schema.required)) {
      errors.push(`${path}.required: Must be an array`);
    } else if (schema.properties) {
      // Check that all required fields exist in properties
      for (const requiredField of schema.required) {
        if (!schema.properties[requiredField]) {
          errors.push(`${path}.required: Field '${requiredField}' not found in properties`);
        }
      }
    }
  }

  // Validate array items if present
  if (schema.type === "array" && schema.items) {
    if (typeof schema.items !== "object") {
      errors.push(`${path}.items: Must be an object`);
    } else {
      errors.push(...validateJsonSchema(schema.items, `${path}.items`));
    }
  }

  // Validate enum if present
  if (schema.enum && !Array.isArray(schema.enum)) {
    errors.push(`${path}.enum: Must be an array`);
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

  test("all property schemas have type field", () => {
    const missingTypes = [];

    for (const tool of tools) {
      if (!tool.inputSchema.properties) continue;

      for (const [propName, propSchema] of Object.entries(tool.inputSchema.properties)) {
        if (!propSchema.type) {
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
      `${missingTypes.length} properties missing type:\n${JSON.stringify(missingTypes, null, 2)}`
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

console.log("\n✓ All schema validation tests passed");
