# UAT Phase 3: Search Capabilities

**Purpose**: Verify hybrid search, FTS operators, and multilingual support
**Duration**: ~10 minutes
**Prerequisites**: Phase 1 seed data exists
**Critical**: Yes (100% pass required)
**Tools Tested**: `search_notes`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. The MCP tool name and exact parameters are specified for each test.

> **Test Data**: For extended multilingual search testing, use content from `tests/uat/data/multilingual/`
> (13 languages). For emoji search, use `tests/uat/data/multilingual/emoji-heavy.txt`.
> Generate with: `cd tests/uat/data/scripts && ./generate-test-data.sh`

---

## Full-Text Search (FTS)

### SEARCH-001: FTS Basic

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "neural networks", mode: "fts", limit: 10 })
```

**Pass Criteria**: Returns ML notes containing "neural networks"

---

### SEARCH-002: FTS OR Operator

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "rust OR python", mode: "fts", limit: 10 })
```

**Pass Criteria**: Returns notes with "rust" OR "python"

---

### SEARCH-003: FTS NOT Operator

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "programming -rust", mode: "fts", limit: 10 })
```

**Pass Criteria**: Results contain "programming" but exclude "rust" content

---

### SEARCH-004: FTS Phrase Search

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "\"neural networks\"", mode: "fts", limit: 10 })
```

**Pass Criteria**: Exact phrase matches only

---

## Multilingual Search

### SEARCH-005: Accent Folding (café)

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "cafe", mode: "fts", limit: 10 })
```

**Pass Criteria**: Finds content containing "café"

---

### SEARCH-006: Accent Folding (naïve/résumé)

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "naive resume", mode: "fts", limit: 10 })
```

**Pass Criteria**: Finds content containing "naïve" and "résumé"

---

### SEARCH-007: Chinese Search

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "人工智能", mode: "fts", limit: 10 })
```

**Pass Criteria**: Finds Chinese AI note (SEED-I18N-001)

---

### SEARCH-008: Chinese Single Character

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "学", mode: "fts", limit: 10 })
```

**Pass Criteria**: Returns results (CJK bigram tokenization works)

---

### SEARCH-009: Arabic RTL Search

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "الذكاء الاصطناعي", mode: "fts", limit: 10 })
```

**Pass Criteria**: Finds Arabic AI note (SEED-I18N-002)

---

## Semantic Search

### SEARCH-010: Semantic - Conceptual

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "machine intelligence", mode: "semantic", limit: 5 })
```

**Pass Criteria**: Finds AI/ML notes even without exact term match
**Note**: Requires embeddings to be generated (may need to wait)

---

## Hybrid Search

### SEARCH-011: Hybrid Search

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "deep learning transformers", mode: "hybrid", limit: 10 })
```

**Pass Criteria**: Returns relevant results combining FTS + semantic

---

### SEARCH-012: Search with Tag Filter

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "neural", mode: "fts", tags: ["uat/ml"], limit: 10 })
```

**Pass Criteria**: All results have `uat/ml` tag

---

## Edge Cases

### SEARCH-013: Empty Results

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "xyznonexistent123", mode: "fts", limit: 10 })
```

**Pass Criteria**: Returns `{ results: [], total: 0 }` (no crash)

---

### SEARCH-014: Special Characters

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "∑ ∏ ∫", mode: "fts", limit: 10 })
```

**Pass Criteria**: No crash, returns results or empty array

---

### SEARCH-015: Emoji Search

**MCP Tool**: `search_notes`

```javascript
search_notes({ query: "🚀", mode: "fts", limit: 10 })
```

**Pass Criteria**: Finds SEED-EDGE-002 with emoji content

---

## Strict Tag Filtering

### SEARCH-016: Strict Required Tags

**MCP Tool**: `search_notes`

```javascript
search_notes({
  query: "neural",
  required_tags: ["uat/ml"],
  limit: 10
})
```

**Pass Criteria**: All results MUST have `uat/ml` tag

---

### SEARCH-017: Strict Excluded Tags

**MCP Tool**: `search_notes`

```javascript
search_notes({
  query: "AI",
  excluded_tags: ["uat/i18n"],
  limit: 10
})
```

**Pass Criteria**: No results have any `uat/i18n` tag

---

### SEARCH-018: Strict Any Tags

**MCP Tool**: `search_notes`

```javascript
search_notes({
  query: "learning",
  any_tags: ["uat/ml/deep-learning", "uat/ml/training"],
  limit: 10
})
```

**Pass Criteria**: All results have AT LEAST ONE of the specified tags

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| SEARCH-001 | FTS Basic | `search_notes` | |
| SEARCH-002 | FTS OR Operator | `search_notes` | |
| SEARCH-003 | FTS NOT Operator | `search_notes` | |
| SEARCH-004 | FTS Phrase Search | `search_notes` | |
| SEARCH-005 | Accent Folding (café) | `search_notes` | |
| SEARCH-006 | Accent Folding (naïve) | `search_notes` | |
| SEARCH-007 | Chinese Search | `search_notes` | |
| SEARCH-008 | Chinese Single Char | `search_notes` | |
| SEARCH-009 | Arabic RTL Search | `search_notes` | |
| SEARCH-010 | Semantic Conceptual | `search_notes` | |
| SEARCH-011 | Hybrid Search | `search_notes` | |
| SEARCH-012 | Search + Tag Filter | `search_notes` | |
| SEARCH-013 | Empty Results | `search_notes` | |
| SEARCH-014 | Special Characters | `search_notes` | |
| SEARCH-015 | Emoji Search | `search_notes` | |
| SEARCH-016 | Strict Required Tags | `search_notes` | |
| SEARCH-017 | Strict Excluded Tags | `search_notes` | |
| SEARCH-018 | Strict Any Tags | `search_notes` | |

**Phase Result**: [ ] PASS / [ ] FAIL (100% required)

**Notes**:
