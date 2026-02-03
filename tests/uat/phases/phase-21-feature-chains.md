# UAT Phase 21: Feature Chain Testing (End-to-End)

**Purpose**: Verify complete workflows that combine multiple system capabilities in realistic user scenarios
**Duration**: ~45 minutes
**Prerequisites**: All previous UAT phases completed, test data available
**Critical**: Yes (100% pass required)

---

## Overview

This phase tests end-to-end workflows that chain together 3+ features. Each chain exercises a realistic user scenario that demonstrates the system working as an integrated whole, not just isolated features.

**Test Methodology**:
- Use `curl` for REST API calls to `http://localhost:3000`
- Use `MCP: tool_name(params)` notation for MCP server operations
- Each chain includes setup, execution, verification, and cleanup
- Store intermediate IDs for cross-chain verification

---

## Chain 1: Document Lifecycle

**Scenario**: Upload code file → Detect type → Create note → AI revision → Embed → Search → Version → Export

**Duration**: ~6 minutes

---

### UAT-21-001: Upload Python Code File

**Description**: Upload a Python source file from test data

**Prerequisites**:
- API server running on `localhost:3000`
- Test file exists: `/home/roctinam/dev/matric-memory/tests/uat/data/documents/code-python.py`

**Steps**:
```bash
# 1. Upload Python file
curl -X POST http://localhost:3000/api/v1/notes/upload \
  -H "Content-Type: multipart/form-data" \
  -F "file=@/home/roctinam/dev/matric-memory/tests/uat/data/documents/code-python.py" \
  -F "tags=uat/chain1,python,code" \
  -F "revision_mode=ai"
```

**Expected Results**:
- HTTP 201 Created
- Response includes `note_id` (UUIDv7)
- `document_type_name: "python"`
- `chunking_strategy: "syntactic"`
- `status: "processing"` (AI revision queued)

**Verification**:
```bash
# Get note details
curl http://localhost:3000/api/v1/notes/{note_id}

# Verify document type
jq '.note.document_type_name' # Should be "python"
jq '.note.chunks | length' # Should be > 0 (multiple code chunks)
```

**Store**: `python_note_id`

**Pass Criteria**:
- File uploaded successfully
- Document type detected as "python"
- Syntactic chunking applied (multiple chunks created)
- AI revision job queued

---

### UAT-21-002: Verify AI Revision Completion

**Description**: Wait for and verify AI revision job completes

**Prerequisites**:
- `python_note_id` from UAT-21-001
- Background job worker running

**Steps**:
```bash
# 1. Wait for job completion (poll every 2s, max 30s)
for i in {1..15}; do
  STATUS=$(curl -s http://localhost:3000/api/v1/jobs/note/{python_note_id} | jq -r '.status')
  if [ "$STATUS" = "completed" ]; then
    echo "Job completed"
    break
  fi
  sleep 2
done

# 2. Get revised content
curl http://localhost:3000/api/v1/notes/{python_note_id}/revisions/latest
```

**Expected Results**:
- Job status transitions: `queued` → `processing` → `completed`
- Latest revision has `revision_type: "ai"`
- Revised content differs from original (improvements applied)
- `revised_at` timestamp present

**Verification**:
```bash
# Check revision count
curl http://localhost:3000/api/v1/notes/{python_note_id}/revisions | jq 'length'
# Should be >= 2 (original + AI revision)
```

**Pass Criteria**:
- AI revision completes within 30 seconds
- Revised content is semantically similar but improved
- Revision history preserved

---

### UAT-21-003: Verify Automatic Embedding

**Description**: Verify note was automatically embedded after revision

**Prerequisites**:
- `python_note_id` with completed AI revision
- Default embedding set configured

**Steps**:
```bash
# 1. Check embedding status
curl http://localhost:3000/api/v1/notes/{python_note_id}/embeddings

# 2. Verify embedding in default set
curl http://localhost:3000/api/v1/embedding-sets/default/notes/{python_note_id}
```

**Expected Results**:
- At least 1 embedding exists
- `embedding_set_id` is the default set
- `dimensions: 768` (or configured dimension)
- `model_name: "nomic-embed-text-v1.5"` (or configured model)
- `embedded_at` timestamp present

**Verification**:
```bash
# Verify vector exists
curl http://localhost:3000/api/v1/notes/{python_note_id}/embeddings | jq '.[0].vector | length'
# Should match configured dimensions
```

**Pass Criteria**:
- Note embedded automatically after revision
- Embedding vector has correct dimensions
- Embedding linked to default set

---

### UAT-21-004: Semantic Search for Code

**Description**: Search for the note using semantic similarity

**Prerequisites**:
- `python_note_id` with embedding

**Steps**:
```bash
# 1. Semantic search for related concepts
curl -X POST http://localhost:3000/api/v1/search/semantic \
  -H "Content-Type: application/json" \
  -d '{
    "query": "data processing with transformation pipeline",
    "limit": 10,
    "threshold": 0.6
  }'
```

**Expected Results**:
- Results array contains `python_note_id`
- `similarity_score` >= 0.6
- Results ordered by similarity (descending)
- Total results > 0

**Verification**:
```bash
# Verify result includes our note
jq '.results[] | select(.note_id == "{python_note_id}")'
# Should return match with similarity score
```

**Pass Criteria**:
- Semantic search finds the uploaded Python code
- Similarity score indicates strong match
- Results ranked correctly

---

### UAT-21-005: Compare Versions (Original vs Revised)

**Description**: List and compare note versions

**Prerequisites**:
- `python_note_id` with multiple revisions

**Steps**:
```bash
# 1. List all revisions
curl http://localhost:3000/api/v1/notes/{python_note_id}/revisions

# 2. Get original version
curl http://localhost:3000/api/v1/notes/{python_note_id}/revisions/0

# 3. Get latest revision
curl http://localhost:3000/api/v1/notes/{python_note_id}/revisions/latest

# 4. Diff between versions
curl http://localhost:3000/api/v1/notes/{python_note_id}/revisions/diff?from=0&to=latest
```

**Expected Results**:
- Revision list shows 2+ versions (0=original, 1=AI revised)
- Each revision has unique `revision_id`
- Diff shows additions/deletions between versions
- Original content preserved in revision 0

**Verification**:
```bash
# Verify revision metadata
jq '.revisions[0].revision_type' # Should be "original"
jq '.revisions[1].revision_type' # Should be "ai"
```

