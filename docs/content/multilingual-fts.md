# Multilingual Full-Text Search Guide

This guide explains how to use Fortémi's multilingual full-text search capabilities to find content across different languages, scripts, and writing systems.

## Overview

Fortémi's full-text search (FTS) supports multiple languages and writing systems through a combination of PostgreSQL text search configurations, trigram indexing, and automatic script detection. The system automatically detects your query language and applies the optimal search strategy.

## Supported Languages

### Latin Scripts (Fully Supported)

These languages use morphological stemming for optimal search quality:

| Language | Config | Features |
|----------|--------|----------|
| **English** | `english` | Porter stemming, stopword removal |
| **German** | `german` | Compound word splitting, umlauts |
| **French** | `french` | Accent handling, elision |
| **Spanish** | `spanish` | Tilde support, stemming |
| **Portuguese** | `portuguese` | Brazilian/European variants |
| **Russian** | `russian` | Cyrillic stemming, case-folding |

**Search Features:**
- Morphological stemming (e.g., "running" matches "run", "runs")
- Boolean operators (OR, NOT)
- Phrase search with quotes
- Case-insensitive matching

### CJK Scripts (Chinese, Japanese, Korean)

CJK languages use character-level indexing for precise matching:

| Script | Languages | Strategy |
|--------|-----------|----------|
| **Han (Chinese)** | Simplified, Traditional | Bigram/trigram character matching |
| **Hiragana/Katakana** | Japanese | Character n-grams |
| **Hangul** | Korean | Syllable matching |

**Search Features:**
- Single character search supported
- Multi-character phrase matching
- Mixed CJK+Latin queries (e.g., "Python 教程")
- No morphological stemming (not applicable)

### Other Scripts

These scripts use character-level matching with basic tokenization:

- **Arabic** (including Persian, Urdu)
- **Cyrillic** (Russian, Ukrainian, Bulgarian)
- **Greek**
- **Hebrew**
- **Devanagari** (Hindi, Sanskrit)
- **Thai**
- **Emoji and Unicode symbols**

## Configuration

Fortémi's multilingual search is controlled via environment variables (feature flags):

### Feature Flags

```bash
# Enable automatic script detection (recommended)
export FTS_SCRIPT_DETECTION=true

# Enable emoji and symbol search via trigrams (recommended)
export FTS_TRIGRAM_FALLBACK=true

# Enable optimized CJK bigram search (requires pg_bigm extension)
export FTS_BIGRAM_CJK=true

# Enable language-specific stemming configs (recommended)
export FTS_MULTILINGUAL_CONFIGS=true
```

### Default Configuration

If no feature flags are set, the system defaults to English-only search with basic functionality.

**Recommended Setup:**
```bash
# In your .env file or environment
FTS_SCRIPT_DETECTION=true
FTS_TRIGRAM_FALLBACK=true
FTS_MULTILINGUAL_CONFIGS=true
FTS_BIGRAM_CJK=true  # If pg_bigm is available
```

## Query Examples

### English (Latin Script)

Standard English search with morphological stemming:

```
# Simple keyword
machine learning

# Multiple words (implicit AND)
python async await

# OR operator
cat OR dog

# NOT operator (exclusion)
python -java

# Phrase search
"artificial intelligence"

# Complex boolean
(python OR javascript) AND "async programming"
```

### Chinese (CJK)

Character-level matching for Chinese text:

```
# Single character
人

# Multi-character phrase
机器学习

# Mixed Chinese + English
Python 教程

# Technical terms
深度学习 neural network
```

### Japanese (CJK)

Hiragana, katakana, and kanji are all searchable:

```
# Hiragana
こんにちは

# Katakana
コンピュータ

# Kanji
機械学習

# Mixed script
プログラミング言語
```

### Korean (CJK)

Hangul syllable matching:

```
# Hangul
프로그래밍

# Technical terms
기계 학습

# Mixed Korean + English
Python 프로그래밍
```

### Russian (Cyrillic)

Cyrillic script with morphological stemming:

```
# Cyrillic text
машинное обучение

# With English
Python программирование

# Boolean operators work
программирование OR разработка
```

### Arabic (RTL Scripts)

Right-to-left script support:

```
# Arabic script
تعلم الآلة

# Technical terms
برمجة الحاسوب

# Mixed with English
Python برمجة
```

### Emoji Search

Emoji characters are searchable via trigram indexes:

```
# Single emoji
🔥

# Multiple emoji
🚀 ⭐

# Emoji + text
python 🐍 tutorial
```

## Search Strategies

Fortémi automatically selects the optimal search strategy based on your query:

### Strategy Selection

| Query Type | Strategy | Index Used | Notes |
|------------|----------|------------|-------|
| Latin alphabet only | FTS (morphological) | GIN tsvector | Stemming, stopwords |
| CJK characters | Bigram/Trigram | GIN bigm/trgm | Character-level |
| Emoji/symbols | Trigram | GIN trgm | Substring matching |
| Mixed scripts | Multi-strategy | Multiple indexes | Combines results |

### Automatic Detection

The system detects script type by analyzing Unicode character ranges:

```
Latin:       U+0041-U+007A, U+00C0-U+02AF
Han (CJK):   U+4E00-U+9FFF, U+3400-U+4DBF
Hiragana:    U+3040-U+309F
Katakana:    U+30A0-U+30FF
Hangul:      U+AC00-U+D7AF
Cyrillic:    U+0400-U+04FF
Arabic:      U+0600-U+06FF
Hebrew:      U+0590-U+05FF
Emoji:       U+1F300-U+1F9FF
```

## Boolean Operators

All search strategies support boolean operators via `websearch_to_tsquery` syntax:

### OR Operator

Find documents containing either term:

```
# English
cat OR dog

# CJK
机器学习 OR 深度学习

# Mixed
Python OR Java OR Rust
```

### NOT Operator (Exclusion)

Exclude documents containing a term:

```
# Exclude term
python -java

# CJK exclusion
编程 -Java

# Multiple exclusions
tutorial -beginner -advanced
```

### Phrase Search

Match exact phrases with quotes:

```
# English phrase
"machine learning"

# CJK phrase
"人工智能"

# Mixed phrase
"Python programming language"
```

### Combined Operators

Complex queries with multiple operators:

```
# Boolean combination
(python OR javascript) AND "async programming" -callback

# CJK + boolean
(机器学习 OR 深度学习) AND Python
```

## Performance Considerations

### Query Latency

Typical search latency by strategy:

| Strategy | P50 Latency | P95 Latency | Notes |
|----------|-------------|-------------|-------|
| English FTS | 20-40ms | 50-80ms | Fastest |
| Trigram (emoji) | 30-60ms | 80-120ms | Character-level |
| Bigram (CJK) | 25-50ms | 70-110ms | Optimized CJK |
| Multi-strategy | 40-80ms | 100-150ms | Combines multiple |

All strategies remain well within the 200ms SLA target.

### Index Sizes

Multilingual indexing increases storage requirements:

| Index Type | Size Multiplier | Purpose |
|------------|-----------------|---------|
| Base (English) | 1.0x | Baseline |
| + Simple config | +0.5x | Universal fallback |
| + Trigram | +1.5x | Emoji, fuzzy match |
| + Bigram (CJK) | +1.0x | CJK optimization |
| **Total** | **3-5x** | Full multilingual |

**Storage Impact:** For a 1GB text corpus, expect 3-5GB total with all indexes. This is an acceptable trade-off for comprehensive multilingual support.

## Troubleshooting

### Issue: CJK Single Character Search Fails

**Symptom:** Searching for a single CJK character returns no results.

**Cause:** `pg_bigm` extension not installed, falling back to `simple` config which requires multi-character tokens.

**Solution:**
1. Check if `pg_bigm` is available:
   ```sql
   SELECT * FROM pg_available_extensions WHERE name = 'pg_bigm';
   ```
2. Install `pg_bigm` (requires compilation):
   ```bash
   # Ubuntu/Debian
   apt-get install postgresql-16-pgdg-pg-bigm
   ```
3. Enable extension:
   ```sql
   CREATE EXTENSION IF NOT EXISTS pg_bigm;
   ```
4. Set feature flag:
   ```bash
   export FTS_BIGRAM_CJK=true
   ```

**Workaround:** If `pg_bigm` is unavailable, search with 2+ characters or use semantic search as fallback.

---

### Issue: Emoji Search Returns No Results

**Symptom:** Searching for emoji characters like "🔥" returns empty results.

**Cause:** `FTS_TRIGRAM_FALLBACK` not enabled.

**Solution:**
1. Enable trigram fallback:
   ```bash
   export FTS_TRIGRAM_FALLBACK=true
   ```
2. Restart the application.
3. Verify `pg_trgm` extension is installed:
   ```sql
   SELECT * FROM pg_available_extensions WHERE name = 'pg_trgm';
   ```

**Note:** `pg_trgm` is included in standard PostgreSQL, no compilation required.

---

### Issue: Boolean Operators Not Working

**Symptom:** Query like "cat OR dog" returns no results or unexpected results.

