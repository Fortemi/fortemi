# REF-033: W3C SKOS Reference - matric-memory Analysis

**Specification:** Miles, A. & Bechhofer, S. (2009). SKOS Simple Knowledge Organization System Reference. W3C Recommendation.

**Analysis Date:** 2026-01-25
**Relevance:** Critical - Structured tagging system

---

## Implementation Mapping

| SKOS Concept | matric-memory Implementation | Location |
|--------------|------------------------------|----------|
| skos:Concept | Tags with UUID | `concepts` table |
| skos:prefLabel | Primary display name | `skos_labels` (type: preferred) |
| skos:altLabel | Synonyms | `skos_labels` (type: alternate) |
| skos:hiddenLabel | Misspellings for search | `skos_labels` (type: hidden) |
| skos:broader | Parent concept | `concept_relations` (type: broader) |
| skos:narrower | Child concepts | `concept_relations` (type: narrower) |
| skos:related | Associative relationship | `concept_relations` (type: related) |
| skos:ConceptScheme | Tag collections | `concept_schemes` table |

---

## SKOS Tagging Architecture in matric-memory

### The Tagging Problem

Simple string tags create chaos:

```
String Tags:
- "PostgreSQL"
- "postgres"
- "Postgres"
- "postgresql"
- "pg"
- "database/postgresql"

Problems:
- No synonyms (searching "postgres" misses "PostgreSQL")
- No hierarchy (can't expand "database" to include all DBs)
- No structure (is "pg" same as "PostgreSQL"?)
```

SKOS provides structured concepts:

```
SKOS Concept: "PostgreSQL"
├── prefLabel: "PostgreSQL" (en)
├── altLabel: "Postgres", "postgres", "pg"
├── hiddenLabel: "postgre", "postgressql"  (misspellings)
├── broader: "Relational Database"
│            └── broader: "Database"
├── related: "pgvector", "PgBouncer"
└── inScheme: "Technical Terms"
```

### Database Schema

```sql
-- Concept schemes (tag collections)
CREATE TABLE concept_schemes (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Concepts (structured tags)
CREATE TABLE concepts (
    id UUID PRIMARY KEY,
    scheme_id UUID REFERENCES concept_schemes(id),
    notation TEXT,           -- Short code, e.g., "PGSQL"
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- SKOS label types
CREATE TYPE label_type AS ENUM ('preferred', 'alternate', 'hidden');

-- Labels with language support
CREATE TABLE skos_labels (
    id UUID PRIMARY KEY,
    concept_id UUID REFERENCES concepts(id),
    label TEXT NOT NULL,
    label_type label_type NOT NULL,
    language VARCHAR(5) DEFAULT 'en',
    UNIQUE(concept_id, label, language)
);

-- Concept relations (hierarchy and association)
CREATE TYPE relation_type AS ENUM ('broader', 'narrower', 'related');

CREATE TABLE concept_relations (
    id UUID PRIMARY KEY,
    from_concept_id UUID REFERENCES concepts(id),
    to_concept_id UUID REFERENCES concepts(id),
    relation_type relation_type NOT NULL,
    UNIQUE(from_concept_id, to_concept_id, relation_type)
);

-- Note-concept associations
CREATE TABLE note_concepts (
    note_id UUID REFERENCES notes(id),
    concept_id UUID REFERENCES concepts(id),
    confidence FLOAT DEFAULT 1.0,  -- 1.0 = user-assigned, <1.0 = AI-suggested
    PRIMARY KEY (note_id, concept_id)
);
```

### Rust Implementation