**Pass Criteria**:
- Version history complete and accessible
- Diff accurately shows changes
- Original content retrievable

---

### UAT-21-006: Export as Markdown with Frontmatter

**Description**: Export note as markdown with YAML frontmatter

**Prerequisites**:
- `python_note_id` fully processed

**Steps**:
```bash
# 1. Export as markdown
curl http://localhost:3000/api/v1/notes/{python_note_id}/export/markdown \
  -H "Accept: text/markdown" \
  -o exported.md

# 2. Verify frontmatter
head -20 exported.md
```

**Expected Results**:
- File starts with `---` (YAML frontmatter delimiter)
- Frontmatter includes:
  - `id: {python_note_id}`
  - `tags: [uat/chain1, python, code]`
  - `created_at: <timestamp>`
  - `document_type: python`
- Markdown content follows frontmatter
- Code blocks properly formatted with ` ```python ` fences

**Verification**:
```bash
# Extract and validate frontmatter
awk '/^---$/{i++}i==1' exported.md | grep -q "id:"
echo $? # Should be 0 (found)
```

**Pass Criteria**:
- Export produces valid markdown
- YAML frontmatter complete and parseable
- Content preserves code structure

---

**Chain 1 Summary**:
- Total steps: 6
- Features exercised: File upload, document type detection, AI revision, embedding, semantic search, versioning, export
- Success criteria: All steps pass with expected results

---

## Chain 2: Geo-Temporal Memory

**Scenario**: Upload GPS image → Extract EXIF → Create memory → Search by location → Search by time → Provenance chain

**Duration**: ~5 minutes

---

### UAT-21-007: Upload Image with GPS EXIF

**Description**: Upload photo with embedded GPS coordinates

**Prerequisites**:
- Test file exists: `/home/roctinam/dev/matric-memory/tests/uat/data/images/paris-eiffel-tower.jpg`
- PostGIS extension enabled

**Steps**:
```bash
# 1. Create note with GPS photo
curl -X POST http://localhost:3000/api/v1/notes/upload \
  -F "file=@/home/roctinam/dev/matric-memory/tests/uat/data/images/paris-eiffel-tower.jpg" \
  -F "tags=uat/chain2,paris,travel" \
  -F "revision_mode=none"
```

**Expected Results**:
- HTTP 201 Created
- Response includes `note_id` and `attachment_id`
- `content_type: "image/jpeg"`
- EXIF extraction job queued

**Store**: `paris_note_id`, `paris_attachment_id`

**Pass Criteria**:
- Image uploaded successfully
- Attachment record created

---

### UAT-21-008: Verify GPS Extraction

**Description**: Verify GPS coordinates extracted from EXIF

**Prerequisites**:
- `paris_attachment_id` from UAT-21-007
- Wait 5 seconds for EXIF extraction

**Steps**:
```bash
# 1. Wait for EXIF extraction
sleep 5

# 2. Get attachment with metadata
curl http://localhost:3000/api/v1/attachments/{paris_attachment_id}
```

**Expected Results**:
- `extracted_metadata.gps.latitude: 48.8584`
- `extracted_metadata.gps.longitude: 2.2945`
- `extracted_metadata.gps.altitude: 35.0`
- `extracted_metadata.datetime_original: "2024-07-14T12:00:00Z"`
- `extracted_metadata.camera.make: "Canon"`

**Verification**:
```bash
# Check GPS extraction
jq '.attachment.extracted_metadata.gps'
# Should show lat/lon/altitude
```

**Pass Criteria**:
- GPS coordinates extracted accurately
- DateTime parsed correctly
- Camera metadata present

---

### UAT-21-009: Verify Provenance Record Created

**Description**: Verify W3C PROV provenance edge created for attachment

**Prerequisites**:
- `paris_attachment_id` with EXIF metadata

**Steps**:
```bash
# 1. Query provenance via MCP
MCP: get_attachment_provenance({
  attachment_id: "{paris_attachment_id}"
})

# Alternative: Direct SQL query
psql -c "
  SELECT
    ST_AsText(pl.point::geometry) as location,
    fp.capture_time,
    fp.event_type
  FROM file_provenance fp
  JOIN prov_location pl ON fp.location_id = pl.id
  WHERE fp.attachment_id = '{paris_attachment_id}'
"
```

**Expected Results**:
- Provenance record exists
- `location: POINT(2.2945 48.8584)`
- `capture_time` range includes `2024-07-14T12:00:00Z`
- `event_type: "photo"`

**Pass Criteria**:
- Provenance chain created automatically
- Spatial and temporal data preserved

---

### UAT-21-010: Search by Location (1km radius)

**Description**: Search for memories near Eiffel Tower

**Prerequisites**:
- `paris_attachment_id` with provenance

**Steps**:
```bash
# 1. Search within 1km of Eiffel Tower
curl -X POST http://localhost:3000/api/v1/search/location \
  -H "Content-Type: application/json" \
  -d '{
    "latitude": 48.8584,
    "longitude": 2.2945,
    "radius_meters": 1000,
    "limit": 10
  }'
```

**Expected Results**:
- Results array includes `paris_attachment_id`
- `distance_m < 1000.0`
- `filename: "paris-eiffel-tower.jpg"`
- Results ordered by distance

**Verification**:
```bash
# Verify our attachment in results
jq '.results[] | select(.attachment_id == "{paris_attachment_id}")'
```

**Pass Criteria**:
- Spatial search finds attachment
- Distance calculated correctly
- Results within specified radius

---

### UAT-21-011: Search by Time Range

**Description**: Search for memories in July 2024

**Prerequisites**:
- `paris_attachment_id` captured on 2024-07-14

**Steps**:
```bash
# 1. Search within July 2024
curl -X POST http://localhost:3000/api/v1/search/temporal \
  -H "Content-Type: application/json" \
  -d '{
    "start_time": "2024-07-01T00:00:00Z",
    "end_time": "2024-07-31T23:59:59Z",
    "limit": 10
  }'