**Cause:** Using old `plainto_tsquery` syntax instead of `websearch_to_tsquery`.

**Solution:** Ensure your Fortémi version is ≥ v2026.2.0, which uses `websearch_to_tsquery` by default.

**Verify:**
```bash
# Check version
curl https://your-instance.com/api/v1/health | jq '.version'
```

---

### Issue: Wrong Language Detected

**Symptom:** Query is interpreted as wrong language (e.g., Russian text treated as Latin).

**Cause:** Script detection failed due to mixed scripts or ambiguous characters.

**Solution:**
1. Use semantic search as fallback (always available).
2. Ensure `FTS_SCRIPT_DETECTION=true` is set.
3. For ambiguous queries, use explicit language hint (future feature).

**Workaround:** Add language-specific characters to disambiguate:
- English: Use common English words
- Russian: Use Cyrillic-only characters
- CJK: Use CJK-specific punctuation

---

### Issue: Slow Queries with Mixed Scripts

**Symptom:** Queries with both CJK and Latin characters take >200ms.

**Cause:** Multi-strategy search must query multiple indexes.

**Solution:**
1. This is expected behavior for mixed-script queries.
2. Typical latency is 100-150ms (within SLA).
3. If latency is critical, split into separate queries:
   ```
   # Instead of: Python 教程
   # Search separately:
   Query 1: Python
   Query 2: 教程
   # Combine results client-side
   ```

---

## Advanced Usage

### Manual Language Override

Future feature (v2026.3.0+): Explicitly specify search language via API parameter:

```bash
# English FTS (planned)
GET /api/v1/search?q=programming&lang=en

# Chinese bigram (planned)
GET /api/v1/search?q=编程&lang=zh

# Russian FTS (planned)
GET /api/v1/search?q=программирование&lang=ru
```

### Script-Specific Parameters

Future feature (v2026.3.0+): Override automatic script detection:

```bash
# Force CJK strategy (planned)
GET /api/v1/search?q=programming&script=han

# Force trigram strategy (planned)
GET /api/v1/search?q=emoji&script=trigram
```

### Search Metadata

Response metadata indicates which strategy was used:

```json
{
  "results": [...],
  "metadata": {
    "detected_language": "zh",
    "search_strategy": "bigram",
    "fts_hits": 15,
    "semantic_hits": 30
  }
}
```

## Hybrid Search Integration

Multilingual FTS is always combined with semantic search via Reciprocal Rank Fusion (RRF):

```
Your Query
    |
    v
+-------------------+        +-------------------+
| FTS Branch        |        | Semantic Branch   |
| (multilingual)    |        | (embeddings)      |
+-------------------+        +-------------------+
    |                            |
    | Results                    | Results
    v                            v
+-------------------------------------------+
| RRF Fusion (k=20)                         |
| - Combines ranked results                 |
| - Language-agnostic scoring               |
+-------------------------------------------+
    |
    v
Final Results
```

**Benefits:**
- FTS finds exact keyword matches in any language
- Semantic search finds conceptually similar content
- RRF combines both for optimal recall and precision
- Cross-lingual retrieval (query in English, find Chinese documents)

## Limitations

### Known Limitations

1. **No cross-lingual FTS**: Full-text search does not translate queries. Use semantic search for cross-lingual retrieval.

2. **Limited Thai support**: Thai language requires word segmentation, not yet implemented. Use semantic search as fallback.

3. **No transliteration**: Queries must use target script (e.g., can't search for Russian with Latin transcription "mashinnoe obuchenie").

4. **CJK compound words**: Multi-character compound words may not match single-character queries without `pg_bigm`.

5. **Diacritic sensitivity**: Some languages (Arabic, Hebrew) may be sensitive to diacritical marks depending on configuration.

### Semantic Search Fallback

When FTS limitations are encountered, semantic search provides universal fallback:

- **Cross-lingual**: Query in any language, find documents in any language
- **Script-agnostic**: No tokenization or stemming required
- **Semantic matching**: Finds conceptually similar content beyond keywords

**Example:**
```
Query: "machine learning tutorial" (English)
Results include:
- English: "Introduction to ML"
- Chinese: "机器学习入门"
- Spanish: "Tutorial de aprendizaje automático"
```

## Related Documentation

- [Search Guide](./search-guide.md) - Hybrid search overview
- [API Reference](./api.md) - REST API endpoints
- [Glossary](./glossary.md) - Technical terminology
- [Architecture](./architecture.md) - System design

---

*Multilingual FTS enables you to find content in any language using the same familiar search syntax. The system automatically detects your query language and applies the optimal search strategy.*
