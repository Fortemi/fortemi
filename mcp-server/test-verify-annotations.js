#!/usr/bin/env node

/**
 * Verify MCP tool annotations are correctly set
 *
 * This test validates that all tools requiring annotations
 * for agentic operation have them properly configured.
 *
 * Issue: #360, #345
 */

import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import assert from 'node:assert/strict';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const INDEX_JS_PATH = join(__dirname, 'index.js');

// Tools that MUST have annotations: { destructiveHint: false }
const REQUIRED_ANNOTATIONS = [
  'delete_note',
  'create_job',
  'list_jobs',
  'get_queue_stats',
  'delete_collection',
  'delete_template',
  'purge_note',
  'purge_notes',
  'purge_all_notes',
];

console.log('Verifying MCP Tool Annotations');
console.log('==============================\n');

try {
  // Parse the tools from index.js
  const content = readFileSync(INDEX_JS_PATH, 'utf8');

  // Extract tools array (simple regex-based extraction)
  const toolsMatch = content.match(/const tools = \[([\s\S]+?)\n\];/);
  if (!toolsMatch) {
    throw new Error('Could not find tools array in index.js');
  }

  const toolsContent = toolsMatch[1];

  let passCount = 0;
  let failCount = 0;

  for (const toolName of REQUIRED_ANNOTATIONS) {
    // Find the tool definition
    const toolRegex = new RegExp(
      `\\{[^}]*name: "${toolName}",[\\s\\S]*?annotations: \\{[\\s\\S]*?destructiveHint: (true|false)`,
      'm'
    );

    const match = toolsContent.match(toolRegex);

    if (!match) {
      console.error(`❌ FAIL: ${toolName} - missing annotations`);
      failCount++;
      continue;
    }

    const destructiveHint = match[1] === 'true';

    if (destructiveHint) {
      console.error(`❌ FAIL: ${toolName} - destructiveHint should be false, got true`);
      failCount++;
    } else {
      console.log(`✓ PASS: ${toolName} - has destructiveHint: false`);
      passCount++;
    }
  }

  console.log(`\n${'='.repeat(50)}`);
  console.log(`Results: ${passCount} passed, ${failCount} failed`);
  console.log(`${'='.repeat(50)}\n`);

  if (failCount > 0) {
    console.error('❌ Test suite FAILED');
    process.exit(1);
  }

  console.log('✓ All annotations verified successfully');
  process.exit(0);

} catch (error) {
  console.error('\n❌ Error running verification:', error.message);
  process.exit(1);
}