```

**Expected Results**:
- Results array includes `paris_attachment_id`
- `capture_time` within specified range
- Temporal ordering (chronological)

**Pass Criteria**:
- Temporal search finds attachment
- Time filtering accurate
- Results ordered by time

---

### UAT-21-012: Retrieve Full Provenance Chain

**Description**: Get complete provenance chain for attachment

**Prerequisites**:
- `paris_attachment_id` with provenance

**Steps**:
```bash
# 1. Get provenance chain
MCP: get_provenance_chain({
  entity_id: "{paris_attachment_id}",
  depth: 3
})
```

**Expected Results**:
- Provenance graph includes:
  - `entity` (attachment)
  - `activity` (photo capture)
  - `agent` (camera device)
  - `location` (Eiffel Tower coordinates)
  - `time` (capture timestamp)
- W3C PROV relationships: `wasGeneratedBy`, `wasAttributedTo`, `atLocation`, `atTime`

**Pass Criteria**:
- Complete provenance chain retrievable
- All W3C PROV elements present
- Graph structure valid

---

**Chain 2 Summary**:
- Total steps: 6
- Features exercised: Image upload, EXIF extraction, provenance tracking, spatial search, temporal search, provenance chain
- Success criteria: GPS metadata extracted and searchable

---

## Chain 3: Knowledge Organization

**Scenario**: Create taxonomy → Tag notes → Collection hierarchy → Strict filter → Search → Graph explore → Export shard

**Duration**: ~7 minutes

---

### UAT-21-013: Create SKOS Concept Scheme

**Description**: Create a SKOS taxonomy for UAT testing

**Prerequisites**:
- API server running

**Steps**:
```bash
# 1. Create concept scheme
curl -X POST http://localhost:3000/api/v1/skos/schemes \
  -H "Content-Type: application/json" \
  -d '{
    "id": "test-uat-taxonomy",
    "title": "UAT Testing Taxonomy",
    "description": "Hierarchical taxonomy for UAT feature chain testing",
    "creator": "UAT Chain 3"
  }'
```

**Expected Results**:
- HTTP 201 Created
- `scheme_id: "test-uat-taxonomy"`
- `created_at` timestamp

**Store**: `scheme_id = "test-uat-taxonomy"`

**Pass Criteria**:
- Concept scheme created successfully

---

### UAT-21-014: Create Hierarchical Concepts

**Description**: Create broader/narrower concept relationships

**Prerequisites**:
- `scheme_id` from UAT-21-013

**Steps**:
```bash
# 1. Create top concept (Programming)
curl -X POST http://localhost:3000/api/v1/skos/schemes/{scheme_id}/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "id": "programming",
    "pref_label": "Programming",
    "definition": "Software development and coding",
    "is_top_concept": true
  }'

# 2. Create narrower concept (Languages)
curl -X POST http://localhost:3000/api/v1/skos/schemes/{scheme_id}/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "id": "programming-languages",
    "pref_label": "Programming Languages",
    "definition": "Different programming languages",
    "broader": ["programming"]
  }'

# 3. Create narrower concepts (Python, Rust)
curl -X POST http://localhost:3000/api/v1/skos/schemes/{scheme_id}/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "id": "python",
    "pref_label": "Python",
    "alt_labels": ["Python3", "Py"],
    "definition": "Python programming language",
    "broader": ["programming-languages"]
  }'

curl -X POST http://localhost:3000/api/v1/skos/schemes/{scheme_id}/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "id": "rust",
    "pref_label": "Rust",
    "definition": "Rust programming language",
    "broader": ["programming-languages"]
  }'
```

**Expected Results**:
- 4 concepts created
- Hierarchy: `programming` → `programming-languages` → `python` / `rust`
- Broader/narrower relationships established

**Verification**:
```bash
# Get concept hierarchy
curl http://localhost:3000/api/v1/skos/schemes/{scheme_id}/concepts/programming/narrower
# Should show programming-languages

curl http://localhost:3000/api/v1/skos/schemes/{scheme_id}/concepts/programming-languages/narrower
# Should show python, rust
```

**Store**: `concept_python`, `concept_rust`

**Pass Criteria**:
- Concept hierarchy created
- Broader/narrower relationships correct

---

### UAT-21-015: Create Collection Hierarchy

**Description**: Create nested collections for organizing notes

**Prerequisites**:
- API server running

**Steps**:
```bash
# 1. Create root collection
curl -X POST http://localhost:3000/api/v1/collections \
  -H "Content-Type: application/json" \
  -d '{
    "name": "UAT Projects",
    "description": "Root collection for UAT testing"
  }'

# 2. Create child collection
curl -X POST http://localhost:3000/api/v1/collections \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Code Samples",
    "description": "Collection of code samples",
    "parent_id": "{root_collection_id}"
  }'
```

**Expected Results**:
- 2 collections created
- Parent-child relationship established
- `path` reflects hierarchy (e.g., "/UAT Projects/Code Samples")

**Store**: `root_collection_id`, `code_collection_id`

**Pass Criteria**:
- Collection hierarchy created
- Paths computed correctly

---

### UAT-21-016: Create Tagged Notes in Collections

**Description**: Create notes with SKOS tags and add to collections

**Prerequisites**:
- `scheme_id`, concepts, and collections from previous steps

**Steps**:
```bash
# 1. Create Python note with SKOS tag
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Python Tutorial\n\nLearn Python basics with examples.",
    "tags": ["uat/chain3", "test-uat-taxonomy:python"],
    "revision_mode": "none"
  }'

# 2. Add to Code Samples collection
curl -X POST http://localhost:3000/api/v1/collections/{code_collection_id}/notes \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "{python_tutorial_note_id}"
  }'

# 3. Create Rust note with SKOS tag
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Rust Guide\n\nSafe systems programming with Rust.",
    "tags": ["uat/chain3", "test-uat-taxonomy:rust"],
    "revision_mode": "none"
  }'

# 4. Add to Code Samples collection
curl -X POST http://localhost:3000/api/v1/collections/{code_collection_id}/notes \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "{rust_guide_note_id}"
  }'
```

**Expected Results**:
- 2 notes created with SKOS tags
- Both notes added to `Code Samples` collection
- Tags include scheme prefix (e.g., `test-uat-taxonomy:python`)

**Store**: `python_tutorial_note_id`, `rust_guide_note_id`

**Pass Criteria**:
- Notes tagged with SKOS concepts
- Notes organized in collections

---

### UAT-21-017: Search with Strict Tag Filtering

**Description**: Search notes using strict SKOS tag filtering

**Prerequisites**:
- Tagged notes from UAT-21-016

**Steps**:
```bash
# 1. Search for Python concept (strict mode)
curl -X POST http://localhost:3000/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "programming",
    "tag_filter": {
      "tags": ["test-uat-taxonomy:python"],
      "mode": "strict"
    },
    "limit": 10
  }'
