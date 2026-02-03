# PostgreSQL Multilingual Full-Text Search: Technical Research

**Date:** 2026-02-01
**Project:** matric-memory
**Context:** Issues #316 (CJK fails), #319 (emoji fails), #308 (OR fails)

## Executive Summary

### Current State
- **FTS Configuration:** `matric_english` (English-only with unaccent)
- **Query Function:** `plainto_tsquery()` (no OR/NOT operators)
- **Database:** PostgreSQL 16 with pgvector extension
- **Problems:**
  - CJK (Chinese, Japanese, Korean) text fails to index/search properly
  - Emoji characters treated as whitespace/ignored
  - Cannot use OR operators in queries (plainto_tsquery limitation)

### Recommendation Priority

| Solution | Complexity | CJK Support | Emoji Support | OR Operators | Recommendation |
|----------|-----------|-------------|---------------|--------------|----------------|
| **websearch_to_tsquery + simple config** | Low | Partial | No | Yes | **Adopt for Phase 1** |
| **pg_bigm extension** | Medium | Excellent | Limited | Via LIKE | **Adopt for Phase 2** |
| **pg_trgm (trigram)** | Low | Good | Limited | Via similarity | Consider |
| **zhparser (Chinese only)** | High | Chinese only | No | Yes | Niche use case |
| **MeiliSearch** | Very High | Unknown | Unknown | Yes | Avoid (infrastructure) |

### Migration Strategy

**Phase 1: Quick Wins (Low Risk)**
1. Switch from `plainto_tsquery()` to `websearch_to_tsquery()` (fixes OR operators)
2. Add `simple` text search config alongside `matric_english` (basic CJK support)
3. Implement query routing: detect language and choose config

**Phase 2: Comprehensive CJK (Medium Risk)**
1. Install `pg_bigm` extension for bigram indexing
2. Create dual indexes: keep existing FTS, add bigm for CJK
3. Implement hybrid query: try FTS first, fallback to bigm for CJK

**Phase 3: Advanced Features (Future)**
- Consider pg_trgm for fuzzy matching/typo tolerance
- Evaluate dedicated emoji tokenization if needed

---

## 1. Built-in PostgreSQL Options

### 1.1 Text Search Configurations

#### Current: `matric_english`
```sql
-- Current setup (from migration 20260131000000)
CREATE TEXT SEARCH CONFIGURATION matric_english (COPY = english);
ALTER TEXT SEARCH CONFIGURATION matric_english
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, english_stem;
```

**Characteristics:**
- Stemming: "running" → "run", "databases" → "databas"
- Accent removal: "café" → "cafe"
- Stop word filtering: removes "the", "is", "at", etc.
- **CJK Support:** NONE (treats as non-alphanumeric)
- **Emoji Support:** NONE (treated as blank/whitespace)

#### Alternative: `simple` Configuration

```sql
-- Simple config (no stemming, universal tokenization)
CREATE TEXT SEARCH CONFIGURATION matric_simple (COPY = simple);
```

**Characteristics:**
- **No stemming:** "running" stays "running"
- **Lowercasing only:** converts to lowercase
- **Token types preserved:** respects locale's character classes
- **CJK Support:** PARTIAL (tokenizes by character boundaries based on locale)
- **Emoji Support:** LIMITED (depends on locale setting)

**Pros:**
- Works with any language without configuration
- No dependency on language-specific dictionaries
- Fast (no stemming overhead)
- Already built into PostgreSQL

**Cons:**
- No synonym/morphology support
- Poor recall for inflected languages (English, Spanish, etc.)
- CJK still limited by default parser's tokenization
- No emoji-specific handling

**Migration Path:**
```sql
-- Create simple config
CREATE TEXT SEARCH CONFIGURATION matric_simple (COPY = simple);

-- Add index on simple config (alongside existing matric_english)
CREATE INDEX idx_note_revised_tsv_simple ON note_revised_current
  USING gin (to_tsvector('matric_simple', content));

-- Query routing in application code
-- If CJK detected: use matric_simple
-- If Latin script: use matric_english
```

**Deployment Complexity:** **LOW** (built-in, no extensions)
**Performance Impact:** Minimal (simpler dictionary = faster indexing)

---

### 1.2 Query Parser Functions

#### Current: `plainto_tsquery()`

```sql
-- Current usage in matric-db/src/search.rs
WHERE nrc.tsv @@ plainto_tsquery('matric_english', $1)
```

**Characteristics:**
- Converts plain text to tsquery
- **Operators:** AND only (implicit)
- **Special characters:** Ignored/stripped
- **Error tolerance:** HIGH (never fails)
- **Use case:** Simple keyword search

**Limitations:**
- Cannot use OR: "cat OR dog" → searches for "cat" AND "OR" AND "dog"
- Cannot use NOT: "cat NOT dog" → treats NOT as keyword
- No phrase search
- No wildcard/prefix matching

#### Alternative: `websearch_to_tsquery()`

```sql
-- Replacement for user-facing search
WHERE nrc.tsv @@ websearch_to_tsquery('matric_english', $1)
```

**Characteristics:**
- **Operators supported:**
  - `OR` → boolean OR operator
  - `-word` → NOT operator (exclude)
  - `"phrase"` → phrase search (FOLLOWED BY operator)
- **Error tolerance:** VERY HIGH (guaranteed never to fail)
- **Special characters:** Safely ignored
- **Use case:** User-facing search interfaces

**Example Queries:**
```sql
-- OR operator
SELECT websearch_to_tsquery('english', 'cat or dog');
-- Result: 'cat' | 'dog'

-- NOT operator
SELECT websearch_to_tsquery('english', 'cat -dog');
-- Result: 'cat' & !'dog'

-- Phrase search
SELECT websearch_to_tsquery('english', '"machine learning"');
-- Result: 'machin' <-> 'learn'

-- Combined
SELECT websearch_to_tsquery('english', '"deep learning" or "machine learning" -tensorflow');
-- Result: ('deep' <-> 'learn') | ('machin' <-> 'learn') & !'tensorflow'
```