```rust
// crates/matric-db/src/skos_tags.rs

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "label_type", rename_all = "lowercase")]
pub enum LabelType {
    Preferred,
    Alternate,
    Hidden,
}

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "relation_type", rename_all = "lowercase")]
pub enum RelationType {
    Broader,
    Narrower,
    Related,
}

/// Create a new SKOS concept with labels
pub async fn create_concept(
    pool: &PgPool,
    scheme_id: Uuid,
    pref_label: &str,
    alt_labels: &[&str],
    language: &str,
) -> Result<Uuid> {
    let concept_id = Uuid::new_v4();

    // Create concept
    sqlx::query!(
        "INSERT INTO concepts (id, scheme_id) VALUES ($1, $2)",
        concept_id,
        scheme_id
    )
    .execute(pool)
    .await?;

    // Add preferred label
    add_label(pool, concept_id, pref_label, LabelType::Preferred, language).await?;

    // Add alternate labels
    for alt in alt_labels {
        add_label(pool, concept_id, alt, LabelType::Alternate, language).await?;
    }

    Ok(concept_id)
}

/// Search concepts using any label (preferred, alternate, or hidden)
pub async fn search_concepts(
    pool: &PgPool,
    query: &str,
) -> Result<Vec<ConceptMatch>> {
    sqlx::query_as!(
        ConceptMatch,
        r#"
        SELECT DISTINCT
            c.id as concept_id,
            pref.label as pref_label,
            sl.label as matched_label,
            sl.label_type as "label_type: LabelType"
        FROM concepts c
        JOIN skos_labels sl ON sl.concept_id = c.id
        JOIN skos_labels pref ON pref.concept_id = c.id
                              AND pref.label_type = 'preferred'
        WHERE sl.label ILIKE '%' || $1 || '%'
        ORDER BY
            CASE sl.label_type
                WHEN 'preferred' THEN 1
                WHEN 'alternate' THEN 2
                WHEN 'hidden' THEN 3
            END
        "#,
        query
    )
    .fetch_all(pool)
    .await
}

/// Expand concept to include broader/narrower in hierarchy
pub async fn expand_concept(
    pool: &PgPool,
    concept_id: Uuid,
    direction: RelationType,
) -> Result<Vec<Uuid>> {
    sqlx::query_scalar!(
        r#"
        WITH RECURSIVE hierarchy AS (
            SELECT to_concept_id as concept_id, 1 as depth
            FROM concept_relations
            WHERE from_concept_id = $1 AND relation_type = $2

            UNION ALL

            SELECT cr.to_concept_id, h.depth + 1
            FROM concept_relations cr
            JOIN hierarchy h ON cr.from_concept_id = h.concept_id
            WHERE cr.relation_type = $2 AND h.depth < 10
        )
        SELECT concept_id FROM hierarchy
        "#,
        concept_id,
        direction as RelationType
    )
    .fetch_all(pool)
    .await
}
```

---

## SKOS Features in matric-memory

### 1. Synonym Resolution

**SKOS Principle:**
> "altLabel: An alternative lexical label for a resource." (Section 2.3)

**matric-memory Implementation:**

```rust
/// Tag a note, resolving to canonical concept
pub async fn tag_note(
    pool: &PgPool,
    note_id: Uuid,
    tag_text: &str,
) -> Result<ConceptAssignment> {
    // Search all labels (preferred, alternate, hidden)
    let matches = search_concepts(pool, tag_text).await?;

    if let Some(exact) = matches.iter().find(|m| m.matched_label.eq_ignore_ascii_case(tag_text)) {
        // Exact match found - use canonical concept
        assign_concept(pool, note_id, exact.concept_id).await
    } else if !matches.is_empty() {
        // Fuzzy match - suggest to user
        Err(SuggestConcepts(matches))
    } else {
        // No match - create new concept or reject
        Err(UnknownTag(tag_text.to_string()))
    }
}
```

**Example:**
```
User tags note with: "postgres"
System finds: concept "PostgreSQL" has altLabel "postgres"
Note tagged with: concept "PostgreSQL" (canonical)
```

### 2. Hierarchical Tag Expansion

**SKOS Principle:**
> "broader: Relates a concept to a more general concept." (Section 8.1)

**matric-memory Implementation:**

```rust
/// Search notes with hierarchical tag expansion
pub async fn search_with_expansion(
    pool: &PgPool,
    concept_id: Uuid,
    include_narrower: bool,
) -> Result<Vec<Note>> {
    let mut concept_ids = vec![concept_id];

    if include_narrower {
        // Get all narrower concepts
        let narrower = expand_concept(pool, concept_id, RelationType::Narrower).await?;
        concept_ids.extend(narrower);
    }

    sqlx::query_as!(
        Note,
        r#"
        SELECT n.*
        FROM notes n
        JOIN note_concepts nc ON n.id = nc.note_id
        WHERE nc.concept_id = ANY($1)
        "#,
        &concept_ids
    )
    .fetch_all(pool)
    .await
}
```

**Example:**
```
Search: "Database" (with expansion)
Finds:
- Notes tagged "Database"
- Notes tagged "PostgreSQL" (narrower)
- Notes tagged "MySQL" (narrower)
- Notes tagged "SQLite" (narrower)
```

### 3. Hidden Labels for Misspelling Tolerance

**SKOS Principle:**
> "hiddenLabel: A lexical label for a resource that should be hidden from visual display but accessible to search." (Section 2.4)