```

**Expected Results**:
- Results contain ONLY notes tagged with `test-uat-taxonomy:python`
- `python_tutorial_note_id` included
- `rust_guide_note_id` excluded (different tag)
- Tag isolation guaranteed

**Verification**:
```bash
# Verify strict filtering
jq '.results[] | .tags' | grep -q "rust"
echo $? # Should be 1 (not found)
```

**Pass Criteria**:
- Strict tag filtering enforced
- No cross-contamination between tags

---

### UAT-21-018: Explore Knowledge Graph

**Description**: Explore knowledge graph from Python concept (2-hop)

**Prerequisites**:
- `python_tutorial_note_id` and concept hierarchy

**Steps**:
```bash
# 1. Explore graph from Python note
curl -X POST http://localhost:3000/api/v1/graph/explore \
  -H "Content-Type: application/json" \
  -d '{
    "start_node_id": "{python_tutorial_note_id}",
    "max_depth": 2,
    "relationship_types": ["tagged_with", "broader", "narrower", "in_collection"]
  }'
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
```bash
# Check graph structure
jq '.graph.nodes | length' # Should be >= 4
jq '.graph.edges[] | .type' # Should include "tagged_with", "broader", "in_collection"
```

**Pass Criteria**:
- Graph exploration traverses multiple relationship types
- 2-hop depth respected
- All relevant connections included

---

### UAT-21-019: Export Knowledge Shard

**Description**: Export collection as knowledge shard (JSON-LD)

**Prerequisites**:
- `code_collection_id` with notes

**Steps**:
```bash
# 1. Export collection as knowledge shard
curl http://localhost:3000/api/v1/collections/{code_collection_id}/export/shard \
  -H "Accept: application/ld+json" \
  -o code-shard.jsonld
```

**Expected Results**:
- JSON-LD file with `@context` referencing W3C SKOS
- Includes:
  - Collection metadata
  - All notes in collection
  - SKOS concept references
  - Provenance information
- File is valid JSON-LD (parseable)

**Verification**:
```bash
# Validate JSON-LD structure
jq '.["@context"]' code-shard.jsonld # Should include SKOS vocab
jq '.notes | length' code-shard.jsonld # Should be 2 (python + rust)
```

**Pass Criteria**:
- Export produces valid JSON-LD
- All collection data included
- SKOS relationships preserved

---

**Chain 3 Summary**:
- Total steps: 7
- Features exercised: SKOS taxonomy, concept hierarchy, collections, tagging, strict filtering, graph exploration, knowledge shard export
- Success criteria: Knowledge graph navigable and exportable

---

## Chain 4: Multilingual Search Pipeline

**Scenario**: Create multilingual notes → FTS each language → CJK bigram → Emoji trigram → Cross-language discovery

**Duration**: ~5 minutes

---

### UAT-21-020: Create Multilingual Notes

**Description**: Create notes in multiple languages

**Prerequisites**:
- API server running
- FTS multilingual configs enabled

**Steps**:
```bash
# 1. English note
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Running Performance\n\nI love running in the morning. It helps me run faster.",
    "tags": ["uat/chain4", "english"],
    "revision_mode": "none"
  }'

# 2. German note
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Laufen\n\nIch laufe gerne am Morgen. Das Laufen läuft gut.",
    "tags": ["uat/chain4", "german"],
    "revision_mode": "none"
  }'

# 3. Chinese note
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# 北京旅行\n\n我去了北京市和北京大学。北京很美。",
    "tags": ["uat/chain4", "chinese"],
    "revision_mode": "none"
  }'

# 4. Emoji note
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Weekend Fun 🎉\n\nHad a great time at the party! 🎉🎊🎈",
    "tags": ["uat/chain4", "emoji"],
    "revision_mode": "none"
  }'
```

**Expected Results**:
- 4 notes created
- Each has unique `note_id`
- Different language content

**Store**: `en_note_id`, `de_note_id`, `zh_note_id`, `emoji_note_id`

**Pass Criteria**:
- Multilingual notes created successfully

---

### UAT-21-021: Test English Stemming

**Description**: Search with stemming for English content

**Prerequisites**:
- `en_note_id` from UAT-21-020

**Steps**:
```bash
# 1. Search for "run" (should match "running", "runs", "run")
curl -X POST http://localhost:3000/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "run",
    "search_mode": "fts",
    "limit": 10
  }'
```

**Expected Results**:
- Results include `en_note_id`
- FTS stemming matches "running" in content
- Rank score reflects match quality

**Verification**:
```bash
# Verify stemming worked
jq '.results[] | select(.note_id == "{en_note_id}")' | jq '.score'
# Should have high score (stemmed match)
```

**Pass Criteria**:
- English stemming matches word variations
- Search finds "running" when querying "run"

---

### UAT-21-022: Test German Stemming

**Description**: Search with German stemming

**Prerequisites**:
- `de_note_id` from UAT-21-020

**Steps**:
```bash
# 1. Search for "laufen" (should match "laufe", "läuft")
curl -X POST http://localhost:3000/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "laufen",
    "search_mode": "fts",
    "language": "de",
    "limit": 10
  }'
```

**Expected Results**:
- Results include `de_note_id`
- German stemming matches "laufe" and "läuft"
- Language-specific config applied

**Pass Criteria**:
- German stemming works correctly
- Umlaut handling correct (ä, ö, ü)

---

### UAT-21-023: Test CJK Bigram Matching

**Description**: Search Chinese text with bigram indexing

**Prerequisites**:
- `zh_note_id` from UAT-21-020
- `FTS_BIGRAM_CJK=true` enabled

**Steps**:
```bash
# 1. Search for "北京" (should match "北京市", "北京大学")
curl -X POST http://localhost:3000/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "北京",
    "search_mode": "fts",
    "limit": 10
  }'
```

**Expected Results**:
- Results include `zh_note_id`
- Bigram matching finds "北京市" and "北京大学"
- Character-level matching (no word boundaries)

**Verification**:
```bash
# Verify CJK search worked
jq '.results[] | select(.note_id == "{zh_note_id}")'
# Should return match
```

**Pass Criteria**:
- CJK bigram search finds character sequences
- No word segmentation required

---

### UAT-21-024: Test Emoji Trigram Matching

**Description**: Search for emoji using trigram indexing

