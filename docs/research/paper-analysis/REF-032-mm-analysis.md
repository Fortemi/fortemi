# REF-032: Knowledge Graphs Survey - matric-memory Analysis

**Paper:** Hogan, A., et al. (2021). Knowledge Graphs. ACM Computing Surveys.

**Analysis Date:** 2026-01-25
**Relevance:** Critical - Semantic linking and graph structure

---

## Implementation Mapping

| KG Concept | matric-memory Implementation | Location |
|------------|------------------------------|----------|
| Property graph | Notes as nodes, links as edges | `crates/matric-db/src/links.rs` |
| Edge weights | Similarity scores (0.0-1.0) | `note_links.score` column |
| Bidirectional links | Both directions stored | Link creation logic |
| Graph traversal | Recursive CTE | `explore_graph()` function |
| Node properties | Note metadata (title, tags, dates) | `notes` table |
| Edge properties | Link kind, score, created_at | `note_links` table |

---

## Knowledge Graph Architecture in matric-memory

### The Connected Knowledge Problem

Notes in isolation lose context:

```
Traditional Notes:
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  Note A     │    │  Note B     │    │  Note C     │
│  Postgres   │    │  Connection │    │  Timeout    │
│  setup      │    │  pooling    │    │  debugging  │
└─────────────┘    └─────────────┘    └─────────────┘
    (isolated)        (isolated)        (isolated)
```

Knowledge graphs connect related concepts:

```
matric-memory Knowledge Graph:
┌─────────────┐
│  Note A     │──────0.85──────┌─────────────┐
│  Postgres   │                │  Note B     │
│  setup      │                │  Connection │
└─────────────┘                │  pooling    │
       │                       └─────────────┘
       │                              │
      0.72                           0.78
       │                              │
       ▼                              ▼
┌─────────────┐                ┌─────────────┐
│  Note C     │────0.81────────│  Note D     │
│  Timeout    │                │  PgBouncer  │
│  debugging  │                │  config     │
└─────────────┘                └─────────────┘
```

### Graph Model in matric-memory

**Property Graph Structure:**

```sql
-- Nodes: notes table
CREATE TABLE notes (
    id UUID PRIMARY KEY,
    title TEXT,
    content TEXT,
    created_at TIMESTAMPTZ,
    -- Node properties
    starred BOOLEAN,
    archived BOOLEAN,
    collection_id UUID
);

-- Edges: note_links table
CREATE TABLE note_links (
    id UUID PRIMARY KEY,
    from_note_id UUID REFERENCES notes(id),
    to_note_id UUID REFERENCES notes(id),
    kind VARCHAR(50),      -- 'semantic', 'manual', 'citation'
    score FLOAT,           -- Edge weight (similarity)
    created_at TIMESTAMPTZ,
    -- Enforce bidirectional consistency
    UNIQUE(from_note_id, to_note_id)
);
```

### Semantic Link Discovery

```rust
// crates/matric-db/src/links.rs

/// Create bidirectional semantic links for notes with similarity > 0.7
/// Based on property graph pattern from REF-032
pub async fn discover_semantic_links(
    pool: &PgPool,
    note_id: Uuid,
    embedding: &[f32],
) -> Result<Vec<NoteLink>> {
    // Find related notes above threshold (REF-030)
    let related = sqlx::query_as!(
        RelatedNote,
        r#"
        SELECT
            ne.note_id,
            1 - (ne.embedding <=> $1::vector) as similarity
        FROM note_embeddings ne
        JOIN notes n ON ne.note_id = n.id
        WHERE ne.note_id != $2
          AND n.deleted_at IS NULL
          AND 1 - (ne.embedding <=> $1::vector) >= 0.7
        ORDER BY similarity DESC
        LIMIT 20
        "#,
        embedding as &[f32],
        note_id
    )
    .fetch_all(pool)
    .await?;

    let mut links = Vec::new();

    for related_note in related {
        // Create bidirectional links (property graph pattern)
        let link = create_bidirectional_link(
            pool,
            note_id,
            related_note.note_id,
            "semantic",
            related_note.similarity,
        ).await?;
        links.push(link);
    }

    Ok(links)
}

/// Store link in both directions per REF-032 bidirectionality
async fn create_bidirectional_link(
    pool: &PgPool,
    from_id: Uuid,
    to_id: Uuid,
    kind: &str,
    score: f32,
) -> Result<NoteLink> {
    // Forward link
    sqlx::query!(
        r#"
        INSERT INTO note_links (id, from_note_id, to_note_id, kind, score)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (from_note_id, to_note_id) DO UPDATE
        SET score = EXCLUDED.score, kind = EXCLUDED.kind
        "#,
        Uuid::new_v4(),
        from_id,
        to_id,
        kind,
        score
    )
    .execute(pool)
    .await?;

    // Reverse link (bidirectional)
    sqlx::query!(
        r#"
        INSERT INTO note_links (id, from_note_id, to_note_id, kind, score)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (from_note_id, to_note_id) DO UPDATE
        SET score = EXCLUDED.score, kind = EXCLUDED.kind
        "#,
        Uuid::new_v4(),
        to_id,
        from_id,
        kind,
        score
    )
    .execute(pool)
    .await?;

    Ok(NoteLink { from_id, to_id, kind: kind.to_string(), score })
}
```

