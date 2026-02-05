#!/usr/bin/env node

/**
 * Unit test to verify reembed_all tool is properly registered
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const indexPath = path.join(__dirname, 'index.js');

async function testToolExists() {
  console.log('Verifying reembed_all tool registration...\n');

  try {
    // Read index.js
    const content = fs.readFileSync(indexPath, 'utf8');

    // Check for case handler
    console.log('1. Checking case handler...');
    if (!content.includes('case "reembed_all":')) {
      throw new Error('FAIL: Case handler not found');
    }
    console.log('   ✓ Case handler found');

    // Check for tool definition
    console.log('\n2. Checking tool definition...');
    if (!content.includes('name: "reembed_all"')) {
      throw new Error('FAIL: Tool definition not found');
    }
    console.log('   ✓ Tool definition found');

    // Check for required parameters
    console.log('\n3. Checking parameters...');
    if (!content.includes('embedding_set_slug')) {
      throw new Error('FAIL: embedding_set_slug parameter not found');
    }
    console.log('   ✓ embedding_set_slug parameter found');

    if (!content.includes('force')) {
      throw new Error('FAIL: force parameter not found');
    }
    console.log('   ✓ force parameter found');

    // Check for API request
    console.log('\n4. Checking API integration...');
    const caseHandlerMatch = content.match(/case "reembed_all":[\s\S]*?break;/);
    if (!caseHandlerMatch) {
      throw new Error('FAIL: Cannot extract case handler');
    }

    const caseHandler = caseHandlerMatch[0];
    if (!caseHandler.includes('job_type: "re_embed_all"')) {
      throw new Error('FAIL: job_type not set correctly');
    }
    console.log('   ✓ job_type set to "re_embed_all"');

    if (!caseHandler.includes('POST", "/api/v1/jobs"')) {
      throw new Error('FAIL: API endpoint not correct');
    }
    console.log('   ✓ API endpoint correct (/api/v1/jobs)');

    if (!caseHandler.includes('payload.embedding_set')) {
      throw new Error('FAIL: embedding_set parameter not passed to payload');
    }
    console.log('   ✓ embedding_set parameter passed to API');

    if (!caseHandler.includes('payload.force')) {
      throw new Error('FAIL: force parameter not passed to payload');
    }
    console.log('   ✓ force parameter passed to API');

    console.log('\n✓ ALL CHECKS PASSED');
    console.log('\nSummary:');
    console.log('- Case handler is properly registered');
    console.log('- Tool definition is complete');
    console.log('- All required parameters are present');
    console.log('- API integration is correct');

  } catch (error) {
    console.error('\n✗ TEST FAILED');
    console.error(error.message || error);
    process.exit(1);
  }
}

// Run the test
testToolExists();
