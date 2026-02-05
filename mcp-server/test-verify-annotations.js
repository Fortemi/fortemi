#!/usr/bin/env node

/**
 * Comprehensive MCP tool annotations verification test
 *
 * Validates that all 97 tools have proper MCP annotations following the
 * permission classification matrix.
 *
 * Issues: #223, #360, #345
 *
 * Tier Classification:
 * - Tier 1 (Read-Only): readOnlyHint: true - Auto-approved in all modes
 * - Tier 2 (Non-Destructive Write): destructiveHint: false - Auto-approved in acceptEdits
 * - Tier 3 (Soft Delete): destructiveHint: false - Recoverable deletion
 * - Tier 4 (Destructive): destructiveHint: true - Always requires approval
 */

import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const INDEX_JS_PATH = join(__dirname, 'index.js');

// ==============================================================================
// TOOL CLASSIFICATION MATRIX
// ==============================================================================

// Tier 1: Read-Only tools (readOnlyHint: true)
// These tools don't modify any state - safe to auto-approve in all modes
const READ_ONLY_TOOLS = [
  'list_notes',
  'get_note',
  'search_notes',
  'list_tags',
  'get_note_links',
  'export_note',
  'list_collections',
  'get_collection',
  'get_collection_notes',
  'explore_graph',
  'list_templates',
  'get_template',
  'list_embedding_sets',
  'get_embedding_set',
  'list_set_members',
      'export_all_notes',
  'backup_status',
  'backup_download',
  'knowledge_archive_download',
  'list_backups',
  'get_backup_info',
  'get_backup_metadata',
  'memory_info',
  'list_concept_schemes',
  'get_concept_scheme',
  'search_concepts',
  'get_concept',
  'get_concept_full',
  'autocomplete_concepts',
  'get_broader',
  'get_narrower',
  'get_related',
  'get_note_concepts',
  'get_governance_stats',
  'get_top_concepts',
  'list_note_versions',
  'get_note_version',
  'diff_note_versions',
  'get_full_document',
  'search_with_dedup',
  'get_chunk_chain',
  'get_documentation',
  'pke_get_address',
  'pke_list_recipients',
  'pke_verify_address',
  'pke_list_keysets',
  'pke_get_active_keyset',
  'list_jobs',
  'get_queue_stats',
];

// Tier 2: Non-Destructive Write tools (destructiveHint: false)
// These create/modify data but changes are recoverable or don't cause data loss
const NON_DESTRUCTIVE_WRITE_TOOLS = [
  'create_note',
  'bulk_create_notes',
  'update_note',
  'set_note_tags',
  'create_collection',
  'move_note_to_collection',
  'create_template',
  'instantiate_template',
  'create_embedding_set',
  'add_set_members',
  'refresh_embedding_set',
  'backup_now',
  'knowledge_shard',
  'database_snapshot',
  'knowledge_archive_upload',
  'update_backup_metadata',
  'create_concept_scheme',
  'create_concept',
  'update_concept',
  'add_broader',
  'add_narrower',
  'add_related',
        'tag_note_concept',
  'untag_note_concept',
  'restore_note_version',
  'pke_generate_keypair',
  'pke_encrypt',
  'pke_decrypt',
  'pke_create_keyset',
  'pke_set_active_keyset',
  'pke_export_keyset',
  'create_job',
];

// Tier 3: Soft Delete tools (destructiveHint: false)
// Marks as deleted but recoverable
const SOFT_DELETE_TOOLS = [
  'delete_note',
  'delete_collection',
  'delete_template',
];

// Tier 4: Destructive tools (destructiveHint: true)
// Permanent data loss or irreversible state changes - ALWAYS requires approval
const DESTRUCTIVE_TOOLS = [
  'purge_note',
  'purge_notes',
  'purge_all_notes',
  'remove_set_member',
  'delete_concept',
  'delete_note_version',
  'database_restore',
  'backup_import',
  'knowledge_shard_import',
  'pke_delete_keyset',
  'pke_import_keyset',
];

// ==============================================================================
// TEST EXECUTION
// ==============================================================================

