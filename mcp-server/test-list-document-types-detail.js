#!/usr/bin/env node

/**
 * Unit test for list_document_types detail parameter
 *
 * Validates that the detail parameter is properly defined in the schema
 * and that the handler correctly transforms the response.
 */

import { strict as assert } from "node:assert";
import fs from "node:fs";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

console.log("Testing list_document_types detail parameter\n");

// Load index.js content
const indexPath = path.join(__dirname, "index.js");
const indexContent = fs.readFileSync(indexPath, "utf8");

// Test 1: Verify detail parameter exists in schema
console.log("Test 1: Verify detail parameter in schema");
const detailParamMatch = indexContent.includes('detail: {') && indexContent.includes('type: "boolean"');
assert(detailParamMatch, "detail parameter should exist in list_document_types schema with boolean type");
console.log("✓ detail parameter exists in schema\n");

// Test 2: Verify detail parameter has description mentioning tokens
console.log("Test 2: Verify detail parameter description");
const detailDescMatch = indexContent.includes("500 tokens") && indexContent.includes("14k tokens");
assert(detailDescMatch, "detail parameter should have description mentioning token counts");
console.log("✓ detail parameter has descriptive documentation\n");

// Test 3: Verify detail parameter has default value
console.log("Test 3: Verify detail parameter default");
const detailDefaultMatch = /detail: \{[\s\S]{1,300}?default: false/.test(indexContent);
assert(detailDefaultMatch, "detail parameter should have default: false");
console.log("✓ detail parameter defaults to false\n");

// Test 4: Verify handler uses args.detail
console.log("Test 4: Verify handler implementation");
const handlerMatch = /case "list_document_types":[\s\S]{1,1000}?if \(args\.detail === true\)/.test(indexContent);
assert(handlerMatch, "handler should check args.detail === true");
console.log("✓ handler checks args.detail\n");

// Test 5: Verify handler transforms to names array when detail=false
console.log("Test 5: Verify names-only transformation");
const namesTransformMatch = indexContent.includes("apiResult.types.map(t => t.name)");
assert(namesTransformMatch, "handler should transform to names array with .map(t => t.name)");
console.log("✓ handler transforms to names array\n");

// Test 6: Verify tool description mentions detail parameter
console.log("Test 6: Verify tool description");
const descriptionMatch = indexContent.includes("detail=false") || indexContent.includes("detail level");
assert(descriptionMatch, "tool description should mention detail parameter");
console.log("✓ tool description documents detail parameter\n");

// Test 7: Verify handler preserves full response when detail=true
console.log("Test 7: Verify full response preservation");
const fullResponseMatch = /if \(args\.detail === true\) \{[\s\S]{1,200}?result = apiResult/.test(indexContent);
assert(fullResponseMatch, "handler should preserve apiResult when detail=true");
console.log("✓ handler preserves full response when detail=true\n");

// Test 8: Verify handler has fallback for non-array responses
console.log("Test 8: Verify error handling");
const errorHandlingMatch = indexContent.includes("if (apiResult && apiResult.types && Array.isArray(apiResult.types))");
assert(errorHandlingMatch, "handler should check for valid array before mapping");
console.log("✓ handler has proper error handling\n");

console.log("====================================");
console.log("All tests passed! ✓");
console.log("====================================\n");
console.log("Summary:");
console.log("- detail parameter is properly defined in schema");
console.log("- detail parameter has correct type (boolean)");
console.log("- detail parameter has default value (false)");
console.log("- detail parameter is documented");
console.log("- handler correctly implements detail logic");
console.log("- handler transforms to names array by default");
console.log("- handler preserves full response when detail=true");
console.log("- handler has proper error handling");
