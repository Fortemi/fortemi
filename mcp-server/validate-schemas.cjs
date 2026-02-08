#!/usr/bin/env node
// Validate MCP tool schemas against JSON Schema draft 2020-12
//
// Usage: node validate-schemas.cjs [path/to/index.js]
//
// Catches the violations that cause Claude API "JSON schema is invalid" errors:
//   - type arrays: type: ["string", "null"]
//   - boolean exclusiveMinimum/Maximum
//   - $ref usage
//   - invalid type values
//   - tuple-form items arrays
//
// Also runs AJV 2020-12 validation if available (from @modelcontextprotocol/sdk deps).

const fs = require('fs');
const path = require('path');

const toolsPath = process.argv[2] || path.resolve(__dirname, 'tools.js');
const src = fs.readFileSync(toolsPath, 'utf8');

// Find the array start — supports both 'export default [' and 'const tools = ['
let marker = 'export default [';
let startIdx = src.indexOf(marker);
let markerLen = marker.length - 1; // offset to the '['
if (startIdx === -1) {
  marker = 'const tools = [';
  startIdx = src.indexOf(marker);
  markerLen = marker.length - 1;
}
if (startIdx === -1) {
  console.error('Could not find tools array in', toolsPath);
  process.exit(1);
}

let depth = 0;
let i = startIdx + markerLen;
let inStr = false;
let strChar = '';
let endIdx = -1;

while (i < src.length) {
  const c = src[i];

  if (inStr) {
    if (c === '\\') { i += 2; continue; }
    if (c === strChar) inStr = false;
    i++;
    continue;
  }

  if (c === '"' || c === "'" || c === '`') { inStr = true; strChar = c; i++; continue; }
  if (c === '[') depth++;
  if (c === ']') { depth--; if (depth === 0) { endIdx = i + 1; break; } }
  i++;
}

if (endIdx === -1) {
  console.error('Could not find end of tools array');
  process.exit(1);
}

// Extract the array literal and eval it
const arrayLiteral = src.substring(startIdx + markerLen, endIdx);
let tools;
try {
  tools = new Function('path', 'return ' + arrayLiteral)(path);
} catch (e) {
  console.error('Failed to parse tools array:', e.message);
  process.exit(1);
}

console.log(`Validating ${tools.length} tool schemas...\n`);

// Recursive schema validator for Claude API 2020-12 rules
function validateSchema(schema, schemaPath, issues) {
  if (!schema || typeof schema !== 'object') return;

  if (Array.isArray(schema.type)) {
    issues.push(`${schemaPath}: type is array ${JSON.stringify(schema.type)} — use anyOf instead`);
  }

  if (schema.exclusiveMinimum === true || schema.exclusiveMinimum === false) {
    issues.push(`${schemaPath}: exclusiveMinimum is boolean — must be a number (e.g. exclusiveMinimum: 0)`);
  }
  if (schema.exclusiveMaximum === true || schema.exclusiveMaximum === false) {
    issues.push(`${schemaPath}: exclusiveMaximum is boolean — must be a number`);
  }

  if (schema['$ref']) {
    issues.push(`${schemaPath}: uses $ref (not supported by Claude API)`);
  }

  if (schema.type && typeof schema.type === 'string') {
    const valid = ['string', 'number', 'integer', 'boolean', 'array', 'object', 'null'];
    if (!valid.includes(schema.type)) {
      issues.push(`${schemaPath}: invalid type "${schema.type}"`);
    }
  }

  if (schema.enum !== undefined && !Array.isArray(schema.enum)) {
    issues.push(`${schemaPath}: enum is not an array`);
  }

  if (schema.required !== undefined && !Array.isArray(schema.required)) {
    issues.push(`${schemaPath}: required is not an array`);
  }

  if (schema.properties) {
    for (const [key, val] of Object.entries(schema.properties)) {
      if (typeof val !== 'object' || val === null) {
        issues.push(`${schemaPath}.properties.${key}: value is not an object`);
      } else {
        validateSchema(val, `${schemaPath}.properties.${key}`, issues);
      }
    }
  }

  if (schema.items) {
    if (Array.isArray(schema.items)) {
      issues.push(`${schemaPath}.items: tuple form (array) — use prefixItems in 2020-12`);
    } else if (typeof schema.items === 'object') {
      validateSchema(schema.items, `${schemaPath}.items`, issues);
    }
  }

  for (const kw of ['anyOf', 'oneOf', 'allOf']) {
    if (Array.isArray(schema[kw])) {
      schema[kw].forEach((s, j) => validateSchema(s, `${schemaPath}.${kw}[${j}]`, issues));
    }
  }

  if (schema.additionalProperties && typeof schema.additionalProperties === 'object') {
    validateSchema(schema.additionalProperties, `${schemaPath}.additionalProperties`, issues);
  }
}

let totalIssues = 0;
tools.forEach((tool, idx) => {
  const issues = [];
  if (tool.inputSchema) {
    validateSchema(tool.inputSchema, 'inputSchema', issues);
  }
  if (issues.length > 0) {
    console.log(`FAIL [${idx}] ${tool.name}:`);
    issues.forEach(iss => console.log(`  ${iss}`));
    totalIssues += issues.length;
  }
});

// AJV deep validation (available via @modelcontextprotocol/sdk dependency)
try {
  const Ajv2020 = require('ajv/dist/2020');
  const ajv = new (Ajv2020.default || Ajv2020)({ strict: false, allErrors: true });

  tools.forEach((tool, idx) => {
    if (tool.inputSchema) {
      try {
        ajv.compile(tool.inputSchema);
      } catch (e) {
        console.log(`FAIL [${idx}] ${tool.name} (AJV): ${e.message.substring(0, 200)}`);
        totalIssues++;
      }
    }
  });
} catch (_) {
  // AJV not available — basic checks are sufficient
}

console.log(`\n${'='.repeat(50)}`);
if (totalIssues === 0) {
  console.log(`PASS: All ${tools.length} tool schemas valid (2020-12)`);
  process.exit(0);
} else {
  console.log(`FAIL: ${totalIssues} issue(s) in ${tools.length} tools`);
  process.exit(1);
}