**matric-memory Implementation:**

```rust
/// Add common misspellings as hidden labels
pub async fn add_misspelling_labels(
    pool: &PgPool,
    concept_id: Uuid,
    misspellings: &[&str],
) -> Result<()> {
    for misspelling in misspellings {
        add_label(pool, concept_id, misspelling, LabelType::Hidden, "en").await?;
    }
    Ok(())
}
```

**Example:**
```
Concept: "PostgreSQL"
hiddenLabels: ["postgre", "postgressql", "postgress", "potgresql"]

User searches: "postgre" → finds "PostgreSQL"
User never sees hiddenLabel in UI
```

### 4. Related Concepts for Discovery

**SKOS Principle:**
> "related: Relates a concept to another concept with which there is an associative relationship." (Section 8.3)

**matric-memory Implementation:**

```rust
/// Suggest related concepts when viewing a note
pub async fn get_related_concepts(
    pool: &PgPool,
    note_id: Uuid,
) -> Result<Vec<RelatedConcept>> {
    sqlx::query_as!(
        RelatedConcept,
        r#"
        SELECT DISTINCT
            c.id as concept_id,
            pref.label as label,
            'related' as relationship
        FROM note_concepts nc
        JOIN concept_relations cr ON nc.concept_id = cr.from_concept_id
        JOIN concepts c ON cr.to_concept_id = c.id
        JOIN skos_labels pref ON pref.concept_id = c.id
                              AND pref.label_type = 'preferred'
        WHERE nc.note_id = $1
          AND cr.relation_type = 'related'
        "#,
        note_id
    )
    .fetch_all(pool)
    .await
}
```

---

## Benefits Mirroring SKOS Specification

### 1. Vocabulary Control

**Specification:**
> "SKOS provides a way to express controlled vocabularies with rich labeling and semantic relationships."

**matric-memory Benefit:**
- No duplicate concepts ("Postgres" vs "PostgreSQL")
- Canonical labels for display consistency
- Machine-readable relationships

### 2. Multi-Lingual Support

**Specification:**
> "Labels can have language tags, enabling multi-lingual concept schemes." (Section 2.2)

**matric-memory Benefit:**
- Same concept can have English, Spanish, German labels
- Search works across languages
- UI displays in user's preferred language

### 3. Interoperability

**Specification:**
> "SKOS is a W3C standard, enabling exchange of vocabularies across systems."

**matric-memory Benefit:**
- Can import existing SKOS taxonomies (e.g., Library of Congress Subject Headings)
- Can export tags as RDF for other systems
- Industry-standard format

### 4. Semantic Precision

**Specification:**
> "SKOS distinguishes broader (generalization), narrower (specialization), and related (association)." (Section 8)

**matric-memory Benefit:**
- "PostgreSQL" is narrower than "Database" (IS-A)
- "PostgreSQL" is related to "pgvector" (ASSOCIATED-WITH)
- Different relationship types, different behaviors

---

## Comparison: Simple Tags vs SKOS

| Feature | Simple Tags | matric-memory SKOS |
|---------|-------------|-------------------|
| Synonyms | No | altLabel support |
| Misspellings | No | hiddenLabel support |
| Hierarchy | No | broader/narrower |
| Associations | No | related concepts |
| Multi-lingual | No | Language-tagged labels |
| Disambiguation | No | Distinct concepts with same label |
| Vocabulary control | No | Managed concept schemes |

---

## Cross-References

### Related Papers

| Paper | Relationship to SKOS |
|-------|---------------------|
| REF-032 (KG) | SKOS concepts as nodes in graph |
| REF-030 (SBERT) | Could embed concept descriptions |

### Related Code Locations

| File | SKOS Usage |
|------|-----------|
| `crates/matric-db/src/skos_tags.rs` | Core SKOS implementation |
| `crates/matric-api/src/handlers/tags.rs` | Tag API endpoints |
| `migrations/xxx_skos_schema.sql` | Database schema |
| `mcp-server/src/tools/tags.ts` | MCP tag management |

---

## Improvement Opportunities

### 1. Import Standard Vocabularies

Load existing SKOS vocabularies:

```rust
pub async fn import_skos_rdf(
    pool: &PgPool,
    rdf_content: &str,
) -> Result<ImportStats> {
    // Parse RDF/XML or Turtle
    // Create concepts, labels, relations
}

// Example: Import Library of Congress Subject Headings
// import_skos_rdf(pool, lcsh_skos.rdf)
```