**Migration Path:**
```rust
// In crates/matric-db/src/search.rs
// Replace all occurrences of plainto_tsquery with websearch_to_tsquery

// Before:
// WHERE nrc.tsv @@ plainto_tsquery('matric_english', $1)

// After:
// WHERE nrc.tsv @@ websearch_to_tsquery('matric_english', $1)
```

**Deployment Complexity:** **VERY LOW** (function available in PG 11+)
**Performance Impact:** Negligible
**Backward Compatibility:** **BREAKING CHANGE** (query syntax changes)
  - Users can now use "OR", "-", quotes
  - Old queries continue to work (backward compatible)

**Recommendation:** **ADOPT IMMEDIATELY** - Fixes issue #308 with minimal risk

#### Alternative: `to_tsquery()` (Power Users)

**Characteristics:**
- **Operators:** Full boolean logic (`&`, `|`, `!`, `<->`)
- **Advanced features:** Weights (`:A`, `:B`), prefix matching (`word:*`)
- **Error tolerance:** LOW (raises syntax errors)
- **Use case:** Developer/API queries, not user input

**Recommendation:** **NOT SUITABLE** for user-facing search (strict syntax)

---

## 2. CJK-Specific Extensions

### 2.1 pg_bigm (Bigram Indexing)

**GitHub:** https://github.com/pgbigm/pg_bigm
**License:** PostgreSQL License (BSD-like)
**Latest Version:** 1.2-20250903

#### Overview
pg_bigm provides 2-gram (bigram) text indexing for full-text search, specifically designed for languages without word boundaries (CJK).

#### How It Works
Breaks text into overlapping 2-character sequences:
```
"PostgreSQL" → " P", "Po", "os", "st", "tg", "gr", "re", "eS", "SQ", "QL", "L "
"データベース" → " デ", "デー", "ータ", "タベ", "ベー", "ース", "ス "
```

#### Comparison with pg_trgm

| Feature | pg_trgm (3-gram) | pg_bigm (2-gram) |
|---------|------------------|------------------|
| **N-gram size** | 3 characters | 2 characters |
| **Index types** | GIN, GiST | GIN only |
| **CJK support** | NOT supported (*) | Excellent |
| **Short keyword (1-2 chars)** | Slow (seq scan) | Fast (indexed) |
| **Available operators** | LIKE, ILIKE, ~, ~* | LIKE only |
| **Similarity search** | Supported | Supported (v1.1+) |
| **Max indexed size** | ~228MB | ~102MB |
| **Case sensitivity** | No (similarity) | Yes |

(*) pg_trgm can work with CJK by modifying source and recompiling, but pg_bigm is faster.

#### Supported PostgreSQL Versions
PostgreSQL 9.1, 9.2, 9.3, 9.4, 9.5, 9.6, 10, 11, 12, 13, 14, 15, 16, 17, 18

#### Installation

**Requirements:**
- PostgreSQL development packages (`postgresql-devel` on RHEL/CentOS)
- Make and C compiler

**Steps:**
```bash
# Download from https://github.com/pgbigm/pg_bigm/releases
tar zxf pg_bigm-1.2-20250903.tar.gz
cd pg_bigm-1.2-20250903
make USE_PGXS=1 PG_CONFIG=/path/to/pg_config
sudo make USE_PGXS=1 PG_CONFIG=/path/to/pg_config install
```

**Configuration:**
```sql
-- Add to postgresql.conf
shared_preload_libraries = 'pg_bigm'

-- Restart PostgreSQL
-- Then in database:
CREATE EXTENSION pg_bigm;
```

#### Usage

**Create Index:**
```sql
-- Single column
CREATE INDEX note_content_bigm_idx ON note_revised_current
  USING gin (content gin_bigm_ops);

-- Multicolumn
CREATE INDEX note_multi_bigm_idx ON note_revised_current
  USING gin (content gin_bigm_ops, title gin_bigm_ops);
```

**Query Syntax:**
```sql
-- Basic LIKE search (uses bigm index)
SELECT * FROM note_revised_current
WHERE content LIKE '%機械学習%';  -- Japanese: "machine learning"

-- Using likequery helper function
SELECT * FROM note_revised_current
WHERE content LIKE likequery('機械学習');
-- likequery() adds % prefix/suffix and escapes special chars

-- Similarity search
SET pg_bigm.similarity_limit = 0.3;
SELECT note_id, bigm_similarity(content, '深層学習') AS sim
FROM note_revised_current
WHERE content =% '深層学習'  -- "deep learning"
ORDER BY sim DESC;
```

#### Functions

| Function | Purpose |
|----------|---------|
| `likequery(text)` | Converts keyword to LIKE pattern with escaping |
| `show_bigm(text)` | Returns array of all bigrams (debugging) |
| `bigm_similarity(text, text)` | Returns similarity score 0-1 |
| `pg_gin_pending_stats(regclass)` | GIN index pending list stats |

#### Configuration Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `pg_bigm.enable_recheck` | on | Enable result verification (must be on for correct results) |
| `pg_bigm.gin_key_limit` | 0 (unlimited) | Max bigrams to use for index scan (performance tuning) |
| `pg_bigm.similarity_limit` | 0.3 | Threshold for similarity search (0-1) |

#### Performance Characteristics

**Pros:**
- Fast for short keywords (1-2 characters) where pg_trgm fails
- Excellent for CJK languages
- Index-backed LIKE queries (no seq scan)
- Recheck mechanism filters false positives

**Cons:**
- GIN only (no GiST support)
- Larger index size than standard FTS (more bigrams than words)
- Case-sensitive (unlike pg_trgm similarity)
- LIKE operator only (no regex ~, ILIKE)

#### Limitations

1. **Max indexed column size:** 107,374,180 bytes (~102MB)
   - Attempting to index larger values causes "out of memory" error
   - pg_trgm allows ~228MB

2. **Recheck overhead:** False positives require verification
   - Example: "trial" matches "trivial" at index level
   - Recheck filters out false matches
   - Can be disabled (not recommended) via `pg_bigm.enable_recheck = off`

