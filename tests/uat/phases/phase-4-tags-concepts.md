# UAT Phase 4: Tags & Concepts

**Purpose**: Validate tag management and SKOS concept operations including listing, setting, and semantic tagging.

**Duration**: ~5 minutes

**Prerequisites**: Phase 1 completion (notes with tags must exist)

**Tools Tested**: `manage_tags` (5 actions), `manage_concepts` (6 actions)

> **MCP-First Requirement**: All tests in this phase use MCP tool calls exclusively. No direct HTTP requests are permitted. The `manage_tags` and `manage_concepts` tools provide complete access to the tagging and SKOS semantic layer.

---

## Test Cases: manage_tags

### TAG-001: List Tags

**Test ID**: TAG-001
**MCP Tool**: `manage_tags` (action: list)
**Description**: List all tags in the system

```javascript
const result = await useTool('manage_tags', {
  action: 'list'
});
```

**Expected Response**:
- Array of tag objects
- Each tag has `name` field
- Includes `uat/` prefixed tags from Phase 1

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Contains at least one `uat/` prefixed tag
- [ ] Tag structure includes name and metadata
- [ ] No duplicate tags in list

---

### TAG-002: Set Tags on Note

**Test ID**: TAG-002
**MCP Tool**: `manage_tags` (action: set)
**Description**: Replace tags on an existing note
**Store**: `tagged_note_id` (from Phase 1)

```javascript
const result = await useTool('manage_tags', {
  action: 'set',
  note_id: tagged_note_id,
  tags: ['uat/new-tag', 'uat/replaced']
});
```

**Expected Response**:
- Success response confirming tag update
- Returns updated note or confirmation

**Pass Criteria**:
- [ ] Returns success status
- [ ] No error messages
- [ ] Note ID matches input
- [ ] Tag update acknowledged

---

### TAG-003: Verify Tags Replaced

**Test ID**: TAG-003
**MCP Tool**: `get_note`
**Description**: Confirm tags were replaced (not appended)

```javascript
const result = await useTool('get_note', {
  id: tagged_note_id
});
```

**Expected Response**:
- Note object with `tags` array
- Contains only `['uat/new-tag', 'uat/replaced']`
- Old tags removed

**Pass Criteria**:
- [ ] Tags array matches expected
- [ ] Contains `uat/new-tag`
- [ ] Contains `uat/replaced`
- [ ] Old tags NOT present (replacement confirmed)

---

### TAG-004: Get Note Concepts

**Test ID**: TAG-004
**MCP Tool**: `manage_tags` (action: get_concepts)
**Description**: Retrieve SKOS concepts associated with a note

```javascript
const result = await useTool('manage_tags', {
  action: 'get_concepts',
  note_id: tagged_note_id
});
```

**Expected Response**:
- Array of concept objects
- May be empty if no concepts tagged
- Each concept has `id`, `pref_label` fields

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Structure matches concept schema
- [ ] No error if empty
- [ ] Concept IDs valid if present

---

### TAG-005: Invalid Action

**Test ID**: TAG-005
**MCP Tool**: `manage_tags` (action: nope)
**Description**: Validate error handling for invalid action
**Isolation**: Required

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

```javascript
const result = await useTool('manage_tags', {
  action: 'nope',
  note_id: tagged_note_id
});
```

**Expected Response**:
- Error response indicating invalid action
- HTTP 400 or validation error

**Pass Criteria**:
- [ ] Returns error (not success)
- [ ] Error message mentions invalid action
- [ ] Does not crash or hang
- [ ] Clear error response format

---

## Test Cases: manage_concepts

### CON-001: Search Concepts

**Test ID**: CON-001
**MCP Tool**: `manage_concepts` (action: search)
**Description**: Search the SKOS concept registry

```javascript
const result = await useTool('manage_concepts', {
  action: 'search'
});
```

**Expected Response**:
- Array of concept objects
- May be empty if no concepts exist
- Each concept has `id`, `pref_label`, `scheme_id`

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Structure matches concept schema
- [ ] No error if empty
- [ ] Includes scheme metadata

**Store**: `concept_id` (first concept ID, if any exist)

---

### CON-002: Autocomplete

**Test ID**: CON-002
**MCP Tool**: `manage_concepts` (action: autocomplete)
**Description**: Autocomplete concept suggestions

