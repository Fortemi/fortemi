# Best Practices

Research-backed guidance for getting the most out of Fortemi. Each section provides a quick recommendation, the reasoning behind it, a decision framework, and common mistakes to avoid.

## Search Mode Selection

**Quick recommendation:** Use hybrid search for most queries. Override to specialized modes only when you know the query type and its characteristics.

### Why Hybrid Works

Hybrid search combines BM25 full-text search (FTS) with dense vector retrieval using Reciprocal Rank Fusion (RRF). This approach:

- Handles both exact keyword matching and conceptual similarity
- Automatically balances term precision and semantic understanding
- Provides robust fallback when one method fails
- Achieves 15-20% better recall than either method alone

The system implements adaptive RRF with dynamic k-values and weights based on query characteristics:

- **Short queries** (1-2 words): k multiplied by 0.7 to favor top results
- **Long queries** (8+ words): k multiplied by 1.3 to consider more candidates
- **Quoted phrases**: k multiplied by 0.6 to focus on exact matches

### Decision Tree

```
Query analysis:
├── Unknown query type or mixed requirements
│   └── Use: hybrid (default)
│
├── Exact phrase, code snippet, known terminology
│   └── Use: fts
│       └── If contains CJK characters: enable FTS_BIGRAM_CJK=true
│       └── If contains emoji/symbols: enable FTS_TRIGRAM_FALLBACK=true
│
├── Conceptual question, "how do I", natural language
│   └── Use: semantic
│
└── Need score magnitude (not just ranking)
    └── Use: hybrid with rsf fusion mode
```

### Performance Characteristics

| Mode | Latency (10K docs) | Best for | Weakness |
|------|-------------------|----------|----------|
| **hybrid** | 45-80ms | General queries, unknown intent | Slightly slower than pure methods |
| **fts** | 15-30ms | Exact terms, code, identifiers | Misses conceptual matches |
| **semantic** | 30-60ms | Conceptual, natural language | Misses exact terminology |
| **adaptive-rrf** | 50-90ms | Mixed query characteristics | Highest latency |
| **rsf** | 50-85ms | Need actual score values | More computationally intensive |

### Adaptive Weights

The system automatically adjusts FTS/semantic weights based on query analysis:

| Query Type | FTS Weight | Semantic Weight | Detection Criteria |
|------------|------------|-----------------|-------------------|
| **Quoted phrases** | 0.70 | 0.30 | Contains "quoted text" |
| **Keyword-heavy** | 0.60 | 0.40 | Short, technical terms, no question words |
| **Balanced** | 0.50 | 0.50 | Default, mixed characteristics |
| **Conceptual** | 0.35 | 0.65 | Contains "how", "why", "what is", longer |

### When to Use RSF

Reciprocal Score Fusion (RSF) preserves score magnitude instead of just rank order:

- **Use when:** You need actual similarity scores for filtering, thresholding, or downstream ranking
- **Benefit:** +6% recall improvement on FIQA benchmark compared to RRF
- **Cost:** 10-15% higher latency due to score normalization

### Common Mistakes

**Using semantic for exact term lookup**
- Problem: Embedding models may generalize away specific terminology
- Solution: Use FTS mode for code, identifiers, or precise terms

**Using FTS for conceptual queries**
- Problem: Keyword matching fails when query and document use different words
- Solution: Use semantic or hybrid for "how to" questions and conceptual searches

**Not enabling multilingual feature flags**
- Problem: Poor results for CJK, emoji, or non-English queries
- Solution: Enable `FTS_SCRIPT_DETECTION=true`, `FTS_BIGRAM_CJK=true`, `FTS_TRIGRAM_FALLBACK=true`

**Over-specifying search mode**
- Problem: Manually choosing modes adds complexity without benefit
- Solution: Let hybrid mode handle most cases, override only when testing specific behaviors

## Embedding Strategy

**Quick recommendation:** Use `nomic-embed-text-v1.5` at 768 dimensions for general use. Consider MRL truncation to 256 dimensions for collections over 50K documents.

### Model Selection

Research (REF-068) shows that smaller models with LLM re-ranking outperform larger models by 7.7-23.1%. The key insight: model size matters less than domain fit and retrieval strategy.

**Domain factor (REF-069) has 1.00 effect size** - the strongest predictor of retrieval quality. Only fine-tune when baseline Recall@10 is below 60%.

| Use Case | Recommended Model | Dimensions | Rationale |
|----------|-------------------|------------|-----------|
| **General knowledge base** | nomic-embed-text-v1.5 | 768 | Best balance: MRL support, good performance, moderate size |
| **Budget/performance** | all-MiniLM-L6-v2 | 384 | 22M params, works best with re-ranking (REF-068) |
| **Multilingual (100+ langs)** | multilingual-e5-large | 1024 | Specialized for cross-lingual retrieval |
| **High accuracy, no MRL** | bge-large-en-v1.5 | 1024 | 335M params, strongest baseline quality |
| **High accuracy + MRL** | mxbai-embed-large-v1 | 1024 | 335M params, MRL support, best of both |

### MRL Trade-offs

Matryoshka Representation Learning (REF-067) enables dimension truncation with minimal quality loss, achieving up to 12x storage and compute savings.

| Dimensions | Storage Reduction | Quality Loss | Use When |
|------------|-------------------|--------------|----------|
| **768** (full) | 1x (baseline) | 0% | Default for most use cases |
| **512** | 1.5x | <1% | Negligible loss, moderate savings |
| **256** | 3x | 1-2% | Good balance for large collections (50K-500K docs) |
| **128** | 6x | 2-3% | Acceptable for very large collections (>500K docs) |
| **64** | 12x | 3-5% | Maximum savings, use only when storage/compute critical |

**MRL quality tradeoff analysis (REF-067):**
- 64-dim: 12x reduction, 3-5% Recall@10 loss
- 128-dim: 6x reduction, 2% loss
- 256-dim: 3x reduction, 1% loss

### Filter Sets vs Full Sets

**Filter sets** share embeddings from the default embedding set:
- Use when: Same model, just filtering results by tag/scheme
- Benefit: No duplicate embeddings, instant "set" creation
- Limitation: Cannot use different models or MRL dimensions

**Full sets** maintain independent embeddings:
- Use when: Different domain needing specialized model
- Use when: Want different MRL truncation per set
- Use when: Need to experiment with models without affecting production
- Cost: Full re-embedding required, storage multiplied

### Decision Tree

```
Embedding strategy selection:
├── Starting new system or general-purpose?
│   └── nomic-embed-text-v1.5, 768-dim, default set
│
├── Collection size > 50K documents?
│   └── Enable MRL, use 256-dim truncation
│       └── If size > 500K: consider 128-dim
│
├── Need multilingual support?
│   └── multilingual-e5-large, full 1024-dim
│
├── Budget-constrained with LLM re-ranking?
│   └── all-MiniLM-L6-v2, 384-dim (REF-068)
│
├── Multiple domains with different needs?
│   └── Full embedding sets per domain
│
└── Just filtering existing embeddings by tag?
    └── Filter sets (shared embeddings)
```

### Two-Stage Retrieval

For collections over 100K documents, use two-stage retrieval:

1. **Coarse stage:** Use 64-dim MRL truncation to scan entire corpus
2. **Fine stage:** Re-rank top 100 candidates with full 768-dim vectors

**Benefits:**
- 128x reduction in floating-point operations (MFLOPS)
- 12x reduction in vector comparison operations
- Maintains 98%+ recall compared to single-stage
- Latency reduction: 200ms -> 50ms for 500K document corpus

**When to enable:**
- Collection size > 100K documents
- Latency requirements < 100ms
- Storage or compute constraints

### Common Mistakes

**Assuming bigger model equals better results**
- Problem: REF-068 shows all-MiniLM-L6-v2 (22M) outperforms bge-large (335M) by 23.1% with re-ranking
- Solution: Choose model based on domain fit and retrieval strategy, not parameter count

**Using non-MRL models with dimension truncation**
- Problem: bge-large-en-v1.5 and all-MiniLM-L6-v2 don't support MRL, truncation degrades quality
- Solution: Use nomic-embed-text-v1.5 or mxbai-embed-large-v1 for MRL support

