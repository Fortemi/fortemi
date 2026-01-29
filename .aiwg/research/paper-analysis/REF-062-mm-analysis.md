# REF-062: W3C PROV - matric-memory Analysis

**Standard:** W3C (2013). PROV-DM: The PROV Data Model. W3C Recommendation.

**Analysis Date:** 2026-01-25
**Relevance:** Critical - Provenance tracking for AI revisions and semantic linking

---

## Implementation Mapping

| PROV Concept | matric-memory Implementation | Location |
|--------------|------------------------------|----------|
| Entity | Notes, revisions, embeddings | `note`, `note_revision`, `note_embeddings` tables |
| Activity | AI revision, embedding generation, semantic linking | `activity_log` table |
| Agent | Ollama models, user, system | `activity_log.actor` + model metadata |
| wasDerivedFrom | Revision lineage, link relationships | `note_revision` tracking, `link` table |
| used | Context notes for AI revision | Provenance metadata in `note_revision.metadata` |
| wasGeneratedBy | Revision created by activity | `note_revision.id` + `activity_log` entries |
| wasAssociatedWith | Activity performed by agent | `activity_log.actor`, `note_revision.model` |
| wasInformedBy | Revision uses previous revision | Implicit via `note_revision` ordering |

---

## The Provenance Problem in AI-Enhanced Knowledge Bases

### Why Provenance Matters for matric-memory

Traditional note-taking systems have simple provenance:
```
User creates note → Note exists
```

AI-enhanced knowledge bases introduce opacity:
```
User creates note →
  System generates embedding →
    System finds related notes →
      System uses context to revise note →
        Which notes influenced the revision?
        What model generated it?
        When was it generated?
        Why was it revised this way?
```