**Prerequisites**:
- `emoji_note_id` from UAT-21-020
- `FTS_TRIGRAM_FALLBACK=true` enabled

**Steps**:
```bash
# 1. Search for "🎉" emoji
curl -X POST http://localhost:3000/api/v1/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "🎉",
    "search_mode": "fts",
    "limit": 10
  }'
```

**Expected Results**:
- Results include `emoji_note_id`
- Trigram matching finds emoji character
- Multiple emoji occurrences detected

**Pass Criteria**:
- Emoji search works via pg_trgm
- Unicode character matching successful

---

### UAT-21-025: Cross-Language Semantic Discovery

**Description**: Use semantic search to find related notes across languages

**Prerequisites**:
- All multilingual notes from UAT-21-020
- Notes embedded in default set

**Steps**:
```bash
# 1. Semantic search for "running exercise"
curl -X POST http://localhost:3000/api/v1/search/semantic \
  -H "Content-Type: application/json" \
  -d '{
    "query": "running exercise fitness",
    "limit": 10,
    "threshold": 0.5
  }'
```

**Expected Results**:
- Results include both `en_note_id` and `de_note_id`
- Semantic similarity bridges language gap
- English and German notes both ranked highly (both about running)

**Verification**:
```bash
# Check cross-language results
jq '.results[] | select(.tags[] | contains("english") or contains("german"))'
# Should return both EN and DE notes
```

**Pass Criteria**:
- Semantic search finds related content across languages
- Embedding model captures multilingual semantics

---

**Chain 4 Summary**:
- Total steps: 6
- Features exercised: Multilingual FTS, stemming (EN/DE), CJK bigram, emoji trigram, semantic cross-language search
- Success criteria: All language-specific searches work correctly

---

## Chain 5: Encryption & Sharing

**Scenario**: Generate keypair → Create note → Encrypt → Share address → Decrypt → Verify content

**Duration**: ~5 minutes

---

### UAT-21-026: Generate PKE Keyset

**Description**: Generate public/private keypair for encryption

**Prerequisites**:
- API server running

**Steps**:
```bash
# 1. Generate new keyset
curl -X POST http://localhost:3000/api/v1/pke/keysets \
  -H "Content-Type: application/json" \
  -d '{
    "name": "UAT Chain 5 Keys",
    "description": "Test keyset for encryption chain"
  }'
```

**Expected Results**:
- HTTP 201 Created
- Response includes `keyset_id`
- `public_key` (base64-encoded)
- Private key stored securely (not returned)

**Store**: `keyset_id`, `public_key`

**Pass Criteria**:
- Keyset generated successfully
- Public key available for sharing

---

### UAT-21-027: Create Sensitive Note

**Description**: Create note with sensitive content

**Prerequisites**:
- API server running

**Steps**:
```bash
# 1. Create note with sensitive data
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# API Key Storage\n\nAPI_KEY=sk_test_1234567890abcdef\nSECRET=super_secret_password",
    "tags": ["uat/chain5", "sensitive"],
    "revision_mode": "none"
  }'
```

**Expected Results**:
- Note created with `note_id`
- Content stored in plaintext initially

**Store**: `sensitive_note_id`

**Pass Criteria**:
- Note created successfully

---

### UAT-21-028: Encrypt Note with PKE

**Description**: Encrypt note content using PKE keyset

**Prerequisites**:
- `sensitive_note_id` and `keyset_id`

**Steps**:
```bash
# 1. Encrypt note
curl -X POST http://localhost:3000/api/v1/notes/{sensitive_note_id}/encrypt \
  -H "Content-Type: application/json" \
  -d '{
    "keyset_id": "{keyset_id}",
    "algorithm": "xchacha20-poly1305"
  }'
```

**Expected Results**:
- HTTP 200 OK
- `encryption_status: "encrypted"`
- Original content replaced with ciphertext
- Ciphertext format: `MMPKE01:<base64-ciphertext>`

**Verification**:
```bash
# Get encrypted note
curl http://localhost:3000/api/v1/notes/{sensitive_note_id}

# Verify content is encrypted
jq '.note.content' | grep -q "MMPKE01"
echo $? # Should be 0 (encrypted)
```

**Pass Criteria**:
- Note encrypted successfully
- Content not readable in plaintext
- Encryption format correct

---

### UAT-21-029: Generate Share Address

**Description**: Create shareable address for encrypted note

**Prerequisites**:
- `sensitive_note_id` (encrypted)

**Steps**:
```bash
# 1. Create share address
curl -X POST http://localhost:3000/api/v1/notes/{sensitive_note_id}/share \
  -H "Content-Type: application/json" \
  -d '{
    "expires_in_hours": 24,
    "max_views": 5
  }'
```

**Expected Results**:
- Share address created: `mm://note/{share_token}`
- `expires_at` timestamp (24 hours from now)
- `remaining_views: 5`
- Share token is URL-safe base64

**Store**: `share_token`

**Pass Criteria**:
- Share address generated
- Expiration and view limits set

---

### UAT-21-030: Decrypt Note

**Description**: Decrypt note using private key

**Prerequisites**:
- `sensitive_note_id` (encrypted)
- `keyset_id` with private key

**Steps**:
```bash
# 1. Decrypt note
curl -X POST http://localhost:3000/api/v1/notes/{sensitive_note_id}/decrypt \
  -H "Content-Type: application/json" \
  -d '{
    "keyset_id": "{keyset_id}"
  }'
```

**Expected Results**:
- HTTP 200 OK
- Response includes `decrypted_content`
- Content matches original: "API_KEY=sk_test_1234567890abcdef"

**Pass Criteria**:
- Decryption successful
- Content matches original exactly

---

### UAT-21-031: Verify Content Integrity

**Description**: Verify decrypted content matches original

**Prerequisites**:
- Decrypted content from UAT-21-030

**Steps**:
```bash
# 1. Hash original content (from UAT-21-027)
echo -n "# API Key Storage\n\nAPI_KEY=sk_test_1234567890abcdef\nSECRET=super_secret_password" | sha256sum

# 2. Hash decrypted content
echo -n "{decrypted_content}" | sha256sum

# 3. Compare hashes
```

**Expected Results**:
- SHA-256 hashes match exactly
- No data loss or corruption
- Content integrity preserved through encryption cycle

**Pass Criteria**:
- Decrypted content identical to original
- No corruption or data loss

---

