# UAT Phase 19: Feature Chains (E2E)

**Purpose**: Verify complete workflows that combine multiple system capabilities in realistic user scenarios
**Duration**: ~45 minutes
**Prerequisites**: All previous UAT phases executed (not necessarily all passed), test data available. Attempt every chain regardless of prior phase failures — record what fails and file issues.
**Critical**: Yes (100% pass required)
**Tools Tested**: `upload_attachment`, `create_note`, `get_note`, `detect_document_type`, `list_document_types`, `list_embedding_sets`, `get_embedding_set`, `search_notes`, `list_note_versions`, `restore_note_version`, `diff_note_versions`, `export_note`, `get_memory_provenance`, `search_memories_by_location`, `search_memories_by_time`, `search_memories_combined`, `create_concept_scheme`, `create_concept`, `add_broader`, `get_narrower`, `create_collection`, `tag_note_concept`, `move_note_to_collection`, `explore_graph`, `export_skos_turtle`, `knowledge_shard`, `pke_encrypt`, `pke_decrypt`, `pke_get_address`, `pke_create_keyset`, `database_snapshot`, `backup_status`, `delete_note`, `list_notes`, `database_restore`, `create_embedding_set`, `refresh_embedding_set`, `delete_collection`, `list_concept_schemes`, `get_knowledge_health`, `get_orphan_tags`, `get_stale_notes`, `get_unlinked_notes`, `health_check`, `reembed_all`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Overview

This phase tests end-to-end workflows that chain together 3+ features. Each chain exercises a realistic user scenario that demonstrates the system working as an integrated whole, not just isolated features.

**Test Methodology**:
- Use MCP tool calls: `tool_name({param: value})`
- Each chain includes setup, execution, verification, and cleanup
- Store intermediate IDs for cross-chain verification
- Expected results follow each tool call

---

## Chain 1: Document Lifecycle

**Scenario**: Upload code file → Detect type → Create note → AI revision → Embed → Search → Version → Export

**Duration**: ~6 minutes

---

### CHAIN-001: Upload Python Code File

**MCP Tools**: `create_note`, `upload_attachment`

**Description**: Create a note and upload a Python source file as attachment using two-step upload flow

**Prerequisites**:
- MCP server running
- Test file exists: `tests/uat/data/documents/code-python.py`

**Steps**:
```javascript
// 1. Create note first
create_note({
  content: "# Python Code Sample",
  tags: ["uat/chain1", "python", "code"]
})
// Expected: returns note_id

// 2. Get upload command (two-step upload — see Phase 2b)
upload_attachment({
  note_id: "{python_note_id}",
  filename: "code-python.py",
  content_type: "text/x-python"
})
// Expected: returns { upload_url, curl_command, max_size: "50MB" }

// 3. Execute the returned curl command with actual file path
// Replace localhost:3000 with https://memory.integrolabs.net
// curl -s -X POST \
//   -F "file=@tests/uat/data/documents/code-python.py;type=text/x-python" \
//   -H "Authorization: Bearer <token>" \
//   "https://memory.integrolabs.net/api/v1/notes/{python_note_id}/attachments/upload"
// Expected: returns attachment metadata with content_type: "text/x-python"
```

**Expected Results**:
- Note created with unique note_id (UUIDv7)
- Tags include "python" and "code"
- Attachment uploaded and persisted
- Attachment metadata includes correct content_type

**Verification**:
```javascript
// Get note details
get_note({ note_id: "{note_id}" })
// Verify tags include "python"
```

**Store**: `python_note_id`

**Pass Criteria**:
- Note created successfully
- Attachment uploaded and persisted via two-step flow
- Tags applied correctly

---

### CHAIN-002: Detect Document Type

**MCP Tools**: `detect_document_type`, `list_document_types`

**Description**: Verify document type detection for Python code

**Prerequisites**:
- `python_note_id` from CHAIN-001

**Steps**:
```javascript
// 1. Detect document type
detect_document_type({
  content: "<Python code content>",
  filename: "code-python.py"
})
// Expected: returns document_type_name: "python", chunking_strategy: "syntactic"

// 2. List all document types
list_document_types({})
// Expected: returns array including Python type definition
```

**Expected Results**:
- Document type detected as "python"
- Chunking strategy is "syntactic" for code
- Python type exists in registry with 131 total types

**Verification**:
```javascript
// Verify Python in registry
list_document_types({})
// Check for entry with name: "python"
```

**Pass Criteria**:
- Document type correctly identified
- Appropriate chunking strategy selected
- Type registry accessible

---

### CHAIN-003: Verify Automatic Embedding

**MCP Tools**: `list_embedding_sets`, `get_embedding_set`, `search_notes`

**Description**: Verify note was automatically embedded in default set

**Prerequisites**:
- `python_note_id` with content
- Default embedding set configured

**Steps**:
```javascript
// 1. List embedding sets
list_embedding_sets({})
// Expected: returns array including default set

// 2. Get embedding set details
get_embedding_set({ set_id: "default" })
// Expected: returns model_name, dimensions, note_count

// 3. Verify note in default set
// Note: Direct "get note embedding" not in MCP tools
// Instead, use semantic search which requires embedding
search_notes({
  query: "Python data processing pipeline",
  limit: 5,
  required_tags: ["uat/chain1"]
})
// Expected: returns python_note_id in results (proves embedding exists)
```

**Expected Results**:
- Default embedding set exists
- Semantic search finds the note (proving embedding exists)
- Results ordered by similarity

**Pass Criteria**:
- Note embedded automatically after creation
- Searchable via semantic similarity
- Default set populated

---

### CHAIN-004: Semantic Search for Code

**MCP Tools**: `search_notes`, `get_note`

**Description**: Search for the note using semantic similarity

**Prerequisites**:
- `python_note_id` with embedding

**Steps**:
```javascript
// 1. Semantic search for related concepts
search_notes({
  query: "data processing with transformation pipeline",
  limit: 10,
  required_tags: ["code"]
})
// Expected: returns results array with similarity scores

// 2. Verify our note in results
// Check if python_note_id appears in results
```

**Expected Results**:
- Results array contains `python_note_id`
- Similarity score indicates relevance
- Results ordered by similarity (descending)
- Total results > 0

**Verification**:
```javascript
// Search specifically for our note
get_note({ note_id: "{python_note_id}" })
// Verify it exists and has expected content
```

**Pass Criteria**:
- Semantic search finds the uploaded Python code
- Similarity score indicates strong match
- Results ranked correctly

---

### CHAIN-005: Compare Versions (List All Revisions)

**MCP Tools**: `list_note_versions`, `get_note_version`, `diff_note_versions`

**Description**: List note versions using versioning system

**Prerequisites**:
- `python_note_id` created

**Steps**:
```javascript
// 1. List all revisions
list_note_versions({ note_id: "{python_note_id}" })
// Expected: returns array of version objects with timestamps
// Note: versions are 1-indexed (first version is 1, not 0)

// 2. Get specific version (version 1 = original)
get_note_version({
  note_id: "{python_note_id}",
  version: 1,
  track: "original"
})
// Expected: returns original content

// 3. Diff between versions (if 2+ versions exist)
diff_note_versions({
  note_id: "{python_note_id}",
  from_version: 1,
  to_version: 2
})
// Expected: returns diff showing changes
// Note: if only 1 version exists, this will error — that's OK, pass based on list result
```

**Expected Results**:
- Version list shows 1+ versions
- Each version has `version` number (1-indexed)
- Diff shows additions/deletions between versions (if 2+ exist)
- Original content preserved in version 1

**Verification**:
```javascript
// Verify version metadata
list_note_versions({ note_id: "{python_note_id}" })
// Check that versions array has length >= 1
```