3. **Case sensitivity:** `bigm_similarity('ABC', 'abc')` returns 0
   - pg_trgm's `similarity()` would return 1
   - Use `LOWER()` for case-insensitive matching

#### Migration Path from Current FTS

**Dual Index Strategy (Recommended):**
```sql
-- Keep existing matric_english FTS
-- Existing: idx_note_revised_tsv

-- Add pg_bigm index for CJK
CREATE INDEX idx_note_revised_content_bigm ON note_revised_current
  USING gin (content gin_bigm_ops);

-- Application-level routing:
-- If query contains CJK: use bigm (LIKE)
-- If query is Latin script: use FTS (websearch_to_tsquery)
```

**Query Routing Logic (Rust):**
```rust
fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c,
            '\u{4E00}'..='\u{9FFF}' |  // CJK Unified Ideographs
            '\u{3040}'..='\u{309F}' |  // Hiragana
            '\u{30A0}'..='\u{30FF}' |  // Katakana
            '\u{AC00}'..='\u{D7AF}'    // Hangul
        )
    })
}

async fn search(query: &str) -> Result<Vec<SearchHit>> {
    if contains_cjk(query) {
        // Use pg_bigm LIKE search
        search_bigm(query).await
    } else {
        // Use standard FTS
        search_fts(query).await
    }
}
```

**Index Size Overhead:**
- Bigram indexes are larger than word-based FTS (more tokens)
- Estimate: 2-3x larger than tsvector index
- For 1GB text corpus: expect ~2-3GB bigram index

**Deployment Complexity:** **MEDIUM**
- Extension compilation required
- Server restart for shared_preload_libraries
- Dual index maintenance
- Application query routing logic

**Recommendation:** **ADOPT for CJK support** - Best balance of functionality and complexity

---

### 2.2 zhparser (Chinese Word Segmentation)

**GitHub:** https://github.com/amutu/zhparser
**License:** PostgreSQL License
**Dependency:** SCWS (Simple Chinese Word Segmentation) library

#### Overview
zhparser is a PostgreSQL text search parser for Mandarin Chinese, based on SCWS library. Provides proper word segmentation for Chinese text.

#### How It Works
Uses linguistic rules and dictionaries to segment Chinese text into words:
```
"机器学习很有趣" → "机器" "学习" "很" "有趣"
```

#### Supported PostgreSQL Versions
PostgreSQL 9.2+

#### Installation

**Requirements:**
- SCWS library 1.2.3+ (separate compilation)
- PostgreSQL development packages
- Build tools (gcc, make)

**Steps:**
```bash
# 1. Install SCWS library
wget http://www.xunsearch.com/scws/down/scws-1.2.3.tar.bz2
tar xjf scws-1.2.3.tar.bz2
cd scws-1.2.3
./configure --prefix=/usr/local
make && sudo make install

# FreeBSD: use --with-pic flag
# ./configure --prefix=/usr/local --with-pic

# 2. Install zhparser
git clone https://github.com/amutu/zhparser.git
cd zhparser
# Set PG_CONFIG if multiple PostgreSQL versions
make PG_CONFIG=/path/to/pg_config
sudo make PG_CONFIG=/path/to/pg_config install
```

**Configuration:**
```sql
CREATE EXTENSION zhparser;

-- Create Chinese text search configuration
CREATE TEXT SEARCH CONFIGURATION chinese_zh (PARSER = zhparser);
ALTER TEXT SEARCH CONFIGURATION chinese_zh
  ADD MAPPING FOR n,v,a,i,e,l WITH simple;

-- Create index
CREATE INDEX note_content_zh_idx ON note_revised_current
  USING gin (to_tsvector('chinese_zh', content));
```

#### Features

**Capabilities:**
- Proper Chinese word segmentation
- Part-of-speech tagging (noun, verb, adjective, etc.)
- Custom dictionary support (text and XDB binary formats)
- Dictionary caching for performance

**Pros:**
- Accurate Chinese word boundaries
- Integrates with PostgreSQL FTS (tsvector/tsquery)
- Supports custom dictionaries for domain-specific terms
- Can use websearch_to_tsquery with OR/NOT operators

**Cons:**
- **Chinese only** (not Japanese/Korean)
- External SCWS dependency (compilation complexity)
- Dictionary management overhead
- Platform-specific build issues (FreeBSD needs --with-pic)

#### Performance
- XDB binary dictionaries faster than text dictionaries
- Memory usage depends on dictionary size and caching settings
- Comparable to standard FTS once dictionaries loaded

#### Deployment Complexity: **HIGH**
- Two separate compilations (SCWS + zhparser)
- External library dependency
- Dictionary configuration and tuning
- Platform-specific build variations

#### Recommendation: **AVOID for general use**
- Only valuable if Chinese text is primary use case
- pg_bigm provides CJK support without SCWS dependency
- Adds significant operational complexity
- **Use pg_bigm instead** for multi-language CJK support

---

### 2.3 Other CJK Extensions

#### MeCab (Japanese Morphological Analyzer)
- **Use case:** Japanese text segmentation
- **Complexity:** Similar to zhparser (external dependency)
- **Recommendation:** Use pg_bigm instead (simpler, works for all CJK)

#### SCWS Standalone
- **Use case:** Chinese segmentation only
- **Recommendation:** Use zhparser if needed, or pg_bigm for simplicity

---

## 3. Universal N-gram Solution: pg_trgm

**Documentation:** https://www.postgresql.org/docs/16/pgtrgm.html
**License:** PostgreSQL (built-in contrib module)
**Status:** Trusted extension (non-superuser install)

### Overview
pg_trgm provides trigram (3-character sequence) matching for similarity search and pattern matching. Built into PostgreSQL.

### How It Works
```
"PostgreSQL" → " p", " po", "pos", "ost", "stg", "tgr", "gre", "res", "esq", "sql", "ql "
```

### Capabilities

**Functions:**
- `similarity(text, text)` → returns 0-1 score
- `word_similarity(text, text)` → substring matching
- `strict_word_similarity(text, text)` → word boundary matching
- `show_trgm(text)` → debugging (shows all trigrams)