**Fine-tuning when baseline is already good**
- Problem: REF-069 shows domain fine-tuning only helps when Recall@10 < 60%
- Solution: Measure baseline first, only fine-tune if retrieval quality is poor

**Not using two-stage retrieval for large corpora**
- Problem: Full-dimension search on 500K+ docs causes 200ms+ latency
- Solution: Enable two-stage retrieval for 128x compute reduction with minimal quality loss

**Mixing MRL dimensions in filter sets**
- Problem: Filter sets share embeddings, cannot have per-"set" truncation
- Solution: Use full embedding sets when you need different MRL dimensions

## Chunking Strategy

**Quick recommendation:** Use semantic chunking for markdown documentation, syntactic chunking for code, and per-section chunking for formal documents. Aim for 500-2000 tokens per chunk with 50-100 character overlap.

### Why Chunking Matters

Effective chunking ensures:
- Embeddings capture cohesive semantic units
- Context windows don't split related information
- Retrieval granularity matches query scope
- Search results provide actionable excerpts

### Size Guidelines

| Embedding Model | Min Tokens | Optimal Range | Max Tokens |
|----------------|------------|---------------|------------|
| **all-MiniLM-L6-v2** | 200 | 500-1500 | 2000 |
| **nomic-embed-text-v1.5** | 300 | 700-1800 | 2500 |
| **bge-large / mxbai-embed-large** | 400 | 800-2000 | 3000 |
| **multilingual-e5-large** | 400 | 800-2000 | 3000 |

**Overlap:** Use 50-100 characters between chunks to avoid splitting sentences mid-thought.

### Decision Table by Document Structure

| Document Structure | Strategy | Reason | Overlap |
|-------------------|----------|--------|---------|
| **Markdown with headings** | `semantic` | Respects heading hierarchy, code blocks, lists | 100 chars |
| **Source code** | `syntactic` | Uses language-specific parsers, preserves function boundaries | 50 chars |
| **Plain prose/narrative** | `sentence` | Natural semantic boundaries, clean splits | 75 chars |
| **Blog posts, articles** | `paragraph` | Paragraph = complete thought unit | 100 chars |
| **Dense unstructured text** | `sliding_window` | Overlapping windows ensure coverage | 200 chars |
| **Mixed/unknown structure** | `recursive` | Tries multiple strategies, falls back gracefully | 100 chars |
| **Formal documents (RFC, spec)** | `per_section` | Section = discrete requirement/specification | Minimal |

### Chunking Strategies Explained

**SemanticChunker**
- Best for: Markdown, documentation, structured notes
- How: Parses markdown AST, splits on headings while preserving code blocks and lists
- Preserves: Code fences, blockquotes, list continuity

**SyntacticChunker**
- Best for: Source code (Python, JavaScript, Rust, Go, Java, etc.)
- How: Uses language-specific parsers (tree-sitter) to split on function/class boundaries
- Preserves: Complete functions, imports, docstrings

**SentenceChunker**
- Best for: Prose, essays, articles, natural language text
- How: Splits on sentence boundaries using linguistic rules
- Preserves: Complete sentences, paragraph context

**ParagraphChunker**
- Best for: Blog posts, formatted articles
- How: Splits on paragraph breaks (double newlines)
- Preserves: Complete paragraphs as semantic units

**SlidingWindowChunker**
- Best for: Dense text without clear structure, transcripts
- How: Fixed-size windows with overlap, ensures no content skipped
- Preserves: Coverage through overlap

**RecursiveChunker**
- Best for: Unknown or mixed content types, fallback strategy
- How: Tries delimiter-based splitting (newlines, sentences, words) recursively
- Preserves: Adaptability to unknown formats

**PerSectionChunker**
- Best for: Formal documents (RFCs, specifications, legal)
- How: Each numbered section becomes a chunk
- Preserves: Requirement/specification atomicity

### Common Mistakes

**Using fixed-size chunks for structured documents**
- Problem: Splits headings, code blocks, and lists mid-unit, destroying semantic coherence
- Solution: Use semantic or syntactic chunking to respect document structure