**Chain 5 Summary**:
- Total steps: 6
- Features exercised: PKE keyset generation, note encryption, share address, decryption, content verification
- Success criteria: Encryption cycle preserves data integrity

---

## Chain 6: Backup & Recovery

**Scenario**: Create data → Database snapshot → Delete data → Restore snapshot → Verify recovery

**Duration**: ~4 minutes

---

### UAT-21-032: Create Test Data for Backup

**Description**: Create multiple notes with relationships

**Prerequisites**:
- API server running

**Steps**:
```bash
# 1. Create note 1
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Backup Test Note 1\n\nThis is the first test note.",
    "tags": ["uat/chain6", "backup"],
    "revision_mode": "none"
  }'

# 2. Create note 2
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Backup Test Note 2\n\nThis is the second test note.",
    "tags": ["uat/chain6", "backup"],
    "revision_mode": "none"
  }'

# 3. Create note 3 with link to note 1
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Backup Test Note 3\n\nThis links to [[note-1]].",
    "tags": ["uat/chain6", "backup"],
    "revision_mode": "none"
  }'

# 4. Create semantic link between note 1 and note 2
curl -X POST http://localhost:3000/api/v1/notes/{backup_note1_id}/links \
  -H "Content-Type: application/json" \
  -d '{
    "target_note_id": "{backup_note2_id}",
    "link_type": "semantic",
    "similarity_score": 0.85
  }'
```

**Expected Results**:
- 3 notes created
- 1 explicit link (note 3 → note 1)
- 1 semantic link (note 1 ↔ note 2)

**Store**: `backup_note1_id`, `backup_note2_id`, `backup_note3_id`

**Pass Criteria**:
- Test data created with relationships

---

### UAT-21-033: Create Database Snapshot

**Description**: Take database backup snapshot

**Prerequisites**:
- Test data from UAT-21-032

**Steps**:
```bash
# 1. Create backup snapshot
curl -X POST http://localhost:3000/api/v1/admin/backup/snapshot \
  -H "Content-Type: application/json" \
  -d '{
    "name": "uat-chain6-snapshot",
    "description": "Backup before deletion test",
    "include_attachments": true
  }'
```

**Expected Results**:
- HTTP 201 Created
- `snapshot_id` returned
- `created_at` timestamp
- `status: "completed"`
- Snapshot includes database dump and attachment blobs

**Store**: `snapshot_id`

**Verification**:
```bash
# List snapshots
curl http://localhost:3000/api/v1/admin/backup/snapshots

# Verify snapshot exists
jq '.snapshots[] | select(.id == "{snapshot_id}")'
```

**Pass Criteria**:
- Snapshot created successfully
- All data included

---

### UAT-21-034: Delete Test Data

**Description**: Delete the test notes to simulate data loss

**Prerequisites**:
- `backup_note1_id`, `backup_note2_id`, `backup_note3_id`

**Steps**:
```bash
# 1. Delete note 1
curl -X DELETE http://localhost:3000/api/v1/notes/{backup_note1_id}

# 2. Delete note 2
curl -X DELETE http://localhost:3000/api/v1/notes/{backup_note2_id}

# 3. Delete note 3
curl -X DELETE http://localhost:3000/api/v1/notes/{backup_note3_id}

# 4. Verify deletion
curl http://localhost:3000/api/v1/notes?tags=uat/chain6,backup
```

**Expected Results**:
- All 3 notes deleted (HTTP 204 No Content)
- Search for tag `uat/chain6` returns empty array
- Links also deleted (cascade)

**Verification**:
```bash
# Confirm notes gone
jq '.notes | length' # Should be 0
```

**Pass Criteria**:
- All test notes deleted
- No orphaned links remain

---

### UAT-21-035: Restore from Snapshot

**Description**: Restore database from backup snapshot

**Prerequisites**:
- `snapshot_id` from UAT-21-033
- Notes deleted in UAT-21-034

**Steps**:
```bash
# 1. Restore snapshot
curl -X POST http://localhost:3000/api/v1/admin/backup/restore \
  -H "Content-Type: application/json" \
  -d '{
    "snapshot_id": "{snapshot_id}",
    "restore_mode": "full"
  }'

# 2. Wait for restore to complete (may take 10-30 seconds)
sleep 15
```

**Expected Results**:
- HTTP 200 OK
- Restore job status: `completed`
- Database state reverted to snapshot time

**Verification**:
```bash
# Check restore status
curl http://localhost:3000/api/v1/admin/backup/restore/{restore_job_id}/status
```

**Pass Criteria**:
- Restore completes without errors

---

### UAT-21-036: Verify Data Recovery

**Description**: Verify all deleted notes and links are restored

**Prerequisites**:
- Restore completed in UAT-21-035

**Steps**:
```bash
# 1. Search for restored notes
curl http://localhost:3000/api/v1/notes?tags=uat/chain6,backup

# 2. Verify note 1 exists
curl http://localhost:3000/api/v1/notes/{backup_note1_id}

# 3. Verify note 2 exists
curl http://localhost:3000/api/v1/notes/{backup_note2_id}

# 4. Verify note 3 exists
curl http://localhost:3000/api/v1/notes/{backup_note3_id}

# 5. Verify links restored
curl http://localhost:3000/api/v1/notes/{backup_note1_id}/links
```

**Expected Results**:
- All 3 notes restored with correct content
- Tag search returns 3 results
- Semantic link between note 1 and note 2 exists
- Explicit link from note 3 to note 1 exists

**Verification**:
```bash
# Check note count
jq '.notes | length' # Should be 3

# Check links
jq '.links | length' # Should be >= 2
```

**Pass Criteria**:
- All notes recovered completely
- All links intact
- No data loss

---

**Chain 6 Summary**:
- Total steps: 5
- Features exercised: Backup snapshot, data deletion, restore, data integrity verification
- Success criteria: Complete data recovery from backup

---

## Chain 7: Embedding Set Focus

**Scenario**: Create set → Define criteria → Auto-populate → Focused search → Re-embed → Compare results

**Duration**: ~6 minutes

---

### UAT-21-037: Create Focused Embedding Set

**Description**: Create embedding set with tag-based criteria

**Prerequisites**:
- API server running

**Steps**:
```bash
# 1. Create embedding set for Python notes only
curl -X POST http://localhost:3000/api/v1/embedding-sets \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Python Code Set",
    "description": "Focused embedding set for Python code",
    "set_type": "full",
    "inclusion_criteria": {
      "tags": ["python", "code"],
      "tag_match_mode": "any"
    },
    "model_config": {
      "model_name": "nomic-embed-text-v1.5",
      "dimensions": 768,
      "truncate_dim": 256
    },
    "auto_embed_rules": {
      "on_create": true,
      "on_update": true
    }
  }'
```

