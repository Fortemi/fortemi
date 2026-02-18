# SKOS Hierarchical Tagging System

Fortémi implements a full W3C SKOS-compliant hierarchical tagging system that automatically manages concept tags for all notes. This document covers the architecture, API, and usage patterns.

## Quick Start: Tag Formats

### Flat Path Format (Recommended)

The simplest way to use tags is with hierarchical paths using `/` separator:

```
# Simple tags
archive
reviewed
important

# Hierarchical tags (max 5 levels)
programming/rust
ai/ml/transformers
projects/matric/features/search
topics/computer-science/databases/postgres
```

These paths are automatically converted to proper SKOS concepts with:
- **Broader/narrower relationships** created from the hierarchy
- **Auto-created concepts** for each path component
- **Full SKOS compliance** behind the scenes

### Tag Matching Behavior

**Case-Insensitive Matching**

All tag operations are case-insensitive:
- `Project:Alpha` matches `project:alpha`, `PROJECT:ALPHA`, etc.
- Searches use `LOWER()` comparison for consistent matching

**Hierarchical Filtering**

When filtering by a parent tag, all child tags are included:
- Filtering by `project` returns notes tagged with `project`, `project/alpha`, `project/beta/feature`, etc.
- Uses prefix matching: `tag LIKE 'parent/%'`
- Enables organizing content in nested hierarchies while querying at any level

```bash
# Returns notes with "programming", "programming/rust", "programming/python", etc.
curl "http://localhost:3000/api/v1/notes?tags=programming"

# Returns notes with exact tag only
curl "http://localhost:3000/api/v1/notes?tags=programming/rust"
```

### Examples in API/MCP

```json
// Creating a note with hierarchical tags
{
  "content": "Learning about Rust ownership...",
  "tags": ["programming/rust", "learning", "memory-management"]
}

// Setting tags on a note
{
  "tags": ["ai/ml/deep-learning", "projects/research"]
}
```

### Long Form (SKOS YAML)

For advanced use cases requiring full SKOS properties:

```yaml
pref_label: Machine Learning
alt_labels:
  - ML
  - machine-learning
definition: A subset of AI that enables systems to learn from data
scope_note: Use for supervised, unsupervised, and reinforcement learning
broader:
  - artificial-intelligence
related:
  - deep-learning
  - neural-networks
facet_type: personality
facet_domain: computer-science
```

---

## Overview

The SKOS (Simple Knowledge Organization System) tagging system provides a rich, semantically-aware hierarchical structure. **Tagging is primarily automatic** — the NLP pipeline generates SKOS concept tags for every note during ingestion. Manual tools exist for curation, governance, and corrections rather than day-to-day creation.

Key features include:

- **Automatic AI Tagging**: Notes are automatically tagged with 8-15 hierarchical concepts across 6 required dimensions during the NLP pipeline — no manual tagging needed
- **W3C SKOS Compliance**: Full support for concept schemes, concepts, labels, semantic relations, and mapping relations
- **Tag-Enriched Embeddings**: SKOS tags are embedded into vectors alongside content, making semantic search and linking more accurate
- **Tag-Boosted Linking**: Semantic links blend embedding similarity with SKOS tag overlap for higher-quality connections
- **Hierarchical Relationships**: Broader/narrower/related semantic relations for concept organization
- **PMEST Faceted Classification**: Built-in facets for type, source, domain, status, and scope
- **Anti-pattern Detection**: Automatic warnings for poor tagging practices
- **MCP Tools**: Curation and governance via `manage_tags` and `manage_concepts` tools

## Architecture

### Database Schema

The SKOS schema consists of the following core tables:

```sql
-- Concept Schemes (namespaces/vocabularies)
skos_concept_scheme
  ├── id (UUID, PK)
  ├── notation (unique identifier)
  ├── title
  ├── description
  └── is_active

-- Concepts (the actual tags)
skos_concept
  ├── id (UUID, PK)
  ├── scheme_id (FK)
  ├── notation (auto-generated if null)
  ├── status (candidate, controlled, deprecated)
  ├── is_top_concept
  ├── facet_type, facet_source, facet_domain, facet_scope
  ├── definition, scope_note
  └── antipatterns (detected issues)

-- Labels (pref, alt, hidden)
skos_concept_label
  ├── concept_id (FK)
  ├── label_type (pref_label, alt_label, hidden_label)
  ├── value
  └── language

-- Semantic Relations (broader, narrower, related)
skos_semantic_relation
  ├── from_concept_id (FK)
  ├── to_concept_id (FK)
  └── relation_type (broader, narrower, related)

-- Note Tagging (links notes to concepts)
note_skos_concept
  ├── note_id (FK)
  ├── concept_id (FK)
  ├── source (api, ai_auto, import)
  ├── confidence
  ├── relevance_score
  └── is_primary

-- Collections (grouping without hierarchy)
skos_collection
  ├── id (UUID, PK)
  ├── uri (unique identifier)
  ├── pref_label
  ├── definition
  ├── is_ordered (boolean)
  └── scheme_id (FK, optional)

-- Collection Membership
skos_collection_member
  ├── collection_id (FK)
  ├── concept_id (FK)
  ├── position (for ordered collections)
  └── added_at
```