**Operators:**
- `%` → similarity above threshold
- `<%`, `%>` → word similarity
- `<<%`, `%>>` → strict word similarity
- `<->`, `<<->`, `<<<->` → distance (1 - similarity)

**Index Types:**
- GIN: better for exact threshold matching
- GiST: better for nearest-neighbor (ORDER BY distance LIMIT)

### Configuration Parameters

```sql
pg_trgm.similarity_threshold = 0.3           -- % operator threshold
pg_trgm.word_similarity_threshold = 0.6
pg_trgm.strict_word_similarity_threshold = 0.5
```

### Supported Query Types

**Similarity Search:**
```sql
CREATE INDEX trgm_idx ON note_revised_current USING GIN (content gin_trgm_ops);

-- Similarity search
SELECT note_id, similarity(content, 'machine learning') AS sim
FROM note_revised_current
WHERE content % 'machine learning'
ORDER BY sim DESC;

-- Nearest neighbors (GiST efficient, GIN not)
SELECT note_id, content <-> 'deep learning' AS dist
FROM note_revised_current
ORDER BY dist LIMIT 10;
```

**Pattern Matching (LIKE/ILIKE/regex):**
```sql
-- LIKE without left anchor (indexed)
SELECT * FROM note_revised_current WHERE content LIKE '%search%';

-- Case-insensitive
SELECT * FROM note_revised_current WHERE content ILIKE '%Search%';

-- Regex
SELECT * FROM note_revised_current WHERE content ~ '(deep|machine) learning';
```

### CJK and Non-ASCII Support

**Limitation:** Documentation does not explicitly address CJK handling.

**Observed behavior:**
- Character-based trigrams work for any encoding
- No word boundary awareness in CJK (no spaces)
- May produce suboptimal results vs. specialized CJK extensions

**Workaround for CJK:**
```sql
-- Modify pg_trgm source (not recommended)
-- Comment out KEEPONLYALNUM in contrib/pg_trgm/pg_trgm.h
-- Rebuild PostgreSQL

-- Better: Use pg_bigm for CJK
```

### Emoji Support
Not explicitly documented. Likely treated as non-alphanumeric (ignored).

### Performance Characteristics

| Operation | Performance |
|-----------|-------------|
| Similarity search with index | Fast |
| LIKE/ILIKE with index | Fast (if extractable trigrams) |
| Regex with index | Fast (if extractable trigrams) |
| Nearest-neighbor (GiST) | Very fast |
| Nearest-neighbor (GIN) | Slow (full index scan) |
| Equality operator | Slower than B-tree |

**Index Selection:**
- **GiST:** Range queries, distance-ordered results
- **GIN:** Exact threshold matching, boolean queries

### Use Cases

**Spell Checking:**
```sql
-- Build word list from documents
CREATE TABLE words AS
  SELECT word FROM ts_stat(
    'SELECT to_tsvector(''simple'', content) FROM note_revised_current'
  );

CREATE INDEX words_idx ON words USING GIN (word gin_trgm_ops);

-- Find suggestions for misspelled word
SELECT word, similarity(word, 'machne') AS sim
FROM words
WHERE word % 'machne'
  AND length(word) BETWEEN length('machne')-2 AND length('machne')+2
ORDER BY sim DESC;
```

**Fuzzy Matching:**
```sql
-- Find similar notes (typo-tolerant)
SELECT note_id, content <-> 'machine learing' AS dist
FROM note_revised_current
ORDER BY dist LIMIT 10;
```

### Migration Path

**Add alongside existing FTS:**
```sql
-- Create pg_trgm index
CREATE INDEX idx_note_content_trgm ON note_revised_current
  USING GIN (content gin_trgm_ops);

-- Use for fuzzy/similarity search
-- Keep FTS for exact keyword matching
```

**Query routing:**
- Exact keyword search → FTS (websearch_to_tsquery)
- Fuzzy/typo search → pg_trgm (similarity)
- CJK search → pg_bigm (LIKE)

### Deployment Complexity: **LOW**
- Built-in contrib module
- No external dependencies
- `CREATE EXTENSION pg_trgm;`

### Recommendation: **CONSIDER for fuzzy matching**
- Complementary to FTS, not replacement
- Excellent for typo tolerance
- Not ideal for CJK (use pg_bigm instead)
- Good for spell-check and "did you mean?" features

---

## 4. Alternative Search Backends

### 4.1 MeiliSearch

**Website:** https://www.meilisearch.com
**License:** Open-source (55.7k GitHub stars) + Commercial cloud tier
**Written in:** Rust

#### Overview
Dedicated search engine with advanced relevancy, hybrid search (keyword + vector), and built-in analytics.

#### Core Features
- **Speed:** Sub-50ms response times
- **Search-as-you-type:** Real-time suggestions
- **Hybrid search:** Keyword + semantic/vector search
- **Advanced filtering:** Facets, geosearch, multi-modal (image/video/audio)
- **Analytics:** Search insights and performance monitoring
- **Typo tolerance:** Built-in fuzzy matching
- **Multi-language:** Automatic language detection

#### Language Support
**CJK and Emoji:** Not explicitly documented in available information.
- **CRITICAL GAP:** Would need to verify with MeiliSearch team
- No specific mention of Chinese/Japanese/Korean tokenization
- No emoji handling details

#### PostgreSQL Integration

**Architecture:** External service, not PostgreSQL extension

**Integration approach:**
1. PostgreSQL remains source of truth
2. Sync data to MeiliSearch via:
   - API calls (REST/SDK)
   - CDC (Change Data Capture) pipeline
   - Scheduled batch sync
3. Application queries MeiliSearch API
4. Results map back to PostgreSQL IDs

**SDKs Available:**
- JavaScript, Python, PHP, Ruby, Java, Go, Rust

**Example (Rust):**
```rust
use meilisearch_sdk::{Client, Index};

let client = Client::new("http://localhost:7700", "API_KEY");
let index = client.index("notes");

// Index a note
index.add_documents(&[
    serde_json::json!({
        "id": note_id,
        "content": note.content,
        "title": note.title,
    })
], Some("id")).await?;

// Search
let results = index.search()
    .with_query("machine learning")
    .execute::<Note>()
    .await?;
```