**Pass Criteria**:
- Version history complete and accessible
- Original content retrievable via `get_note_version`
- Diff accurately shows changes (if multiple versions exist)

---

### CHAIN-006: Export as Markdown with Frontmatter

**MCP Tool**: `export_note`

**Description**: Export note as markdown with YAML frontmatter

**Prerequisites**:
- `python_note_id` fully processed

**Steps**:
```javascript
// 1. Export as markdown
export_note({
  note_id: "{python_note_id}",
  format: "markdown"
})
// Expected: returns markdown string with YAML frontmatter

// 2. Verify frontmatter structure
// Content should start with "---"
// Include: id, tags, created_at, document_type
// Followed by markdown content
```

**Expected Results**:
- Export returns markdown string
- Starts with `---` (YAML frontmatter delimiter)
- Frontmatter includes:
  - `id: {python_note_id}`
  - `tags: [uat/chain1, python, code]`
  - `created_at: <timestamp>`
- Markdown content follows frontmatter
- Code blocks properly formatted with ` ```python ` fences

**Pass Criteria**:
- Export produces valid markdown
- YAML frontmatter complete and parseable
- Content preserves code structure

---

### CHAIN-006b: Chain 1 Error — Search Non-Existent Embedding Set

**Isolation**: Required

**MCP Tool**: `search_notes`

**Description**: Attempt to search within an embedding set that doesn't exist.

```javascript
search_notes({
  query: "python code",
  embedding_set_id: "00000000-0000-0000-0000-000000000000",
  limit: 10
})
```

**Pass Criteria**: Returns **404 Not Found** — embedding set does not exist. No results returned.

---

**Chain 1 Summary**:
- Total steps: 7
- Features exercised: File upload, document type detection, embedding, semantic search, versioning, export
- Success criteria: All steps pass with expected results

---

## Chain 2: Geo-Temporal Memory

**Scenario**: Upload GPS-tagged photo → EXIF extraction creates provenance → Search by location → Search by time → Provenance chain

**Duration**: ~5 minutes

> **Note**: This chain uses the actual provenance creation path: upload a GPS-tagged JPEG → EXIF extraction job automatically creates provenance records (location, capture time). Provenance can also be created explicitly via MCP tools: `create_provenance_location`, `create_provenance_device`, `create_file_provenance` ([#261](https://git.integrolabs.net/Fortemi/fortemi/issues/261)). The `create_note` API does NOT support inline `metadata.location`.

---

### CHAIN-007: Create Memory with GPS-Tagged Photo

**MCP Tools**: `create_note`, `upload_attachment`

**Description**: Create note and upload GPS-tagged photo to trigger EXIF extraction and automatic provenance creation

**Prerequisites**:
- MCP server running
- PostGIS extension enabled
- Test file: `tests/uat/data/provenance/paris-eiffel-tower.jpg` (GPS: 48.8584°N, 2.2945°E)

**Steps**:
```javascript
// 1. Create note for the memory
create_note({
  content: "# Paris Trip\n\nVisited the Eiffel Tower on a beautiful summer day.",
  tags: ["uat/chain2", "paris", "travel"],
  revision_mode: "none"
})
// Expected: returns note_id

// 2. Upload GPS-tagged photo (two-step upload)
upload_attachment({
  note_id: "{paris_note_id}",
  filename: "paris-eiffel-tower.jpg",
  content_type: "image/jpeg"
})
// Expected: returns { upload_url, curl_command, max_size: "50MB" }

// 3. Execute the returned curl command with actual file
// curl -s -X POST \
//   -F "file=@tests/uat/data/provenance/paris-eiffel-tower.jpg;type=image/jpeg" \
//   -H "Authorization: Bearer <token>" \
//   "https://memory.integrolabs.net/api/v1/notes/{paris_note_id}/attachments/upload"

// 4. Wait 3-5 seconds for EXIF extraction job to process
```

**Expected Results**:
- Note created with note_id
- Attachment uploaded successfully
- EXIF extraction job triggered automatically
- GPS coordinates extracted from JPEG EXIF data

**Store**: `paris_note_id`, `paris_attachment_id`

**Pass Criteria**:
- Note and attachment created
- EXIF extraction job queued/completed

---

### CHAIN-008: Verify Provenance Record Created

**MCP Tool**: `get_memory_provenance`

**Description**: Verify W3C PROV provenance created automatically from EXIF data

**Prerequisites**:
- `paris_note_id` from CHAIN-007
- Wait 3-5 seconds for EXIF extraction

**Steps**:
```javascript
// 1. Get memory provenance
get_memory_provenance({
  note_id: "{paris_note_id}"
})
// Expected: returns provenance chain with location, time, activity
```

**Expected Results**:
- Provenance record exists
- Location extracted from EXIF GPS: approximately POINT(2.2945 48.8584)
- Capture time extracted from EXIF DateTimeOriginal
- Event type included

**Pass Criteria**:
- Provenance chain created automatically from EXIF data
- Spatial and temporal data preserved

---

### CHAIN-009: Search by Location (1km radius)

**MCP Tool**: `search_memories_by_location`

**Description**: Search for memories near Eiffel Tower

**Prerequisites**:
- `paris_note_id` with provenance from CHAIN-008

**Steps**:
```javascript
// 1. Search within 1km of Eiffel Tower
search_memories_by_location({
  lat: 48.8584,
  lon: 2.2945,
  radius: 1000,
  limit: 10
})
// Expected: returns results array with paris_note_id
```

**Expected Results**:
- Results array includes `paris_note_id`
- Distance < 1000.0 meters
- Results ordered by distance
- Location-based filtering works

**Pass Criteria**:
- Spatial search finds note
- Distance calculated correctly
- Results within specified radius

---

### CHAIN-010: Search by Time Range

**MCP Tool**: `search_memories_by_time`

**Description**: Search for memories captured within the EXIF timestamp range

**Prerequisites**:
- `paris_note_id` with capture time from EXIF

**Steps**:
```javascript
// 1. Search for memories from the EXIF capture date
// The exact date depends on the EXIF data in paris-eiffel-tower.jpg
// Use a wide range to ensure we capture it
search_memories_by_time({
  start: "2020-01-01T00:00:00Z",
  end: "2026-12-31T23:59:59Z",
  limit: 10
})
// Expected: returns results array with paris_note_id
```

**Expected Results**:
- Results array includes `paris_note_id`
- Capture time within specified range
- Temporal ordering (chronological)

**Pass Criteria**:
- Temporal search finds note
- Time filtering accurate
- Results ordered by time

---

### CHAIN-011: Combined Spatial-Temporal Search

**MCP Tool**: `search_memories_combined`

**Description**: Search for memories near Eiffel Tower within the EXIF time range

**Prerequisites**:
- `paris_note_id` with location and time from provenance

**Steps**:
```javascript
// 1. Combined search
search_memories_combined({
  lat: 48.8584,
  lon: 2.2945,
  radius: 5000,
  start: "2020-01-01T00:00:00Z",
  end: "2026-12-31T23:59:59Z",
  limit: 10
})
// Expected: returns results matching both location AND time criteria
```

**Expected Results**:
- Results include `paris_note_id`
- Both spatial and temporal filters applied
- Combined query more precise than either alone

**Pass Criteria**:
- Combined search works correctly
- Filters intersect (AND operation)
- Results satisfy both criteria

---

### CHAIN-012: Retrieve Full Provenance Chain

**MCP Tool**: `get_memory_provenance`

**Description**: Get complete provenance chain for memory

**Prerequisites**:
- `paris_note_id` with provenance

**Steps**:
```javascript
// 1. Get provenance chain
get_memory_provenance({
  note_id: "{paris_note_id}"
})
// Expected: returns W3C PROV graph with entities, activities, agents
```

**Expected Results**:
- Provenance graph includes:
  - Entity (note)
  - Activity (memory capture)
  - Location (Eiffel Tower coordinates from EXIF)
  - Time (capture timestamp from EXIF)
  - Device (camera make/model from EXIF, if available)
- W3C PROV relationships present
- Graph structure valid

**Pass Criteria**:
- Complete provenance chain retrievable
- All W3C PROV elements present
- Graph structure valid

---

### CHAIN-012b: Chain 2 Error — Spatial Search with Impossible Coordinates

**Isolation**: Required

**MCP Tool**: `search_memories_by_location`

**Description**: Search with coordinates outside valid range (latitude > 90).

```javascript
search_memories_by_location({
  latitude: 999.0,
  longitude: 999.0,
  radius_km: 10
})
```

**Pass Criteria**: Returns **400 Bad Request** — invalid coordinates. Alternatively, returns empty results array gracefully.

---

**Chain 2 Summary**:
- Total steps: 7
- Features exercised: GPS-tagged photo upload, EXIF extraction, automatic provenance creation, spatial search, temporal search, combined search, provenance chain
- Success criteria: Geo-temporal search functional via EXIF-derived provenance
- **Dependency**: Requires attachment uploads to work (#252 must be fixed)

---

## Chain 3: Knowledge Organization

**Scenario**: Create taxonomy → Tag notes → Collection hierarchy → Strict filter → Search → Graph explore → Export shard

**Duration**: ~7 minutes

---

### CHAIN-013: Create SKOS Concept Scheme

**MCP Tool**: `create_concept_scheme`

**Description**: Create a SKOS taxonomy for UAT testing

**Prerequisites**:
- MCP server running

**Steps**:
```javascript
// 1. Create concept scheme
create_concept_scheme({
  scheme_id: "test-uat-taxonomy",
  title: "UAT Testing Taxonomy",
  description: "Hierarchical taxonomy for UAT feature chain testing",
  creator: "UAT Chain 3"
})
// Expected: returns scheme_id, created_at timestamp
```

**Expected Results**:
- Scheme created successfully
- scheme_id: "test-uat-taxonomy"
- Title and description stored

**Store**: `scheme_id = "test-uat-taxonomy"`

**Pass Criteria**:
- Concept scheme created successfully

---

### CHAIN-014: Create Hierarchical Concepts

**MCP Tools**: `create_concept`, `add_broader`, `get_narrower`

**Description**: Create broader/narrower concept relationships

**Prerequisites**:
- `scheme_id` from CHAIN-013

**Steps**:
```javascript
// 1. Create top concept (Programming)
create_concept({
  scheme_id: "test-uat-taxonomy",
  concept_id: "programming",
  pref_label: "Programming",
  definition: "Software development and coding"
})
// Expected: returns concept created