### Validation Rules (ANSI/NISO Z39.19 Compliant)

The system enforces these validation rules:

| Rule | Limit | Description |
|------|-------|-------------|
| Max Depth | 5 levels | Maximum hierarchy depth |
| Max Breadth | 200 children | Maximum children per concept |
| Max Polyhierarchy | 3 parents | Maximum broader concepts |
| Literary Warrant | 3+ notes | Notes needed before "controlled" status |

### Anti-Pattern Detection

The system automatically detects and warns about these anti-patterns:

- **Over-Nesting** (`over_nesting`): Hierarchy deeper than 4 levels
- **Meta Tags** (`meta_tag`): Generic tags like "important", "todo", "remember"
- **Orphan Tags** (`orphan`): Tags with no associated notes
- **Synonym Sprawl** (`synonym_sprawl`): Similar tags that should be consolidated
- **Mixed Hierarchy** (`mixed_hierarchy`): Tags mixing unrelated domains

## API Reference

### Concept Schemes

```
GET    /api/v1/concepts/schemes           List all schemes
POST   /api/v1/concepts/schemes           Create a new scheme
GET    /api/v1/concepts/schemes/:id       Get scheme by ID
PATCH  /api/v1/concepts/schemes/:id       Update a scheme
GET    /api/v1/concepts/schemes/:id/top-concepts  Get top-level concepts
```

### Concepts

```
GET    /api/v1/concepts                   Search concepts
POST   /api/v1/concepts                   Create a concept
GET    /api/v1/concepts/autocomplete      Autocomplete search
GET    /api/v1/concepts/:id               Get concept by ID
GET    /api/v1/concepts/:id/full          Get concept with all relations
PATCH  /api/v1/concepts/:id               Update a concept
DELETE /api/v1/concepts/:id               Delete a concept (if unused)
```

### Hierarchy Operations

```
GET    /api/v1/concepts/:id/broader       Get broader concepts
POST   /api/v1/concepts/:id/broader       Add broader relation
DELETE /api/v1/concepts/:id/broader/:broader_id  Remove broader relation

GET    /api/v1/concepts/:id/narrower      Get narrower concepts
POST   /api/v1/concepts/:id/narrower      Add narrower relation

GET    /api/v1/concepts/:id/related       Get related concepts
POST   /api/v1/concepts/:id/related       Add related relation
DELETE /api/v1/concepts/:id/related/:related_id  Remove related relation
```

### Collections (W3C SKOS Section 9)

```
GET    /api/v1/concepts/collections       List all collections
POST   /api/v1/concepts/collections       Create a new collection
GET    /api/v1/concepts/collections/:id   Get collection with members
PATCH  /api/v1/concepts/collections/:id   Update collection properties
DELETE /api/v1/concepts/collections/:id   Delete a collection

PUT    /api/v1/concepts/collections/:id/members           Replace all members
POST   /api/v1/concepts/collections/:id/members/:concept_id   Add member
DELETE /api/v1/concepts/collections/:id/members/:concept_id   Remove member
```

### Note Tagging

```
GET    /api/v1/notes/:id/concepts         Get concepts for a note
POST   /api/v1/notes/:id/concepts         Tag note with concept
DELETE /api/v1/notes/:id/concepts/:concept_id  Remove concept from note
```

### Governance

```
GET    /api/v1/concepts/governance/stats  Get governance statistics
```

## SKOS Collections

Collections provide a way to group related concepts without imposing hierarchical relationships. This complements ConceptSchemes (which provide vocabulary namespaces) and semantic relations (which define broader/narrower hierarchies).

### Purpose