**Not overlapping chunks**
- Problem: Information spanning chunk boundaries is lost, reducing recall
- Solution: Use 50-100 character overlap to maintain context continuity

**Chunks too small**
- Problem: Insufficient context for embeddings, noisy retrieval results
- Solution: Keep chunks above 500 tokens for most embedding models

**Chunks too large**
- Problem: Diluted semantic signal, poor match granularity, exceeds model context
- Solution: Keep chunks below 2500 tokens, prefer smaller when possible

**Wrong chunking strategy for document type**
- Problem: Using sentence chunker on code, or syntactic chunker on prose
- Solution: Match strategy to document structure (see decision table above)

**Ignoring chunk overlap in retrieval**
- Problem: Duplicate content in search results confuses users
- Solution: De-duplicate overlapping chunks in post-processing or rank aggregation

## Tag Organization

**Quick recommendation:** Let the NLP pipeline handle content-based tagging automatically. Only add **organizational tags** manually — project names, status markers, and scope identifiers that can't be inferred from content.

### Automatic vs Manual Tags

Fortémi's NLP pipeline generates 8-15 SKOS concept tags per note covering domain, topic, methodology, application, technique, and content-type. These **content-based tags are fully automatic** and don't require manual intervention.

**What the AI tags automatically:**
- Domain classification (`domain/programming`, `domain/science/physics`)
- Topic identification (`topic/rust/ownership`, `topic/machine-learning`)
- Methodology (`methodology/experimental`, `methodology/literature-review`)
- Content type (`content-type/tutorial`, `content-type/research-paper`)

**What you should tag manually:**
- Project assignment (`project/alpha`, `client/acme`)
- Status tracking (`status/draft`, `status/reviewed`, `status/archived`)
- Scope/visibility (`scope/personal`, `scope/work`, `scope/public`)
- Source tracking (`source/meeting`, `source/conversation`)

### Why Hierarchical Tags

Hierarchical tags with SKOS semantics provide:
- **Inheritance:** Search for `domain/programming` finds notes also tagged `domain/programming/rust`
- **Filtering:** Strict tag filtering guarantees data isolation for multi-tenancy
- **Navigation:** Tree structure enables browsing and exploration
- **Clarity:** Path format shows context: `project/fortemi/architecture`

### PMEST Faceted Classification

For the manual organizational tags, PMEST provides a useful framework:

**Personality (Type)** - What kind of resource? (auto-detected as `content-type`)
- Auto-generated by AI — typically don't need manual `type/` tags

**Matter (Source)** - Where did it come from?
- `source/book`, `source/article`, `source/video`, `source/conversation`
- Useful for filtering by provenance

**Energy (Domain)** - What subject area?
- Auto-generated by AI — the pipeline handles `domain/` classification

**Space (Scope)** - What context?
- `scope/personal` - Personal use only
- `scope/work` - Work-related content
- `scope/public` - Shareable externally

**Time (Status)** - What stage?
- `status/active` - Currently working on
- `status/archived` - Completed or deprecated
- `status/someday` - Future consideration

### Example Tag Set

A note's tags are a combination of **AI-generated** and **manual** tags:

```
# Auto-generated by NLP pipeline (don't add these manually):
domain/technology/databases/postgresql
topic/indexing/btree
content-type/technical-note
methodology/analysis

# Manually added (organizational):
project/fortemi
scope/work
status/active
```

### Anti-Pattern Detection

The system automatically detects tag organization issues:

| Anti-Pattern | Example | Problem | Fix |
|-------------|---------|---------|-----|
| **over_nesting** | `a/b/c/d/e/f/g` | Too deep (>4 levels), hard to navigate | Simplify to 3-4 levels max |
| **meta_tag** | `important`, `todo`, `urgent` | Generic, non-descriptive, status better in fields | Use specific domain tags, track status separately |
| **orphan** | Tag with 0 notes | Unused tag clutters namespace | Delete or archive unused tags |
| **synonym_sprawl** | `tech`, `technology`, `technical` | Inconsistent terminology | Pick one canonical form, merge others |
| **mixed_hierarchy** | `technology/rust` + `rust/programming` | Inconsistent structure | Standardize on one hierarchy pattern |

