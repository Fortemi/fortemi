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

The SKOS (Simple Knowledge Organization System) tagging system replaces the legacy flat tag system with a rich, semantically-aware hierarchical structure. Key features include:

- **W3C SKOS Compliance**: Full support for concept schemes, concepts, labels, semantic relations, and mapping relations
- **Automatic AI Tagging**: Notes are automatically tagged with relevant concepts during the NLP pipeline
- **Hierarchical Relationships**: Broader/narrower/related semantic relations for concept organization
- **PMEST Faceted Classification**: Built-in facets for type, source, domain, status, and scope
- **Anti-pattern Detection**: Automatic warnings for poor tagging practices
- **MCP Tools**: Full MCP integration for agentic access

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
| Max Breadth | 10 children | Maximum children per concept |
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

The following MCP tools are available for AI agents:

### Scheme Management
- `list_concept_schemes` - List all concept schemes
- `create_concept_scheme` - Create a new scheme

### Concept Management
- `search_concepts` - Search for concepts
- `create_concept` - Create a new concept
- `get_concept_full` - Get concept with all details
- `update_concept` - Update concept properties
- `delete_concept` - Delete an unused concept

### Hierarchy
- `add_broader` - Add a broader relation
- `add_narrower` - Add a narrower relation
- `add_related` - Add a related relation
- `get_broader` - Get broader concepts
- `get_narrower` - Get narrower concepts
- `get_related` - Get related concepts

### Tagging
- `tag_note_concept` - Tag a note with a concept
- `untag_note_concept` - Remove a concept from a note
- `get_note_concepts` - Get all concepts for a note

### Governance
- `get_governance_stats` - Get usage and quality statistics

## Automatic Tagging Pipeline

Notes are automatically tagged with SKOS concepts as part of the NLP processing pipeline. When a note is created or updated, the system:

1. **Queues the `ConceptTagging` job** - Added to the job queue with priority 4
2. **Analyzes content** - Uses AI to identify 3-7 relevant concepts
3. **Matches or creates concepts** - Searches for existing concepts; creates new ones as candidates
4. **Tags the note** - Associates concepts with relevance scores

### Job Types and Priorities

| Job Type | Priority | Description |
|----------|----------|-------------|
| AiRevision | 8 | Content enhancement |
| Embedding | 5 | Vector generation |
| **ConceptTagging** | **4** | SKOS tagging |
| Linking | 3 | Semantic linking |
| TitleGeneration | 2 | Title generation |
| ContextUpdate | 1 | Context enrichment |

### Tagging Sources

Concept tags are attributed to their source:

- `api` - Manually added via API
- `ai_auto` - Automatically generated by AI
- `import` - Imported from external source
- `user` - Added by user interface

## Usage Examples

### Creating a Concept Hierarchy

```bash
# Create a top-level concept
curl -X POST http://localhost:3000/api/v1/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "scheme_id": "...",
    "pref_label": "Programming Languages",
    "status": "controlled"
  }'

# Create a child concept
curl -X POST http://localhost:3000/api/v1/concepts \
  -H "Content-Type: application/json" \
  -d '{
    "scheme_id": "...",
    "pref_label": "Rust",
    "broader_ids": ["programming-languages-id"]
  }'
```

### Searching Concepts

```bash
# Autocomplete search
curl "http://localhost:3000/api/v1/concepts/autocomplete?q=rust&limit=5"

# Full search with filters
curl "http://localhost:3000/api/v1/concepts?query=programming&status=controlled"
```

### Manually Queueing Concept Tagging

```bash
# Re-run concept tagging for a note
curl -X POST http://localhost:3000/api/v1/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "note_id": "...",
    "job_type": "concept_tagging"
  }'
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

### For AI Agents

1. **Use specific labels**: Prefer "Rust programming" over "programming"
2. **Check for existing concepts**: Search before creating duplicates
3. **Set appropriate status**: Use "candidate" for auto-created concepts
4. **Add alt_labels**: Include common synonyms for better matching

### For Manual Curation

1. **Review candidates regularly**: Promote good concepts to "controlled"
2. **Build hierarchy**: Connect related concepts with broader/narrower
3. **Add scope notes**: Document concept boundaries for disambiguation
4. **Monitor anti-patterns**: Address over-nesting and meta-tag warnings

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

// Dedicated strict search tool
search_notes_strict({
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
search_notes_strict({
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