- **Thematic Grouping**: Organize concepts by topic, project, or context without hierarchy
- **Learning Paths**: Use ordered collections to define sequences (e.g., "Rust Fundamentals" → "Ownership" → "Lifetimes")
- **Workflows**: Define process steps or stages
- **Reference Sets**: Curate collections of related concepts (e.g., "Programming Languages", "Data Science Tools")

### Ordered vs Unordered Collections

| Type | Use Case | Position Field |
|------|----------|----------------|
| **Unordered** | General grouping, reference sets | NULL |
| **Ordered** | Learning paths, workflows, sequences | Integer (1, 2, 3...) |

### Creating a Collection

```bash
# Create an unordered collection
curl -X POST http://localhost:3000/api/v1/concepts/collections \
  -H "Content-Type: application/json" \
  -d '{
    "uri": "http://matric.local/collections/programming-langs",
    "pref_label": "Programming Languages",
    "definition": "Collection of programming language concepts",
    "is_ordered": false,
    "scheme_id": "scheme-uuid-here"
  }'

# Create an ordered collection (learning path)
curl -X POST http://localhost:3000/api/v1/concepts/collections \
  -H "Content-Type: application/json" \
  -d '{
    "uri": "http://matric.local/collections/rust-learning-path",
    "pref_label": "Rust Learning Path",
    "definition": "Recommended sequence for learning Rust",
    "is_ordered": true,
    "scheme_id": "scheme-uuid-here"
  }'
```

### Adding Members

```bash
# Add a concept to a collection
curl -X POST http://localhost:3000/api/v1/concepts/collections/{collection-id}/members/{concept-id}

# Replace all members at once (useful for reordering)
curl -X PUT http://localhost:3000/api/v1/concepts/collections/{collection-id}/members \
  -H "Content-Type: application/json" \
  -d '{
    "members": [
      {"concept_id": "concept-1-uuid", "position": 1},
      {"concept_id": "concept-2-uuid", "position": 2},
      {"concept_id": "concept-3-uuid", "position": 3}
    ]
  }'

# Remove a member
curl -X DELETE http://localhost:3000/api/v1/concepts/collections/{collection-id}/members/{concept-id}
```

### Reordering Members (Ordered Collections)

For ordered collections, use the `PUT /members` endpoint to replace all members with new positions:

```bash
curl -X PUT http://localhost:3000/api/v1/concepts/collections/{collection-id}/members \
  -H "Content-Type: application/json" \
  -d '{
    "members": [
      {"concept_id": "ownership-uuid", "position": 1},
      {"concept_id": "borrowing-uuid", "position": 2},
      {"concept_id": "lifetimes-uuid", "position": 3}
    ]
  }'
```

### Listing Collections

```bash
# List all collections
curl http://localhost:3000/api/v1/concepts/collections

# Filter by scheme
curl "http://localhost:3000/api/v1/concepts/collections?scheme_id={scheme-uuid}"

# Get collection with members
curl http://localhost:3000/api/v1/concepts/collections/{collection-id}
```

### Example: Learning Path Collection

```bash
# 1. Create an ordered collection for Rust learning
COLLECTION_ID=$(curl -X POST http://localhost:3000/api/v1/concepts/collections \
  -H "Content-Type: application/json" \
  -d '{
    "pref_label": "Rust Fundamentals",
    "is_ordered": true
  }' | jq -r '.id')

# 2. Add concepts in sequence
curl -X PUT http://localhost:3000/api/v1/concepts/collections/$COLLECTION_ID/members \
  -H "Content-Type: application/json" \
  -d '{
    "members": [
      {"concept_id": "basics-uuid", "position": 1},
      {"concept_id": "ownership-uuid", "position": 2},
      {"concept_id": "borrowing-uuid", "position": 3},
      {"concept_id": "lifetimes-uuid", "position": 4},
      {"concept_id": "traits-uuid", "position": 5}
    ]
  }'

# 3. Retrieve the learning path
curl http://localhost:3000/api/v1/concepts/collections/$COLLECTION_ID
```

### Collections vs Hierarchies

| Feature | Collections | Hierarchies (broader/narrower) |
|---------|-------------|-------------------------------|
| Relationship | Membership | Semantic subsumption |
| Ordering | Optional | None |
| Purpose | Grouping, curation | Knowledge organization |
| Constraint | None | Max depth, polyhierarchy limits |
| Example | "ML Algorithms" collection | "Machine Learning" → "Deep Learning" |