// 2. Create narrower concept (Languages)
create_concept({
  scheme_id: "test-uat-taxonomy",
  concept_id: "programming-languages",
  pref_label: "Programming Languages",
  definition: "Different programming languages"
})
// Expected: returns concept created

// 3. Add broader relationship (Languages → Programming)
add_broader({
  scheme_id: "test-uat-taxonomy",
  concept_id: "programming-languages",
  broader_id: "programming"
})
// Expected: relationship created

// 4. Create Python concept
create_concept({
  scheme_id: "test-uat-taxonomy",
  concept_id: "python",
  pref_label: "Python",
  alt_labels: ["Python3", "Py"],
  definition: "Python programming language"
})
// Expected: returns concept created

// 5. Add broader relationship (Python → Languages)
add_broader({
  scheme_id: "test-uat-taxonomy",
  concept_id: "python",
  broader_id: "programming-languages"
})
// Expected: relationship created

// 6. Create Rust concept
create_concept({
  scheme_id: "test-uat-taxonomy",
  concept_id: "rust",
  pref_label: "Rust",
  definition: "Rust programming language"
})
// Expected: returns concept created

// 7. Add broader relationship (Rust → Languages)
add_broader({
  scheme_id: "test-uat-taxonomy",
  concept_id: "rust",
  broader_id: "programming-languages"
})
// Expected: relationship created
```

**Expected Results**:
- 4 concepts created
- Hierarchy: `programming` → `programming-languages` → `python` / `rust`
- Broader/narrower relationships established

**Verification**:
```javascript
// Get narrower concepts of programming
get_narrower({
  scheme_id: "test-uat-taxonomy",
  concept_id: "programming"
})
// Should show programming-languages

get_narrower({
  scheme_id: "test-uat-taxonomy",
  concept_id: "programming-languages"
})
// Should show python, rust
```

**Store**: `concept_python`, `concept_rust`

**Pass Criteria**:
- Concept hierarchy created
- Broader/narrower relationships correct

---

### CHAIN-015: Create Collection Hierarchy

**MCP Tool**: `create_collection`

**Description**: Create nested collections for organizing notes

**Prerequisites**:
- MCP server running

**Steps**:
```javascript
// 1. Create root collection
create_collection({
  name: "UAT Projects",
  description: "Root collection for UAT testing"
})
// Expected: returns collection_id

// 2. Create child collection
create_collection({
  name: "Code Samples",
  description: "Collection of code samples",
  parent_id: "{root_collection_id}"
})
// Expected: returns collection_id with parent reference
```

**Expected Results**:
- 2 collections created
- Parent-child relationship established
- Path reflects hierarchy (e.g., "/UAT Projects/Code Samples")

**Store**: `root_collection_id`, `code_collection_id`

**Pass Criteria**:
- Collection hierarchy created
- Paths computed correctly

---

### CHAIN-016: Create Tagged Notes in Collections

**MCP Tools**: `create_note`, `tag_note_concept`, `move_note_to_collection`

**Description**: Create notes with SKOS tags and add to collections

**Prerequisites**:
- `scheme_id`, concepts, and collections from previous steps

**Steps**:
```javascript
// 1. Create Python note
create_note({
  content: "# Python Tutorial\n\nLearn Python basics with examples.",
  tags: ["uat/chain3"]
})
// Expected: returns note_id

// 2. Tag with SKOS concept
tag_note_concept({
  note_id: "{python_tutorial_note_id}",
  scheme_id: "test-uat-taxonomy",
  concept_id: "python"
})
// Expected: SKOS tag applied

// 3. Add to Code Samples collection
move_note_to_collection({
  note_id: "{python_tutorial_note_id}",
  collection_id: "{code_collection_id}"
})
// Expected: note added to collection

// 4. Create Rust note
create_note({
  content: "# Rust Guide\n\nSafe systems programming with Rust.",
  tags: ["uat/chain3"]
})
// Expected: returns note_id

// 5. Tag with SKOS concept
tag_note_concept({
  note_id: "{rust_guide_note_id}",
  scheme_id: "test-uat-taxonomy",
  concept_id: "rust"
})
// Expected: SKOS tag applied