console.log('MCP Tool Annotations Verification');
console.log('==================================\n');

function extractToolsFromFile(content) {
  const toolsMatch = content.match(/const tools = \[([\s\S]+?)\n\];/);
  if (!toolsMatch) {
    throw new Error('Could not find tools array in index.js');
  }
  return toolsMatch[1];
}

function getToolAnnotation(toolsContent, toolName) {
  // Match the entire tool object and extract annotations
  // This regex captures: { name: "toolName", ..., inputSchema: {...}, annotations?: {...} }
  const toolPattern = new RegExp(
    `\\{[\\s\\S]*?name:\\s*"${toolName}"[\\s\\S]*?inputSchema:\\s*\\{[\\s\\S]*?\\}[\\s\\S]*?\\},[\\s\\S]*?(annotations:\\s*\\{([^}]*)\\})?[\\s\\S]*?\\},`,
    'm'
  );

  // Better approach: find the tool block by looking for name and finding the next annotations
  const lines = toolsContent.split('\n');
  let inTool = false;
  let foundName = false;
  let depth = 0;
  let annotationsBlock = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (line.includes(`name: "${toolName}"`)) {
      foundName = true;
      inTool = true;
      continue;
    }

    if (foundName && !annotationsBlock) {
      // Look for annotations: { ... }
      if (line.includes('annotations:')) {
        // Extract the annotations object
        let annotationStr = '';
        let braceCount = 0;
        for (let j = i; j < lines.length && j < i + 10; j++) {
          annotationStr += lines[j];
          braceCount += (lines[j].match(/\{/g) || []).length;
          braceCount -= (lines[j].match(/\}/g) || []).length;
          if (braceCount === 0 && annotationStr.includes('{')) break;
        }

        // Parse the annotations
        const readOnlyMatch = annotationStr.match(/readOnlyHint:\s*(true|false)/);
        const destructiveMatch = annotationStr.match(/destructiveHint:\s*(true|false)/);
        const idempotentMatch = annotationStr.match(/idempotentHint:\s*(true|false)/);

        annotationsBlock = {
          found: true,
          readOnlyHint: readOnlyMatch ? readOnlyMatch[1] === 'true' : undefined,
          destructiveHint: destructiveMatch ? destructiveMatch[1] === 'true' : undefined,
          idempotentHint: idempotentMatch ? idempotentMatch[1] === 'true' : undefined,
        };
        break;
      }

      // If we hit the next tool definition before finding annotations, there are none
      if (line.includes('name: "') && !line.includes(toolName)) {
        break;
      }
    }
  }

  return annotationsBlock || { found: false };
}