Use collections when you want to group concepts thematically without implying a broader/narrower relationship. Use hierarchies when concepts have true subsumption relationships.

## MCP Tools

In core mode (default), SKOS operations are accessed through two consolidated tools focused on **curation and governance** rather than creation:

### `manage_tags` — Tag Curation

Since tags are auto-generated, this tool is primarily for reviewing and adjusting tags:

| Action | Purpose |
|--------|---------|
| `list` | List all tags with usage counts — find orphans or sprawl |
| `set` | Replace a note's user tags (organizational tags, not AI concepts) |
| `tag_concept` | Manually tag a note with a specific SKOS concept (override/addition) |
| `untag_concept` | Remove an incorrect auto-generated concept tag |
| `get_concepts` | Review what concepts the AI assigned to a note |

### `manage_concepts` — Vocabulary Governance & Scheme Management

The concept vocabulary grows automatically as the AI tags notes. This tool manages that vocabulary and its concept schemes:

| Action | Purpose |
|--------|---------|
| `search` | Find concepts in the vocabulary |
| `autocomplete` | Type-ahead concept search |
| `get` / `get_full` | Inspect a concept and its relations |
| `stats` | Governance statistics (orphans, candidates needing review) |
| `top` | View top-level concepts in a scheme |
| `list_schemes` | List all concept schemes |
| `create_scheme` | Create a new concept scheme (taxonomy) |
| `get_scheme` | Get scheme details |
| `update_scheme` | Update scheme metadata |
| `delete_scheme` | Delete a concept scheme |

### Full Mode Tools

With `MCP_TOOL_MODE=full`, granular tools are available for advanced vocabulary management including concept CRUD, hierarchy manipulation (broader/narrower/related), collection management, and scheme administration. See [MCP Reference](./mcp.md) for details.

## Automatic Tagging Pipeline

**Tagging is fully automatic.** When a note is created or revised, the NLP pipeline generates SKOS concept tags without any manual intervention. The system:

1. **Queues the `ConceptTagging` job** as part of the Phase 1 NLP pipeline
2. **Analyzes content with AI** to identify 8-15 hierarchical concept tags across 6 required and 3 optional dimensions
3. **Matches or creates concepts** - Reuses existing concepts where possible; creates new ones as candidates with proper broader/narrower wiring
4. **Tags the note** with relevance scores and marks the first tag as primary
5. **Triggers Phase 2 jobs** - After tagging completes, the system queues Embedding and Linking jobs that use the tags for enrichment

### AI Tagging Dimensions

The concept tagging AI generates tags across these dimensions:

| Dimension | Required | Example |
|-----------|----------|---------|
| **Domain** | Yes | `science/machine-learning` |
| **Topic** | Yes | `nlp/transformers` |
| **Methodology** | Yes | `methodology/experimental` |
| **Application** | Yes | `application/healthcare` |
| **Technique** | Yes | `technique/attention-mechanism` |
| **Content-type** | Yes | `content-type/research-paper` |
| **Evaluation** | No | `evaluation/benchmark` |
| **Tool/Framework** | No | `tool/pytorch` |
| **Era/Context** | No | `era/modern-ai` |

### NLP Pipeline

Concept tagging is part of a coordinated pipeline:

**Phase 1** (runs in parallel):
- AI Revision (content enhancement)
- Title Generation
- **Concept Tagging** (prerequisite for subsequent phases)
- Metadata Extraction
- Document Type Inference

**Phase 2** (after concept tagging completes):
- **Related Concept Inference** — identifies cross-dimensional associative relationships between the note's concepts and creates `skos:related` edges (see [Automatic Related Concept Detection](#automatic-related-concept-detection) below)
- **Embedding** — concept labels and their relationships (broader, narrower, related) are embedded into vectors alongside content, producing semantically richer embeddings

**Phase 3** (after embedding completes):
- **Linking** — semantic links blend embedding similarity with SKOS tag overlap for higher-quality connections

This ordering ensures that concept relationships are established before embeddings are generated, so the full concept graph context informs both embeddings and linking.

### Automatic Related Concept Detection

After concept tagging completes, the `RelatedConceptInference` pipeline step uses the LLM to identify associative relationships between the concepts tagged on a note.

**What it does:**
- Queries the leaf concepts (non-root) tagged on the note
- Asks the LLM to identify cross-dimensional associations (e.g., `technique/attention-mechanism` related to `domain/machine-learning`)
- Creates `skos:related` edges between associated concepts with a confidence score
- The `skos:related` relation is symmetric — the reciprocal edge is created automatically by a database trigger

