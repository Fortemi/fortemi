# Multilingual Full-Text Search Architecture Design

**Version:** 1.0
**Date:** 2026-02-01
**Author:** Architecture Designer
**Status:** Draft

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current State Analysis](#current-state-analysis)
3. [Requirements Analysis](#requirements-analysis)
4. [Proposed Architecture](#proposed-architecture)
5. [Component Design](#component-design)
6. [Data Model Changes](#data-model-changes)
7. [Migration Strategy](#migration-strategy)
8. [API Design](#api-design)
9. [Performance Analysis](#performance-analysis)
10. [Risk Assessment](#risk-assessment)
11. [Implementation Roadmap](#implementation-roadmap)
12. [Architectural Decision Records](#architectural-decision-records)

---

## 1. Executive Summary

This document presents the architecture for adding multilingual Full-Text Search (FTS) capabilities to matric-memory. The design supports CJK (Chinese, Japanese, Korean) scripts, emoji/symbol search, and additional scripts (Arabic, Cyrillic, Hebrew, etc.) while maintaining backward compatibility with existing English content and preserving the hybrid search (FTS + semantic + RRF) architecture.

### Key Design Decisions

1. **Script-Aware Hybrid Approach**: Combine PostgreSQL's language-specific configurations with pg_trgm for universal fallback
2. **Automatic Language Detection**: Server-side detection with optional client hints
3. **Multi-Configuration Search**: Query across multiple text search configurations with unified ranking
4. **Semantic Search Remains Language-Agnostic**: Multilingual embedding models handle cross-lingual retrieval

---

## 2. Current State Analysis

### 2.1 Current Architecture Diagram

```
                     ┌──────────────────────────────────────────────────────────────┐
                     │                      Search Request                          │
                     │                    (query: string)                           │
                     └─────────────────────────┬────────────────────────────────────┘
                                               │
                     ┌─────────────────────────▼────────────────────────────────────┐
                     │                  HybridSearchEngine                          │
                     │                 (matric-search crate)                        │
                     │                                                              │
                     │  ┌─────────────────┐         ┌─────────────────┐            │
                     │  │   FTS Branch    │         │ Semantic Branch │            │
                     │  │  (if weight>0)  │         │  (if weight>0)  │            │
                     │  └────────┬────────┘         └────────┬────────┘            │
                     └───────────┼───────────────────────────┼─────────────────────┘
                                 │                           │
                     ┌───────────▼───────────┐   ┌───────────▼───────────┐
                     │     PgFtsSearch       │   │  PgEmbeddingRepository │
                     │   (matric-db crate)   │   │   (matric-db crate)    │
                     └───────────┬───────────┘   └───────────┬───────────┘
                                 │                           │
                     ┌───────────▼───────────┐   ┌───────────▼───────────┐
                     │   matric_english      │   │      pgvector          │
                     │   text search config  │   │   cosine similarity    │
                     │   + GIN index on tsv  │   │   + HNSW/IVFFlat       │
                     └───────────┬───────────┘   └───────────┬───────────┘
                                 │                           │
                                 └───────────┬───────────────┘
                                             │
                     ┌───────────────────────▼───────────────────────────┐
                     │                   RRF Fusion                      │
                     │             (Reciprocal Rank Fusion)              │
                     │                   k=20 (adaptive)                 │
                     └───────────────────────┬───────────────────────────┘
                                             │
                     ┌───────────────────────▼───────────────────────────┐
                     │                  Deduplication                    │
                     │             (chunked document handling)           │
                     └───────────────────────┬───────────────────────────┘
                                             │
                     ┌───────────────────────▼───────────────────────────┐
                     │                 Search Results                    │
                     │            Vec<EnhancedSearchHit>                 │
                     └──────────────────────────────────────────────────┘
```

### 2.2 Current FTS Implementation

**Text Search Configuration:**
```sql
-- From migration 20260131000000_fts_unicode_normalization.sql
CREATE TEXT SEARCH CONFIGURATION matric_english (COPY = english);
ALTER TEXT SEARCH CONFIGURATION matric_english
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, english_stem;
```

**Generated Column (note_revised_current):**
```sql
tsv tsvector GENERATED ALWAYS AS (to_tsvector('matric_english', content)) STORED
```

**GIN Index:**
```sql
CREATE INDEX idx_note_revised_tsv ON note_revised_current USING gin (tsv);
```

**Search Query Pattern:**
```sql
WHERE nrc.tsv @@ plainto_tsquery('matric_english', $1)
   OR to_tsvector('matric_english', COALESCE(n.title, '')) @@ plainto_tsquery('matric_english', $1)
```

### 2.3 Current Limitations

| Limitation | Impact | Example |
|------------|--------|---------|
| English-only stemming | CJK characters not tokenized | "Hello" matches but "" does not |
| No word boundaries for CJK | Cannot search Chinese phrases | "machine learning" matches "learning" but "" does not match "" |
| Emoji treated as noise | Cannot search by emoji | "" not searchable |
| Single tsvector column | No language-specific optimization | German compound words not split |
| No trigram fallback | Partial matches fail for non-Latin | "Mosc" cannot find "Moscow" in Cyrillic |

---

## 3. Requirements Analysis

### 3.1 Functional Requirements

| ID | Requirement | Priority | Notes |
|----|-------------|----------|-------|
| FR-1 | Search Chinese text (Simplified & Traditional) | Must | Support character/phrase search |
| FR-2 | Search Japanese text (Kanji, Hiragana, Katakana) | Must | Mixed script handling |
| FR-3 | Search Korean text (Hangul) | Must | Syllable block handling |
| FR-4 | Search emoji and symbols | Should | Unicode emoji support |
| FR-5 | Search Cyrillic script (Russian, Ukrainian) | Should | Common non-Latin script |
| FR-6 | Search Arabic script | Should | RTL script support |
| FR-7 | Search Hebrew script | Could | RTL script support |
| FR-8 | Cross-lingual search | Could | Query in English, find Chinese docs |
| FR-9 | Backward compatibility | Must | Existing English searches unchanged |
| FR-10 | Hybrid search preservation | Must | FTS + semantic + RRF continues working |

### 3.2 Non-Functional Requirements

| ID | Requirement | Target | Notes |
|----|-------------|--------|-------|
| NFR-1 | Search latency p95 | <300ms (10k docs) | Max 50% increase from current |
| NFR-2 | Index size growth | <3x current | Acceptable for multi-config |
| NFR-3 | Migration duration | <10 min (100k notes) | Online migration |
| NFR-4 | Zero downtime | 100% | Rolling deployment |
| NFR-5 | Memory overhead | <500MB | Language detection models |

---

## 4. Proposed Architecture

### 4.1 High-Level Architecture

```
                     ┌──────────────────────────────────────────────────────────────┐
                     │                      Search Request                          │
                     │           (query: string, lang_hint?: string)                │
                     └─────────────────────────┬────────────────────────────────────┘
                                               │
                     ┌─────────────────────────▼────────────────────────────────────┐
                     │                  Query Preprocessor                          │
                     │            (NEW: matric-search crate)                        │
                     │                                                              │
                     │  ┌─────────────────────────────────────────────────────┐    │
                     │  │              Language/Script Detector               │    │
                     │  │   - Unicode script analysis (fast)                  │    │
                     │  │   - Optional: lingua-rs for ambiguous text          │    │
                     │  │   - Returns: ScriptProfile { scripts, primary }     │    │
                     │  └─────────────────────────────────────────────────────┘    │
                     └─────────────────────────┬────────────────────────────────────┘
                                               │
                     ┌─────────────────────────▼────────────────────────────────────┐
                     │              Multilingual HybridSearchEngine                 │
                     │                                                              │
                     │  ┌─────────────────────────────────────────────────────┐    │
                     │  │               FTS Strategy Selector                 │    │
                     │  │  - Latin/English: matric_english config             │    │
                     │  │  - CJK: pg_bigm or zhparser/mecab                   │    │
                     │  │  - Mixed: Multi-config OR with trigram fallback     │    │
                     │  │  - Emoji/Symbols: pg_trgm exact match               │    │
                     │  └─────────────────────────────────────────────────────┘    │
                     │                                                              │
                     │  ┌─────────────────┐         ┌─────────────────┐            │
                     │  │   FTS Branch    │         │ Semantic Branch │            │
                     │  │   (multi-cfg)   │         │  (unchanged)    │            │
                     │  └────────┬────────┘         └────────┬────────┘            │
                     └───────────┼───────────────────────────┼─────────────────────┘
                                 │                           │
        ┌────────────────────────┼────────────────────────┐  │
        │                        │                        │  │
┌───────▼───────┐   ┌────────────▼────────────┐   ┌───────▼──▼───────┐
│ Language-     │   │    Universal Trigram    │   │    pgvector      │
│ Specific FTS  │   │     (pg_trgm/bigm)      │   │    (unchanged)   │
│               │   │                         │   │                  │
│ - matric_en   │   │  - CJK character match  │   │ - nomic-embed    │
│ - matric_de   │   │  - Emoji exact match    │   │ - multilingual   │
│ - matric_ru   │   │  - Partial match        │   │   by nature      │
│ - matric_zh   │   │  - Fuzzy search         │   │                  │
└───────┬───────┘   └────────────┬────────────┘   └────────┬─────────┘
        │                        │                         │
        └────────────┬───────────┘                         │
                     │                                     │
        ┌────────────▼─────────────┐                       │
        │   Multi-Source Merger    │                       │
        │   (dedupe + normalize)   │                       │
        └────────────┬─────────────┘                       │
                     │                                     │
                     └──────────────┬──────────────────────┘
                                    │
                     ┌──────────────▼──────────────────────┐
                     │          Adaptive RRF Fusion        │
                     │     (k adjusted for script type)    │
                     └──────────────┬──────────────────────┘
                                    │
                     ┌──────────────▼──────────────────────┐
                     │         Search Results              │
                     └─────────────────────────────────────┘
```

### 4.2 Component Interaction Sequence

```
┌─────────┐ ┌──────────────┐ ┌──────────────┐ ┌────────────┐ ┌──────────┐ ┌───────────┐
│  Client │ │ matric-api   │ │matric-search │ │ matric-db  │ │PostgreSQL│ │  pgvector │
└────┬────┘ └──────┬───────┘ └──────┬───────┘ └─────┬──────┘ └────┬─────┘ └─────┬─────┘
     │             │                │               │             │             │
     │ GET /search?q=...           │               │             │             │
     │────────────>│               │               │             │             │
     │             │               │               │             │             │
     │             │ SearchRequest │               │             │             │
     │             │──────────────>│               │             │             │
     │             │               │               │             │             │
     │             │               │ detect_scripts(query)       │             │
     │             │               │───────────────────>         │             │
     │             │               │ ScriptProfile     │         │             │
     │             │               │<───────────────────         │             │
     │             │               │               │             │             │
     │             │               │ select_fts_strategy()       │             │
     │             │               │───────────────────>         │             │
     │             │               │               │             │             │
     │             │               │               │             │             │
     │             │     ┌─────────┴─────────┐     │             │             │
     │             │     │ Parallel Retrieval│     │             │             │
     │             │     └─────────┬─────────┘     │             │             │
     │             │               │               │             │             │
     │             │               │ fts_search_multilingual()   │             │
     │             │               │──────────────>│             │             │
     │             │               │               │ SQL queries │             │
     │             │               │               │────────────>│             │
     │             │               │               │   results   │             │
     │             │               │               │<────────────│             │
     │             │               │   FTS hits    │             │             │
     │             │               │<──────────────│             │             │
     │             │               │               │             │             │
     │             │               │ find_similar()│             │             │
     │             │               │──────────────>│             │             │
     │             │               │               │ vector query│             │
     │             │               │               │────────────>│────────────>│
     │             │               │               │<────────────│<────────────│
     │             │               │  semantic hits│             │             │
     │             │               │<──────────────│             │             │
     │             │               │               │             │             │
     │             │               │ rrf_fuse(fts, semantic)     │             │
     │             │               │──────────────────────────>  │             │
     │             │               │               │             │             │
     │             │ EnhancedSearchHits           │             │             │
     │             │<──────────────│               │             │             │
     │             │               │               │             │             │
     │ JSON response              │               │             │             │
     │<────────────│               │               │             │             │
     │             │               │               │             │             │
```

---

## 5. Component Design

### 5.1 Script Detection Module

**Location:** `crates/matric-search/src/script_detection.rs`

```rust
/// Unicode script profile for a text query
pub struct ScriptProfile {
    /// Primary script (most characters)
    pub primary: Script,
    /// All scripts detected with character counts
    pub scripts: HashMap<Script, usize>,
    /// Total character count
    pub total_chars: usize,
    /// Whether text contains emoji
    pub has_emoji: bool,
    /// Confidence score (0.0-1.0)
    pub confidence: f32,
}

pub enum Script {
    Latin,
    Han,         // Chinese/Japanese Kanji
    Hiragana,
    Katakana,
    Hangul,      // Korean
    Cyrillic,
    Arabic,
    Hebrew,
    Devanagari,
    Thai,
    Unknown,
}

/// Detect scripts in query text using Unicode properties
/// Fast: O(n) single pass, no external models
pub fn detect_scripts(text: &str) -> ScriptProfile {
    // Implementation uses unicode_script crate
    // Handles mixed scripts (e.g., "Hello" -> Latin 62.5%, Han 37.5%)
}
```

### 5.2 FTS Strategy Selector

**Location:** `crates/matric-search/src/fts_strategy.rs`

```rust
pub enum FtsStrategy {
    /// English/Latin optimized (current behavior)
    English,
    /// German with compound word splitting
    German,
    /// Russian/Ukrainian Cyrillic
    Russian,
    /// Chinese using pg_bigm or zhparser
    Chinese,
    /// Japanese with MeCab or pg_bigm
    Japanese,
    /// Korean with pg_bigm
    Korean,
    /// Trigram-based universal (fallback)
    Trigram,
    /// Multi-config search (mixed scripts)
    Multi(Vec<FtsStrategy>),
}

pub struct FtsStrategyConfig {
    /// Strategies to use for this query
    pub strategies: Vec<FtsStrategy>,
    /// Weight distribution (sum = 1.0)
    pub weights: Vec<f32>,
    /// Whether to include trigram fallback
    pub include_trigram: bool,
}

/// Select FTS strategy based on script profile
pub fn select_strategy(profile: &ScriptProfile, config: &SearchConfig) -> FtsStrategyConfig {
    match profile.primary {
        Script::Latin if profile.confidence > 0.9 => FtsStrategyConfig::single(FtsStrategy::English),
        Script::Han => FtsStrategyConfig::single(FtsStrategy::Chinese),
        Script::Hangul => FtsStrategyConfig::single(FtsStrategy::Korean),
        Script::Cyrillic => FtsStrategyConfig::single(FtsStrategy::Russian),
        _ if profile.has_emoji => FtsStrategyConfig::with_trigram(FtsStrategy::Trigram),
        _ => FtsStrategyConfig::multi_with_fallback(profile),
    }
}
```

### 5.3 Multilingual FTS Repository

**Location:** `crates/matric-db/src/search_multilingual.rs`

```rust
pub struct PgMultilingualFtsSearch {
    pool: Pool<Postgres>,
}

impl PgMultilingualFtsSearch {
    /// Execute multilingual FTS search
    pub async fn search_multilingual(
        &self,
        query: &str,
        strategy: &FtsStrategyConfig,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        match strategy.strategies.as_slice() {
            [FtsStrategy::English] => self.search_english(query, limit, exclude_archived).await,
            [FtsStrategy::Chinese] => self.search_cjk_bigm(query, limit, exclude_archived).await,
            [FtsStrategy::Trigram] => self.search_trigram(query, limit, exclude_archived).await,
            strategies => self.search_multi_config(query, strategies, limit, exclude_archived).await,
        }
    }

    /// CJK search using pg_bigm (bigram-based)
    async fn search_cjk_bigm(&self, query: &str, limit: i64, exclude_archived: bool) -> Result<Vec<SearchHit>> {
        // Uses bigm_similarity() and =% operator
        let sql = r#"
            SELECT n.id as note_id,
                   bigm_similarity(nrc.content, $1) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   ...
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE nrc.content =% $1  -- pg_bigm similarity operator
              AND n.deleted_at IS NULL
            ORDER BY score DESC
            LIMIT $2
        "#;
        // ...
    }

    /// Trigram search for universal fallback (emoji, symbols, partial matches)
    async fn search_trigram(&self, query: &str, limit: i64, exclude_archived: bool) -> Result<Vec<SearchHit>> {
        // Uses pg_trgm similarity() and % operator
        let sql = r#"
            SELECT n.id as note_id,
                   similarity(nrc.content, $1) AS score,
                   substring(nrc.content for 200) AS snippet,
                   n.title,
                   ...
            FROM note_revised_current nrc
            JOIN note n ON n.id = nrc.note_id
            WHERE nrc.content % $1  -- pg_trgm similarity operator
              AND n.deleted_at IS NULL
            ORDER BY score DESC
            LIMIT $2
        "#;
        // ...
    }
}
```

---

## 6. Data Model Changes

### 6.1 Schema Changes

```sql
-- Migration: 20260202000000_multilingual_fts.sql

-- =============================================================================
-- Step 1: Enable required extensions
-- =============================================================================
CREATE EXTENSION IF NOT EXISTS pg_trgm;  -- Trigram for universal fallback

-- pg_bigm for CJK (if available, otherwise graceful degradation)
DO $$
BEGIN
    CREATE EXTENSION IF NOT EXISTS pg_bigm;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'pg_bigm not available, CJK search will use pg_trgm fallback';
END $$;

-- =============================================================================
-- Step 2: Create language-specific text search configurations
-- =============================================================================

-- German (compound word splitting)
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_german CASCADE;
CREATE TEXT SEARCH CONFIGURATION matric_german (COPY = german);
ALTER TEXT SEARCH CONFIGURATION matric_german
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, german_stem;

-- Russian
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_russian CASCADE;
CREATE TEXT SEARCH CONFIGURATION matric_russian (COPY = russian);
ALTER TEXT SEARCH CONFIGURATION matric_russian
  ALTER MAPPING FOR hword, hword_part, word
  WITH unaccent, russian_stem;

-- Simple config for non-stemmed languages (CJK fallback)
DROP TEXT SEARCH CONFIGURATION IF EXISTS matric_simple CASCADE;
CREATE TEXT SEARCH CONFIGURATION matric_simple (COPY = simple);

-- =============================================================================
-- Step 3: Add language hint column to note table
-- =============================================================================
ALTER TABLE note
  ADD COLUMN IF NOT EXISTS detected_language TEXT,
  ADD COLUMN IF NOT EXISTS language_confidence REAL;

COMMENT ON COLUMN note.detected_language IS 'ISO 639-1 language code detected from content';
COMMENT ON COLUMN note.language_confidence IS 'Confidence score (0.0-1.0) for detected language';

-- =============================================================================
-- Step 4: Create trigram indexes for universal search
-- =============================================================================

-- GIN trigram index on content for fuzzy/partial matching
CREATE INDEX IF NOT EXISTS idx_note_revised_trgm
  ON note_revised_current
  USING gin (content gin_trgm_ops);

-- GIN trigram index on title
CREATE INDEX IF NOT EXISTS idx_note_title_trgm
  ON note
  USING gin (title gin_trgm_ops);

-- =============================================================================
-- Step 5: Create bigram indexes for CJK (if pg_bigm available)
-- =============================================================================
DO $$
BEGIN
    -- Only create if pg_bigm extension exists
    IF EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_bigm') THEN
        CREATE INDEX IF NOT EXISTS idx_note_revised_bigm
          ON note_revised_current
          USING gin (content gin_bigm_ops);

        CREATE INDEX IF NOT EXISTS idx_note_title_bigm
          ON note
          USING gin (title gin_bigm_ops);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'Could not create pg_bigm indexes: %', SQLERRM;
END $$;

-- =============================================================================
-- Step 6: Function to detect dominant script
-- =============================================================================
CREATE OR REPLACE FUNCTION detect_dominant_script(text_content TEXT)
RETURNS TEXT AS $$
DECLARE
    han_count INTEGER := 0;
    latin_count INTEGER := 0;
    cyrillic_count INTEGER := 0;
    hangul_count INTEGER := 0;
    total_count INTEGER := 0;
    ch TEXT;
    code_point INTEGER;
BEGIN
    IF text_content IS NULL OR length(text_content) = 0 THEN
        RETURN 'unknown';
    END IF;

    -- Count characters by Unicode range
    FOR i IN 1..length(text_content) LOOP
        ch := substring(text_content FROM i FOR 1);
        code_point := ascii(ch);

        -- Skip whitespace and punctuation
        IF code_point > 64 THEN
            total_count := total_count + 1;

            CASE
                -- Latin A-Z, a-z, Extended Latin
                WHEN code_point BETWEEN 65 AND 90 OR code_point BETWEEN 97 AND 122
                     OR code_point BETWEEN 192 AND 687 THEN
                    latin_count := latin_count + 1;
                -- CJK Unified Ideographs (Chinese/Japanese Kanji)
                WHEN code_point BETWEEN 19968 AND 40959 THEN
                    han_count := han_count + 1;
                -- Cyrillic
                WHEN code_point BETWEEN 1024 AND 1279 THEN
                    cyrillic_count := cyrillic_count + 1;
                -- Korean Hangul
                WHEN code_point BETWEEN 44032 AND 55215 THEN
                    hangul_count := hangul_count + 1;
                ELSE
                    NULL;
            END CASE;
        END IF;
    END LOOP;

    IF total_count = 0 THEN
        RETURN 'unknown';
    END IF;

    -- Return dominant script (>50% of characters)
    IF han_count::FLOAT / total_count > 0.5 THEN
        RETURN 'han';
    ELSIF hangul_count::FLOAT / total_count > 0.5 THEN
        RETURN 'hangul';
    ELSIF cyrillic_count::FLOAT / total_count > 0.5 THEN
        RETURN 'cyrillic';
    ELSIF latin_count::FLOAT / total_count > 0.5 THEN
        RETURN 'latin';
    ELSE
        RETURN 'mixed';
    END IF;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =============================================================================
-- Step 7: Update detected_language for existing notes (batch job)
-- =============================================================================
-- This will be done by a background job to avoid blocking migration
-- See: matric-jobs/src/handlers/language_detection.rs
```

### 6.2 Index Strategy Summary

| Script | Primary Index | Fallback Index | Extension |
|--------|--------------|----------------|-----------|
| Latin/English | GIN tsvector (matric_english) | GIN pg_trgm | pg_trgm |
| German | GIN tsvector (matric_german) | GIN pg_trgm | pg_trgm |
| Russian | GIN tsvector (matric_russian) | GIN pg_trgm | pg_trgm |
| Chinese | GIN pg_bigm | GIN pg_trgm | pg_bigm |
| Japanese | GIN pg_bigm | GIN pg_trgm | pg_bigm |
| Korean | GIN pg_bigm | GIN pg_trgm | pg_bigm |
| Emoji/Symbols | GIN pg_trgm | - | pg_trgm |
| Mixed/Unknown | Multi-config search | GIN pg_trgm | pg_trgm |

### 6.3 Estimated Index Size Impact

| Configuration | Index Type | Estimated Size (10k notes) | Estimated Size (100k notes) |
|--------------|------------|---------------------------|----------------------------|
| Current (tsvector only) | GIN | ~50MB | ~500MB |
| + pg_trgm | GIN | ~100MB | ~1GB |
| + pg_bigm | GIN | ~80MB | ~800MB |
| **Total** | Combined | ~230MB | ~2.3GB |

**Growth factor:** ~4.6x (within 3x target if pg_bigm omitted: ~3x)

---

## 7. Migration Strategy

### 7.1 Migration Phases

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          Migration Timeline                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Phase 1: Infrastructure (Zero Downtime)                                         │
│  ────────────────────────────────────────                                        │
│  Duration: ~2 minutes                                                            │
│                                                                                  │
│  ┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐           │
│  │ Enable     │───>│ Create     │───>│ Create     │───>│ Add        │           │
│  │ Extensions │    │ Text Cfgs  │    │ Functions  │    │ Columns    │           │
│  └────────────┘    └────────────┘    └────────────┘    └────────────┘           │
│                                                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Phase 2: Index Creation (Background, Non-Blocking)                              │
│  ──────────────────────────────────────────────────                              │
│  Duration: ~5-30 minutes (depends on data size)                                  │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────┐           │
│  │        CREATE INDEX CONCURRENTLY (parallel)                       │           │
│  │                                                                   │           │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐   │           │
│  │  │ idx_note_       │  │ idx_note_       │  │ idx_note_       │   │           │
│  │  │ revised_trgm    │  │ title_trgm      │  │ revised_bigm    │   │           │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────┘   │           │
│  │                                                                   │           │
│  └──────────────────────────────────────────────────────────────────┘           │
│                                                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Phase 3: Language Detection Backfill (Background Job)                           │
│  ─────────────────────────────────────────────────────                           │
│  Duration: ~10-60 minutes (batch processing)                                     │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────┐           │
│  │        Language Detection Job (1000 notes/batch)                  │           │
│  │                                                                   │           │
│  │  For each note:                                                   │           │
│  │  1. Detect script from content                                    │           │
│  │  2. Update detected_language column                               │           │
│  │  3. Update language_confidence column                             │           │
│  │                                                                   │           │
│  └──────────────────────────────────────────────────────────────────┘           │
│                                                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Phase 4: Feature Flag Rollout (Gradual)                                         │
│  ─────────────────────────────────────────                                       │
│  Duration: 1-7 days                                                              │
│                                                                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │
│  │ 10% traffic │───>│ 25% traffic │───>│ 50% traffic │───>│ 100%        │       │
│  │ canary      │    │ expansion   │    │ majority    │    │ full rollout│       │
│  └─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘       │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 7.2 Rollback Plan

```sql
-- Rollback migration if issues detected

-- 1. Disable multilingual search in application (feature flag)
-- UPDATE user_config SET value = '{"multilingual_fts": false}'::jsonb WHERE key = 'search_config';

-- 2. Drop new indexes (if needed)
DROP INDEX IF EXISTS idx_note_revised_trgm;
DROP INDEX IF EXISTS idx_note_title_trgm;
DROP INDEX IF EXISTS idx_note_revised_bigm;
DROP INDEX IF EXISTS idx_note_title_bigm;

-- 3. Drop language columns (optional, safe to leave)
-- ALTER TABLE note DROP COLUMN IF EXISTS detected_language;
-- ALTER TABLE note DROP COLUMN IF EXISTS language_confidence;

-- 4. Keep text search configurations (no harm in keeping)
-- Original matric_english remains unchanged

-- Application automatically falls back to current behavior when feature flag disabled
```

---

## 8. API Design

### 8.1 Search Endpoint Changes

**Current Endpoint:** `GET /api/v1/search`

**Proposed Changes (Backward Compatible):**

```yaml
# OpenAPI 3.1 specification addition
paths:
  /api/v1/search:
    get:
      summary: Search notes (multilingual)
      operationId: searchNotes
      tags: [Search]
      parameters:
        - name: q
          in: query
          required: true
          description: Search query (supports any Unicode text)
          schema:
            type: string
          examples:
            english: { value: "machine learning" }
            chinese: { value: "" }
            emoji: { value: "" }
            mixed: { value: "Python " }
        - name: limit
          in: query
          schema:
            type: integer
            default: 20
        - name: mode
          in: query
          description: Search mode
          schema:
            type: string
            enum: [hybrid, fts, semantic]
            default: hybrid
        - name: filters
          in: query
          description: Filter expression
          schema:
            type: string
        # NEW PARAMETERS
        - name: lang
          in: query
          description: |
            Language hint (ISO 639-1 code). Optional - auto-detected if not provided.
            Improves search accuracy when language is known.
          schema:
            type: string
            enum: [en, de, ru, zh, ja, ko, ar, he]
          example: "zh"
        - name: script
          in: query
          description: |
            Script hint. Optional - auto-detected if not provided.
            Use for mixed-script queries or to force specific handling.
          schema:
            type: string
            enum: [latin, han, cyrillic, hangul, arabic, hebrew]
          example: "han"
```

### 8.2 Response Format (Unchanged)

The response format remains unchanged to maintain backward compatibility:

```json
{
  "results": [
    {
      "note_id": "01902f4c-...",
      "score": 0.87,
      "snippet": "...",
      "title": "...",
      "tags": ["tag1", "tag2"]
    }
  ],
  "total": 42,
  "metadata": {
    "fts_hits": 15,
    "semantic_hits": 30,
    "fusion_method": "rrf",
    "detected_language": "zh",    // NEW: returned for transparency
    "search_strategy": "bigram"   // NEW: returned for debugging
  }
}
```

---

## 9. Performance Analysis

### 9.1 Query Performance Comparison

| Query Type | Current (English-only) | Proposed (Multilingual) | Delta |
|------------|----------------------|------------------------|-------|
| English phrase | ~50ms | ~55ms | +10% |
| Chinese phrase | N/A (no results) | ~80ms | NEW |
| Mixed script | ~50ms (partial) | ~120ms | +140% |
| Emoji search | N/A (no results) | ~60ms | NEW |
| Trigram fallback | N/A | ~100ms | NEW |

### 9.2 Index Build Time

| Index | 10k Notes | 100k Notes | 1M Notes |
|-------|-----------|------------|----------|
| GIN tsvector (existing) | ~10s | ~2min | ~20min |
| GIN pg_trgm | ~30s | ~5min | ~50min |
| GIN pg_bigm | ~20s | ~4min | ~40min |
| **Total concurrent** | ~30s | ~5min | ~50min |

### 9.3 Memory Requirements

| Component | Memory Usage |
|-----------|-------------|
| pg_trgm index (10k notes) | ~100MB |
| pg_bigm index (10k notes) | ~80MB |
| Script detection (runtime) | ~5MB |
| Language detection model (optional) | ~50MB |
| **Total additional** | ~235MB |

---

## 10. Risk Assessment

### 10.1 Risk Matrix

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| pg_bigm unavailable on hosting | Medium | High | Graceful degradation to pg_trgm |
| Index size exceeds storage | Low | Medium | Monitor growth, compression options |
| Query latency regression | Medium | Medium | Feature flag, A/B testing |
| False positives in CJK search | Medium | Low | Tune similarity thresholds |
| Migration blocks writes | Low | High | CONCURRENTLY index creation |
| Language detection errors | Medium | Low | Allow manual override, semantic fallback |

### 10.2 Mitigation Details

**Risk: pg_bigm unavailable**
- Detection: Check `pg_extension` table during migration
- Fallback: Use pg_trgm with bigram configuration (less optimal but functional)
- Code path: `if has_bigm { use_bigm() } else { use_trgm_bigram() }`

**Risk: Query latency regression**
- Feature flag: `MULTILINGUAL_FTS_ENABLED` environment variable
- Metrics: Track p50/p95/p99 latency by script type
- Rollback: Disable feature flag, automatic fallback to English-only

---

## 11. Implementation Roadmap

### 11.1 Phase Timeline

```
┌────────────────────────────────────────────────────────────────────────────────┐
│ Week 1-2: Foundation                                                            │
├────────────────────────────────────────────────────────────────────────────────┤
│ [ ] Script detection module (matric-search/src/script_detection.rs)            │
│ [ ] FTS strategy selector (matric-search/src/fts_strategy.rs)                  │
│ [ ] Database migration (extensions, configs, columns)                          │
│ [ ] Unit tests for script detection                                            │
├────────────────────────────────────────────────────────────────────────────────┤
│ Week 3-4: Core Implementation                                                   │
├────────────────────────────────────────────────────────────────────────────────┤
│ [ ] Multilingual FTS repository (matric-db/src/search_multilingual.rs)         │
│ [ ] pg_trgm integration                                                        │
│ [ ] pg_bigm integration (with graceful degradation)                            │
│ [ ] Update HybridSearchEngine to use new strategies                            │
│ [ ] Integration tests with multilingual fixtures                               │
├────────────────────────────────────────────────────────────────────────────────┤
│ Week 5: API & Testing                                                           │
├────────────────────────────────────────────────────────────────────────────────┤
│ [ ] API parameter additions (lang, script hints)                               │
│ [ ] Response metadata additions                                                │
│ [ ] Feature flag implementation                                                │
│ [ ] End-to-end tests                                                           │
│ [ ] Performance benchmarks                                                     │
├────────────────────────────────────────────────────────────────────────────────┤
│ Week 6: Rollout                                                                 │
├────────────────────────────────────────────────────────────────────────────────┤
│ [ ] Staging deployment                                                         │
│ [ ] Language detection backfill job                                            │
│ [ ] Gradual production rollout (10% -> 25% -> 50% -> 100%)                    │
│ [ ] Documentation updates                                                      │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 11.2 Test Cases

| Test ID | Description | Input | Expected Output |
|---------|-------------|-------|-----------------|
| ML-001 | Chinese phrase search | "" | Notes containing  |
| ML-002 | Japanese mixed script | "" | Notes containing  |
| ML-003 | Korean search | "" | Notes containing  |
| ML-004 | Emoji search | "" | Notes containing  |
| ML-005 | Mixed English+Chinese | "Python " | Notes with Python and/or  |
| ML-006 | Russian search | "" | Notes containing  |
| ML-007 | Partial match (trigram) | "prog" | Notes containing "programming" |
| ML-008 | Accent folding preserved | "cafe" | Notes containing "cafe" or "cafe" |

---

## 12. Architectural Decision Records

### ADR-ML-001: Script Detection Approach

**Status:** Proposed

**Context:**
Need to determine language/script of search queries to select appropriate FTS strategy.

**Decision:**
Use Unicode script property analysis as primary method, with optional lingua-rs for ambiguous cases.

**Rationale:**
- Unicode analysis is fast (O(n), no external models)
- Covers 95% of cases accurately
- lingua-rs adds ~50MB memory but handles edge cases
- No external API dependencies

**Consequences:**
- Fast detection for clear-script queries
- May misclassify very short queries (1-2 chars)
- Semantic search provides fallback for misclassified queries

---

### ADR-ML-002: CJK Search Strategy

**Status:** Proposed

**Context:**
PostgreSQL's built-in text search does not support CJK word segmentation.

**Decision:**
Use pg_bigm (bigram) as primary CJK strategy with pg_trgm (trigram) as fallback.

**Alternatives Considered:**
1. **zhparser + MeCab**: Best segmentation but requires external dictionaries
2. **pg_trgm only**: Works but less accurate for CJK
3. **pg_bigm**: Good balance of accuracy and simplicity
4. **External search engine (Elasticsearch)**: Overkill for current scale

**Rationale:**
- pg_bigm designed specifically for CJK
- No external dependencies (pure PostgreSQL extension)
- Falls back gracefully if unavailable
- Reasonable accuracy for common use cases

**Consequences:**
- May not match specialized CJK search engines
- Compound words not segmented (acceptable for notes)
- Semantic search compensates for FTS limitations

---

### ADR-ML-003: Multi-Index Strategy

**Status:** Proposed

**Context:**
Supporting multiple scripts requires multiple indexes, increasing storage.

**Decision:**
Create trigram index by default, bigram index conditionally (if pg_bigm available).

**Rationale:**
- pg_trgm is universally available
- pg_bigm provides CJK optimization but not critical
- Index storage within acceptable bounds (3x growth)
- Query planner selects optimal index automatically

**Consequences:**
- ~3x index size growth
- Slight insert overhead for index maintenance
- Faster mixed-script queries

---

### ADR-ML-004: Backward Compatibility

**Status:** Proposed

**Context:**
Existing users expect current English search behavior to remain unchanged.

**Decision:**
All changes are additive; default behavior unchanged without new parameters.

**Rationale:**
- Feature flag controls new behavior
- API parameters are optional
- Existing queries route to matric_english by default
- No migration of existing data required

**Consequences:**
- Zero-risk upgrade for existing users
- New features opt-in
- Slightly more complex codebase

---

## Appendix A: PostgreSQL Extension Availability

| Extension | PostgreSQL 14+ | PostgreSQL 16 | Cloud (RDS/Cloud SQL) | Docker |
|-----------|---------------|---------------|----------------------|--------|
| pg_trgm | Built-in | Built-in | Available | Available |
| pg_bigm | Manual install | Manual install | Limited | Available |
| unaccent | Built-in | Built-in | Available | Available |
| zhparser | Manual install | Manual install | Not available | Available |

## Appendix B: Unicode Script Ranges

```
Latin:       U+0041-U+007A, U+00C0-U+02AF (Extended)
Han (CJK):   U+4E00-U+9FFF (Unified), U+3400-U+4DBF (Extension A)
Hiragana:    U+3040-U+309F
Katakana:    U+30A0-U+30FF
Hangul:      U+AC00-U+D7AF (Syllables), U+1100-U+11FF (Jamo)
Cyrillic:    U+0400-U+04FF
Arabic:      U+0600-U+06FF
Hebrew:      U+0590-U+05FF
Emoji:       U+1F300-U+1F9FF (Miscellaneous Symbols and Pictographs)
```

## Appendix C: Test Data Fixtures

```sql
-- Insert multilingual test notes
INSERT INTO note (id, title, format, source, created_at_utc, updated_at_utc)
VALUES
  (gen_uuid_v7(), '', 'markdown', 'test', NOW(), NOW()),
  (gen_uuid_v7(), '', 'markdown', 'test', NOW(), NOW()),
  (gen_uuid_v7(), '', 'markdown', 'test', NOW(), NOW()),
  (gen_uuid_v7(), '', 'markdown', 'test', NOW(), NOW()),
  (gen_uuid_v7(), 'Emoji Test ', 'markdown', 'test', NOW(), NOW()),
  (gen_uuid_v7(), 'Mixed Python ', 'markdown', 'test', NOW(), NOW());

-- Insert corresponding content
INSERT INTO note_original (note_id, content, hash)
SELECT id, title, md5(title)
FROM note WHERE source = 'test';
```

---

**Document Status:** Draft for Review
**Next Steps:**
1. Review with team
2. Prototype script detection module
3. Test pg_bigm availability in target environments
4. Finalize migration strategy based on feedback