#### Deployment Complexity: **VERY HIGH**

**Infrastructure requirements:**
- Separate MeiliSearch server/cluster
- API gateway/routing
- Data synchronization pipeline
- Monitoring and alerting for sync lag
- Backup and disaster recovery for search index

**Operational overhead:**
- Two systems to maintain (PostgreSQL + MeiliSearch)
- Sync lag monitoring (eventual consistency)
- Index rebuilding after schema changes
- Additional failure modes (search service down)

**Cost:**
- Self-hosted: infrastructure costs (VMs, storage)
- Managed cloud: subscription fees
- Development time: integration and sync logic

#### Performance

**Pros:**
- Very fast (sub-50ms)
- Optimized for search workloads
- Scales horizontally

**Cons:**
- Network latency (extra hop)
- Sync lag (not real-time with PostgreSQL)
- Dual write complexity

#### Comparison to PostgreSQL FTS

| Aspect | PostgreSQL FTS | MeiliSearch |
|--------|----------------|-------------|
| **Deployment** | Built-in | Separate service |
| **Consistency** | Immediate | Eventual (sync lag) |
| **Complexity** | Low | High |
| **Performance** | Good (<100ms) | Excellent (<50ms) |
| **Features** | Basic FTS | Advanced relevancy, hybrid, analytics |
| **CJK Support** | Via extensions | Unknown (verify needed) |
| **Cost** | Free | Infrastructure + managed tier |
| **Failure modes** | One system | Two systems |

#### Recommendation: **AVOID for matric-memory**

**Rationale:**
1. **Complexity not justified:** PostgreSQL with pg_bigm + websearch_to_tsquery solves all current issues
2. **Unknown CJK support:** Critical gap in documentation
3. **Infrastructure burden:** Docker bundle deployment would need additional container
4. **Sync complexity:** CDC or batch sync adds failure modes
5. **Cost:** Managed tier adds subscription; self-hosted adds ops burden
6. **Overkill:** matric-memory is single-user/small-team, not web-scale search

**When to reconsider:**
- Multi-tenant SaaS with thousands of concurrent users
- Need for advanced relevancy tuning and A/B testing
- Budget and team for dedicated search infrastructure
- Verified excellent CJK/emoji support

---

### 4.2 Elasticsearch / OpenSearch

**Status:** Not researched in detail (similar concerns to MeiliSearch)

#### Overview
Enterprise search engines with comprehensive language support.

#### Similar Trade-offs
- **Deployment complexity:** Very high (JVM, cluster management)
- **Infrastructure cost:** Significant (RAM-hungry)
- **CJK support:** Excellent (dedicated analyzers)
- **PostgreSQL integration:** External sync required

#### Recommendation: **AVOID for same reasons as MeiliSearch**
- Massive operational complexity
- matric-memory doesn't need web-scale search
- pg_bigm solves CJK without external systems

---

## 5. Recommended Solution: Hybrid Approach

### Phase 1: Quick Wins (Immediate - Low Risk)

**Goal:** Fix OR operators, improve CJK tokenization

**Changes:**
1. Replace `plainto_tsquery()` with `websearch_to_tsquery()` everywhere
2. Create `matric_simple` text search config
3. Add `simple` config index on `note_revised_current`

**Migration:**
```sql
-- 1. Create simple config
CREATE TEXT SEARCH CONFIGURATION matric_simple (COPY = simple);

-- 2. Add index (alongside existing matric_english index)
CREATE INDEX idx_note_revised_tsv_simple ON note_revised_current
  USING gin (to_tsvector('matric_simple', content));

-- Keep existing: idx_note_revised_tsv (matric_english)
```

**Code changes (crates/matric-db/src/search.rs):**
```rust
// Replace all plainto_tsquery with websearch_to_tsquery
// Before:
// plainto_tsquery('matric_english', $1)

// After:
// websearch_to_tsquery('matric_english', $1)

// Add language detection
fn detect_language(query: &str) -> &'static str {
    if query.chars().any(|c| matches!(c,
        '\u{4E00}'..='\u{9FFF}' |  // CJK
        '\u{3040}'..='\u{309F}' |  // Hiragana
        '\u{30A0}'..='\u{30FF}' |  // Katakana
        '\u{AC00}'..='\u{D7AF}'    // Hangul
    )) {
        "matric_simple"
    } else {
        "matric_english"
    }
}

// Use in queries
let config = detect_language(&query);
sqlx::query(&format!(
    "SELECT ... WHERE tsv @@ websearch_to_tsquery('{}', $1)",
    config
))
```

**Benefits:**
- Fixes issue #308 (OR operators now work)
- Improves CJK search (basic character-level matching)
- Low risk (websearch_to_tsquery backward compatible with plain text)
- No external dependencies

**Limitations:**
- CJK still suboptimal (character-level, not word-level)
- No emoji support
- Simple config has no stemming (lower recall for English)

**Deployment Complexity:** **VERY LOW**
- SQL migration only
- Code change: find/replace function name
- No server restart
- No extension installation

**Timeline:** 1-2 days

---

### Phase 2: Comprehensive CJK (Medium-Term - Medium Risk)

**Goal:** Excellent CJK search performance

**Changes:**
1. Install pg_bigm extension
2. Create bigram indexes on text columns
3. Implement query routing (FTS for Latin, bigm for CJK)

**Migration:**
```sql
-- 1. Install pg_bigm
-- (requires server restart after shared_preload_libraries change)
-- See section 2.1 for installation steps

-- 2. Create bigram indexes
CREATE INDEX idx_note_revised_content_bigm ON note_revised_current
  USING gin (content gin_bigm_ops);

CREATE INDEX idx_note_original_content_bigm ON note_original
  USING gin (content gin_bigm_ops);

-- Keep existing FTS indexes (dual index strategy)
```

