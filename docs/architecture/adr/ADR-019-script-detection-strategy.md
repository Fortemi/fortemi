# ADR-019: Script Detection Strategy

**Status:** Accepted (Implemented 2026-02-01)
**Date:** 2026-02-01
**Decision Makers:** @roctinam
**Technical Story:** Automatically detect query language/script to route to appropriate FTS strategy

## Context

The multilingual FTS strategy (ADR-017) requires routing queries to the appropriate search backend:
- Latin script queries → tsvector with `matric_english` config
- CJK queries → pg_bigm or pg_trgm bigram search
- Cyrillic queries → tsvector with `matric_russian` config
- Mixed scripts → multi-strategy OR search
- Emoji queries → trigram exact match

### Problem Statement

How should the system determine which search strategy to use for a given query?

Options considered:
1. **User-specified language hint** (explicit)
2. **External language detection API** (ML-based)
3. **Unicode script analysis** (rule-based)
4. **Character n-gram language detection** (statistical)

### Requirements

| Requirement | Priority |
|-------------|----------|
| Fast detection (<1ms) | Must |
| No external dependencies | Should |
| Handle mixed-script queries | Must |
| Detect emoji | Must |
| Accurate for clear cases | Must |
| Reasonable for ambiguous cases | Should |

## Decision

Use **Unicode script analysis** as the primary detection method, with optional support for client-provided language hints.

### Script Detection Algorithm

```rust
pub struct ScriptProfile {
    /// Primary script (highest character count)
    pub primary: Script,
    /// All scripts detected with character counts
    pub scripts: HashMap<Script, usize>,
    /// Total character count (excluding whitespace/punctuation)
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
    Emoji,
    Unknown,
}

/// Detect scripts in query text using Unicode properties
/// Performance: O(n) single pass, no external models
pub fn detect_scripts(text: &str) -> ScriptProfile {
    let mut scripts: HashMap<Script, usize> = HashMap::new();
    let mut total_chars = 0;
    let mut has_emoji = false;

    for ch in text.chars() {
        if ch.is_whitespace() || ch.is_ascii_punctuation() {
            continue;
        }
        total_chars += 1;

        let script = match ch {
            // Latin
            'A'..='Z' | 'a'..='z' | '\u{00C0}'..='\u{02AF}' => Script::Latin,
            // CJK Unified Ideographs (Chinese/Japanese Kanji)
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => Script::Han,
            // Japanese Hiragana
            '\u{3040}'..='\u{309F}' => Script::Hiragana,
            // Japanese Katakana
            '\u{30A0}'..='\u{30FF}' => Script::Katakana,
            // Korean Hangul
            '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}' => Script::Hangul,
            // Cyrillic
            '\u{0400}'..='\u{04FF}' => Script::Cyrillic,
            // Arabic
            '\u{0600}'..='\u{06FF}' => Script::Arabic,
            // Hebrew
            '\u{0590}'..='\u{05FF}' => Script::Hebrew,
            // Emoji (Miscellaneous Symbols and Pictographs)
            '\u{1F300}'..='\u{1F9FF}' | '\u{2600}'..='\u{26FF}' => {
                has_emoji = true;
                Script::Emoji
            }
            _ => Script::Unknown,
        };

        *scripts.entry(script).or_insert(0) += 1;
    }

    // Determine primary script
    let primary = scripts.iter()
        .max_by_key(|(_, count)| *count)
        .map(|(script, _)| script.clone())
        .unwrap_or(Script::Unknown);

    // Calculate confidence (percentage of primary script)
    let confidence = if total_chars > 0 {
        scripts.get(&primary).copied().unwrap_or(0) as f32 / total_chars as f32
    } else {
        0.0
    };

    ScriptProfile { primary, scripts, total_chars, has_emoji, confidence }
}
```

### Strategy Selection