### Hierarchy Depth Guidelines

| Depth | Example | Use When |
|-------|---------|----------|
| **1 level** | `technology` | Very broad categorization, rare |
| **2 levels** | `technology/rust` | Common, good balance |
| **3 levels** | `technology/rust/async` | Specific topic, still manageable |
| **4 levels** | `technology/rust/async/tokio` | Maximum recommended depth |
| **5+ levels** | `technology/rust/async/tokio/runtime/config` | Avoid - too complex |

### Common Mistakes

**Manually tagging what the AI handles**
- Problem: Adding `domain/technology/rust` manually when the AI already generates these tags
- Solution: Let the NLP pipeline handle content-based tagging. Only add organizational tags (project, status, scope) manually

**Not reviewing auto-generated concepts**
- Problem: AI-generated concepts accumulate in "candidate" status, synonym sprawl develops
- Solution: Periodically review `GET /api/v1/concepts?status=candidate`, promote good concepts, merge duplicates

**Over-nesting beyond 4 levels**
- Problem: Deep hierarchies become unmanageable, hard to browse or remember
- Solution: Limit to 3-4 levels, use multiple facets instead of single deep tree

**Generic meta-tags**
- Problem: Tags like `important`, `todo`, `review` don't describe content, just status
- Solution: Use note fields for status, use tags for domain/topic classification

**Synonym sprawl**
- Problem: `tech`, `technology`, `technical` fragment search results
- Solution: Choose one canonical term, create SKOS `broader`/`narrower` relations to merge. The AI may generate synonyms — curate periodically

**Mixing unrelated domains in same hierarchy**
- Problem: `work/project/rust/meeting` mixes organizational structure with domain topics
- Solution: Use separate facets: `domain/technology/rust` + `scope/work` + `type/project`

**Not using SKOS schemes for isolation**
- Problem: All users' tags in global namespace, risk of collision or leakage
- Solution: Create SKOS scheme per tenant, use `required_schemes` filter for strict isolation

## Multi-Tenancy and Isolation

**Quick recommendation:** Use SKOS scheme-based isolation with `required_schemes` filtering for guaranteed data segregation between tenants, teams, or projects.

### Strict Tag Filtering Architecture

Fortemi implements **pre-search filtering** using PostgreSQL WHERE clauses before vector search. This provides:

- **100% isolation guarantee:** Filters applied at SQL level, impossible to leak across boundaries
- **Performance:** Filters reduce search space, making vector operations faster
- **Security:** Trust boundary at database layer, not application logic

### Pattern for Multi-Tenancy

1. **Create SKOS scheme per tenant**
   ```
   POST /tags/schemes
   {
     "uri": "scheme://tenant-acme-corp",
     "name": "Acme Corp Knowledge Base"
   }
   ```

2. **Tag all tenant content within their scheme**
   ```
   All Acme Corp notes tagged with:
   - scheme://tenant-acme-corp/projects/roadrunner
   - scheme://tenant-acme-corp/teams/engineering
   ```

3. **Apply `required_schemes` filter on all searches**
   ```json
   {
     "query": "widget specifications",
     "required_schemes": ["scheme://tenant-acme-corp"]
   }
   ```

4. **Result:** Only notes within `scheme://tenant-acme-corp` are considered, regardless of query

### Filter Types

| Filter | Logic | Use Case | Example |
|--------|-------|----------|---------|
| **required_tags** | AND | Must have all tags | `["project/matric", "status/active"]` |
| **any_tags** | OR | Must have at least one tag | `["language/rust", "language/python"]` |
| **excluded_tags** | NOT | Must not have any tag | `["status/archived", "scope/private"]` |
| **required_schemes** | Isolation | Must be in scheme(s) | `["scheme://tenant-acme"]` |
| **excluded_schemes** | Exclusion | Must not be in scheme(s) | `["scheme://deprecated"]` |

### Combining Filters

Filters are applied as SQL WHERE clauses before vector search:

```sql
WHERE
  -- required_tags: note must have ALL of these
  note_id IN (SELECT note_id FROM note_tags WHERE tag_id = 'project/matric')
  AND note_id IN (SELECT note_id FROM note_tags WHERE tag_id = 'status/active')

  -- any_tags: note must have AT LEAST ONE of these
  AND note_id IN (
    SELECT note_id FROM note_tags
    WHERE tag_id IN ('language/rust', 'language/python')
  )

  -- excluded_tags: note must have NONE of these
  AND note_id NOT IN (
    SELECT note_id FROM note_tags
    WHERE tag_id IN ('status/archived', 'scope/private')
  )

  -- required_schemes: note must be in this scheme
  AND note_id IN (
    SELECT note_id FROM note_tags nt
    JOIN tags t ON nt.tag_id = t.id
    WHERE t.scheme_id IN ('scheme://tenant-acme')
  )
```

### Common Mistakes

**Relying on soft tag filtering for security**
- Problem: Application-layer filtering can be bypassed by bugs or injection
- Solution: Use `required_schemes` with SQL-level enforcement for security boundaries

**Forgetting to filter in all query paths**
- Problem: Search endpoint filtered, but graph traversal or related notes APIs not filtered
- Solution: Apply tenant scheme filter in middleware/context, enforce at database layer

**Not using scheme isolation for strong boundaries**
- Problem: Using regular tags for tenant separation, risk of tag collision or manual error
- Solution: SKOS schemes provide URI-based namespacing with strict semantics

**Over-filtering**
- Problem: Applying too many filter combinations makes search too restrictive, zero results
- Solution: Start broad (scheme isolation only), add specific filters progressively

**Mixing isolation and categorization**
- Problem: Using `required_schemes` for topic filtering instead of security isolation
- Solution: Schemes for tenant boundaries, regular tags for topic categorization within tenant

## Performance Optimization

**Quick recommendation:** Start with default settings. Tune HNSW `ef_search` and enable batch operations as you scale past 10K documents. Use strict tag filtering to reduce search space.

### HNSW Parameter Tuning

HNSW (Hierarchical Navigable Small World) indexes provide approximate nearest neighbor search. The `ef_search` parameter controls the accuracy-speed tradeoff.

**Adaptive ef_search formula:**
```
ef_search = base_ef * max(1.0, log2(corpus_size / 10000) * scale_factor)
```

**Recall target table:**

| Recall Target | base_ef | scale_factor | 10K docs | 100K docs | 1M docs |
|---------------|---------|--------------|----------|-----------|---------|
| **Fast** (85%) | 40 | 0.5 | 40 | 53 | 73 |
| **Balanced** (92%) | 80 | 1.0 | 80 | 160 | 293 |
| **High** (96%) | 150 | 1.5 | 150 | 375 | 825 |
| **Exhaustive** (99%) | 300 | 2.0 | 300 | 900 | 2400 |

**When to tune:**
- Collection size > 50K documents: Increase ef_search for maintained recall
- Latency requirements < 50ms: Decrease ef_search, accept lower recall
- Precision-critical queries: Use "High" or "Exhaustive" preset

### Batch Operations

**Embedding jobs:** Batch 50-100 documents per job for optimal throughput
```
POST /jobs/embed
{
  "note_ids": [100 IDs],
  "embedding_set_id": "default"
}
```

**Bulk imports:** Use bulk note creation endpoint with pre-computed embeddings
```
POST /notes/bulk
{
  "notes": [50-100 note objects]
}
```

**Benefits:**
- 10x reduction in API round-trips
- Shared connection pooling
- Transaction batching
- Embedding model batch inference

### Query Optimization

**Pre-filter with strict tags:**
```json
{
  "query": "rust async patterns",
  "required_schemes": ["scheme://my-tenant"],
  "required_tags": ["domain/technology/rust"]
}
```
- Reduces search space by 90%+ for tenanted systems
- Allows smaller `ef_search` with same recall
- Faster vector operations on smaller candidate set