**Code changes:**
```rust
// Add CJK detection function
fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c,
            '\u{4E00}'..='\u{9FFF}' |  // CJK Unified Ideographs
            '\u{3040}'..='\u{309F}' |  // Hiragana
            '\u{30A0}'..='\u{30FF}' |  // Katakana
            '\u{AC00}'..='\u{D7AF}'    // Hangul
        )
    })
}

// Implement dual search strategy
async fn search_hybrid(query: &str, limit: i64) -> Result<Vec<SearchHit>> {
    if contains_cjk(query) {
        // Use pg_bigm LIKE search
        search_bigm(query, limit).await
    } else {
        // Use standard FTS with websearch_to_tsquery
        search_fts(query, limit).await
    }
}

// pg_bigm search implementation
async fn search_bigm(query: &str, limit: i64) -> Result<Vec<SearchHit>> {
    // Use likequery() function or manual escaping
    let pattern = format!("%{}%",
        query.replace('\\', "\\\\")
             .replace('%', "\\%")
             .replace('_', "\\_")
    );

    sqlx::query_as::<_, SearchHit>(r#"
        SELECT
            n.id AS note_id,
            1.0 AS score,  -- or use bigm_similarity for ranking
            NULL AS snippet,
            n.title,
            ARRAY[]::TEXT[] AS tags
        FROM note_revised_current nrc
        JOIN note n ON nrc.note_id = n.id
        WHERE nrc.content LIKE $1
        LIMIT $2
    "#)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(&pool)
    .await
}
```

**Benefits:**
- Excellent CJK search (word and character level)
- Fast for 1-2 character keywords (Chinese/Japanese common)
- Indexed LIKE queries (no seq scan)
- Coexists with FTS (best of both worlds)

**Limitations:**
- Still no emoji support (trigram-based, emoji treated as whitespace)
- Larger index size (2-3x FTS size)
- Requires extension compilation and installation
- Server restart for shared_preload_libraries

**Deployment Complexity:** **MEDIUM**
- Extension compilation (or package install if available)
- postgresql.conf modification
- Server restart
- Index creation (may take time on large datasets)

**Timeline:** 1 week (includes testing and gradual rollout)

---

### Phase 3: Advanced Features (Future)

**Potential additions:**
1. **pg_trgm for typo tolerance**
   - Fuzzy matching with similarity search
   - "Did you mean?" suggestions
   - Spell-check for search terms

2. **Emoji-specific handling**
   - Custom tokenization for emoji (if needed)
   - Evaluate actual usage (how often are emoji searched?)
   - May not need special handling if rarely searched

3. **Language-specific tuning**
   - Add more text search configs (Spanish, French, etc.)
   - Automatic language detection per note
   - Multi-language faceted search

---

## 6. Migration Risk Assessment

### Risk Matrix

