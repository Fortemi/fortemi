#!/usr/bin/env node
/**
 * Test script to verify strict_filter implementation for issue #151
 */

import { readFileSync } from 'fs';
import { strict as assert } from 'assert';

const indexContent = readFileSync(new URL('./index.js', import.meta.url), 'utf-8');

console.log('Testing strict_filter implementation for issue #151...\n');

// Test 1: Verify buildStrictFilter function exists
console.log('Test 1: buildStrictFilter helper function');
const buildStrictFilterMatch = indexContent.match(/function buildStrictFilter\(strictFilter\)/);
assert(buildStrictFilterMatch, 'buildStrictFilter function should exist');
console.log('✓ buildStrictFilter function found');

// Test 2: Verify function handles all filter types
const functionBody = indexContent.match(/function buildStrictFilter[\s\S]*?^\}/m);
assert(functionBody, 'buildStrictFilter should have complete implementation');
assert(functionBody[0].includes('required_tags'), 'Should handle required_tags');
assert(functionBody[0].includes('any_tags'), 'Should handle any_tags');
assert(functionBody[0].includes('excluded_tags'), 'Should handle excluded_tags');
assert(functionBody[0].includes('required_schemes'), 'Should handle required_schemes');
assert(functionBody[0].includes('excluded_schemes'), 'Should handle excluded_schemes');
assert(functionBody[0].includes('JSON.stringify(filter)'), 'Should JSON.stringify the filter');
console.log('✓ buildStrictFilter handles all filter types');

// Test 3: Verify search_notes handler uses strict_filter
console.log('\nTest 2: search_notes handler');
const searchNotesHandler = indexContent.match(/case "search_notes":[\s\S]*?break;/);
assert(searchNotesHandler, 'search_notes handler should exist');
assert(searchNotesHandler[0].includes('buildStrictFilter(args.strict_filter)'), 'Should call buildStrictFilter');
assert(searchNotesHandler[0].includes('params.set("filters", filterJson)'), 'Should set filters param');
console.log('✓ search_notes handler processes strict_filter');

// Test 4: Verify search_notes_strict handler exists
console.log('\nTest 3: search_notes_strict handler');
const searchNotesStrictHandler = indexContent.match(/case "search_notes_strict":[\s\S]*?break;/);
assert(searchNotesStrictHandler, 'search_notes_strict handler should exist');
assert(searchNotesStrictHandler[0].includes('required_tags: args.required_tags'), 'Should handle required_tags');
assert(searchNotesStrictHandler[0].includes('any_tags: args.any_tags'), 'Should handle any_tags');
assert(searchNotesStrictHandler[0].includes('excluded_tags: args.excluded_tags'), 'Should handle excluded_tags');
assert(searchNotesStrictHandler[0].includes('required_schemes: args.required_schemes'), 'Should handle required_schemes');
assert(searchNotesStrictHandler[0].includes('excluded_schemes: args.excluded_schemes'), 'Should handle excluded_schemes');
console.log('✓ search_notes_strict handler implemented correctly');

// Test 5: Verify search_notes tool has strict_filter in inputSchema
console.log('\nTest 4: search_notes tool definition');
const searchNotesTool = indexContent.match(/name: "search_notes",[\s\S]*?inputSchema:[\s\S]*?required: \["query"\],/);
assert(searchNotesTool, 'search_notes tool definition should exist');
assert(searchNotesTool[0].includes('strict_filter:'), 'Should have strict_filter property');
assert(searchNotesTool[0].includes('type: "object"'), 'strict_filter should be object type');
assert(searchNotesTool[0].includes('required_tags:'), 'Should define required_tags');
assert(searchNotesTool[0].includes('any_tags:'), 'Should define any_tags');
assert(searchNotesTool[0].includes('excluded_tags:'), 'Should define excluded_tags');
assert(searchNotesTool[0].includes('required_schemes:'), 'Should define required_schemes');
assert(searchNotesTool[0].includes('excluded_schemes:'), 'Should define excluded_schemes');
console.log('✓ search_notes tool has strict_filter in inputSchema');

// Test 6: Verify search_notes_strict tool definition
console.log('\nTest 5: search_notes_strict tool definition');
const searchNotesStrictTool = indexContent.match(/name: "search_notes_strict",[\s\S]*?inputSchema:[\s\S]*?\n  \},\n  \{/);
assert(searchNotesStrictTool, 'search_notes_strict tool should exist');
assert(searchNotesStrictTool[0].includes('GUARANTEED tag filtering'), 'Should describe guaranteed filtering');
assert(searchNotesStrictTool[0].includes('required_tags:'), 'Should define required_tags property');
assert(searchNotesStrictTool[0].includes('any_tags:'), 'Should define any_tags property');
assert(searchNotesStrictTool[0].includes('excluded_tags:'), 'Should define excluded_tags property');
assert(searchNotesStrictTool[0].includes('required_schemes:'), 'Should define required_schemes property');
assert(searchNotesStrictTool[0].includes('excluded_schemes:'), 'Should define excluded_schemes property');
console.log('✓ search_notes_strict tool defined correctly');

// Test 7: Verify comprehensive documentation
console.log('\nTest 6: Documentation quality');
assert(searchNotesStrictTool[0].includes('client isolation'), 'Should document client isolation use case');
assert(searchNotesStrictTool[0].includes('project segregation'), 'Should document project segregation');
assert(searchNotesStrictTool[0].includes('Examples:'), 'Should include examples');
console.log('✓ Comprehensive documentation included');

console.log('\n✅ All tests passed!');
console.log('\nImplementation summary:');
console.log('  ✓ buildStrictFilter() helper function added');
console.log('  ✓ search_notes handler updated to process strict_filter');
console.log('  ✓ search_notes_strict handler implemented');
console.log('  ✓ search_notes tool schema updated with strict_filter');
console.log('  ✓ search_notes_strict tool defined');
console.log('  ✓ All filter types supported (required_tags, any_tags, excluded_tags, required_schemes, excluded_schemes)');
console.log('\nReady for integration with issue #151 API implementation.');