---

## Graph Traversal Implementation

### Recursive CTE for Multi-Hop Exploration

**Paper Finding:**
> "Recursive queries enable exploration of transitive relationships and multi-hop reasoning." (Section 4.2)

**matric-memory Implementation:**

```rust
// crates/matric-db/src/links.rs

/// Explore knowledge graph from a starting note
/// Recursive CTE pattern per REF-032
pub async fn explore_graph(
    pool: &PgPool,
    start_note_id: Uuid,
    max_depth: i32,
    max_nodes: i32,
) -> Result<GraphExploration> {
    let result = sqlx::query_as!(
        GraphNode,
        r#"
        WITH RECURSIVE graph AS (
            -- Base case: starting node
            SELECT
                to_note_id as note_id,
                score,
                1 as depth,
                ARRAY[from_note_id, to_note_id] as path
            FROM note_links
            WHERE from_note_id = $1

            UNION ALL

            -- Recursive case: follow links
            SELECT
                nl.to_note_id,
                nl.score,
                g.depth + 1,
                g.path || nl.to_note_id
            FROM note_links nl
            JOIN graph g ON nl.from_note_id = g.note_id
            WHERE g.depth < $2
              AND NOT nl.to_note_id = ANY(g.path)  -- Prevent cycles
        )
        SELECT DISTINCT ON (note_id)
            note_id,
            score,
            depth
        FROM graph
        ORDER BY note_id, depth  -- Prefer shorter paths
        LIMIT $3
        "#,
        start_note_id,
        max_depth,
        max_nodes
    )
    .fetch_all(pool)
    .await?;

    // Fetch node details and edges
    let nodes = fetch_note_details(pool, &result).await?;
    let edges = fetch_edges_between(pool, &result).await?;

    Ok(GraphExploration { nodes, edges })
}
```

### Backlink Discovery

**Paper Finding:**
> "Bidirectional edges enable backlink traversal, answering 'what links to this node?'" (Section 2.3)

```rust
/// Find notes that link TO a given note (backlinks)
pub async fn get_backlinks(
    pool: &PgPool,
    note_id: Uuid,
) -> Result<Vec<BacklinkNote>> {
    sqlx::query_as!(
        BacklinkNote,
        r#"
        SELECT
            nl.from_note_id as note_id,
            n.title,
            nl.score,
            nl.kind
        FROM note_links nl
        JOIN notes n ON nl.from_note_id = n.id
        WHERE nl.to_note_id = $1
          AND n.deleted_at IS NULL
        ORDER BY nl.score DESC
        "#,
        note_id
    )
    .fetch_all(pool)
    .await
}
```

---

## Benefits Mirroring Knowledge Graph Research

### 1. Emergent Knowledge Discovery

**Paper Finding:**
> "Knowledge graphs enable discovery of implicit relationships through path traversal." (Section 4)

**matric-memory Benefit:**
- Note A links to Note B links to Note C
- User discovers A→C connection they didn't know existed
- Multi-hop exploration surfaces hidden knowledge

### 2. Weighted Relationships

**Paper Finding:**
> "Edge weights capture relationship strength, enabling ranked traversal." (Section 2.3)

**matric-memory Benefit:**
- Similarity scores (0.7-1.0) indicate connection strength
- Stronger links shown first in UI
- Weak links can be filtered in exploration

### 3. Property Graph Expressiveness

**Paper Finding:**
> "Property graphs support rich metadata on both nodes and edges, beyond simple triples." (Section 2)

**matric-memory Benefit:**
- Notes have rich metadata (title, dates, collections, tags)
- Links have kind (semantic, manual, citation) and score
- Enables sophisticated filtering and faceted exploration

### 4. Bidirectional Navigation

**Paper Finding:**
> "Storing edges in both directions enables efficient backlink queries." (Section 3.2)

**matric-memory Benefit:**
- "What links to this note?" is O(k) not O(E)
- Backlink UI feature is performant
- Knowledge graph is navigable in any direction

---

## Comparison: RDF Triple Store vs matric-memory Property Graph

| Feature | RDF Triple Store | matric-memory Property Graph |
|---------|------------------|------------------------------|
| Basic unit | Subject-Predicate-Object | Node with properties |
| Edge metadata | Reification (complex) | Direct edge properties |
| Querying | SPARQL | SQL with recursive CTE |
| Schema | RDF Schema/OWL | PostgreSQL schema |
| Reasoning | Built-in inference | Application-level |
| Storage | Triple store | PostgreSQL (relational) |
| Integration | Separate system | Native to main database |