```javascript
const result = await useTool('manage_concepts', {
  action: 'autocomplete',
  q: 'test',
  limit: 5
});
```

**Expected Response**:
- Array of concept suggestions
- Results match prefix/partial query
- Limited to 5 results

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Result count ≤ 5
- [ ] Suggestions relevant to query
- [ ] Handles empty results gracefully

---

### CON-003: Stats

**Test ID**: CON-003
**MCP Tool**: `manage_concepts` (action: stats)
**Description**: Retrieve concept system statistics

```javascript
const result = await useTool('manage_concepts', {
  action: 'stats'
});
```

**Expected Response**:
- Object with concept counts
- Fields like `total_concepts`, `total_schemes`
- Numeric values ≥ 0

**Pass Criteria**:
- [ ] Returns valid JSON object
- [ ] Contains count fields
- [ ] All counts are numbers
- [ ] No negative values

---

### CON-004: Top Concepts

**Test ID**: CON-004
**MCP Tool**: `manage_concepts` (action: top)
**Description**: List most-used concepts

```javascript
const result = await useTool('manage_concepts', {
  action: 'top',
  limit: 10
});
```

**Expected Response**:
- Array of top concepts by usage
- May be empty if no usage data
- Includes usage count metadata

**Pass Criteria**:
- [ ] Returns valid JSON array
- [ ] Result count ≤ 10
- [ ] Ordered by usage (if multiple)
- [ ] Handles empty results gracefully

---

### CON-005: Get Concept

**Test ID**: CON-005
**MCP Tool**: `manage_concepts` (action: get)
**Description**: Retrieve single concept by ID
**Conditional**: Requires `concept_id` from CON-001

```javascript
if (concept_id) {
  const result = await useTool('manage_concepts', {
    action: 'get',
    id: concept_id
  });
}
```

**Expected Response**:
- Single concept object
- Contains `id`, `pref_label`, `definition` fields
- If no concepts exist, skip test gracefully

**Pass Criteria**:
- [ ] Returns valid JSON object (if concept exists)
- [ ] ID matches requested concept
- [ ] Includes core concept fields
- [ ] 404 error acceptable if no concepts exist

---

### CON-006: Get Full Concept

**Test ID**: CON-006
**MCP Tool**: `manage_concepts` (action: get_full)
**Description**: Retrieve concept with full SKOS hierarchy
**Conditional**: Requires `concept_id` from CON-001

```javascript
if (concept_id) {
  const result = await useTool('manage_concepts', {
    action: 'get_full',
    id: concept_id
  });
}
```

**Expected Response**:
- Concept object with expanded relationships
- Includes `broader`, `narrower`, `related` concepts
- Full SKOS semantic network

**Pass Criteria**:
- [ ] Returns valid JSON object (if concept exists)
- [ ] Contains relationship fields
- [ ] More detailed than `get` action
- [ ] 404 error acceptable if no concepts exist

---

### CON-007: Invalid Action

**Test ID**: CON-007
**MCP Tool**: `manage_concepts` (action: nope)
**Description**: Validate error handling for invalid action
**Isolation**: Required

> **STOP — ISOLATED CALL**: This test expects an error. Execute this MCP call ALONE in its own turn. Do NOT batch with other tool calls. See [Negative Test Isolation Protocol](README.md#negative-test-isolation-protocol).

```javascript
const result = await useTool('manage_concepts', {
  action: 'nope'
});
```

**Expected Response**:
- Error response indicating invalid action
- HTTP 400 or validation error

**Pass Criteria**:
- [ ] Returns error (not success)
- [ ] Error message mentions invalid action
- [ ] Does not crash or hang
- [ ] Clear error response format

---

## Phase 4 Summary

| Category | Count | Pass | Fail |
|----------|-------|------|------|
| Tag Management | 5 | - | - |
| Concept Search | 2 | - | - |
| Concept Stats | 2 | - | - |
| Concept Retrieval | 2 | - | - |
| Error Handling | 2 | - | - |
| **Total** | **12** | **-** | **-** |

**Phase 4 Result**: [ ] PASS [ ] FAIL

**Notes**:
- CON-005 and CON-006 are conditional on concepts existing in the system
- If no concepts exist, these tests should report PASS with "N/A - No concepts" note
- Tag operations assume Phase 1 seed data with `uat/` prefixed tags
- SKOS concept system may be empty in fresh deployments
- `manage_tags` action `set` replaces tags, does NOT append