**When it runs:** After `ConceptTagging` completes, before embedding generation. Skips notes with fewer than 3 leaf concepts (insufficient signal for meaningful inference).

**How to identify inferred relations:** Relations created by this step have `created_by: "related_concept_inference"`. They are standard `skos:related` edges and can be queried and managed through the normal concepts API.

### Tagging Sources

Concept tags are attributed to their source:

- `ai_auto` - Automatically generated by the NLP pipeline (the primary source)
- `api` - Manually added via API for corrections or additions
- `import` - Imported from external source
- `user` - Added by user interface

### When to Manually Tag

Because tagging is automatic, manual tagging is typically used for:

- **Corrections**: Remove an incorrect auto-tag or add a missing one
- **Organizational tags**: Apply project/client tags not inferable from content (e.g., `project/alpha`)
- **Status tracking**: Tags like `status/reviewed` or `scope/confidential` that reflect decisions, not content
- **Governance**: Promoting candidate concepts to "controlled" status after review

## Usage Examples

### Reviewing Auto-Generated Tags

After creating a note, the NLP pipeline automatically generates concept tags. Review them:

```bash
# Check what concepts the AI assigned
curl "http://localhost:3000/api/v1/notes/{id}/concepts"
```

The response shows each concept with its source (`ai_auto`), relevance score, and whether it's the primary tag.

### Correcting Auto-Tags

```bash
# Remove an incorrect auto-tag
curl -X DELETE "http://localhost:3000/api/v1/notes/{id}/concepts/{concept_id}"

# Add a missing concept tag manually
curl -X POST "http://localhost:3000/api/v1/notes/{id}/concepts" \
  -H "Content-Type: application/json" \
  -d '{
    "concept_id": "existing-concept-uuid"
  }'
```

### Curating the Concept Vocabulary

As the AI creates concepts, periodically review and promote quality ones:

```bash
# Find candidate concepts (auto-created, not yet reviewed)
curl "http://localhost:3000/api/v1/concepts?status=candidate"

# Promote a quality concept to controlled status
curl -X PATCH "http://localhost:3000/api/v1/concepts/{id}" \
  -H "Content-Type: application/json" \
  -d '{"status": "controlled"}'

# Check governance statistics
curl "http://localhost:3000/api/v1/concepts/governance/stats"
```

### Creating Concepts Manually (Advanced)

Manual concept creation is useful for organizational structures the AI can't infer:

```bash
# Create a project-specific concept hierarchy
curl -X POST http://localhost:3000/api/v1/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "scheme_id": "...",
    "pref_label": "Project Alpha",
    "status": "controlled"
  }'
```

### Re-running Concept Tagging

After model upgrades or to regenerate tags with a different model:

```bash
# Re-tag a single note
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "...",
    "job_type": "concept_tagging"
  }'

# Bulk re-tag with a specific model
# Via MCP: bulk_reprocess_notes with steps: ["concept_tagging"]
```

## PMEST Facets

The system supports PMEST-based faceted classification:

| Facet | Field | Values |
|-------|-------|--------|
| Personality (Type) | `facet_type` | note, project, reference, journal |
| Matter (Source) | `facet_source` | book, article, video, conversation |
| Energy (Domain) | `facet_domain` | Hierarchical subject areas |
| Space (Scope) | `facet_scope` | personal, work, public |
| Time (Status) | Tag status | active, archived, someday |

## Migration from Flat Tags

The SKOS system completely replaces the legacy flat tag system. No migration is needed - the flat tag tables can be deprecated. The new system:

1. Automatically creates concepts from AI analysis
2. Maintains full hierarchy support
3. Tracks provenance (source, confidence, relevance)
4. Supports governance workflows

## Best Practices

### Let the AI Tag, Then Curate

The most effective workflow is:

1. **Create notes without worrying about tags** — the NLP pipeline handles concept tagging automatically
2. **Review auto-tags periodically** — check `GET /api/v1/concepts?status=candidate` for new concepts
3. **Promote quality concepts** — move good candidates to "controlled" status
4. **Merge duplicates** — consolidate synonym sprawl using broader/narrower relations
5. **Add organizational tags manually** — project identifiers, status markers, and scope tags that aren't inferable from content

### For AI Agents Using MCP