// 6. Add to Code Samples collection
move_note_to_collection({
  note_id: "{rust_guide_note_id}",
  collection_id: "{code_collection_id}"
})
// Expected: note added to collection
```

**Expected Results**:
- 2 notes created with SKOS tags
- Both notes added to `Code Samples` collection
- SKOS tags link to taxonomy concepts

**Store**: `python_tutorial_note_id`, `rust_guide_note_id`

**Pass Criteria**:
- Notes tagged with SKOS concepts
- Notes organized in collections

---

### CHAIN-017: Search with Strict Tag Filtering

**MCP Tool**: `search_notes`

**Description**: Search notes using strict SKOS tag filtering

**Prerequisites**:
- Tagged notes from CHAIN-016

**Steps**:
```javascript
// 1. Search for Python concept (strict mode)
search_notes({
  query: "programming",
  required_tags: ["test-uat-taxonomy:python"],
  limit: 10
})
// Expected: returns ONLY notes tagged with test-uat-taxonomy:python
```

**Expected Results**:
- Results contain ONLY notes tagged with `test-uat-taxonomy:python`
- `python_tutorial_note_id` included
- `rust_guide_note_id` excluded (different tag)
- Tag isolation guaranteed

**Verification**:
```javascript
// Search for Rust tag separately
search_notes({
  query: "programming",
  required_tags: ["test-uat-taxonomy:rust"],
  limit: 10
})
// Should return only rust_guide_note_id
```

**Pass Criteria**:
- Strict tag filtering enforced
- No cross-contamination between tags

---

### CHAIN-018: Explore Knowledge Graph

**MCP Tool**: `explore_graph`

**Description**: Explore knowledge graph from Python concept (2-hop)

**Prerequisites**:
- `python_tutorial_note_id` and concept hierarchy

**Steps**:
```javascript
// 1. Explore graph from Python note
explore_graph({
  start_node_id: "{python_tutorial_note_id}",
  max_depth: 2,
  direction: "both"
})
// Expected: returns graph with nodes and edges
```

**Expected Results**:
- Graph includes:
  - Start node: `python_tutorial_note_id`
  - 1-hop: `test-uat-taxonomy:python` concept (via `tagged_with`)
  - 1-hop: `code_collection_id` (via `in_collection`)
  - 2-hop: `test-uat-taxonomy:programming-languages` (via `broader`)
  - 2-hop: Other notes in same collection
- Total nodes >= 4
- Edges show relationship types

**Verification**:
```javascript
// Verify graph structure
explore_graph({
  start_node_id: "{python_tutorial_note_id}",
  max_depth: 2,
  direction: "both"
})
// Check nodes array length >= 4
// Check edges array has relationship types
```

**Pass Criteria**:
- Graph exploration traverses multiple relationship types
- 2-hop depth respected
- All relevant connections included

---

### CHAIN-019: Export Knowledge Shard

**MCP Tools**: `export_skos_turtle`, `knowledge_shard`

**Description**: Export collection as knowledge shard (Turtle/JSON-LD)

**Prerequisites**:
- `code_collection_id` with notes

**Steps**:
```javascript
// 1. Export SKOS scheme as Turtle
export_skos_turtle({
  scheme_id: "test-uat-taxonomy"
})
// Expected: returns Turtle RDF serialization

// 2. Export knowledge shard to file
knowledge_shard({
  output_dir: "/tmp/uat"
})
// Expected: returns { saved_to: "/tmp/uat/chain3-shard.tar.gz", ... }
```

**Expected Results**:
- Turtle export includes SKOS concepts with relationships
- Knowledge shard includes:
  - Collection metadata
  - All notes in collection
  - SKOS concept references
  - Provenance information
- Format is valid RDF/JSON-LD (parseable)

**Verification**:
```javascript
// Verify export completeness
// Check that output includes:
// - skos:ConceptScheme
// - skos:Concept entries
// - skos:broader/narrower relationships
```

**Pass Criteria**:
- Export produces valid RDF/JSON-LD
- All collection data included
- SKOS relationships preserved

---

### CHAIN-019b: Chain 3 Error — Tag Concept to Non-Existent Note

**Isolation**: Required

**MCP Tool**: `tag_note_concept`

**Description**: Attempt to tag a SKOS concept to a note that doesn't exist.

```javascript
tag_note_concept({
  note_id: "00000000-0000-0000-0000-000000000000",
  concept_id: "{python_concept_id}"
})
```

**Pass Criteria**: Returns **404 Not Found** — note does not exist. No orphaned concept-tag created.

---

**Chain 3 Summary**:
- Total steps: 8
- Features exercised: SKOS taxonomy, concept hierarchy, collections, tagging, strict filtering, graph exploration, knowledge shard export
- Success criteria: Knowledge graph navigable and exportable

---

## Chain 4: Multilingual Search Pipeline

**Scenario**: Create multilingual notes → FTS each language → CJK bigram → Emoji trigram → Cross-language discovery

**Duration**: ~5 minutes

---

### CHAIN-020: Create Multilingual Notes

**MCP Tool**: `create_note`

**Description**: Create notes in multiple languages

**Prerequisites**:
- MCP server running
- FTS multilingual configs enabled

**Steps**:
```javascript
// 1. English note
create_note({
  content: "# Running Performance\n\nI love running in the morning. It helps me run faster.",
  tags: ["uat/chain4", "english"]
})
// Expected: returns note_id

// 2. German note
create_note({
  content: "# Laufen\n\nIch laufe gerne am Morgen. Das Laufen läuft gut.",
  tags: ["uat/chain4", "german"]
})
// Expected: returns note_id

// 3. Chinese note
create_note({
  content: "# 北京旅行\n\n我去了北京市和北京大学。北京很美。",
  tags: ["uat/chain4", "chinese"]
})
// Expected: returns note_id