**Use MRL for faster vector comparisons:**
- 256-dim vs 768-dim: 3x fewer floating-point operations
- 128-dim vs 768-dim: 6x reduction
- Enable for collections > 50K documents

**Enable two-stage retrieval:**
```json
{
  "query": "machine learning fundamentals",
  "two_stage": true,
  "coarse_dim": 64,
  "coarse_top_k": 100,
  "fine_dim": 768
}
```
- 128x reduction in MFLOPS
- Stage 1: Fast 64-dim scan finds top 100
- Stage 2: Precise 768-dim re-ranking
- Use for collections > 100K documents

### Hardware Reference Tiers

| Scale | Documents | Hardware | Key Optimization |
|-------|-----------|----------|-----------------|
| **Small** | <10K | 2 CPU, 4GB RAM, 20GB SSD | Default settings sufficient |
| **Medium** | 10K-100K | 4 CPU, 8GB RAM, 100GB SSD | MRL 256-dim, batch jobs |
| **Large** | 100K-500K | 8 CPU, 16GB RAM, 500GB SSD | Two-stage retrieval, ef_search tuning |
| **Very Large** | 500K-2M | 16 CPU, 32GB RAM, 1TB NVMe | MRL 128-dim, aggressive batching, read replicas |
| **Enterprise** | >2M | 32+ CPU, 64GB+ RAM, NVMe RAID | Sharding, distributed search, dedicated vector DB |

**PostgreSQL configuration for scale:**
- `shared_buffers`: 25% of RAM
- `effective_cache_size`: 50% of RAM
- `maintenance_work_mem`: 1GB for index builds
- `max_connections`: 100-200 (use connection pooling)

### Common Mistakes

**Over-tuning HNSW for small collections**
- Problem: Adjusting ef_search for 5K document corpus provides minimal benefit
- Solution: Use defaults until collection exceeds 50K documents

**Not using strict filters to reduce search space**
- Problem: Searching entire corpus when query scope is narrow (single project/tenant)
- Solution: Apply `required_schemes` and `required_tags` filters to reduce candidate set by 90%+

**Running embedding jobs sequentially**
- Problem: Processing 10K notes one-by-one takes hours
- Solution: Batch 50-100 notes per job, run jobs in parallel

**Not enabling MRL for large collections**
- Problem: Full 768-dim vectors on 500K corpus causes 200ms+ latency
- Solution: Use 256-dim MRL truncation for 3x speedup with <2% quality loss

**Using cloud embedding models for high-volume workloads**
- Problem: API rate limits and latency accumulate, expensive at scale
- Solution: Use local Ollama for >1000 embeds/day, cloud for bursty workloads

**Not using connection pooling**
- Problem: Opening new PostgreSQL connection per request adds 50-100ms overhead
- Solution: Use connection pool (sqlx default), configure `max_connections` appropriately

## Security

**Quick recommendation:** Use API keys for scripts and CLI tools, OAuth2 with PKCE for web applications, PKE encryption for sensitive documents, and local Ollama for privacy-sensitive embeddings.

### Authentication and Authorization

Fortemi supports multiple authentication mechanisms based on use case:

| Need | Solution | Setup | Security Level |
|------|----------|-------|----------------|
| **Script/CLI access** | API key with minimal scope | Generate via `/oauth/register` | Medium - Revocable, scoped |
| **Web application** | OAuth2 with PKCE | Standard OAuth2 flow | High - Time-limited tokens |
| **Sensitive documents** | PKE encryption | X25519 key exchange + AES-256-GCM | Very High - E2E encryption |
| **Privacy-sensitive embeddings** | Local Ollama | Run inference on-premise | High - No data leaves network |
| **Multi-user access control** | OAuth2 scopes + SKOS schemes | Combine with `required_schemes` | High - SQL-enforced isolation |
| **Backup encryption** | Knowledge shard encryption | Export with PKE encryption | High - Encrypted at rest |

### API Key Best Practices

**Generate minimal-scope keys:**
```bash
curl -X POST https://memory.example.com/oauth/register \
  -H "Content-Type: application/json" \
  -d '{
    "client_name": "Backup Script",
    "grant_types": ["client_credentials"],
    "scope": "read"
  }'
```