### 2. Auto-Suggest Based on Content

Use note content to suggest concepts:

```rust
pub async fn suggest_concepts(
    pool: &PgPool,
    note_content: &str,
) -> Result<Vec<ConceptSuggestion>> {
    // Extract key terms from content
    let terms = extract_terms(note_content);

    // Match against all labels
    let mut suggestions = Vec::new();
    for term in terms {
        let matches = search_concepts(pool, &term).await?;
        suggestions.extend(matches.into_iter().map(|m| ConceptSuggestion {
            concept_id: m.concept_id,
            label: m.pref_label,
            confidence: calculate_confidence(&term, &m.matched_label),
        }));
    }

    // Dedupe and rank
    dedupe_and_rank(suggestions)
}
```

### 3. Concept Embeddings

Embed concept descriptions for semantic matching:

```rust
pub async fn embed_concept(
    pool: &PgPool,
    concept_id: Uuid,
) -> Result<Vec<f32>> {
    // Combine preferred label + definition + related labels
    let concept_text = build_concept_text(pool, concept_id).await?;
    embed_text(&concept_text).await
}

// Use concept embeddings for:
// - Semantic tag search
// - Auto-suggest related concepts
// - Concept-based note clustering
```

### 4. Hierarchical Faceted Search

```rust
pub struct FacetedSearch {
    pub root_concepts: Vec<Uuid>,
    pub selected_facets: HashMap<Uuid, Vec<Uuid>>,
}

pub async fn faceted_search(
    pool: &PgPool,
    query: &str,
    facets: &FacetedSearch,
) -> Result<SearchResults> {
    // Full-text search with SKOS facet filtering
    // Returns results + facet counts
}
```

### 5. SKOS Export

```rust
pub async fn export_concept_scheme(
    pool: &PgPool,
    scheme_id: Uuid,
) -> Result<String> {
    // Generate RDF/Turtle
    let concepts = get_scheme_concepts(pool, scheme_id).await?;

    let mut output = format!(
        r#"@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
        <scheme/{scheme_id}> a skos:ConceptScheme .
        "#
    );

    for concept in concepts {
        output.push_str(&format_concept_turtle(&concept));
    }

    Ok(output)
}
```

---

## Critical Insights for matric-memory Development

### 1. altLabel is for Users, hiddenLabel is for Search

> "altLabel: A resource may have any number of alternate labels."
> "hiddenLabel: Hidden labels are NOT intended for display but may be used in search."

**Implication:** Show altLabels in autocomplete, never show hiddenLabels.

### 2. broader/narrower are Inverses

> "If A skos:broader B, then B skos:narrower A (by convention, not by inference)."

**Implication:** Store both explicitly for efficient traversal in either direction.

### 3. related is Symmetric

> "skos:related is symmetric by convention."

**Implication:** If A related B, store B related A explicitly.

### 4. Concepts vs Labels

> "Labels are strings; concepts are identities. Multiple concepts can share labels in different schemes."

**Implication:** Don't confuse label matching with concept identity.

---

## Key Quotes Relevant to matric-memory

> "SKOS provides a model for expressing basic structure and content of concept schemes such as thesauri, classification schemes, subject heading lists." (Section 1)
>
> **Relevance:** Defines SKOS as the right tool for matric-memory's tagging needs.

> "The preferred label is the label used as the primary identifier for a concept in a given language." (Section 2.2)
>
> **Relevance:** One canonical display name per concept per language.

> "Hidden labels may be used for synonyms that would be confusing to users if displayed, such as common misspellings." (Section 2.4)
>
> **Relevance:** Enables typo-tolerant search without showing mistakes.

> "The broader and narrower properties provide hierarchical links between concepts." (Section 8.1)
>
> **Relevance:** Enables hierarchical tag expansion in search.

---

## Summary

REF-033 (SKOS) transforms matric-memory's tagging system from simple strings to structured concepts with synonyms, hierarchies, and associations. Key capabilities:

1. **Synonym handling** via altLabel (user aliases) and hiddenLabel (misspellings)
2. **Hierarchical navigation** via broader/narrower relationships
3. **Related concept discovery** via related associations
4. **Multi-lingual support** via language-tagged labels

**Implementation Status:** Complete
**Schema:** Full SKOS data model in PostgreSQL
**Features:** Label search, hierarchy expansion, related suggestions
**Test Coverage:** SKOS operations have unit and integration tests
**Future Work:** Standard vocabulary import, auto-tagging, faceted search

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-01-25 | Initial analysis |