// 4. Emoji note
create_note({
  content: "# Weekend Fun 🎉\n\nHad a great time at the party! 🎉🎊🎈",
  tags: ["uat/chain4", "emoji"]
})
// Expected: returns note_id
```

**Expected Results**:
- 4 notes created
- Each has unique note_id
- Different language content

**Store**: `en_note_id`, `de_note_id`, `zh_note_id`, `emoji_note_id`

**Pass Criteria**:
- Multilingual notes created successfully

---

### CHAIN-021: Test English Stemming

**MCP Tool**: `search_notes`

**Description**: Search with stemming for English content

**Prerequisites**:
- `en_note_id` from CHAIN-020

**Steps**:
```javascript
// 1. Search for "run" (should match "running", "runs", "run")
search_notes({
  query: "run",
  limit: 10,
  required_tags: ["uat/chain4"]
})
// Expected: returns results including en_note_id
```

**Expected Results**:
- Results include `en_note_id`
- FTS stemming matches "running" in content
- Rank score reflects match quality

**Verification**:
```javascript
// Verify stemming worked by checking results
search_notes({
  query: "run",
  limit: 10,
  required_tags: ["english"]
})
// Should return en_note_id with good score
```

**Pass Criteria**:
- English stemming matches word variations
- Search finds "running" when querying "run"

---

### CHAIN-022: Test German Stemming

**MCP Tool**: `search_notes`

**Description**: Search with German stemming

**Prerequisites**:
- `de_note_id` from CHAIN-020

**Steps**:
```javascript
// 1. Search for "laufen" (should match "laufe", "läuft")
search_notes({
  query: "laufen",
  limit: 10,
  required_tags: ["uat/chain4"]
})
// Expected: returns results including de_note_id
```

**Expected Results**:
- Results include `de_note_id`
- German stemming matches "laufe" and "läuft"
- Language-specific config applied

**Pass Criteria**:
- German stemming works correctly
- Umlaut handling correct (ä, ö, ü)

---

### CHAIN-023: Test CJK Bigram Matching

**MCP Tool**: `search_notes`

**Description**: Search Chinese text with bigram indexing

**Prerequisites**:
- `zh_note_id` from CHAIN-020
- FTS_BIGRAM_CJK=true enabled

**Steps**:
```javascript
// 1. Search for "北京" (should match "北京市", "北京大学")
search_notes({
  query: "北京",
  limit: 10,
  required_tags: ["uat/chain4"]
})
// Expected: returns results including zh_note_id
```

**Expected Results**:
- Results include `zh_note_id`
- Bigram matching finds "北京市" and "北京大学"
- Character-level matching (no word boundaries)

**Verification**:
```javascript
// Verify CJK search worked
search_notes({
  query: "北京",
  limit: 10,
  required_tags: ["chinese"]
})
// Should return zh_note_id
```

**Pass Criteria**:
- CJK bigram search finds character sequences
- No word segmentation required

---

### CHAIN-024: Test Emoji Trigram Matching

**MCP Tool**: `search_notes`

**Description**: Search for emoji using trigram indexing

**Prerequisites**:
- `emoji_note_id` from CHAIN-020
- FTS_TRIGRAM_FALLBACK=true enabled

**Steps**:
```javascript
// 1. Search for "🎉" emoji
search_notes({
  query: "🎉",
  limit: 10,
  required_tags: ["uat/chain4"]
})
// Expected: returns results including emoji_note_id
```

**Expected Results**:
- Results include `emoji_note_id`
- Trigram matching finds emoji character
- Multiple emoji occurrences detected

**Pass Criteria**:
- Emoji search works via pg_trgm
- Unicode character matching successful

---

### CHAIN-025: Cross-Language Semantic Discovery

**MCP Tool**: `search_notes`

**Description**: Use semantic search to find related notes across languages

**Prerequisites**:
- All multilingual notes from CHAIN-020
- Notes embedded in default set

**Steps**:
```javascript
// 1. Semantic search for "running exercise"
search_notes({
  query: "running exercise fitness",
  limit: 10,
  required_tags: ["uat/chain4"]
})
// Expected: returns both en_note_id and de_note_id
```

**Expected Results**:
- Results include both `en_note_id` and `de_note_id`
- Semantic similarity bridges language gap
- English and German notes both ranked highly (both about running)

**Verification**:
```javascript
// Check cross-language results
search_notes({
  query: "running exercise fitness",
  limit: 10
})
// Should return both EN and DE notes
```

**Pass Criteria**:
- Semantic search finds related content across languages
- Embedding model captures multilingual semantics

---

### CHAIN-025b: Chain 4 Error — Search with Empty Query

**Isolation**: Required

**MCP Tool**: `search_notes`

**Description**: Attempt a search with an empty query string.

```javascript
search_notes({
  query: "",
  limit: 10
})
```

**Pass Criteria**: Returns **400 Bad Request** — query string cannot be empty. Alternatively, returns empty results array gracefully.

---

**Chain 4 Summary**:
- Total steps: 7
- Features exercised: Multilingual FTS, stemming (EN/DE), CJK bigram, emoji trigram, semantic cross-language search
- Success criteria: All language-specific searches work correctly

---

## Chain 5: Encryption & Sharing

**Scenario**: Generate keypair → Create note → Encrypt → Share address → Decrypt → Verify content

**Duration**: ~5 minutes

---

### CHAIN-026: Generate PKE Keyset

**MCP Tool**: `pke_create_keyset`

**Description**: Generate public/private keypair for encryption

**Prerequisites**:
- MCP server running

**Steps**:
```javascript
// 1. Generate new keyset
pke_create_keyset({
  name: "UAT Chain 5 Keys",
  description: "Test keyset for encryption chain"
})
// Expected: returns keyset_id, public_key (base64)
```

**Expected Results**:
- Keyset created successfully
- Response includes keyset_id
- Public key available for sharing
- Private key stored securely (not returned)

**Store**: `keyset_id`, `public_key`

**Pass Criteria**:
- Keyset generated successfully
- Public key available for sharing

---

### CHAIN-027: Create Sensitive Note

**MCP Tool**: `create_note`

**Description**: Create note with sensitive content

**Prerequisites**:
- MCP server running

**Steps**:
```javascript
// 1. Create note with sensitive data
create_note({
  content: "# API Key Storage\n\nAPI_KEY=sk_test_1234567890abcdef\nSECRET=super_secret_password",
  tags: ["uat/chain5", "sensitive"]
})
// Expected: returns note_id
```

**Expected Results**:
- Note created with note_id
- Content stored initially (before encryption)

**Store**: `sensitive_note_id`

**Pass Criteria**:
- Note created successfully

---

### CHAIN-028: Encrypt Note with PKE

**MCP Tools**: `pke_encrypt`, `get_note`

**Description**: Encrypt note content using PKE keyset

**Prerequisites**:
- `sensitive_note_id` and `keyset_id`

**Steps**:
```javascript
// 1. Encrypt note
pke_encrypt({
  note_id: "{sensitive_note_id}",
  keyset_id: "{keyset_id}"
})
// Expected: returns encrypted content, encryption metadata
```

**Expected Results**:
- Encryption successful
- Content replaced with ciphertext
- Ciphertext format: `MMPKE01:<base64-ciphertext>`

**Verification**:
```javascript
// Get encrypted note
get_note({ note_id: "{sensitive_note_id}" })
// Verify content starts with "MMPKE01" (encrypted)
```

**Pass Criteria**:
- Note encrypted successfully
- Content not readable in plaintext
- Encryption format correct

---

### CHAIN-029: Get PKE Address for Sharing

**MCP Tool**: `pke_get_address`

**Description**: Retrieve the PKE address for the keyset (used to share the public key with recipients)

**Prerequisites**:
- `keyset_id` from CHAIN-026

**Steps**:
```javascript
// 1. Get PKE address for keyset
pke_get_address({
  keyset_id: "{keyset_id}"
})
// Expected: returns address (public key identifier for sharing)
```

**Expected Results**:
- Address returned (base64-encoded public key identifier)
- Address can be shared with recipients for multi-party encryption
- Address format is consistent and URL-safe

**Store**: `pke_address`

**Pass Criteria**:
- PKE address retrieved successfully
- Address is non-empty and well-formed

---

### CHAIN-030: Decrypt Note

**MCP Tool**: `pke_decrypt`

**Description**: Decrypt note using private key

**Prerequisites**:
- `sensitive_note_id` (encrypted)
- `keyset_id` with private key

**Steps**:
```javascript
// 1. Decrypt note
pke_decrypt({
  note_id: "{sensitive_note_id}",
  keyset_id: "{keyset_id}"
})
// Expected: returns decrypted_content
```

**Expected Results**:
- Decryption successful
- Response includes decrypted_content
- Content matches original: "API_KEY=sk_test_1234567890abcdef"

**Pass Criteria**:
- Decryption successful
- Content matches original exactly

---

### CHAIN-031: Verify Content Integrity After Decrypt

**MCP Tool**: `get_note`

**Description**: Verify the note content is restored to plaintext after decryption

**Prerequisites**:
- `sensitive_note_id` decrypted in CHAIN-030

**Steps**:
```javascript
// 1. Get the note after decryption
get_note({ id: "{sensitive_note_id}" })
// Expected: returns note with original plaintext content

