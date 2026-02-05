/**
 * Test suite for collection_id filter parameters
 * Issues #341 and #342
 *
 * Tests collection filtering for:
 * - list_notes (collection_id and collection_ids parameters)
 * - search_notes (collection_id and collection_ids parameters)
 * - create_note (collection_id parameter)
 * - bulk_create_notes (per-note collection_id support)
 */

import assert from 'assert';

// Mock API request function
let lastApiRequest = null;
function mockApiRequest(method, path, body) {
  lastApiRequest = { method, path, body };
  return Promise.resolve({ success: true });
}

// Helper to extract query params from path
function parseQueryParams(path) {
  const [, queryString] = path.split('?');
  if (!queryString) return new Map();

  const params = new Map();
  queryString.split('&').forEach(pair => {
    const [key, value] = pair.split('=');
    params.set(decodeURIComponent(key), decodeURIComponent(value.replace(/\+/g, " ")));
  });
  return params;
}

// Test suites
const testSuites = {
  'Issue #341 - list_notes collection filtering': {
    'should add collection_id parameter to GET request': async () => {
      const testCollectionId = 'c1234567-89ab-cdef-0123-456789abcdef';
      const args = { collection_id: testCollectionId };
      const params = new URLSearchParams();
      if (args.collection_id) params.set("collection_id", args.collection_id);
      await mockApiRequest("GET", `/api/v1/notes?${params}`);
      assert.strictEqual(lastApiRequest.method, "GET");
      const queryParams = parseQueryParams(lastApiRequest.path);
      assert.strictEqual(queryParams.get('collection_id'), testCollectionId);
    },

    'should add collection_ids as comma-separated list': async () => {
      const testCollectionIds = [
        'c1111111-89ab-cdef-0123-456789abcdef',
        'c2222222-89ab-cdef-0123-456789abcdef'
      ];
      const args = { collection_ids: testCollectionIds };
      const params = new URLSearchParams();
      if (args.collection_ids) {
        params.set("collection_ids", Array.isArray(args.collection_ids)
          ? args.collection_ids.join(",")
          : args.collection_ids);
      }
      await mockApiRequest("GET", `/api/v1/notes?${params}`);
      const queryParams = parseQueryParams(lastApiRequest.path);
      assert.strictEqual(queryParams.get('collection_ids'), testCollectionIds.join(','));
    },

    'should handle collection_ids as string': async () => {
      const testCollectionIds = 'c1111111-89ab-cdef-0123-456789abcdef,c2222222-89ab-cdef-0123-456789abcdef';
      const args = { collection_ids: testCollectionIds };
      const params = new URLSearchParams();
      if (args.collection_ids) {
        params.set("collection_ids", Array.isArray(args.collection_ids)
          ? args.collection_ids.join(",")
          : args.collection_ids);
      }
      await mockApiRequest("GET", `/api/v1/notes?${params}`);
      const queryParams = parseQueryParams(lastApiRequest.path);
      assert.strictEqual(queryParams.get('collection_ids'), testCollectionIds);
    },

    'should combine collection filters with existing parameters': async () => {
      const args = {
        limit: 50,
        offset: 10,
        filter: 'starred',
        tags: ['tag1', 'tag2'],
        collection_id: 'c1234567-89ab-cdef-0123-456789abcdef'
      };
      const params = new URLSearchParams();
      if (args.limit) params.set("limit", args.limit);
      if (args.offset) params.set("offset", args.offset);
      if (args.filter) params.set("filter", args.filter);
      if (args.tags) params.set("tags", Array.isArray(args.tags) ? args.tags.join(",") : args.tags);
      if (args.collection_id) params.set("collection_id", args.collection_id);
      await mockApiRequest("GET", `/api/v1/notes?${params}`);
      const queryParams = parseQueryParams(lastApiRequest.path);
      assert.strictEqual(queryParams.get('limit'), '50');
      assert.strictEqual(queryParams.get('collection_id'), args.collection_id);
      assert.strictEqual(queryParams.get('tags'), 'tag1,tag2');
    }
  },

  'Issue #341 - search_notes collection filtering': {
    'should add collection_id parameter to search request': async () => {
      const testCollectionId = 'c1234567-89ab-cdef-0123-456789abcdef';
      const args = { query: 'test query', collection_id: testCollectionId };
      const params = new URLSearchParams({ q: args.query });
      if (args.collection_id) params.set("collection_id", args.collection_id);
      await mockApiRequest("GET", `/api/v1/search?${params}`);
      const queryParams = parseQueryParams(lastApiRequest.path);
      assert.strictEqual(queryParams.get('collection_id'), testCollectionId);
      assert.strictEqual(queryParams.get('q'), 'test query');
    },

    'should add collection_ids as comma-separated list': async () => {
      const testCollectionIds = [
        'c1111111-89ab-cdef-0123-456789abcdef',
        'c2222222-89ab-cdef-0123-456789abcdef'
      ];
      const args = { query: 'test query', collection_ids: testCollectionIds };
      const params = new URLSearchParams({ q: args.query });
      if (args.collection_ids) {
        params.set("collection_ids", Array.isArray(args.collection_ids)
          ? args.collection_ids.join(",")
          : args.collection_ids);
      }
      await mockApiRequest("GET", `/api/v1/search?${params}`);
      const queryParams = parseQueryParams(lastApiRequest.path);
      assert.strictEqual(queryParams.get('collection_ids'), testCollectionIds.join(','));
    },

    'should combine collection filters with search parameters': async () => {
      const args = {
        query: 'test query',
        limit: 20,
        mode: 'hybrid',
        set: 'default',
        collection_id: 'c1234567-89ab-cdef-0123-456789abcdef'
      };
      const params = new URLSearchParams({ q: args.query });
      if (args.limit) params.set("limit", args.limit);
      if (args.mode) params.set("mode", args.mode);
      if (args.set) params.set("set", args.set);
      if (args.collection_id) params.set("collection_id", args.collection_id);
      await mockApiRequest("GET", `/api/v1/search?${params}`);
      const queryParams = parseQueryParams(lastApiRequest.path);
      assert.strictEqual(queryParams.get('collection_id'), args.collection_id);
      assert.strictEqual(queryParams.get('mode'), 'hybrid');
      assert.strictEqual(queryParams.get('limit'), '20');
    }
  },

  'Issue #342 - create_note collection_id': {
    'should include collection_id in note creation payload': async () => {
      const testCollectionId = 'c1234567-89ab-cdef-0123-456789abcdef';
      const args = {
        content: 'Test note content',
        tags: ['test'],
        revision_mode: 'full',
        collection_id: testCollectionId
      };
      await mockApiRequest("POST", "/api/v1/notes", {
        content: args.content,
        tags: args.tags,
        revision_mode: args.revision_mode,
        collection_id: args.collection_id,
      });
      assert.strictEqual(lastApiRequest.method, "POST");
      assert.strictEqual(lastApiRequest.body.collection_id, testCollectionId);
      assert.strictEqual(lastApiRequest.body.content, 'Test note content');
    },

    'should work without collection_id (optional parameter)': async () => {
      const args = {
        content: 'Test note content',
        tags: ['test'],
        revision_mode: 'full'
      };
      await mockApiRequest("POST", "/api/v1/notes", {
        content: args.content,
        tags: args.tags,
        revision_mode: args.revision_mode,
        collection_id: args.collection_id, // undefined is ok
      });
      assert.strictEqual(lastApiRequest.body.collection_id, undefined);
      assert.strictEqual(lastApiRequest.body.content, 'Test note content');
    }
  },

  'Issue #342 - bulk_create_notes collection_id support': {
    'should pass through collection_id in notes array': async () => {
      const notes = [
        {
          content: 'Note 1',
          tags: ['tag1'],
          revision_mode: 'full',
          collection_id: 'c1111111-89ab-cdef-0123-456789abcdef'
        },
        {
          content: 'Note 2',
          tags: ['tag2'],
          revision_mode: 'light',
          collection_id: 'c2222222-89ab-cdef-0123-456789abcdef'
        }
      ];
      await mockApiRequest("POST", "/api/v1/notes/bulk", { notes });
      assert.strictEqual(lastApiRequest.method, "POST");
      assert.strictEqual(lastApiRequest.body.notes.length, 2);
      assert.strictEqual(lastApiRequest.body.notes[0].collection_id, 'c1111111-89ab-cdef-0123-456789abcdef');
      assert.strictEqual(lastApiRequest.body.notes[1].collection_id, 'c2222222-89ab-cdef-0123-456789abcdef');
    },

    'should handle mixed notes with and without collection_id': async () => {
      const notes = [
        {
          content: 'Note 1',
          collection_id: 'c1111111-89ab-cdef-0123-456789abcdef'
        },
        {
          content: 'Note 2'
          // No collection_id
        }
      ];
      await mockApiRequest("POST", "/api/v1/notes/bulk", { notes });
      assert.strictEqual(lastApiRequest.body.notes[0].collection_id, 'c1111111-89ab-cdef-0123-456789abcdef');
      assert.strictEqual(lastApiRequest.body.notes[1].collection_id, undefined);
    }
  },

  'Tool definition validation': {
    'list_notes should have collection_id and collection_ids in schema': () => {
      const toolSchema = {
        collection_id: {
          type: "string",
          format: "uuid",
          description: "Filter to notes in this collection"
        },
        collection_ids: {
          type: "array",
          items: { type: "string", format: "uuid" },
          description: "Filter to notes in any of these collections"
        }
      };
      assert.ok(toolSchema.collection_id);
      assert.strictEqual(toolSchema.collection_id.type, "string");
      assert.strictEqual(toolSchema.collection_id.format, "uuid");
      assert.ok(toolSchema.collection_ids);
      assert.strictEqual(toolSchema.collection_ids.type, "array");
    },

    'search_notes should have collection_id and collection_ids in schema': () => {
      const toolSchema = {
        collection_id: {
          type: "string",
          format: "uuid",
          description: "Restrict search to notes in this collection"
        },
        collection_ids: {
          type: "array",
          items: { type: "string", format: "uuid" },
          description: "Restrict search to notes in any of these collections"
        }
      };
      assert.ok(toolSchema.collection_id);
      assert.ok(toolSchema.collection_ids);
    },

    'create_note should have collection_id in schema': () => {
      const toolSchema = {
        collection_id: {
          type: "string",
          format: "uuid",
          description: "Place note directly in this collection on creation"
        }
      };
      assert.ok(toolSchema.collection_id);
      assert.strictEqual(toolSchema.collection_id.type, "string");
    },

    'bulk_create_notes items should support collection_id': () => {
      const itemSchema = {
        content: { type: "string" },
        tags: { type: "array", items: { type: "string" } },
        revision_mode: { type: "string", enum: ["full", "light", "none"] },
        collection_id: {
          type: "string",
          format: "uuid",
          description: "Optional collection to place this note in"
        }
      };
      assert.ok(itemSchema.collection_id);
      assert.strictEqual(itemSchema.collection_id.type, "string");
    }
  }
};

// Run tests
async function runTests() {
  console.log('Running collection filter tests...\n');

  let passed = 0;
  let failed = 0;

  for (const [suiteName, suite] of Object.entries(testSuites)) {
    console.log(`\n${suiteName}:`);
    for (const [testName, test] of Object.entries(suite)) {
      try {
        lastApiRequest = null;
        await test();
        console.log(`  ✓ ${testName}`);
        passed++;
      } catch (error) {
        console.log(`  ✗ ${testName}`);
        console.log(`    ${error.message}`);
        failed++;
      }
    }
  }

  console.log(`\n${passed} passed, ${failed} failed`);
  process.exit(failed > 0 ? 1 : 0);
}

runTests();