```rust
pub fn select_strategy(profile: &ScriptProfile) -> FtsStrategyConfig {
    // Single-script queries with high confidence
    if profile.confidence > 0.9 {
        return match profile.primary {
            Script::Latin => FtsStrategyConfig::single(FtsStrategy::English),
            Script::Han => FtsStrategyConfig::single(FtsStrategy::Chinese),
            Script::Hiragana | Script::Katakana => FtsStrategyConfig::single(FtsStrategy::Japanese),
            Script::Hangul => FtsStrategyConfig::single(FtsStrategy::Korean),
            Script::Cyrillic => FtsStrategyConfig::single(FtsStrategy::Russian),
            Script::Emoji => FtsStrategyConfig::single(FtsStrategy::Trigram),
            _ => FtsStrategyConfig::single(FtsStrategy::Trigram),
        };
    }

    // Mixed scripts or low confidence
    if profile.has_emoji {
        // Emoji present: include trigram strategy
        FtsStrategyConfig::multi_with_trigram(profile)
    } else {
        // Multi-script: search across all detected scripts
        FtsStrategyConfig::multi_from_profile(profile)
    }
}
```

### API Support for Language Hints

Optional client-provided hints for edge cases:

```rust
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<i32>,
    /// Optional language hint (ISO 639-1 code)
    /// Overrides automatic detection
    pub lang: Option<String>,
    /// Optional script hint
    /// Overrides automatic detection
    pub script: Option<String>,
}
```

When hints are provided:
1. Skip automatic detection
2. Use hint directly for strategy selection
3. Log hint usage for analytics

## Consequences

### Positive

- **Fast detection**: O(n) single pass, typically <1ms
- **No external dependencies**: Pure Rust Unicode handling
- **Handles mixed scripts**: Detects multiple scripts, selects primary
- **Emoji detection**: Specific handling for emoji characters
- **Transparent**: Detection results available in API response metadata
- **Override capability**: Clients can provide hints for edge cases
- **Testable**: Deterministic algorithm, easy to unit test

### Negative

- **No semantic understanding**: Cannot distinguish Portuguese from Spanish (both Latin)
- **Short query ambiguity**: 1-2 character queries may be misclassified
- **Limited emoji coverage**: Only common emoji ranges detected
- **No context awareness**: Does not consider user history or note content

### Mitigations

1. **Latin language variants**: Use same `matric_english` config (unaccent handles diacritics)
2. **Short query ambiguity**: Semantic search compensates; low impact
3. **Emoji coverage**: Add additional Unicode ranges as needed
4. **Context**: Future enhancement - consider user's note language distribution

## Alternatives Considered

### 1. External Language Detection API

Use ML-based language detection service (e.g., Google Cloud Translation, AWS Comprehend).

**Rejected because:**
- Adds latency (10-100ms network round-trip)
- External dependency (cost, availability)
- Overkill for script detection (full language detection not needed)
- Privacy concerns (sending queries to external service)

### 2. Embedded ML Model (lingua-rs)

Use statistical language detection library in Rust.

**Partially adopted:** May add as optional enhancement for ambiguous Latin-script queries.

**Not primary because:**
- ~50MB memory overhead for language models
- Slower than Unicode analysis (still <10ms, but 10x slower)
- Diminishing returns for script detection (Unicode sufficient)

### 3. User-Specified Language Only

Require users to select query language.

**Rejected because:**
- Poor UX (extra step for every search)
- Users don't know their query's "language"
- Mixed-script queries cannot be specified
- Breaks existing API compatibility

### 4. Content-Based Detection

Analyze note content distribution to predict query language.

**Deferred:** Potential future enhancement for personalization.

**Not primary because:**
- Assumes query matches content distribution (not always true)
- Complex implementation (requires content analysis)
- Query-level detection is more immediate and accurate

## Implementation

**Code Location:**
- `crates/matric-search/src/script_detection.rs` - Core detection module
- `crates/matric-search/src/fts_strategy.rs` - Strategy selection

**Key Changes:**
1. Add `unicode_script` crate dependency (optional, for extended coverage)
2. Implement `ScriptProfile` and `detect_scripts()` function
3. Implement `select_strategy()` function
4. Integrate detection into search request processing
5. Add `detected_language` and `search_strategy` to response metadata

**Dependencies:**
```toml
[dependencies]
# Optional: for extended Unicode script detection
unicode-script = "0.5"
```

**Testing:**
- Unit tests for each script type
- Mixed-script queries
- Edge cases (empty, whitespace-only, short queries)
- Emoji detection
- Confidence calculation

## References

- Unicode Script Property: https://www.unicode.org/reports/tr24/
- Unicode Script Ranges: https://www.unicode.org/charts/
- Rust unicode-script crate: https://docs.rs/unicode-script/
- Architecture Design: `.aiwg/working/discovery/multilingual-fts/designs/architecture-design.md`