1. **Don't manually tag notes after creation** — the pipeline does this automatically
2. **Use `manage_tags` → `get_concepts`** to review what the system assigned
3. **Only use `tag_concept`/`untag_concept`** for corrections, not routine tagging
4. **Use `manage_concepts` → `stats`** to monitor vocabulary health
5. **Use `manage_concepts` → `search`** before creating new concepts — they may already exist

### For Manual Curation

1. **Review candidates regularly**: Promote good concepts to "controlled"
2. **Build hierarchy**: Connect related concepts with broader/narrower
3. **Add scope notes**: Document concept boundaries for disambiguation
4. **Monitor anti-patterns**: Address over-nesting and meta-tag warnings
5. **Add alt_labels**: Include common synonyms for better matching across content

## Strict Tag Filtering

While SKOS tags power fuzzy semantic search, they also support **strict filtering** for guaranteed data segregation. This is critical for:

- **Client isolation**: Ensure searches never return data from other clients
- **Project segregation**: Keep project-specific notes separated
- **Access control foundation**: Building block for multi-tenancy

### Filter Types

| Filter | Logic | Use Case |
|--------|-------|----------|
| `required_tags` | AND | Notes MUST have ALL these tags (or their children) |
| `any_tags` | OR | Notes MUST have AT LEAST ONE (or their children) |
| `excluded_tags` | NOT | Notes MUST NOT have ANY of these (or their children) |
| `required_schemes` | Isolation | Notes ONLY from these vocabularies |
| `excluded_schemes` | Exclusion | Notes NOT from these vocabularies |

**Note:** All tag filters use case-insensitive matching and hierarchical prefix matching. For example:
- `required_tags: ["project"]` matches notes tagged with `project`, `Project/Alpha`, `PROJECT/BETA`, etc.
- `excluded_tags: ["draft"]` excludes notes tagged with `draft`, `Draft`, `draft/wip`, etc.

### How It Works

Strict filters are applied as **pre-search WHERE clauses** at the database level:

```
┌─────────────────────────────────────────────────┐
│  All Notes                                       │
│  ┌───────────────────────────────────────────┐  │
│  │  Strict Filter (guaranteed isolation)      │  │
│  │  ┌─────────────────────────────────────┐  │  │
│  │  │  Fuzzy Search (FTS + Semantic)      │  │  │
│  │  │  - Only within filtered set         │  │  │
│  │  └─────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

### API Usage

```bash
# Search within a specific client's notes
curl "http://localhost:3000/api/v1/search?query=authentication" \
  -H "Content-Type: application/json" \
  -d '{
    "strict_filter": {
      "required_schemes": ["client-acme"]
    }
  }'

# Find high-priority project notes, excluding archived
curl "http://localhost:3000/api/v1/search?query=api" \
  -d '{
    "strict_filter": {
      "required_tags": ["project:matric"],
      "any_tags": ["priority:high", "priority:critical"],
      "excluded_tags": ["status:archived"]
    }
  }'
```

### MCP Tool Usage

```javascript
// Using search_notes with strict filter
search_notes({
  query: "authentication",
  strict_filter: {
    required_schemes: ["client-acme"]
  }
})

// Strict tag filtering via search_notes
search_notes({
  query: "API design",
  required_tags: ["project:matric"],
  any_tags: ["status:active", "status:review"],
  excluded_tags: ["draft"],
  mode: "hybrid"
})
```

### Important: Opt-In Model

Strict filtering is **entirely optional**. The system provides the capability but doesn't enforce it:

- Without filters: Full corpus search (existing behavior)
- With filters: Guaranteed isolation

This is by design - not all use cases need tenancy. Applications requiring strict isolation should:

1. Apply filters consistently at the API/middleware layer
2. Use scheme-based isolation for strong boundaries
3. Consider auditing unfiltered queries for compliance

### Scheme-Based Isolation

For strongest isolation, use **scheme-based filtering**:

```javascript
// Create a scheme per client/tenant
create_concept_scheme({
  notation: "client-acme",
  title: "ACME Corporation"
})

// All ACME tags go in this scheme
create_concept({
  scheme_id: "client-acme",
  pref_label: "Project Alpha"
})

// Search guarantees ONLY ACME data
search_notes({
  required_schemes: ["client-acme"],
  query: "quarterly report"
})
```

### See Also

- [Strict Tag Filtering Design](./strict-tag-filtering-design.md) - Implementation details

## Related Documentation

- [API Reference](./api.md)
- [MCP Tools](./mcp.md)
- [Database Schema](../../migrations/20260118000000_skos_tags.sql)