// 2. Compare with original
// Original: "# API Key Storage\n\nAPI_KEY=sk_test_1234567890abcdef\nSECRET=super_secret_password"
// Content should match exactly (no longer starts with "MMPKE01")
```

**Expected Results**:
- Note content is plaintext (not ciphertext)
- Content matches original exactly
- No data loss or corruption through encrypt/decrypt cycle
- Content integrity preserved

**Pass Criteria**:
- Decrypted content identical to original
- No corruption or data loss

---

### CHAIN-031b: Chain 5 Error — Encrypt with Non-Existent Keyset

**Isolation**: Required

**MCP Tool**: `pke_encrypt`

**Description**: Attempt to encrypt a note using a keyset that doesn't exist.

```javascript
pke_encrypt({
  note_id: "{sensitive_note_id}",
  keyset_id: "nonexistent-keyset-id",
  recipients: ["addr_00000000"]
})
```

**Pass Criteria**: Returns **404 Not Found** — keyset does not exist. Note content unchanged.

---

**Chain 5 Summary**:
- Total steps: 7
- Features exercised: PKE keyset generation, note encryption, PKE address sharing, decryption, content integrity verification
- Success criteria: Encryption cycle preserves data integrity

---

## Chain 6: Backup & Recovery

**Scenario**: Create data → Database snapshot → Delete data → Restore snapshot → Verify recovery

**Duration**: ~4 minutes

> **DATA DESTRUCTION WARNING**: Chain 6 performs a **full database restore** (CHAIN-035) that **wipes ALL data** created by Chains 1-5 after the snapshot point. This is intentional for testing restore functionality. Chains 7-8 must recreate any prerequisite data independently. If running chains in isolation, execute Chain 6 last.

---

### CHAIN-032: Create Test Data for Backup

**MCP Tools**: `create_note`, `get_note_links`

**Description**: Create multiple notes with relationships

**Prerequisites**:
- MCP server running

**Steps**:
```javascript
// 1. Create note 1
create_note({
  content: "# Backup Test Note 1\n\nThis is the first test note.",
  tags: ["uat/chain6", "backup"]
})
// Expected: returns note_id

// 2. Create note 2
create_note({
  content: "# Backup Test Note 2\n\nThis is the second test note.",
  tags: ["uat/chain6", "backup"]
})
// Expected: returns note_id

// 3. Create note 3 (will create link later)
create_note({
  content: "# Backup Test Note 3\n\nThis links to note 1.",
  tags: ["uat/chain6", "backup"]
})
// Expected: returns note_id

// 4. Get links for verification
get_note_links({ note_id: "{backup_note1_id}" })
// Expected: returns links array
```

**Expected Results**:
- 3 notes created
- Each has unique note_id
- Tags applied correctly

**Store**: `backup_note1_id`, `backup_note2_id`, `backup_note3_id`

**Pass Criteria**:
- Test data created successfully

---

### CHAIN-033: Create Database Snapshot

**MCP Tools**: `database_snapshot`, `backup_status`

**Description**: Take database backup snapshot

**Prerequisites**:
- Test data from CHAIN-032

**Steps**:
```javascript
// 1. Create backup snapshot
database_snapshot({
  name: "uat-chain6-snapshot",
  description: "Backup before deletion test",
  include_attachments: true
})
// Expected: returns snapshot_id, status: "completed"
```

**Expected Results**:
- Snapshot created successfully
- snapshot_id returned
- created_at timestamp
- Status: "completed"
- Snapshot includes database dump

**Store**: `snapshot_id`

**Verification**:
```javascript
// Check backup status
backup_status({})
// Verify snapshot exists in list
```

**Pass Criteria**:
- Snapshot created successfully
- All data included

---

### CHAIN-034: Delete Test Data

**MCP Tools**: `delete_note`, `list_notes`

**Description**: Delete the test notes to simulate data loss

**Prerequisites**:
- `backup_note1_id`, `backup_note2_id`, `backup_note3_id`

**Steps**:
```javascript
// 1. Delete note 1
delete_note({ note_id: "{backup_note1_id}" })
// Expected: note deleted

// 2. Delete note 2
delete_note({ note_id: "{backup_note2_id}" })
// Expected: note deleted

// 3. Delete note 3
delete_note({ note_id: "{backup_note3_id}" })
// Expected: note deleted

// 4. Verify deletion
list_notes({ tags: ["uat/chain6", "backup"] })
// Expected: empty array
```

**Expected Results**:
- All 3 notes deleted
- Search for tag `uat/chain6` returns empty array
- Links also deleted (cascade)

**Verification**:
```javascript
// Confirm notes gone
list_notes({ tags: ["uat/chain6"] })
// Should return empty array
```

**Pass Criteria**:
- All test notes deleted
- No orphaned links remain

---

### CHAIN-035: Restore from Snapshot

> **DATA DESTRUCTION**: This test performs `restore_mode: "full"` which **erases all data** created after the snapshot. All notes, tags, collections, embeddings, SKOS concepts, and PKE data from Chains 1-5 will be permanently lost. Chains 7-8 recreate their own prerequisite data.

**MCP Tools**: `database_restore`, `backup_status`

**Description**: Restore database from backup snapshot

**Prerequisites**:
- `snapshot_id` from CHAIN-033
- Notes deleted in CHAIN-034

**Steps**:
```javascript
// 1. Restore snapshot
database_restore({
  snapshot_id: "{snapshot_id}",
  restore_mode: "full"
})
// Expected: restore job started, returns job_id

// 2. Wait for restore to complete
// Poll status or wait ~15 seconds

// 3. Check restore completion
backup_status({})
// Expected: restore status: "completed"
```

**Expected Results**:
- Restore job completes successfully
- Database state reverted to snapshot time

**Verification**:
```javascript
// Check restore status
backup_status({})
// Verify last restore completed
```

**Pass Criteria**:
- Restore completes without errors

---

### CHAIN-036: Verify Data Recovery

**MCP Tools**: `list_notes`, `get_note`, `get_note_links`

**Description**: Verify all deleted notes and links are restored

**Prerequisites**:
- Restore completed in CHAIN-035

**Steps**:
```javascript
// 1. Search for restored notes
list_notes({ tags: ["uat/chain6", "backup"] })
// Expected: returns 3 notes

// 2. Verify note 1 exists
get_note({ note_id: "{backup_note1_id}" })
// Expected: note found

// 3. Verify note 2 exists
get_note({ note_id: "{backup_note2_id}" })
// Expected: note found

// 4. Verify note 3 exists
get_note({ note_id: "{backup_note3_id}" })
// Expected: note found

// 5. Verify links restored
get_note_links({ note_id: "{backup_note1_id}" })
// Expected: links array (if any existed before backup)
```

**Expected Results**:
- All 3 notes restored with correct content
- Tag search returns 3 results
- Links intact (if any existed)

**Verification**:
```javascript
// Check note count
list_notes({ tags: ["uat/chain6"] })
// Should return 3 notes

// Check links
get_note_links({ note_id: "{backup_note1_id}" })
// Should return links array
```

**Pass Criteria**:
- All notes recovered completely
- All links intact
- No data loss

---

### CHAIN-036b: Chain 6 Error — Restore Non-Existent Snapshot

**Isolation**: Required

**MCP Tool**: `database_restore`

**Description**: Attempt to restore from a snapshot ID that doesn't exist.

```javascript
database_restore({
  snapshot_id: "nonexistent-snapshot-id-00000",
  restore_mode: "full"
})
```

**Pass Criteria**: Returns **404 Not Found** — snapshot does not exist. Database state unchanged.

---

**Chain 6 Summary**:
- Total steps: 6
- Features exercised: Backup snapshot, data deletion, restore, data integrity verification
- Success criteria: Complete data recovery from backup

---

## Chain 7: Embedding Set Focus

**Scenario**: Create set → Define criteria → Auto-populate → Focused search → Re-embed → Compare results

**Duration**: ~6 minutes

---

### CHAIN-037: Create Focused Embedding Set

**MCP Tool**: `create_embedding_set`

**Description**: Create embedding set with tag-based criteria

**Prerequisites**:
- MCP server running

**Steps**:
```javascript
// 1. Create embedding set for Python notes only
create_embedding_set({
  name: "Python Code Set",
  description: "Focused embedding set for Python code",
  set_type: "full",
  inclusion_criteria: {
    tags: ["python", "code"]
  },
  model_config: {
    model_name: "nomic-embed-text-v1.5",
    dimensions: 768,
    truncate_dim: 256
  },
  auto_embed_rules: {
    on_create: true,
    on_update: true
  }
})
// Expected: returns embedding_set_id
```

**Expected Results**:
- Embedding set created successfully
- embedding_set_id returned
- set_type: "full"
- Auto-embed rules configured

**Store**: `python_set_id`

**Pass Criteria**:
- Embedding set created with criteria

---

### CHAIN-038: Create Matching and Non-Matching Notes

**MCP Tool**: `create_note`

**Description**: Create notes that match and don't match set criteria

**Prerequisites**:
- `python_set_id` from CHAIN-037

**Steps**:
```javascript
// 1. Create Python note (should match criteria)
create_note({
  content: "# Python Data Classes\n\nUsing dataclasses for clean code.",
  tags: ["uat/chain7", "python", "code"]
})
// Expected: returns note_id