try {
  const content = readFileSync(INDEX_JS_PATH, 'utf8');
  const toolsContent = extractToolsFromFile(content);

  let passCount = 0;
  let failCount = 0;
  const failures = [];

  // Build complete tool list
  const allExpectedTools = [
    ...READ_ONLY_TOOLS,
    ...NON_DESTRUCTIVE_WRITE_TOOLS,
    ...SOFT_DELETE_TOOLS,
    ...DESTRUCTIVE_TOOLS,
  ];

  console.log(`Expected tools: ${allExpectedTools.length}`);
  console.log(`  - Read-only: ${READ_ONLY_TOOLS.length}`);
  console.log(`  - Non-destructive write: ${NON_DESTRUCTIVE_WRITE_TOOLS.length}`);
  console.log(`  - Soft delete: ${SOFT_DELETE_TOOLS.length}`);
  console.log(`  - Destructive: ${DESTRUCTIVE_TOOLS.length}`);
  console.log();

  // Check for duplicate classifications
  const allTools = [...allExpectedTools];
  const duplicates = allTools.filter((tool, index) => allTools.indexOf(tool) !== index);
  if (duplicates.length > 0) {
    console.error('ERROR: Duplicate tool classifications detected:', duplicates);
    process.exit(1);
  }

  // Verify Read-Only tools have readOnlyHint: true
  console.log('\n--- Tier 1: Read-Only Tools (readOnlyHint: true) ---');
  for (const toolName of READ_ONLY_TOOLS) {
    const annotation = getToolAnnotation(toolsContent, toolName);
    if (!annotation.found) {
      console.error(`  FAIL: ${toolName} - missing annotations`);
      failures.push({ tool: toolName, reason: 'missing annotations' });
      failCount++;
    } else if (annotation.readOnlyHint !== true) {
      console.error(`  FAIL: ${toolName} - expected readOnlyHint: true, got: ${annotation.readOnlyHint}`);
      failures.push({ tool: toolName, reason: `expected readOnlyHint: true, got: ${annotation.readOnlyHint}` });
      failCount++;
    } else {
      console.log(`  PASS: ${toolName}`);
      passCount++;
    }
  }

  // Verify Non-Destructive Write tools have destructiveHint: false
  console.log('\n--- Tier 2: Non-Destructive Write Tools (destructiveHint: false) ---');
  for (const toolName of NON_DESTRUCTIVE_WRITE_TOOLS) {
    const annotation = getToolAnnotation(toolsContent, toolName);
    if (!annotation.found) {
      console.error(`  FAIL: ${toolName} - missing annotations`);
      failures.push({ tool: toolName, reason: 'missing annotations' });
      failCount++;
    } else if (annotation.destructiveHint !== false) {
      console.error(`  FAIL: ${toolName} - expected destructiveHint: false, got: ${annotation.destructiveHint}`);
      failures.push({ tool: toolName, reason: `expected destructiveHint: false, got: ${annotation.destructiveHint}` });
      failCount++;
    } else {
      console.log(`  PASS: ${toolName}`);
      passCount++;
    }
  }

  // Verify Soft Delete tools have destructiveHint: false
  console.log('\n--- Tier 3: Soft Delete Tools (destructiveHint: false) ---');
  for (const toolName of SOFT_DELETE_TOOLS) {
    const annotation = getToolAnnotation(toolsContent, toolName);
    if (!annotation.found) {
      console.error(`  FAIL: ${toolName} - missing annotations`);
      failures.push({ tool: toolName, reason: 'missing annotations' });
      failCount++;
    } else if (annotation.destructiveHint !== false) {
      console.error(`  FAIL: ${toolName} - expected destructiveHint: false, got: ${annotation.destructiveHint}`);
      failures.push({ tool: toolName, reason: `expected destructiveHint: false, got: ${annotation.destructiveHint}` });
      failCount++;
    } else {
      console.log(`  PASS: ${toolName}`);
      passCount++;
    }
  }

  // Verify Destructive tools have destructiveHint: true
  console.log('\n--- Tier 4: Destructive Tools (destructiveHint: true) ---');
  for (const toolName of DESTRUCTIVE_TOOLS) {
    const annotation = getToolAnnotation(toolsContent, toolName);
    if (!annotation.found) {
      console.error(`  FAIL: ${toolName} - missing annotations`);
      failures.push({ tool: toolName, reason: 'missing annotations' });
      failCount++;
    } else if (annotation.destructiveHint !== true) {
      console.error(`  FAIL: ${toolName} - expected destructiveHint: true, got: ${annotation.destructiveHint}`);
      failures.push({ tool: toolName, reason: `expected destructiveHint: true, got: ${annotation.destructiveHint}` });
      failCount++;
    } else {
      console.log(`  PASS: ${toolName}`);
      passCount++;
    }
  }

  // Summary
  console.log(`\n${'='.repeat(60)}`);
  console.log(`Results: ${passCount} passed, ${failCount} failed of ${allExpectedTools.length} tools`);
  console.log(`${'='.repeat(60)}`);

  if (failures.length > 0) {
    console.log('\nFailures:');
    for (const f of failures) {
      console.log(`  - ${f.tool}: ${f.reason}`);
    }
    console.error('\nTest suite FAILED');
    process.exit(1);
  }

  console.log('\nAll annotations verified successfully!');
  process.exit(0);

} catch (error) {
  console.error('\nError running verification:', error.message);
  process.exit(1);
}