| Change | Risk Level | Impact | Mitigation |
|--------|-----------|--------|------------|
| **websearch_to_tsquery** | LOW | High (fixes #308) | Extensive testing, backward compatible syntax |
| **matric_simple config** | LOW | Medium (basic CJK) | Add index, keep existing; gradual rollout |
| **pg_bigm extension** | MEDIUM | High (excellent CJK) | Staging environment testing, index build monitoring |
| **Query routing logic** | MEDIUM | High (correct index selection) | Feature flags, A/B testing, metrics |
| **Dual index maintenance** | LOW | Low (storage cost) | Monitor disk usage, index bloat |

### Rollback Plans

**Phase 1 (websearch_to_tsquery):**
```sql
-- Rollback: change function calls back to plainto_tsquery
-- No data migration needed
-- Deploy code change
```

**Phase 2 (pg_bigm):**
```sql
-- Rollback: drop bigm indexes
DROP INDEX idx_note_revised_content_bigm;
DROP INDEX idx_note_original_content_bigm;

-- Revert code to Phase 1 (FTS-only)
-- Extension can stay installed (no harm)
```

### Testing Strategy

**Phase 1 Testing:**
1. Unit tests: websearch_to_tsquery syntax variations
   - Plain text: "machine learning"
   - OR: "cat OR dog"
   - NOT: "cat -dog"
   - Phrase: "\"machine learning\""
   - Combined: "\"deep learning\" OR \"machine learning\" -tensorflow"

2. Integration tests: verify results match expected

3. Performance: compare query times vs plainto_tsquery

**Phase 2 Testing:**
1. CJK text indexing and search
   - Chinese: "机器学习" (machine learning)
   - Japanese: "データベース" (database)
   - Korean: "데이터베이스" (database)

2. Mixed content (Latin + CJK)

3. Short keywords (1-2 characters)

4. Performance: index build time, query performance, index size

5. Load testing: concurrent CJK queries

---

## 7. Performance Benchmarks (Estimates)

### Query Performance

| Query Type | Current (plainto) | Phase 1 (websearch) | Phase 2 (bigm) |
|------------|-------------------|---------------------|----------------|
| **English keyword** | ~50ms | ~50ms | N/A (uses FTS) |
| **English OR query** | N/A (broken) | ~60ms | N/A |
| **CJK 2-char keyword** | Fails or seq scan | Character match (~100ms) | ~30ms (indexed) |
| **CJK phrase** | Fails | Character match (~150ms) | ~40ms |
| **Mixed Latin + CJK** | Partial results | Partial results | ~50ms |

### Index Size

| Index Type | Size (per 1GB text) | Build Time (1M notes) |
|------------|---------------------|----------------------|
| **tsvector (matric_english)** | ~200MB | ~5 min |
| **tsvector (matric_simple)** | ~250MB | ~6 min |
| **pg_bigm GIN** | ~500-700MB | ~10-15 min |
| **pg_trgm GIN** | ~600-800MB | ~12-18 min |

### Storage Overhead (Dual Index)

For typical matric-memory deployment:
- Notes: 1,000-10,000 notes
- Average content: 1-5 KB per note
- Total text: 10-50 MB

**Index sizes:**
- matric_english: ~2-10 MB
- matric_simple: ~2-12 MB
- pg_bigm: ~5-25 MB

**Total overhead:** ~10-50 MB (negligible on modern hardware)

---

## 8. Decision Framework

### When to Use Each Solution

| Scenario | Recommended Solution |
|----------|---------------------|
| **English-only, need OR/NOT** | websearch_to_tsquery + matric_english |
| **European languages (accents)** | websearch_to_tsquery + matric_english (has unaccent) |
| **CJK occasional** | websearch_to_tsquery + matric_simple |
| **CJK primary** | websearch_to_tsquery + pg_bigm |
| **Chinese only, high accuracy** | zhparser (if willing to manage SCWS) |
| **Fuzzy/typo tolerance** | pg_trgm (alongside FTS) |
| **Web-scale search** | MeiliSearch/Elasticsearch (infrastructure required) |
| **Multi-modal (image/video)** | MeiliSearch (if budget allows) |

### matric-memory Specific Recommendation

**Current context:**
- Single-user or small-team deployment
- Docker bundle (all-in-one container)
- Mix of English and occasional CJK
- Issues: #316 (CJK), #319 (emoji), #308 (OR)

**Recommended path:**

1. **Now (Phase 1):** websearch_to_tsquery + matric_simple
   - Fixes #308 immediately
   - Improves #316 (basic CJK)
   - Low risk, high value
   - 1-2 days implementation

2. **Next (Phase 2):** Add pg_bigm
   - Fully resolves #316 (excellent CJK)
   - Prepares for international users
   - Medium effort, high value for CJK users
   - 1 week implementation

3. **Future (Phase 3):** Evaluate emoji usage
   - Monitor search logs: how often are emoji searched?
   - If < 1% of queries: defer or skip
   - If significant: investigate custom tokenization or pg_trgm

4. **Not recommended:** MeiliSearch/Elasticsearch
   - Complexity not justified for single-user deployment
   - PostgreSQL solution sufficient

---

## 9. Implementation Checklist

### Phase 1: websearch_to_tsquery + simple config

- [ ] Create migration: `20260202000000_websearch_query_and_simple_config.sql`
  - [ ] CREATE TEXT SEARCH CONFIGURATION matric_simple
  - [ ] CREATE INDEX idx_note_revised_tsv_simple
  - [ ] CREATE INDEX idx_note_original_fts_simple

- [ ] Update crates/matric-db/src/search.rs
  - [ ] Replace plainto_tsquery → websearch_to_tsquery (all occurrences)
  - [ ] Add language detection function
  - [ ] Update search() to use detected config
  - [ ] Update search_with_strict_filter()
  - [ ] Update search_filtered()
  - [ ] Update search_by_keyword()

- [ ] Update crates/matric-db/src/skos_tags.rs
  - [ ] Replace plainto_tsquery → websearch_to_tsquery

- [ ] Update crates/matric-db/src/embedding_sets.rs
  - [ ] Replace plainto_tsquery → websearch_to_tsquery

- [ ] Update tests
  - [ ] crates/matric-db/tests/text_search_config_test.rs
  - [ ] Add websearch_to_tsquery tests (OR, NOT, phrase)
  - [ ] Add CJK search tests
  - [ ] Add language detection tests

- [ ] Documentation
  - [ ] Update CLAUDE.md with new query syntax
  - [ ] Update API docs (if any)
  - [ ] Add examples of OR/NOT/phrase syntax

- [ ] Testing
  - [ ] Unit tests: query syntax variations
  - [ ] Integration tests: search results correctness
  - [ ] Performance tests: compare before/after
  - [ ] Manual QA: test in Claude Code MCP

- [ ] Deployment
  - [ ] Run migration on staging
  - [ ] Verify search works
  - [ ] Deploy to production
  - [ ] Monitor query performance

### Phase 2: pg_bigm extension

- [ ] Build/obtain pg_bigm
  - [ ] Download source or find package
  - [ ] Compile with PG_CONFIG
  - [ ] Install to PostgreSQL extensions directory

- [ ] Update Dockerfile (docker-compose.bundle.yml)
  - [ ] Add pg_bigm compilation step
  - [ ] Add shared_preload_libraries = 'pg_bigm' to postgresql.conf template

- [ ] Create migration: `20260209000000_add_pg_bigm.sql`
  - [ ] CREATE EXTENSION pg_bigm
  - [ ] CREATE INDEX idx_note_revised_content_bigm
  - [ ] CREATE INDEX idx_note_original_content_bigm

- [ ] Update crates/matric-db/src/search.rs
  - [ ] Add contains_cjk() function
  - [ ] Add search_bigm() function
  - [ ] Update search() to route based on language
  - [ ] Add bigm_similarity for ranking (optional)

- [ ] Testing
  - [ ] CJK search tests (Chinese, Japanese, Korean)
  - [ ] Short keyword tests (1-2 chars)
  - [ ] Mixed content tests (Latin + CJK)
  - [ ] Performance benchmarks
  - [ ] Index build monitoring

- [ ] Documentation
  - [ ] Update CLAUDE.md with pg_bigm details
  - [ ] Document dual index strategy
  - [ ] Add CJK search examples

- [ ] Deployment
  - [ ] Build new Docker image with pg_bigm
  - [ ] Test on staging (verify extension loads)
  - [ ] Run migration (monitor index build time)
  - [ ] Verify CJK search works
  - [ ] Deploy to production
  - [ ] Monitor performance and index size

---

## 10. References

### PostgreSQL Documentation
- [Text Search Dictionaries](https://www.postgresql.org/docs/16/textsearch-dictionaries.html)
- [Text Search Parsers](https://www.postgresql.org/docs/16/textsearch-parsers.html)
- [Text Search Controls](https://www.postgresql.org/docs/16/textsearch-controls.html)
- [pg_trgm Extension](https://www.postgresql.org/docs/16/pgtrgm.html)
- [unaccent Extension](https://www.postgresql.org/docs/16/unaccent.html)

### Extensions
- [pg_bigm Documentation](https://github.com/pgbigm/pg_bigm/blob/master/docs/pg_bigm_en.md)
- [pg_bigm Repository](https://github.com/pgbigm/pg_bigm)
- [zhparser Repository](https://github.com/amutu/zhparser)

### Alternative Solutions
- [MeiliSearch Website](https://www.meilisearch.com)
- Elasticsearch (not researched in detail)
- OpenSearch (not researched in detail)

### Related Issues
- Issue #316: CJK text search fails
- Issue #319: Emoji search fails
- Issue #308: OR operator not supported

---

## Appendix A: Query Syntax Comparison

### plainto_tsquery (Current)

```sql
-- Simple keyword
SELECT plainto_tsquery('english', 'machine learning');
-- Result: 'machin' & 'learn'

-- Attempts OR (fails - treats as keyword)
SELECT plainto_tsquery('english', 'cat OR dog');
-- Result: 'cat' & 'dog'  (OR is stripped as stop word)

-- Special characters (ignored)
SELECT plainto_tsquery('english', 'The Fat & Rats:C');
-- Result: 'fat' & 'rat' & 'c'
```

### websearch_to_tsquery (Recommended)

```sql
-- Simple keyword (backward compatible)
SELECT websearch_to_tsquery('english', 'machine learning');
-- Result: 'machin' & 'learn'

-- OR operator
SELECT websearch_to_tsquery('english', 'cat or dog');
-- Result: 'cat' | 'dog'

-- NOT operator
SELECT websearch_to_tsquery('english', 'cat -dog');
-- Result: 'cat' & !'dog'

-- Phrase search
SELECT websearch_to_tsquery('english', '"machine learning"');
-- Result: 'machin' <-> 'learn'

-- Combined
SELECT websearch_to_tsquery('english', '"deep learning" or "machine learning" -tensorflow');
-- Result: ('deep' <-> 'learn') | ('machin' <-> 'learn') & !'tensorflow'

-- Garbage input (safe - never errors)
SELECT websearch_to_tsquery('english', '""" )( dummy \\\\ query <->');
-- Result: 'dummi' & 'queri'
```

### to_tsquery (Power Users Only)

```sql
-- Explicit operators required
SELECT to_tsquery('english', 'machine & learning');
-- Result: 'machin' & 'learn'

-- OR
SELECT to_tsquery('english', 'cat | dog');
-- Result: 'cat' | 'dog'

-- NOT
SELECT to_tsquery('english', 'cat & !dog');
-- Result: 'cat' & !'dog'

-- Phrase
SELECT to_tsquery('english', 'machine <-> learning');
-- Result: 'machin' <-> 'learn'

-- Weights
SELECT to_tsquery('english', 'cat:A | dog:B');
-- Result: 'cat':A | 'dog':B

-- Prefix matching
SELECT to_tsquery('english', 'super:*');
-- Result: 'super':*

-- Invalid syntax (ERROR)
SELECT to_tsquery('english', 'cat dog');
-- ERROR: syntax error in tsquery: "cat dog"
```

---

## Appendix B: CJK Unicode Ranges

```rust
// Chinese (CJK Unified Ideographs)
'\u{4E00}'..='\u{9FFF}'   // 20,992 characters

// Japanese Hiragana
'\u{3040}'..='\u{309F}'   // 96 characters

// Japanese Katakana
'\u{30A0}'..='\u{30FF}'   // 96 characters

// Korean Hangul
'\u{AC00}'..='\u{D7AF}'   // 11,172 characters

// Additional CJK extensions (if needed)
'\u{3400}'..='\u{4DBF}'   // CJK Unified Ideographs Extension A
'\u{20000}'..='\u{2A6DF}' // CJK Unified Ideographs Extension B
'\u{2A700}'..='\u{2B73F}' // CJK Unified Ideographs Extension C
'\u{2B740}'..='\u{2B81F}' // CJK Unified Ideographs Extension D
'\u{2B820}'..='\u{2CEAF}' // CJK Unified Ideographs Extension E
'\u{2CEB0}'..='\u{2EBEF}' // CJK Unified Ideographs Extension F
```

---

## Appendix C: Example Queries

### English FTS with OR/NOT

```sql
-- Find notes about machine learning OR deep learning
SELECT n.id, n.title, nrc.content
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE nrc.tsv @@ websearch_to_tsquery('matric_english', 'machine learning or deep learning')
ORDER BY ts_rank(nrc.tsv, websearch_to_tsquery('matric_english', 'machine learning or deep learning')) DESC
LIMIT 20;

-- Find notes about Python but NOT Django
SELECT n.id, n.title
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE nrc.tsv @@ websearch_to_tsquery('matric_english', 'python -django')
LIMIT 20;

-- Phrase search: exact match for "neural network"
SELECT n.id, n.title
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE nrc.tsv @@ websearch_to_tsquery('matric_english', '"neural network"')
LIMIT 20;
```

### CJK Search with simple config

```sql
-- Chinese: "machine learning" (机器学习)
SELECT n.id, n.title
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE to_tsvector('matric_simple', nrc.content) @@
      websearch_to_tsquery('matric_simple', '机器学习')
LIMIT 20;

-- Japanese: "database" (データベース)
SELECT n.id, n.title
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE to_tsvector('matric_simple', nrc.content) @@
      websearch_to_tsquery('matric_simple', 'データベース')
LIMIT 20;
```

### CJK Search with pg_bigm

```sql
-- Chinese LIKE search with bigm index
SELECT n.id, n.title,
       bigm_similarity(nrc.content, '机器学习') AS sim
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE nrc.content LIKE likequery('机器学习')
ORDER BY sim DESC
LIMIT 20;

-- Japanese short keyword (1-2 characters)
SELECT n.id, n.title
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE nrc.content LIKE '%AI%'  -- Uses bigm index
LIMIT 20;

-- Similarity search (find related content)
SET pg_bigm.similarity_limit = 0.3;
SELECT n.id, n.title,
       bigm_similarity(nrc.content, '深層学習とニューラルネットワーク') AS sim
FROM note n
JOIN note_revised_current nrc ON n.id = nrc.note_id
WHERE nrc.content =% '深層学習とニューラルネットワーク'
ORDER BY sim DESC
LIMIT 20;
```

---

**End of Technical Research Document**