// 2. Create Rust note (should NOT match criteria)
create_note({
  content: "# Rust Ownership\n\nUnderstanding ownership in Rust.",
  tags: ["uat/chain7", "rust", "code"]
})
// Expected: returns note_id

// 3. Create general note (should NOT match criteria)
create_note({
  content: "# Meeting Notes\n\nDiscussed project timeline.",
  tags: ["uat/chain7", "meeting"]
})
// Expected: returns note_id
```

**Expected Results**:
- 3 notes created
- Only Python note matches `python_set_id` criteria

**Store**: `py_dataclass_note_id`, `rust_ownership_note_id`, `meeting_note_id`

**Pass Criteria**:
- Test notes created

---

### CHAIN-039: Verify Auto-Population

**MCP Tools**: `get_embedding_set`, `search_notes`

**Description**: Verify only matching notes added to embedding set

**Prerequisites**:
- Notes from CHAIN-038
- Wait 5 seconds for auto-embed

**Steps**:
```javascript
// 1. Get embedding set details
get_embedding_set({ set_id: "{python_set_id}" })
// Expected: returns note_count: 1

// 2. Verify only Python note in set
// (Direct "list notes in set" not in MCP tools)
// Use search scoped to set instead
search_notes({
  query: "dataclasses",
  embedding_set_id: "{python_set_id}",
  limit: 10
})
// Expected: returns only py_dataclass_note_id
```

**Expected**: `search_notes` with `embedding_set_id` filter returns only notes in that embedding set.

**Expected Results**:
- Set contains only matching notes
- Python note included
- Rust and meeting notes excluded

**Verification**:
```javascript
// Check embedding set stats
get_embedding_set({ set_id: "{python_set_id}" })
// Verify note_count matches expected (1)
```

**Pass Criteria**:
- Auto-population respects inclusion criteria
- Only matching notes embedded

---

### CHAIN-040: Focused Search Within Set

**MCP Tool**: `search_notes`

**Description**: Search within focused embedding set

**Prerequisites**:
- `python_set_id` with embedded notes

**Steps**:
```javascript
// 1. Search within Python set
search_notes({
  query: "clean code patterns",
  embedding_set_id: "{python_set_id}",
  limit: 10
})
// Expected: returns results ONLY from python_set_id
```

**Expected Results**:
- Results ONLY from `python_set_id`
- `py_dataclass_note_id` included (if similarity threshold met)
- Rust and meeting notes excluded (not in set)
- Guaranteed data isolation

**Verification**:
```javascript
// Verify no cross-contamination
search_notes({
  query: "ownership",
  embedding_set_id: "{python_set_id}",
  limit: 10
})
// Should NOT return rust_ownership_note_id
```

**Pass Criteria**:
- Search scoped to embedding set only
- No results from outside set

---

### CHAIN-041: Update Model Configuration

**MCP Tool**: `refresh_embedding_set`

**Description**: Change embedding model config for set

**Prerequisites**:
- `python_set_id` with existing embeddings

**Steps**:
```javascript
// 1. Refresh embedding set with new config
refresh_embedding_set({
  set_id: "{python_set_id}",
  truncate_dim: 128
})
// Expected: re-embedding started, returns job_id
```

**Expected Results**:
- Model config update accepted
- Re-embedding job started
- Old embeddings marked stale

**Pass Criteria**:
- Model config updated

---

### CHAIN-042: Re-Embed and Compare Results

**MCP Tools**: `search_notes`, `get_embedding_set`

**Description**: Trigger re-embedding and compare search results

**Prerequisites**:
- Updated model config from CHAIN-041

**Steps**:
```javascript
// 1. Wait for re-embedding (job from CHAIN-041)
// Poll or wait ~10 seconds

// 2. Search again with new embeddings
search_notes({
  query: "clean code patterns",
  embedding_set_id: "{python_set_id}",
  limit: 10
})
// Expected: returns results with new embeddings (128-dim)

// 3. Compare similarity scores with original search (CHAIN-040)
// Scores may differ slightly due to dimension change
```

**Expected Results**:
- Re-embedding completes successfully
- New embeddings use truncate_dim=128
- Search results may differ slightly (due to dimension change)
- Storage reduced (128-dim vs 256-dim)

**Verification**:
```javascript
// Check embedding set details
get_embedding_set({ set_id: "{python_set_id}" })
// Verify model config shows truncate_dim: 128
```

**Pass Criteria**:
- Re-embedding with new config successful
- Dimension reduction applied
- Search still works with new embeddings

---

### CHAIN-042b: Chain 7 Error — Create Embedding Set with Invalid Config

**Isolation**: Required

**MCP Tool**: `create_embedding_set`

**Description**: Attempt to create an embedding set referencing a non-existent embedding config.

```javascript
create_embedding_set({
  slug: "uat-error-test-set",
  name: "Error Test Set",
  embedding_config_id: "00000000-0000-0000-0000-000000000000"
})
```

**Pass Criteria**: Returns **400 Bad Request** or **404 Not Found** — embedding config does not exist. No orphaned embedding set created.

---

**Chain 7 Summary**:
- Total steps: 7
- Features exercised: Embedding set creation, inclusion criteria, auto-population, focused search, model config update, re-embedding
- Success criteria: Embedding sets provide guaranteed data isolation

---

## Chain 8: Full Observability

**Scenario**: Health check → Knowledge stats → Identify issues → Remediate

**Duration**: ~4 minutes

---

### CHAIN-043: Get Knowledge Health Score

**MCP Tool**: `get_knowledge_health`

**Description**: Check overall knowledge base health

**Prerequisites**:
- Data from all previous chains exists

**Steps**:
```javascript
// 1. Get health score
get_knowledge_health({})
// Expected: returns health_score (0.0 to 1.0) with metrics
```

**Expected Results**:
- Health score returned (0.0 to 1.0)
- Metrics include:
  - total_notes
  - notes_with_tags (percentage)
  - notes_with_embeddings (percentage)
  - orphan_notes (count)
  - stale_embeddings (count)
  - broken_links (count)

**Store**: Initial health score

**Verification**:
```javascript
// Check metrics
// health_score should be > 0.7 (healthy)
// total_notes should be > 20 (from all chains)
```

**Pass Criteria**:
- Health endpoint returns metrics

---

### CHAIN-044: Identify Orphan Tags

**MCP Tool**: `get_orphan_tags`

**Description**: Find tags not used by any notes

**Prerequisites**:
- Knowledge base with tags from chains

**Steps**:
```javascript
// 1. Get orphan tags
get_orphan_tags({})
// Expected: returns list of tags with usage_count: 0
```

**Expected Results**:
- List of tags with usage_count: 0
- Tags created but not attached to notes
- Recommendations for cleanup

**Verification**:
```javascript
// Check for orphans
// May be 0 if all tags in use
```

**Pass Criteria**:
- Orphan detection works

---

### CHAIN-045: Identify Stale and Unlinked Notes

**MCP Tools**: `get_stale_notes`, `get_unlinked_notes`

**Description**: Find notes without links or embeddings

**Prerequisites**:
- Knowledge base from chains

**Steps**:
```javascript
// 1. Get stale notes (no embeddings)
get_stale_notes({})
// Expected: returns list of notes without embeddings