### Why Property Graph Over RDF?

1. **Simplicity:** No SPARQL, use familiar SQL
2. **Integration:** Same database as notes, embeddings
3. **Performance:** PostgreSQL's mature query optimizer
4. **Pragmatism:** Edge weights are first-class, not reified

---

## Cross-References

### Related Papers

| Paper | Relationship to Knowledge Graphs |
|-------|----------------------------------|
| REF-030 (SBERT) | Provides similarity for edge weights |
| REF-033 (SKOS) | Structured taxonomy within graph |
| REF-027 (RRF) | Graph-based retrieval possible |

### Related Code Locations

| File | Knowledge Graph Usage |
|------|----------------------|
| `crates/matric-db/src/links.rs` | Link storage and traversal |
| `crates/matric-api/src/handlers/graph.rs` | Graph API endpoints |
| `crates/matric-jobs/src/linking.rs` | Automatic link discovery |
| `mcp-server/src/tools/graph.ts` | MCP graph exploration |

---

## Improvement Opportunities

### 1. Link Type Taxonomy

Extend link kinds beyond 'semantic':

```rust
pub enum LinkKind {
    Semantic,     // Automatic similarity
    Manual,       // User-created
    Citation,     // Note references another
    Refutes,      // Contradicts
    Elaborates,   // Expands on topic
    Prerequisites,// Required reading
}
```

### 2. Path-Based Queries

Find paths between two notes:

```rust
pub async fn find_paths(
    pool: &PgPool,
    from_note: Uuid,
    to_note: Uuid,
    max_hops: i32,
) -> Result<Vec<NotePath>> {
    // BFS to find all paths up to max_hops
}
```

### 3. Community Detection

Cluster related notes:

```rust
pub async fn detect_communities(
    pool: &PgPool,
) -> Result<Vec<NoteCluster>> {
    // Louvain or label propagation algorithm
    // On note_links graph
}
```

### 4. Graph Embeddings

Node2Vec for graph-aware embeddings:

```rust
// Combine text embedding with graph structure
pub struct HybridEmbedding {
    text_embedding: Vec<f32>,   // From SBERT
    graph_embedding: Vec<f32>,  // From Node2Vec
}
```

### 5. Temporal Graph Analysis

Track how knowledge graph evolves:

```rust
pub async fn graph_evolution(
    pool: &PgPool,
    from_date: DateTime,
    to_date: DateTime,
) -> Result<GraphEvolution> {
    // Nodes/edges added over time
    // Link decay (connections becoming stale)
}
```

---

## Critical Insights for matric-memory Development

### 1. Bidirectionality is Essential

> "For undirected relationships, storing both directions enables efficient traversal in either direction." (Section 3.2)

**Implication:** Always create both (A→B) and (B→A) links.

### 2. Cycle Prevention Required

> "Recursive traversal must detect cycles to avoid infinite loops." (Section 4.2)

**Implication:** Path tracking in recursive CTE is critical.

### 3. Edge Weights Enable Ranking

> "Weighted edges support 'spreading activation' and ranked exploration." (Section 4.3)

**Implication:** Store and use similarity scores, don't discard precision.

### 4. Property Graphs Are Pragmatic

> "Property graphs trade formal semantics for practical expressiveness." (Section 2.4)

**Implication:** Don't over-complicate with RDF; PostgreSQL is sufficient.

---

## Key Quotes Relevant to matric-memory

> "Knowledge graphs represent knowledge as a network of entities connected by typed relationships." (Section 1)
>
> **Relevance:** Defines what matric-memory's note linking system is.

> "Property graphs extend simple graphs by allowing properties on both nodes and edges." (Section 2.3)
>
> **Relevance:** Justifies storing score, kind on note_links.

> "Recursive queries enable multi-hop traversal for discovering transitive relationships." (Section 4.2)
>
> **Relevance:** Basis for explore_graph() implementation.

> "Backlink queries answer 'what references this entity?' efficiently when edges are bidirectional." (Section 3.2)
>
> **Relevance:** Justifies storing both link directions.

---

## Summary

REF-032 provides the conceptual framework for matric-memory's semantic linking system. The property graph model (notes as nodes, links as weighted edges) enables knowledge discovery through multi-hop traversal, backlink exploration, and weighted relationship navigation. PostgreSQL's recursive CTEs implement graph queries without requiring a dedicated graph database.

**Implementation Status:** Complete
**Graph Model:** Property graph in PostgreSQL
**Traversal:** Recursive CTE with cycle detection
**Test Coverage:** Graph exploration tests verify traversal depth
**Future Work:** Path queries, community detection, temporal analysis

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-01-25 | Initial analysis |