**Expected Results**:
- HTTP 201 Created
- `embedding_set_id` returned
- `set_type: "full"`
- Auto-embed rules configured

**Store**: `python_set_id`

**Pass Criteria**:
- Embedding set created with criteria

---

### UAT-21-038: Create Matching and Non-Matching Notes

**Description**: Create notes that match and don't match set criteria

**Prerequisites**:
- `python_set_id` from UAT-21-037

**Steps**:
```bash
# 1. Create Python note (should match criteria)
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Python Data Classes\n\nUsing dataclasses for clean code.",
    "tags": ["uat/chain7", "python", "code"],
    "revision_mode": "none"
  }'

# 2. Create Rust note (should NOT match criteria)
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Rust Ownership\n\nUnderstanding ownership in Rust.",
    "tags": ["uat/chain7", "rust", "code"],
    "revision_mode": "none"
  }'

# 3. Create general note (should NOT match criteria)
curl -X POST http://localhost:3000/api/v1/notes \
  -H "Content-Type: application/json" \
  -d '{
    "content": "# Meeting Notes\n\nDiscussed project timeline.",
    "tags": ["uat/chain7", "meeting"],
    "revision_mode": "none"
  }'
```

**Expected Results**:
- 3 notes created
- Only Python note matches `python_set_id` criteria

**Store**: `py_dataclass_note_id`, `rust_ownership_note_id`, `meeting_note_id`

**Pass Criteria**:
- Test notes created

---

### UAT-21-039: Verify Auto-Population

**Description**: Verify only matching notes added to embedding set

**Prerequisites**:
- Notes from UAT-21-038
- Wait 5 seconds for auto-embed

**Steps**:
```bash
# 1. Wait for auto-embed
sleep 5

# 2. List notes in embedding set
curl http://localhost:3000/api/v1/embedding-sets/{python_set_id}/notes
```

**Expected Results**:
- Results include `py_dataclass_note_id`
- Results do NOT include `rust_ownership_note_id`
- Results do NOT include `meeting_note_id`
- Only 1 note in set

**Verification**:
```bash
# Check note count in set
jq '.notes | length' # Should be 1
```

**Pass Criteria**:
- Auto-population respects inclusion criteria
- Only matching notes embedded

---

### UAT-21-040: Focused Search Within Set

**Description**: Search within focused embedding set

**Prerequisites**:
- `python_set_id` with embedded notes

**Steps**:
```bash
# 1. Search within Python set
curl -X POST http://localhost:3000/api/v1/search/semantic \
  -H "Content-Type: application/json" \
  -d '{
    "query": "clean code patterns",
    "embedding_set_id": "{python_set_id}",
    "limit": 10,
    "threshold": 0.5
  }'
```

**Expected Results**:
- Results ONLY from `python_set_id`
- `py_dataclass_note_id` included (if similarity > 0.5)
- Rust and meeting notes excluded (not in set)
- Guaranteed data isolation

**Verification**:
```bash
# Verify no cross-contamination
jq '.results[] | .note_id' | grep -q "{rust_ownership_note_id}"
echo $? # Should be 1 (not found)
```

**Pass Criteria**:
- Search scoped to embedding set only
- No results from outside set

---

### UAT-21-041: Update Model Configuration

**Description**: Change embedding model config for set

**Prerequisites**:
- `python_set_id` with existing embeddings

**Steps**:
```bash
# 1. Update model config (change truncate dimension)
curl -X PATCH http://localhost:3000/api/v1/embedding-sets/{python_set_id} \
  -H "Content-Type: application/json" \
  -d '{
    "model_config": {
      "model_name": "nomic-embed-text-v1.5",
      "dimensions": 768,
      "truncate_dim": 128
    }
  }'
```

**Expected Results**:
- HTTP 200 OK
- `model_config.truncate_dim` updated to 128
- Re-embedding required (old embeddings marked stale)

**Pass Criteria**:
- Model config updated

---

### UAT-21-042: Re-Embed and Compare Results

**Description**: Trigger re-embedding and compare search results

**Prerequisites**:
- Updated model config from UAT-21-041

**Steps**:
```bash
# 1. Trigger re-embedding
curl -X POST http://localhost:3000/api/v1/embedding-sets/{python_set_id}/re-embed

# 2. Wait for re-embedding
sleep 10

# 3. Search again with new embeddings
curl -X POST http://localhost:3000/api/v1/search/semantic \
  -H "Content-Type: application/json" \
  -d '{
    "query": "clean code patterns",
    "embedding_set_id": "{python_set_id}",
    "limit": 10,
    "threshold": 0.5
  }'

# 4. Compare similarity scores with original search (UAT-21-040)
```

**Expected Results**:
- Re-embedding completes successfully
- New embeddings use truncate_dim=128
- Search results may differ slightly (due to dimension change)
- Storage reduced (128-dim vs 256-dim)

**Verification**:
```bash
# Check embedding dimensions
curl http://localhost:3000/api/v1/embedding-sets/{python_set_id}/notes/{py_dataclass_note_id}/embedding \
  | jq '.vector | length'
# Should be 128 (truncated)
```

**Pass Criteria**:
- Re-embedding with new config successful
- Dimension reduction applied
- Search still works with new embeddings

---

**Chain 7 Summary**:
- Total steps: 6
- Features exercised: Embedding set creation, inclusion criteria, auto-population, focused search, model config update, re-embedding
- Success criteria: Embedding sets provide guaranteed data isolation

---

## Chain 8: Full Observability

**Scenario**: Health check → Knowledge stats → Identify issues → Remediate

**Duration**: ~4 minutes

---

### UAT-21-043: Get Knowledge Health Score

**Description**: Check overall knowledge base health

**Prerequisites**:
- Data from all previous chains exists

**Steps**:
```bash
# 1. Get health score
curl http://localhost:3000/api/v1/observability/health/knowledge
```

**Expected Results**:
- HTTP 200 OK
- `health_score` (0.0 to 1.0)
- Metrics include:
  - `total_notes`
  - `notes_with_tags` (percentage)
  - `notes_with_embeddings` (percentage)
  - `orphan_notes` (count)
  - `stale_embeddings` (count)
  - `broken_links` (count)