// 2. Get unlinked notes (no connections)
get_unlinked_notes({})
// Expected: returns list of isolated notes
```

**Expected Results**:
- Stale embeddings list (if any exist)
- Unlinked notes list (isolated notes)
- Each includes note_id, title, created_at

**Verification**:
```javascript
// Check for issues
// Verify returned arrays
```

**Pass Criteria**:
- Issue detection identifies stale/unlinked content

---

### CHAIN-046: Export Knowledge Health Report

**MCP Tools**: `health_check`, `get_knowledge_health`

**Description**: Generate comprehensive health report

**Prerequisites**:
- Health data from previous steps

**Steps**:
```javascript
// 1. Get comprehensive health check
health_check({})
// Expected: returns system health including DB, services, knowledge stats

// 2. Get full knowledge health
get_knowledge_health({})
// Expected: returns detailed metrics and recommendations
```

**Expected Results**:
- System health status (API, DB, services)
- Knowledge health metrics
- Issue counts and recommendations
- Overall health score

**Verification**:
```javascript
// Verify comprehensive data
// Check that all components reporting
```

**Pass Criteria**:
- Health reporting comprehensive
- Issues identified accurately

---

### CHAIN-047: Remediate Identified Issues

**MCP Tools**: `reembed_all`, `get_knowledge_health`

**Description**: Clean up orphan tags and re-embed stale notes

**Prerequisites**:
- Issue lists from CHAIN-044 and CHAIN-045

**Steps**:
```javascript
// 1. Re-embed stale notes
reembed_all({})
// Expected: re-embedding job started

// 2. Wait for re-embedding
// Poll or wait ~10 seconds

// 3. Re-check health score
get_knowledge_health({})
// Expected: health score improved or maintained
```

**Expected Results**:
- Stale notes re-embedded
- Health score improved (higher than initial)
- All metrics in healthy ranges

**Verification**:
```javascript
// Compare health scores
// New score should be >= initial score
```

**Pass Criteria**:
- Cleanup operations complete successfully
- Health score improves or stays high

---

### CHAIN-047b: Chain 8 Error — Reembed Non-Existent Embedding Set

**Isolation**: Required

**MCP Tool**: `refresh_embedding_set`

**Description**: Attempt to refresh an embedding set that doesn't exist.

```javascript
refresh_embedding_set({
  set_id: "00000000-0000-0000-0000-000000000000"
})
```

**Pass Criteria**: Returns **404 Not Found** — embedding set does not exist. No side effects.

---

**Chain 8 Summary**:
- Total steps: 6
- Features exercised: Health monitoring, issue detection, health reporting, remediation
- Success criteria: Observability provides actionable insights

---

## Cleanup

### CHAIN-048: Delete All Chain Test Data

**MCP Tools**: `list_notes`, `delete_note`, `delete_collection`, `list_concept_schemes`

**Description**: Clean up all test data created during feature chains

**Prerequisites**:
- All chain tests completed

**Steps**:
```javascript
// 1. Delete notes by tag (chain 1)
list_notes({ tags: ["uat/chain1"] })
// For each note_id: delete_note({ note_id })

// 2. Delete notes by tag (chain 2)
list_notes({ tags: ["uat/chain2"] })
// For each note_id: delete_note({ note_id })

// 3. Delete notes by tag (chain 3)
list_notes({ tags: ["uat/chain3"] })
// For each note_id: delete_note({ note_id })

// 4. Delete notes by tag (chain 4)
list_notes({ tags: ["uat/chain4"] })
// For each note_id: delete_note({ note_id })

// 5. Delete notes by tag (chain 5)
list_notes({ tags: ["uat/chain5"] })
// For each note_id: delete_note({ note_id })

// 6. Delete notes by tag (chain 6)
list_notes({ tags: ["uat/chain6"] })
// For each note_id: delete_note({ note_id })

// 7. Delete notes by tag (chain 7)
list_notes({ tags: ["uat/chain7"] })
// For each note_id: delete_note({ note_id })

// 8. Delete collections
delete_collection({ collection_id: "{code_collection_id}" })
delete_collection({ collection_id: "{root_collection_id}" })

// 9. Delete PKE keysets
// (Note: keyset deletion not in MCP tools list)
// Manual cleanup or archive keysets

// 10. Verify cleanup
list_notes({ tags: ["uat/chain1"] })
// Expected: empty array
```

**Expected Results**:
- All test data deleted
- Database returned to pre-chain state
- No orphaned resources

**Verification**:
```javascript
// Verify cleanup
list_notes({ tags: ["uat/chain1"] })
// Should return empty array

// Verify SKOS scheme cleanup
list_concept_schemes({})
// test-uat-taxonomy should not appear (if deleted)
```

**Pass Criteria**:
- All test data removed
- No cleanup errors

---

## Phase Summary

| Chain | Name | Steps | MCP Tool(s) | Status |
|-------|------|-------|-------------|--------|
| Chain 1 | Document Lifecycle | 7 | `upload_attachment`, `create_note`, `get_note`, `detect_document_type`, `list_document_types`, `list_embedding_sets`, `get_embedding_set`, `search_notes`, `list_note_versions`, `restore_note_version`, `diff_note_versions`, `export_note` | |
| Chain 2 | Geo-Temporal Memory | 7 | `create_note`, `upload_attachment`, `get_memory_provenance`, `search_memories_by_location`, `search_memories_by_time`, `search_memories_combined` | |
| Chain 3 | Knowledge Organization | 8 | `create_concept_scheme`, `create_concept`, `add_broader`, `get_narrower`, `create_collection`, `tag_note_concept`, `move_note_to_collection`, `search_notes`, `explore_graph`, `export_skos_turtle`, `knowledge_shard` | |
| Chain 4 | Multilingual Search | 7 | `create_note`, `search_notes` | |
| Chain 5 | Encryption & Sharing | 7 | `pke_create_keyset`, `create_note`, `pke_encrypt`, `get_note`, `pke_get_address`, `pke_decrypt` | |
| Chain 6 | Backup & Recovery | 6 | `create_note`, `get_note_links`, `database_snapshot`, `backup_status`, `delete_note`, `list_notes`, `database_restore`, `get_note` | |
| Chain 7 | Embedding Set Focus | 7 | `create_embedding_set`, `create_note`, `get_embedding_set`, `search_notes`, `refresh_embedding_set` | |
| Chain 8 | Full Observability | 6 | `get_knowledge_health`, `get_orphan_tags`, `get_stale_notes`, `get_unlinked_notes`, `health_check`, `reembed_all` | |
| Cleanup | Delete Test Data | 1 | `list_notes`, `delete_note`, `delete_collection`, `list_concept_schemes` | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Total Steps**: 56
**Total Duration**: ~45 minutes
**Features Integrated**: 25+ features across 8 workflows

**Notes**:
- All chains must pass for phase to pass
- Document any failures with error messages
- Store all intermediate IDs for debugging
- Chains build on previous phase features
- This phase validates end-to-end system integration
- MCP tools used throughout (no REST API calls)
- Some operations (like direct embedding inspection) are inferred via search results
- PKE keyset deletion and SKOS scheme deletion may need manual cleanup if MCP tools don't expose delete endpoints