**Scopes:**
- `read` - Search, retrieve notes (safe for most scripts)
- `write` - Create, update notes
- `delete` - Delete notes, tags
- `admin` - User management, system configuration
- `mcp` - MCP server token introspection

**Rotation schedule:**
- Human API keys: Every 90 days
- Service accounts: Every 180 days
- Compromised keys: Immediate revocation

### OAuth2 Flow for Web Apps

**PKCE (Proof Key for Code Exchange) prevents authorization code interception:**

1. Generate code_verifier (random string)
2. Compute code_challenge = base64url(sha256(code_verifier))
3. Redirect to `/oauth/authorize?code_challenge=...`
4. Exchange authorization code + code_verifier for token
5. Use token for API requests until expiration

**Token lifetimes:**
- Access token: 1 hour
- Refresh token: 30 days
- Rotate access token before expiration using refresh token

### PKE Encryption for Sensitive Documents

**Use case:** Share sensitive notes without server having decryption key

**Key exchange:**
```
1. Recipient generates X25519 key pair (private key never leaves device)
2. Sender retrieves recipient's public key
3. Sender encrypts note with AES-256-GCM, uses ECDH shared secret for key derivation
4. Encrypted note stored on server, only recipient can decrypt
```

**Encryption command:**
```bash
POST /notes/{id}/encrypt
{
  "recipient_public_key": "base64-encoded-x25519-pubkey"
}
```

**Decryption (client-side):**
```bash
GET /notes/{id}/encrypted
# Returns encrypted payload
# Client decrypts using private key + ECDH
```

### Privacy-Sensitive Embeddings

**Problem:** Sending documents to OpenAI/cloud providers for embedding reveals content

**Solution:** Use local Ollama instance

**Setup:**
```bash
# Install Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# Pull embedding model
ollama pull nomic-embed-text

# Configure Fortémi
export OLLAMA_HOST=http://localhost:11434
export EMBEDDING_MODEL=nomic-embed-text
```

**Benefits:**
- No data leaves local network
- No API rate limits or costs
- Full control over model versions
- Compliance with data residency requirements

### Multi-Tenant Security

**Pattern:**
1. Create OAuth2 client per tenant
2. Create SKOS scheme per tenant
3. Inject `required_schemes` filter from OAuth token claims
4. Enforce at middleware layer before any database query

**Implementation:**
```rust
// Extract tenant from OAuth token
let tenant_id = claims.get("tenant_id")?;

// Enforce scheme filter
query.required_schemes = vec![format!("scheme://{}", tenant_id)];

// All subsequent queries isolated to this tenant
```

### Common Mistakes

**Over-privileged API keys**
- Problem: CLI script has `admin` scope, compromised key grants full access
- Solution: Use minimal scope (`read` or `write` only), never `admin` for automation

**Not rotating tokens**
- Problem: Long-lived tokens increase risk window if compromised
- Solution: Rotate API keys every 90 days, use short-lived OAuth tokens (1 hour)

**Storing secrets in code**
- Problem: Hardcoded API keys committed to git, exposed in logs
- Solution: Use environment variables, secret management (HashiCorp Vault, AWS Secrets Manager)

**Using cloud inference for sensitive data without considering privacy**
- Problem: Medical records, legal documents, PII sent to OpenAI for embedding
- Solution: Use local Ollama for regulated/sensitive content, cloud for non-sensitive

**Not enforcing tenant isolation at database layer**
- Problem: Application logic filters by tenant, but SQL injection or bug bypasses filter
- Solution: Use `required_schemes` with SQL WHERE clause, enforce in middleware

**Unencrypted backups**
- Problem: Exporting notes to disk without encryption, readable by anyone with file access
- Solution: Use knowledge shard encryption, export with PKE encryption enabled

**Missing audit logging**
- Problem: Cannot track who accessed sensitive documents or detect breaches
- Solution: Enable audit logging for read/write operations on sensitive schemes

---

For detailed configuration, see [Configuration Reference](./configuration.md). For deployment patterns, see [Use Cases](./use-cases.md). For API documentation, visit `/docs` on your running instance.