**Without provenance:**
- Users can't trust AI revisions
- Debugging is impossible (why did the model say this?)
- Reproducibility is lost (can't recreate the revision)
- Attribution is unclear (which context notes were influential?)

**W3C PROV provides the framework to answer these questions.**

---

## PROV Core Model in matric-memory Context

### Three Core Types

```
┌─────────────────────────────────────────────────────────────┐
│                         ENTITY                               │
│  - Original note (note_original)                            │
│  - Revised note (note_revision)                             │
│  - Note embedding (note_embeddings)                         │
│  - Semantic link (link)                                     │
│  - Context note (used in revision)                          │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ wasGeneratedBy / used
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                        ACTIVITY                              │
│  - AI revision generation (revise_note)                     │
│  - Embedding generation (generate_embedding)                │
│  - Semantic link discovery (discover_semantic_links)        │
│  - Hybrid search execution (hybrid_search)                  │
└─────────────────────────────────────────────────────────────┘
                            │
                            │ wasAssociatedWith
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                          AGENT                               │
│  - User (person)                                            │
│  - ollama:mistral (software agent)                          │
│  - ollama:nomic-embed-text (software agent)                 │
│  - matric-jobs worker (software agent)                      │
└─────────────────────────────────────────────────────────────┘
```

---

## Current Provenance Tracking in matric-memory

### Activity Log Table

```sql
-- migrations/xxx_activity_log.sql

CREATE TABLE activity_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor TEXT NOT NULL,  -- PROV:Agent
    action TEXT NOT NULL, -- PROV:Activity type
    note_id UUID REFERENCES note(id),
    meta JSONB NOT NULL DEFAULT '{}'::jsonb
);

-- Examples of current tracking
INSERT INTO activity_log (actor, action, note_id, meta) VALUES
  ('user', 'create_note', 'uuid-1234', '{}'),
  ('user', 'update_original', 'uuid-1234', '{}'),
  ('user', 'revise', 'uuid-1234', '{}');
```

**Current Limitations:**
- No tracking of which notes informed AI revisions
- No link between activity and generated entity
- Model information is separate from activity log
- Context notes not recorded

### Note Revision Table

```sql
-- migrations/20260118100000_dual_track_versioning.sql

CREATE TABLE note_revision (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id),
    content TEXT NOT NULL,
    rationale TEXT,  -- Why this revision was made
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    model TEXT,      -- PROV:Agent (which model)
    metadata JSONB   -- Could store provenance
);
```

**Provenance Opportunity:**
The `metadata` field can store PROV relationships:
```json
{
  "prov:wasGeneratedBy": "activity:revision-20260125-001",
  "prov:used": ["note:uuid-abc", "note:uuid-def"],
  "prov:wasAssociatedWith": "agent:ollama:mistral",
  "context_scores": {
    "note:uuid-abc": 0.87,
    "note:uuid-def": 0.81
  }
}
```

---

## Proposed PROV Implementation for AI Revision

### Scenario: AI Revises Note with Context

**User action:**
```
POST /notes/{id}/revise
```

**Behind the scenes (PROV entities and activities):**

```
ENTITY: note:original:uuid-1234
  - prov:type = 'note-original'
  - content = "PostgreSQL setup guide..."
  - created_at = 2026-01-20T10:00:00Z

ACTIVITY: act:embed:20260125-001
  - prov:type = 'embedding-generation'
  - started = 2026-01-25T14:00:00Z
  - ended = 2026-01-25T14:00:02Z
  - used(entity:note:original:uuid-1234)
  - wasAssociatedWith(agent:ollama:nomic-embed-text)

ENTITY: embedding:uuid-1234
  - prov:type = 'note-embedding'
  - vector = [0.023, -0.156, ...]
  - wasGeneratedBy(act:embed:20260125-001)
  - wasDerivedFrom(entity:note:original:uuid-1234)

ACTIVITY: act:search-context:20260125-002
  - prov:type = 'semantic-search'
  - started = 2026-01-25T14:00:02Z
  - ended = 2026-01-25T14:00:03Z
  - used(entity:embedding:uuid-1234)
  - wasInformedBy(act:embed:20260125-001)

ENTITY: note:uuid-abc (context note 1)
  - similarity_score = 0.87
  - title = "PostgreSQL connection pooling"

ENTITY: note:uuid-def (context note 2)
  - similarity_score = 0.81
  - title = "PgBouncer configuration"

ACTIVITY: act:revise:20260125-003
  - prov:type = 'ai-revision'
  - started = 2026-01-25T14:00:03Z
  - ended = 2026-01-25T14:00:15Z
  - used(entity:note:original:uuid-1234)
  - used(entity:note:uuid-abc)
  - used(entity:note:uuid-def)
  - wasAssociatedWith(agent:ollama:mistral)
  - wasInformedBy(act:search-context:20260125-002)

ENTITY: note:revision:uuid-5678
  - prov:type = 'note-revision'
  - content = "PostgreSQL setup guide (enhanced with pooling context)..."
  - rationale = "Added connection pooling best practices"
  - wasGeneratedBy(act:revise:20260125-003)
  - wasDerivedFrom(entity:note:original:uuid-1234)
  - wasDerivedFrom(entity:note:uuid-abc)    # Context influence
  - wasDerivedFrom(entity:note:uuid-def)    # Context influence
```

---

## Schema Proposals for PROV Tracking

### Option 1: Embedded PROV in Metadata (Minimal Change)

**Enhance existing `note_revision.metadata` field:**

```sql
-- No schema change needed, use existing metadata JSONB

-- When creating revision, store PROV relationships
INSERT INTO note_revision (id, note_id, content, rationale, model, metadata)
VALUES (
  $1, $2, $3, $4, 'ollama:mistral',
  jsonb_build_object(
    'prov', jsonb_build_object(
      'wasGeneratedBy', 'activity:revision:' || $1::text,
      'used', jsonb_build_array(
        jsonb_build_object('entity', 'note:' || $context_note_1, 'score', 0.87),
        jsonb_build_object('entity', 'note:' || $context_note_2, 'score', 0.81)
      ),
      'wasAssociatedWith', 'agent:ollama:mistral',
      'activity_metadata', jsonb_build_object(
        'started', NOW() - INTERVAL '12 seconds',
        'ended', NOW(),
        'context_count', 2
      )
    )
  )
);
```

**Query provenance:**

```sql
-- Find all notes that influenced this revision
SELECT
  r.id,
  r.content,
  jsonb_array_elements(r.metadata->'prov'->'used') as context_notes
FROM note_revision r
WHERE r.id = $revision_id;
```

**Pros:**
- No schema change
- Immediate implementation
- JSONB allows flexible PROV structure

**Cons:**
- Querying is harder (JSONB path expressions)
- No referential integrity for note UUIDs in JSONB
- Can't index efficiently

---

### Option 2: Dedicated Provenance Tables (Full PROV)

**New tables for complete W3C PROV implementation:**

```sql
-- PROV Entity tracking
CREATE TABLE prov_entity (
    id UUID PRIMARY KEY,
    prov_type TEXT NOT NULL,  -- 'note-original', 'note-revision', 'embedding', 'link'
    entity_ref UUID NOT NULL,  -- Foreign key to actual entity
    entity_table TEXT NOT NULL, -- 'note', 'note_revision', 'note_embeddings', 'link'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    attributes JSONB NOT NULL DEFAULT '{}'::jsonb
);

-- PROV Activity tracking
CREATE TABLE prov_activity (
    id UUID PRIMARY KEY,
    activity_type TEXT NOT NULL,  -- 'ai-revision', 'embedding-generation', 'semantic-linking'
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ,
    actor TEXT NOT NULL,  -- PROV:Agent
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

-- PROV Relations: wasGeneratedBy
CREATE TABLE prov_was_generated_by (
    entity_id UUID NOT NULL REFERENCES prov_entity(id),
    activity_id UUID NOT NULL REFERENCES prov_activity(id),
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (entity_id, activity_id)
);

-- PROV Relations: used
CREATE TABLE prov_used (
    activity_id UUID NOT NULL REFERENCES prov_activity(id),
    entity_id UUID NOT NULL REFERENCES prov_entity(id),
    role TEXT,  -- 'source-note', 'context-note', 'embedding'
    usage_metadata JSONB,  -- e.g., similarity score
    PRIMARY KEY (activity_id, entity_id)
);

-- PROV Relations: wasDerivedFrom
CREATE TABLE prov_was_derived_from (
    generated_entity_id UUID NOT NULL REFERENCES prov_entity(id),
    used_entity_id UUID NOT NULL REFERENCES prov_entity(id),
    derivation_type TEXT,  -- 'revision', 'context-influence', 'semantic-link'
    PRIMARY KEY (generated_entity_id, used_entity_id)
);

-- PROV Relations: wasAssociatedWith (Activity-Agent)
CREATE TABLE prov_was_associated_with (
    activity_id UUID NOT NULL REFERENCES prov_activity(id),
    agent_id TEXT NOT NULL,  -- 'user', 'ollama:mistral', 'ollama:nomic-embed-text'
    role TEXT,  -- 'generator', 'embedder', 'linker'
    PRIMARY KEY (activity_id, agent_id)
);

-- PROV Relations: wasInformedBy (Activity-Activity)
CREATE TABLE prov_was_informed_by (
    informed_activity_id UUID NOT NULL REFERENCES prov_activity(id),
    informing_activity_id UUID NOT NULL REFERENCES prov_activity(id),
    PRIMARY KEY (informed_activity_id, informing_activity_id)
);

-- Indexes for efficient provenance queries
CREATE INDEX idx_prov_entity_ref ON prov_entity(entity_ref, entity_table);
CREATE INDEX idx_prov_activity_type ON prov_activity(activity_type, started_at DESC);
CREATE INDEX idx_prov_generated_by_entity ON prov_was_generated_by(entity_id);
CREATE INDEX idx_prov_used_activity ON prov_used(activity_id);
CREATE INDEX idx_prov_derived_from ON prov_was_derived_from(generated_entity_id);
```

**Pros:**
- Full W3C PROV compliance
- Efficient queries with proper indexes
- Referential integrity
- Supports complex provenance queries

**Cons:**
- More tables to maintain
- Higher storage overhead
- More complex to implement

---

### Option 3: Hybrid Approach (Recommended)

**Use existing tables + minimal provenance table:**

```sql
-- Lightweight provenance for context tracking
CREATE TABLE note_revision_context (
    revision_id UUID NOT NULL REFERENCES note_revision(id) ON DELETE CASCADE,
    context_note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    similarity_score FLOAT NOT NULL,
    role TEXT NOT NULL DEFAULT 'context',  -- 'context', 'citation', 'contradiction'
    PRIMARY KEY (revision_id, context_note_id)
);

CREATE INDEX idx_revision_context_note ON note_revision_context(context_note_id);
CREATE INDEX idx_revision_context_score ON note_revision_context(revision_id, similarity_score DESC);

-- Enhance activity_log with PROV concepts
ALTER TABLE activity_log ADD COLUMN IF NOT EXISTS prov_activity_id UUID;
ALTER TABLE activity_log ADD COLUMN IF NOT EXISTS generated_entity_id UUID;
ALTER TABLE activity_log ADD COLUMN IF NOT EXISTS generated_entity_type TEXT;

COMMENT ON COLUMN activity_log.prov_activity_id IS 'PROV:Activity identifier for tracing';
COMMENT ON COLUMN activity_log.generated_entity_id IS 'PROV:Entity generated by this activity';
```

**Usage:**

```sql
-- Record AI revision with provenance
BEGIN;

-- 1. Create revision (PROV:Entity)
INSERT INTO note_revision (id, note_id, content, rationale, model)
VALUES ($revision_id, $note_id, $content, $rationale, 'ollama:mistral');

-- 2. Record context notes (PROV:used)
INSERT INTO note_revision_context (revision_id, context_note_id, similarity_score)
VALUES
  ($revision_id, $context_1, 0.87),
  ($revision_id, $context_2, 0.81);

-- 3. Log activity (PROV:Activity + wasGeneratedBy)
INSERT INTO activity_log (
  id, actor, action, note_id,
  prov_activity_id, generated_entity_id, generated_entity_type, meta
)
VALUES (
  gen_random_uuid(),
  'ollama:mistral',
  'ai_revision',
  $note_id,
  $activity_id,
  $revision_id,
  'note_revision',
  jsonb_build_object('context_count', 2, 'duration_ms', 12000)
);

COMMIT;
```

**Pros:**
- Minimal schema changes
- Efficient queries for common use case ("what context informed this?")
- Referential integrity for context notes
- Activity log enhancement supports full provenance without duplication

**Cons:**
- Not full PROV compliance (subset implementation)
- Some PROV queries require JSONB traversal

---

## Rust Implementation Examples

### PROV-Aware Revision Creation

```rust
// crates/matric-db/src/revisions.rs

use uuid::Uuid;
use sqlx::PgPool;

/// PROV metadata for revision
#[derive(Debug, Clone)]
pub struct RevisionProvenance {
    pub activity_id: Uuid,
    pub context_notes: Vec<ContextNote>,
    pub agent: String,  // e.g., "ollama:mistral"
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct ContextNote {
    pub note_id: Uuid,
    pub similarity_score: f32,
    pub role: String,  // 'context', 'citation', etc.
}

/// Create AI revision with full PROV tracking
pub async fn create_revision_with_provenance(
    pool: &PgPool,
    note_id: Uuid,
    content: &str,
    rationale: &str,
    provenance: RevisionProvenance,
) -> Result<Uuid> {
    let revision_id = Uuid::new_v4();

    let mut tx = pool.begin().await?;

    // 1. Create revision entity (PROV:Entity)
    sqlx::query!(
        r#"
        INSERT INTO note_revision (id, note_id, content, rationale, model, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        revision_id,
        note_id,
        content,
        rationale,
        provenance.agent,
        provenance.ended_at
    )
    .execute(&mut *tx)
    .await?;

    // 2. Record context notes (PROV:used)
    for context in &provenance.context_notes {
        sqlx::query!(
            r#"
            INSERT INTO note_revision_context (revision_id, context_note_id, similarity_score, role)
            VALUES ($1, $2, $3, $4)
            "#,
            revision_id,
            context.note_id,
            context.similarity_score,
            context.role
        )
        .execute(&mut *tx)
        .await?;
    }

    // 3. Log activity with PROV relationships (PROV:Activity + wasGeneratedBy)
    let duration_ms = (provenance.ended_at - provenance.started_at).num_milliseconds();

    sqlx::query!(
        r#"
        INSERT INTO activity_log (
            id, at_utc, actor, action, note_id,
            prov_activity_id, generated_entity_id, generated_entity_type, meta
        )
        VALUES ($1, $2, $3, 'ai_revision', $4, $5, $6, 'note_revision', $7)
        "#,
        Uuid::new_v4(),
        provenance.ended_at,
        provenance.agent,
        note_id,
        provenance.activity_id,
        revision_id,
        serde_json::json!({
            "context_count": provenance.context_notes.len(),
            "duration_ms": duration_ms,
            "started_at": provenance.started_at,
            "ended_at": provenance.ended_at
        })
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(revision_id)
}
```

### Querying Provenance

```rust
// crates/matric-db/src/provenance.rs

/// PROV query result showing what influenced a revision
#[derive(Debug, Clone)]
pub struct RevisionProvenance {
    pub revision_id: Uuid,
    pub note_id: Uuid,
    pub agent: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub context_notes: Vec<ContextNoteInfo>,
}

#[derive(Debug, Clone)]
pub struct ContextNoteInfo {
    pub note_id: Uuid,
    pub title: String,
    pub similarity_score: f32,
    pub snippet: String,
}

/// Fetch full provenance for a revision (PROV:used + PROV:wasAssociatedWith)
pub async fn get_revision_provenance(
    pool: &PgPool,
    revision_id: Uuid,
) -> Result<RevisionProvenance> {
    // Get revision metadata
    let revision = sqlx::query!(
        r#"
        SELECT note_id, model, created_at
        FROM note_revision
        WHERE id = $1
        "#,
        revision_id
    )
    .fetch_one(pool)
    .await?;

    // Get context notes (PROV:used entities)
    let context_notes = sqlx::query_as!(
        ContextNoteInfo,
        r#"
        SELECT
            nrc.context_note_id as note_id,
            n.title,
            nrc.similarity_score,
            substring(no.content from 1 for 200) as snippet
        FROM note_revision_context nrc
        JOIN note n ON n.id = nrc.context_note_id
        JOIN note_original no ON no.note_id = n.id
        WHERE nrc.revision_id = $1
        ORDER BY nrc.similarity_score DESC
        "#,
        revision_id
    )
    .fetch_all(pool)
    .await?;

    Ok(RevisionProvenance {
        revision_id,
        note_id: revision.note_id,
        agent: revision.model.unwrap_or_else(|| "unknown".to_string()),
        created_at: revision.created_at,
        context_notes,
    })
}

/// Find all revisions influenced by a specific note (reverse provenance)
pub async fn get_notes_influenced_by(
    pool: &PgPool,
    context_note_id: Uuid,
) -> Result<Vec<Uuid>> {
    let revision_ids = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT revision_id
        FROM note_revision_context
        WHERE context_note_id = $1
        ORDER BY revision_id
        "#,
        context_note_id
    )
    .fetch_all(pool)
    .await?;

    Ok(revision_ids)
}
```

---

## Benefits of PROV for matric-memory

### 1. Transparent AI Decision Making

**Paper Finding:**
> "Use of W3C PROV has been previously demonstrated as a means to increase reproducibility and trust of computer-generated outputs."

**matric-memory Benefit:**
- Users see which notes influenced AI revisions
- "Why did the AI add this paragraph?" → Check context notes
- Trust increases when provenance is visible

**UI Mockup:**
```
Revision #3 (AI-generated by ollama:mistral)
─────────────────────────────────────────────

Context used for this revision:
• PostgreSQL Connection Pooling (87% similar)
  "PgBouncer reduces connection overhead..."

• Database Performance Tuning (81% similar)
  "Connection limits affect throughput..."

Generated at: 2026-01-25 14:00:15 UTC
Duration: 12 seconds
```

### 2. Debugging AI Behavior

**Scenario:**
```
User: "The AI added incorrect information about PostgreSQL timeouts"

Developer with PROV:
1. Query revision provenance
2. See context notes used (PROV:used)
3. Check if context notes contain error
4. Fix context note → re-run revision
5. Verify improvement
```

**Without PROV:**
```
Developer: "I don't know why the model said that. Unclear what context it had."
```

### 3. Reproducibility

**Paper Finding:**
> "Provenance tracking transforms opaque processes into transparent, auditable workflows."

**matric-memory Benefit:**
- Can recreate revision with same context notes
- Verify that model behavior is consistent
- Debug regressions (did context change? did model change?)

```rust
pub async fn reproduce_revision(
    pool: &PgPool,
    original_revision_id: Uuid,
) -> Result<Uuid> {
    // 1. Fetch original provenance
    let prov = get_revision_provenance(pool, original_revision_id).await?;

    // 2. Re-run with same context
    let new_revision_id = create_revision_with_same_context(
        pool,
        prov.note_id,
        prov.context_notes,
        prov.agent,
    ).await?;

    Ok(new_revision_id)
}
```

### 4. Attribution and Credit

**Paper Finding:**
> "wasAttributedTo relations enable proper credit assignment to agents."

**matric-memory Benefit:**
- Which notes contributed to knowledge synthesis?
- If note A's content appears in note B's revision, note A is credited
- Knowledge graph shows influence chains

```sql
-- Find notes that have influenced multiple revisions (influential notes)
SELECT
    context_note_id,
    COUNT(DISTINCT revision_id) as revision_count,
    AVG(similarity_score) as avg_influence
FROM note_revision_context
GROUP BY context_note_id
HAVING COUNT(DISTINCT revision_id) >= 5
ORDER BY revision_count DESC;
```

### 5. Compliance and Auditability

For organizations requiring audit trails:

```sql
-- Full audit trail for a note's evolution
WITH RECURSIVE revision_history AS (
    -- Base: original note
    SELECT
        no.note_id,
        NULL::uuid as revision_id,
        no.content,
        n.created_at_utc as created_at,
        'user' as agent,
        0 as generation
    FROM note_original no
    JOIN note n ON n.id = no.note_id
    WHERE no.note_id = $1

    UNION ALL

    -- Recursive: all revisions
    SELECT
        nr.note_id,
        nr.id as revision_id,
        nr.content,
        nr.created_at,
        nr.model as agent,
        rh.generation + 1
    FROM note_revision nr
    JOIN revision_history rh ON rh.note_id = nr.note_id
)
SELECT
    rh.*,
    COALESCE(
        json_agg(
            json_build_object(
                'context_note_id', nrc.context_note_id,
                'similarity', nrc.similarity_score
            )
        ) FILTER (WHERE nrc.context_note_id IS NOT NULL),
        '[]'::json
    ) as context_notes
FROM revision_history rh
LEFT JOIN note_revision_context nrc ON nrc.revision_id = rh.revision_id
GROUP BY rh.note_id, rh.revision_id, rh.content, rh.created_at, rh.agent, rh.generation
ORDER BY rh.generation;
```

---

## Cross-References

### Related Papers

| Paper | Relationship to PROV |
|-------|---------------------|
| REF-056 (FAIR) | FAIR R1.2 requires provenance metadata; PROV provides the standard |
| REF-032 (Knowledge Graphs) | PROV relationships form a provenance graph |
| REF-029 (DPR) | Dense retrieval provides context notes (PROV:used entities) |
| REF-027 (RRF) | Hybrid search finds context; PROV tracks which notes were used |

### Related Code Locations

| File | PROV Usage |
|------|-----------|
| `crates/matric-db/src/revisions.rs` | Revision creation with provenance |
| `crates/matric-db/src/provenance.rs` | Provenance query functions (proposed) |
| `migrations/xxx_provenance.sql` | PROV schema additions (proposed) |
| `crates/matric-api/src/handlers/revisions.rs` | API endpoint exposing provenance |

---

## Implementation Roadmap

### Phase 1: Basic Context Tracking (MVP)

**Schema:**
```sql
CREATE TABLE note_revision_context (
    revision_id UUID NOT NULL REFERENCES note_revision(id),
    context_note_id UUID NOT NULL REFERENCES note(id),
    similarity_score FLOAT NOT NULL,
    PRIMARY KEY (revision_id, context_note_id)
);
```

**Code Changes:**
- Update `create_revision()` to accept context notes
- Store context notes in new table
- Add API endpoint `GET /revisions/{id}/provenance`

**Estimated Effort:** 1 week

### Phase 2: Activity-Entity Linking

**Schema:**
```sql
ALTER TABLE activity_log ADD COLUMN generated_entity_id UUID;
ALTER TABLE activity_log ADD COLUMN generated_entity_type TEXT;
```

**Code Changes:**
- Link activity_log entries to generated entities
- Add temporal queries (activity started/ended)
- API endpoint for activity timeline

**Estimated Effort:** 3 days

### Phase 3: Full PROV Compliance

**Schema:**
- Implement full PROV tables (Option 2 from schema proposals)
- Migration to populate from existing data

**Code Changes:**
- PROV-compliant API endpoints
- Export to PROV-JSON format
- Visualization of provenance graphs

**Estimated Effort:** 2 weeks

---

## Critical Insights for matric-memory Development

### 1. Provenance is Not Optional for AI Systems

> "Use of W3C PROV has been previously demonstrated as a means to increase reproducibility and trust of computer-generated outputs."

**Implication:** matric-memory MUST track provenance for AI revisions to be trustworthy.

### 2. Context Notes Are Critical Entities

The notes used as context for AI revision are `PROV:used` entities. Without tracking them:
- Revisions are unexplainable
- Bugs can't be debugged
- Users can't understand AI decisions

**Implication:** Every AI revision must record which notes were in the context window.

### 3. Lightweight Provenance Is Better Than None

Full W3C PROV compliance is ideal, but even basic tracking (revision → context notes) provides 80% of the value.

**Implication:** Start with Phase 1 (context tracking), expand later.

### 4. Provenance Enables Feedback Loops

If users can see which notes influenced a revision, they can:
- Fix incorrect context notes
- Add missing context
- Improve the knowledge base quality

**Implication:** Provenance isn't just audit trail—it's a quality improvement tool.

---

## Key Quotes Relevant to matric-memory

> "PROV-DM is the conceptual data model that forms a basis for the W3C provenance family of specifications."
>
> **Relevance:** Provides a standard framework matric-memory can adopt for AI provenance.

> "Entity: Physical, digital, conceptual thing with fixed aspects."
>
> **Relevance:** Notes and revisions are PROV entities with immutable content (versioned).

> "Activity: Something that occurs over a period of time and acts upon or with entities."
>
> **Relevance:** AI revision, embedding generation, semantic linking are PROV activities.

> "Agent: Something that bears responsibility for activity occurring."
>
> **Relevance:** Ollama models, users, and system components are PROV agents.

> "wasDerivedFrom: Entity is transformed from, created from, or affected by another entity."
>
> **Relevance:** Revised notes are derived from original notes and context notes.

---

## Summary

REF-062 (W3C PROV) provides the conceptual framework and standard vocabulary for tracking provenance in matric-memory's AI-enhanced workflows. By implementing PROV relationships—particularly `prov:used` (context notes), `prov:wasGeneratedBy` (revision creation), and `prov:wasAssociatedWith` (model attribution)—matric-memory can transform opaque AI revisions into transparent, auditable, reproducible processes.

**Implementation Status:** Partial (activity_log exists, but no context tracking)
**Priority:** High (critical for AI trustworthiness)
**Recommended Approach:** Hybrid (Option 3 - minimal tables + enhanced activity_log)
**Estimated Effort:** Phase 1 (1 week), Phase 2 (3 days), Phase 3 (2 weeks)
**Expected Benefit:**
- User trust in AI revisions increases
- Debugging AI behavior becomes possible
- Reproducibility enables regression testing
- Compliance and audit requirements satisfied

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-01-25 | Initial analysis with full PROV mapping and implementation proposals |