**Store**: Initial health score

**Verification**:
```bash
# Check metrics
jq '.metrics.total_notes' # Should be > 20 (from all chains)
jq '.metrics.health_score' # Should be > 0.7 (healthy)
```

**Pass Criteria**:
- Health endpoint returns metrics

---

### UAT-21-044: Identify Orphan Tags

**Description**: Find tags not used by any notes

**Prerequisites**:
- Knowledge base with tags from chains

**Steps**:
```bash
# 1. Get orphan tags
curl http://localhost:3000/api/v1/observability/issues/orphan-tags
```

**Expected Results**:
- List of tags with `usage_count: 0`
- Tags created but not attached to notes
- Recommendations for cleanup

**Verification**:
```bash
# Check for orphans
jq '.orphan_tags | length'
# May be 0 if all tags in use
```

**Pass Criteria**:
- Orphan detection works

---

### UAT-21-045: Identify Stale and Unlinked Notes

**Description**: Find notes without links or embeddings

**Prerequisites**:
- Knowledge base from chains

**Steps**:
```bash
# 1. Get stale notes (no embeddings)
curl http://localhost:3000/api/v1/observability/issues/stale-embeddings

# 2. Get unlinked notes (no connections)
curl http://localhost:3000/api/v1/observability/issues/unlinked-notes
```

**Expected Results**:
- Stale embeddings list (if any exist)
- Unlinked notes list (isolated notes)
- Each includes `note_id`, `title`, `created_at`

**Verification**:
```bash
# Check for issues
jq '.stale_embeddings | length'
jq '.unlinked_notes | length'
```

**Pass Criteria**:
- Issue detection identifies stale/unlinked content

---

### UAT-21-046: Get Tag Cooccurrence Matrix

**Description**: Analyze tag relationships and clusters

**Prerequisites**:
- Notes with tags from all chains

**Steps**:
```bash
# 1. Get tag cooccurrence matrix
curl http://localhost:3000/api/v1/observability/stats/tag-cooccurrence \
  -H "Content-Type: application/json" \
  -d '{
    "min_cooccurrence": 2,
    "limit": 50
  }'
```

**Expected Results**:
- Matrix showing tag pairs that occur together
- Format: `{tag1, tag2, count}`
- Example: `{python, code, 5}` (5 notes tagged with both)
- Insights into tag clustering

**Verification**:
```bash
# Check matrix
jq '.cooccurrences[] | select(.tag1 == "python" and .tag2 == "code")'
# Should show cooccurrence count
```

**Pass Criteria**:
- Tag cooccurrence analysis works
- Matrix shows relationships

---

### UAT-21-047: Remediate Identified Issues

**Description**: Clean up orphan tags and re-embed stale notes

**Prerequisites**:
- Issue lists from UAT-21-044 and UAT-21-045

**Steps**:
```bash
# 1. Delete orphan tags (if any exist)
curl -X DELETE http://localhost:3000/api/v1/tags/cleanup/orphans

# 2. Re-embed stale notes
curl -X POST http://localhost:3000/api/v1/embeddings/re-embed-stale

# 3. Wait for re-embedding
sleep 10

# 4. Re-check health score
curl http://localhost:3000/api/v1/observability/health/knowledge
```

**Expected Results**:
- Orphan tags deleted (if any existed)
- Stale notes re-embedded
- Health score improved (higher than initial)
- All metrics in healthy ranges

**Verification**:
```bash
# Compare health scores
# New score should be >= initial score
jq '.metrics.health_score'
```

**Pass Criteria**:
- Cleanup operations complete successfully
- Health score improves or stays high

---

**Chain 8 Summary**:
- Total steps: 5
- Features exercised: Health monitoring, issue detection, tag analysis, remediation
- Success criteria: Observability provides actionable insights

---

## Cleanup

### UAT-21-048: Delete All Chain Test Data

**Description**: Clean up all test data created during feature chains

**Prerequisites**:
- All chain tests completed

**Steps**:
```bash
# 1. Delete all notes with uat/chain* tags
for i in {1..8}; do
  curl -X POST http://localhost:3000/api/v1/notes/bulk-delete \
    -H "Content-Type: application/json" \
    -d "{\"tag_filter\": {\"tags\": [\"uat/chain$i\"], \"mode\": \"strict\"}}"
done

# 2. Delete SKOS concept scheme
curl -X DELETE http://localhost:3000/api/v1/skos/schemes/test-uat-taxonomy

# 3. Delete embedding sets
curl -X DELETE http://localhost:3000/api/v1/embedding-sets/{python_set_id}

# 4. Delete PKE keysets
curl -X DELETE http://localhost:3000/api/v1/pke/keysets/{keyset_id}

# 5. Delete backup snapshot
curl -X DELETE http://localhost:3000/api/v1/admin/backup/snapshots/{snapshot_id}

# 6. Delete collections
curl -X DELETE http://localhost:3000/api/v1/collections/{code_collection_id}
curl -X DELETE http://localhost:3000/api/v1/collections/{root_collection_id}
```

**Expected Results**:
- All test data deleted
- Database returned to pre-chain state
- No orphaned resources

**Verification**:
```bash
# Verify cleanup
curl http://localhost:3000/api/v1/notes?tags=uat/chain1
# Should return empty array

curl http://localhost:3000/api/v1/skos/schemes/test-uat-taxonomy
# Should return 404
```

**Pass Criteria**:
- All test data removed
- No cleanup errors

---

## Phase Summary

| Chain | Name | Steps | Status |
|-------|------|-------|--------|
| Chain 1 | Document Lifecycle | 6 | |
| Chain 2 | Geo-Temporal Memory | 6 | |
| Chain 3 | Knowledge Organization | 7 | |
| Chain 4 | Multilingual Search | 6 | |
| Chain 5 | Encryption & Sharing | 6 | |
| Chain 6 | Backup & Recovery | 5 | |
| Chain 7 | Embedding Set Focus | 6 | |
| Chain 8 | Full Observability | 5 | |
| Cleanup | Delete Test Data | 1 | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Total Steps**: 48
**Total Duration**: ~45 minutes
**Features Integrated**: 25+ features across 8 workflows

**Notes**:
- All chains must pass for phase to pass
- Document any failures with error messages
- Store all intermediate IDs for debugging
- Chains build on previous phase features
- This phase validates end-to-end system integration
