# Stakeholder Requirements: Multilingual Full-Text Search Support

## Executive Summary

matric-memory currently supports only English full-text search using PostgreSQL's `matric_english` text search configuration with `plainto_tsquery()`. This limitation prevents global users from effectively searching content in their native languages, including CJK scripts, emoji, right-to-left languages, and other non-Latin scripts. Additionally, advanced query operators (OR, phrase search, wildcards) are not supported. This document outlines stakeholder requirements to transform matric-memory into a truly multilingual knowledge base.

## Context

- **Current State**: PostgreSQL FTS with English-only `matric_english` configuration
- **Known Issues**:
  - #316: Single CJK character search fails
  - #319: Emoji not searchable
  - #308: OR operator not supported
- **Target**: Support global users consuming content in native languages with advanced search capabilities

---

## SR-001: CJK Language Support (Chinese, Japanese, Korean)

### Business Value

CJK languages represent over 1.5 billion native speakers and are critical for enterprise adoption in Asian markets. Users storing technical documentation, research notes, or knowledge bases in Chinese, Japanese, or Korean cannot effectively search their content, severely limiting product utility in these markets.

### User Stories

**US-001.1: Chinese Content Search**

**As a** Chinese-speaking knowledge worker
**I want** to search notes containing Chinese characters (Simplified and Traditional)
**So that** I can find relevant information in my native language without English keywords

**Acceptance Criteria:**

- Given a note containing "人工智能" (artificial intelligence)
- When I search for "人工" or "智能"
- Then I should see the note in search results
- And single character searches like "人" should return relevant results (addresses #316)
- And mixed queries like "AI 人工智能" should work

**US-001.2: Japanese Content Search**

**As a** Japanese researcher
**I want** to search notes with Kanji, Hiragana, and Katakana
**So that** I can find technical content regardless of character set used

**Acceptance Criteria:**

- Given notes containing "データベース" (Katakana), "データ" (mixed), "漢字" (Kanji)
- When I search using any Japanese script
- Then results should match across all three character types
- And verb conjugations should be handled (e.g., "書く" matches "書いた")

**US-001.3: Korean Content Search**

**As a** Korean student
**I want** to search notes in Hangul
**So that** I can organize study materials in my native language

**Acceptance Criteria:**

- Given notes with "데이터베이스" (database)
- When I search "데이터" or "베이스"
- Then I should see matching results
- And syllable-based search should work naturally

**US-001.4: Mixed CJK + English Content**

**As a** bilingual technical writer
**I want** to search notes containing both CJK and English
**So that** I can find content regardless of which language I use in the query

**Acceptance Criteria:**

- Given note: "PostgreSQL 数据库支持 full-text search"
- When I search "PostgreSQL 数据库" or "database search"
- Then I should see the note
- And language detection should handle mixed queries automatically

### Priority

**MUST HAVE** - Critical for Asian market adoption

### Technical Considerations

- PostgreSQL text search configurations: `simple`, `chinese`, `japanese` (via extensions)
- Third-party options: Zhparser extension, MeCab tokenizer for Japanese
- N-gram indexing for character-level search
- Performance impact of CJK tokenization

### Open Questions

1. Should we support both Simplified and Traditional Chinese as separate configurations?
2. What is the minimum acceptable search performance for CJK queries (current: <200ms p95)?
3. Do we need morphological analysis for Japanese verbs/adjectives?

---

## SR-002: Emoji and Symbol Search

### Business Value

Modern knowledge bases include emoji as semantic markers for tagging, categorization, and visual navigation. Technical content includes mathematical symbols, currency symbols, and special characters that users need to search. Without emoji/symbol search, users lose a significant dimension of content organization and discovery.

### User Stories

**US-002.1: Emoji as Semantic Markers**

**As a** productivity user
**I want** to search for emoji used as tags or markers
**So that** I can find notes categorized by visual symbols

**Acceptance Criteria:**

- Given notes tagged with "🔴 urgent", "🟢 completed", "📌 pinned"
- When I search "🔴" or ":red_circle:"
- Then I should see all urgent notes (addresses #319)
- And emoji should be indexed as searchable tokens
- And multiple emoji queries should work: "🔴 📌"

**US-002.2: Mathematical and Technical Symbols**

**As a** mathematics student
**I want** to search for mathematical symbols and equations
**So that** I can find formulas and technical content

**Acceptance Criteria:**

- Given notes with "∑", "∫", "√", "±", "≤"
- When I search for these symbols
- Then results should include exact symbol matches
- And Unicode math symbols should be preserved in indexing

**US-002.3: Currency and Business Symbols**

**As a** financial analyst
**I want** to search for currency symbols and amounts
**So that** I can find budget notes and financial data

**Acceptance Criteria:**

- Given notes with "€500", "$1000", "¥10000"
- When I search "€" or "euro"
- Then I should find relevant financial notes
- And symbol-to-name mapping should work bidirectionally

### Priority

**SHOULD HAVE** - Enhances modern UX and technical content support

### Technical Considerations

- Emoji normalization (skin tones, variants)
- Unicode symbol categories to preserve
- Impact on index size with emoji tokens
- Compatibility with `simple` text search configuration

### Assumptions

1. Users prefer exact emoji matches over fuzzy emoji search
2. Emoji should be treated as whole tokens, not decomposed
3. Symbol search precision is more important than recall

---

## SR-003: Arabic Script Support (Right-to-Left Languages)

### Business Value

Arabic is spoken by 400+ million people across MENA region. Supporting RTL languages enables adoption in markets including Saudi Arabia, UAE, Egypt, and Persian/Hebrew-speaking communities. RTL support requires special handling of text direction and diacritics.

### User Stories

**US-003.1: Arabic Content Search**

**As an** Arabic-speaking researcher
**I want** to search notes in Arabic script
**So that** I can maintain my knowledge base in my native language

**Acceptance Criteria:**

- Given notes with "قاعدة البيانات" (database)
- When I search "قاعدة" or "البيانات"
- Then results should match correctly
- And text direction (RTL) should be handled properly
- And word boundaries should respect Arabic script rules

**US-003.2: Diacritics and Vowel Marks**

**As a** Quranic studies student
**I want** to search with or without diacritical marks
**So that** I can find content regardless of vowelization

**Acceptance Criteria:**

- Given text with diacritics: "كَتَبَ" vs without: "كتب"
- When I search either form
- Then both should match
- And diacritic-insensitive search should be configurable

**US-003.3: Persian and Hebrew Support**

**As a** Persian/Hebrew speaker
**I want** to search notes in Farsi or Hebrew
**So that** I can use the system in my language

**Acceptance Criteria:**

- Given notes in Persian "پایگاه داده" or Hebrew "מסד נתונים"
- When I search in these languages
- Then results should be accurate
- And RTL display should work correctly

### Priority

**SHOULD HAVE** - Important for MENA and Middle East markets

### Technical Considerations

- PostgreSQL `arabic` text search configuration
- Diacritic normalization strategies
- RTL text handling in API responses
- Stemming support for Arabic morphology

### Constraints

1. UI/frontend must support RTL display (out of scope for this backend work)
2. Arabic stemming quality varies across PostgreSQL versions

---

## SR-004: Cyrillic Script Support

### Business Value

Cyrillic-using countries (Russia, Ukraine, Belarus, Bulgaria, Serbia) represent significant user base in Eastern Europe. Many technical and scientific communities use Cyrillic for documentation. Supporting Cyrillic enables adoption in these markets.

### User Stories

**US-004.1: Russian Language Search**

**As a** Russian-speaking developer
**I want** to search notes in Russian
**So that** I can document projects in my native language

**Acceptance Criteria:**

- Given notes with "база данных" (database)
- When I search "база" or "данных"
- Then results should match
- And case-insensitivity should work (Б/б)
- And morphological variants should match (e.g., "база"/"базы")

**US-004.2: Mixed Cyrillic + Latin Content**

**As a** bilingual technical writer
**I want** to search notes with both Cyrillic and English
**So that** I can find mixed-language documentation

**Acceptance Criteria:**

- Given note: "PostgreSQL база данных supports FTS"
- When I search "PostgreSQL база" or "database FTS"
- Then I should see the note
- And transliteration should not be required

**US-004.3: Ukrainian and Bulgarian Support**

**As a** Ukrainian/Bulgarian user
**I want** to search in my specific Cyrillic variant
**So that** I can use language-specific characters (і, ї, ґ for Ukrainian)

**Acceptance Criteria:**

- Given notes in Ukrainian "інформація" or Bulgarian "информация"
- When I search with language-specific characters
- Then results should be accurate
- And character variants should be handled correctly

### Priority

**SHOULD HAVE** - Enables Eastern European market adoption

### Technical Considerations

- PostgreSQL `russian` text search configuration
- Stemming dictionaries for Slavic languages
- Character set normalization across Cyrillic variants

---

## SR-005: Other Scripts and Languages

### Business Value

Supporting Thai, Vietnamese, Devanagari, and Greek ensures global reach and enables adoption in Southeast Asia, India, and Mediterranean regions. These languages have unique tokenization requirements.

### User Stories

**US-005.1: Thai Language Search**

**As a** Thai student
**I want** to search notes in Thai script
**So that** I can organize study materials without English

**Acceptance Criteria:**

- Given notes with "ฐานข้อมูล" (database)
- When I search Thai text
- Then results should match
- And word segmentation should work (Thai has no spaces)

**US-005.2: Vietnamese Diacritics**

**As a** Vietnamese knowledge worker
**I want** to search with Vietnamese diacritical marks
**So that** I can find content accurately

**Acceptance Criteria:**

- Given notes with "cơ sở dữ liệu"
- When I search with correct tone marks
- Then results should match exactly
- And diacritic-insensitive search should be optional

**US-005.3: Devanagari (Hindi/Sanskrit)**

**As a** Hindi-speaking researcher
**I want** to search notes in Devanagari script
**So that** I can maintain knowledge base in Hindi

**Acceptance Criteria:**

- Given notes with "डेटाबेस" (database)
- When I search Devanagari text
- Then results should match
- And conjunct characters should be handled correctly

**US-005.4: Greek Language Support**

**As a** Greek academic
**I want** to search notes in Greek
**So that** I can document research in my native language

**Acceptance Criteria:**

- Given notes with "βάση δεδομένων" (database)
- When I search Greek text
- Then results should match
- And case-insensitivity should work (Β/β)

### Priority

**COULD HAVE** - Enhances global reach, lower priority than CJK/Arabic/Cyrillic

### Technical Considerations

- Thai word segmentation (no space separators)
- Vietnamese tone mark normalization
- Devanagari conjuncts and ligatures
- Greek accent mark handling

### Open Questions

1. Should Thai support use dictionary-based or n-gram tokenization?
2. What is the expected user volume for each language to prioritize implementation?

---

## SR-006: Advanced Query Operators

### Business Value

Power users need advanced search capabilities for precise information retrieval. The current `plainto_tsquery()` limitation prevents Boolean operators, phrase search, and wildcards, severely limiting search expressiveness and user productivity.

### User Stories

**US-006.1: OR Operator Support**

**As a** power user
**I want** to use OR operators in search queries
**So that** I can find notes matching any of multiple terms

**Acceptance Criteria:**

- Given notes about "PostgreSQL" and notes about "MySQL"
- When I search "PostgreSQL OR MySQL"
- Then I should see results containing either term (addresses #308)
- And OR should have lower precedence than AND
- And multiple ORs should work: "A OR B OR C"

**US-006.2: Phrase Search (Exact Match)**

**As a** researcher
**I want** to search for exact phrases in quotes
**So that** I can find specific terminology or quotes

**Acceptance Criteria:**

- Given note: "full-text search is powerful"
- When I search `"full-text search"`
- Then only exact phrase matches should appear
- And word order should be preserved
- And phrase + keyword should work: `"full-text search" PostgreSQL`

**US-006.3: AND Operator (Explicit)**

**As a** technical user
**I want** to explicitly use AND operators
**So that** I can construct precise queries

**Acceptance Criteria:**

- Given notes with "PostgreSQL" and "search"
- When I search "PostgreSQL AND search"
- Then only notes containing both terms should appear
- And implicit AND (space-separated) should still work
- And AND should combine with OR: "(A OR B) AND C"

**US-006.4: NOT Operator (Exclusion)**

**As a** knowledge worker
**I want** to exclude terms from search results
**So that** I can filter out irrelevant matches

**Acceptance Criteria:**

- Given notes about "PostgreSQL indexing" and "MySQL indexing"
- When I search "indexing NOT MySQL"
- Then only PostgreSQL-related notes should appear
- And NOT should work with phrases: `NOT "MySQL indexing"`

**US-006.5: Wildcard and Prefix Search**

**As a** developer
**I want** to use wildcards for partial word matching
**So that** I can find terms with unknown suffixes/prefixes

**Acceptance Criteria:**

- Given notes with "searching", "searchable", "researcher"
- When I search "search*"
- Then all variants should appear
- And prefix matching should be efficient (indexed)
- And `?` should match single character: "wom?n" matches "woman"/"women"

**US-006.6: Grouping with Parentheses**

**As a** power user
**I want** to group query terms with parentheses
**So that** I can control operator precedence

**Acceptance Criteria:**

- When I search "(PostgreSQL OR MySQL) AND indexing"
- Then query should be parsed correctly
- And precedence should be: parentheses > NOT > AND > OR
- And nested parentheses should work

### Priority

**MUST HAVE** (OR operator, phrase search) - Critical for power users
**SHOULD HAVE** (wildcards, NOT) - Enhances search precision

### Technical Considerations

- Switch from `plainto_tsquery()` to `websearch_to_tsquery()` or `to_tsquery()`
- Query syntax validation and error handling
- Performance impact of complex Boolean queries
- Need for query parser to handle user-friendly syntax

### Assumptions

1. Users familiar with Google/Boolean search will expect similar syntax
2. Invalid query syntax should return helpful error messages, not fail silently
3. OR operator is case-insensitive (OR, or, Or all work)

### Open Questions

1. Should we support proximity search: `"word1 word2"~5` (within 5 words)?
2. What is the maximum query complexity to prevent DoS (e.g., 10 operators max)?
3. Should we provide query auto-correction for syntax errors?

---

## Cross-Cutting Requirements

### NFR-001: Performance

**Description**: Search performance must not degrade significantly with multilingual support

**Acceptance Criteria:**

- Search queries complete in <200ms at p95 (current SLA)
- Index size increase <50% compared to English-only
- Concurrent search throughput: 100 queries/second
- Language detection overhead <10ms per query

**Priority**: MUST HAVE

### NFR-002: Language Detection

**Description**: System should automatically detect query and content language

**Acceptance Criteria:**

- Automatic language detection for queries without explicit configuration
- Fallback to `simple` configuration for unknown languages
- User override option for language selection
- Mixed-language queries handled gracefully

**Priority**: SHOULD HAVE

### NFR-003: Backward Compatibility

**Description**: Existing English-only deployments must continue working

**Acceptance Criteria:**

- Default configuration remains `matric_english`
- Migration path for existing indexes
- No breaking changes to search API
- Configuration flag to enable multilingual features

**Priority**: MUST HAVE

### NFR-004: Index Storage

**Description**: Multilingual indexes must be storage-efficient

**Acceptance Criteria:**

- Support for multiple text search configurations on same content
- Configurable per-tenant language preferences
- Index compression for large vocabularies
- Monitoring for index bloat

**Priority**: SHOULD HAVE

---

## Stakeholder Matrix

| Stakeholder | Interest | Influence | Requirements Focus |
|------------|----------|-----------|-------------------|
| Global End Users | High | Low | Language-specific search quality (SR-001 to SR-005) |
| Product Owner | High | High | Market expansion, feature parity with competitors |
| Enterprise Customers | High | Medium | CJK support (SR-001), RTL (SR-003), compliance |
| Dev Team | High | Medium | Implementation feasibility, performance (NFR-001) |
| DevOps/SRE | Medium | Medium | Index size (NFR-004), query performance |
| Support Team | Medium | Low | Error handling, documentation |

---

## Risk Analysis

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| CJK tokenization degrades performance | Medium | High | Benchmark early, use index partitioning, limit to opt-in tenants |
| PostgreSQL extensions (zhparser) unavailable in Docker | Medium | High | Provide fallback n-gram strategy, document extension install |
| Index size explosion with multiple languages | Medium | Medium | Implement lazy indexing, compress vocabularies, monitor limits |
| Query parser complexity introduces bugs | High | Medium | Comprehensive test suite, fuzzing, gradual rollout |
| Language detection accuracy <90% | Medium | Low | Allow manual override, log misdetections for tuning |
| RTL display issues (out of scope but user-facing) | Low | Medium | Document frontend requirements, provide API hints |

---

## Implementation Estimate

### Complexity: **High**

**Rationale**: Requires PostgreSQL text search configuration changes, query parser rewrite, extensive testing across languages, potential third-party extensions.

### Estimated Effort

- **CJK Support (SR-001)**: 10-15 person-days
  - Research: zhparser vs n-gram (2 days)
  - Implementation: indexing + query (5 days)
  - Testing: edge cases, performance (3 days)
  - Documentation: 2 days

- **Emoji/Symbols (SR-002)**: 5-7 person-days
  - Implementation: token preservation (3 days)
  - Testing: Unicode edge cases (2 days)
  - Documentation: 1 day

- **Arabic/RTL (SR-003)**: 8-10 person-days
  - Implementation: configuration + diacritics (5 days)
  - Testing: RTL edge cases (2 days)
  - Documentation: 2 days

- **Cyrillic (SR-004)**: 5-7 person-days
  - Implementation: configuration + stemming (3 days)
  - Testing: language variants (2 days)
  - Documentation: 1 day

- **Other Scripts (SR-005)**: 8-12 person-days
  - Implementation: Thai/Vietnamese/Devanagari/Greek (6 days)
  - Testing: complex scripts (3 days)
  - Documentation: 2 days

- **Advanced Operators (SR-006)**: 12-15 person-days
  - Query parser rewrite (6 days)
  - Testing: Boolean logic, edge cases (4 days)
  - Performance optimization (2 days)
  - Documentation: 2 days

**Total Estimated Effort**: 48-66 person-days (10-13 weeks for 1 developer)

### Recommended Approach

**Phase 1 (Must Have)**: 4-6 weeks
- Advanced operators (SR-006: OR, phrase search)
- CJK support (SR-001: Chinese, Japanese, Korean)
- Performance benchmarking (NFR-001)

**Phase 2 (Should Have)**: 3-4 weeks
- Emoji/symbols (SR-002)
- Arabic/RTL (SR-003)
- Cyrillic (SR-004)

**Phase 3 (Could Have)**: 2-3 weeks
- Other scripts (SR-005)
- Language detection (NFR-002)
- Optimization and tuning

### Team Composition

- 1 Backend Engineer (PostgreSQL/Rust expertise)
- 0.5 QA Engineer (i18n testing expertise)
- 0.25 DevOps (Docker/extension management)

### Critical Dependencies

1. PostgreSQL version compatibility (14+ recommended for `websearch_to_tsquery`)
2. Docker base image with extension support
3. Access to native speakers for acceptance testing
4. Performance baseline establishment before changes

---

## Open Questions

### High Priority

1. **CJK Tokenization Strategy**: zhparser extension vs n-gram indexing vs hybrid?
2. **Language Detection**: Client-side hints vs server-side detection vs explicit configuration?
3. **Index Strategy**: Single `simple` config vs per-language configs vs hybrid?
4. **Performance SLA**: Can we maintain <200ms p95 with CJK tokenization?

### Medium Priority

5. **OR Operator Syntax**: Support both `OR` and `|` (pipe)?
6. **Query Complexity Limits**: Maximum operators/length to prevent abuse?
7. **Fallback Behavior**: What happens when language-specific config unavailable?
8. **Emoji Normalization**: Preserve skin tone variants or normalize?

### Low Priority

9. **Transliteration**: Should we support Romanization search (e.g., "Beijing" finds "北京")?
10. **Mixed-Script Highlighting**: How to highlight matches across different scripts?

---

## Success Metrics

### Functional Completeness

- [ ] 100% of identified languages supported with text search configurations
- [ ] All advanced operators (OR, AND, NOT, phrase, wildcard) functional
- [ ] Language detection accuracy >90%
- [ ] Zero regressions in English-only search

### Performance

- [ ] Search latency <200ms p95 across all languages
- [ ] Index size increase <50% vs English-only baseline
- [ ] Query throughput >100 queries/second under load

### Quality

- [ ] Test coverage >90% for query parser and language handling
- [ ] Acceptance testing with native speakers for CJK/Arabic/Cyrillic
- [ ] Documentation completeness: 100% of features documented
- [ ] Zero P0/P1 bugs in production after 30 days

### Adoption

- [ ] >20% of users enable multilingual features within 90 days
- [ ] User satisfaction (NPS) for search improves by >10 points
- [ ] Support tickets related to search decrease by >30%

---

## Next Steps

### Immediate Actions (Week 1)

1. **Stakeholder Review**: Present requirements to product owner and dev team for approval
2. **Feasibility Spike**: 2-day research on zhparser vs n-gram for CJK
3. **Performance Baseline**: Establish current search performance metrics
4. **Test Environment**: Set up PostgreSQL with candidate extensions

### Follow-Up Required (Week 2-3)

5. **Technical Design Document**: Architecture for multilingual indexing
6. **Query Parser Design**: Syntax specification for advanced operators
7. **Test Plan**: Comprehensive i18n test strategy with edge cases
8. **Migration Plan**: Strategy for upgrading existing English-only indexes

### Decisions Needed

- [ ] Approve phased rollout (Phase 1-3) vs all-at-once implementation
- [ ] Confirm performance SLA acceptable for CJK languages
- [ ] Approve extension dependencies (zhparser, MeCab) or mandate pure PostgreSQL
- [ ] Determine MVP scope: which languages must be in initial release?

---

## Appendix: Language Statistics

### User Base Projections

| Language Group | Native Speakers | Internet Users | Priority |
|---------------|----------------|----------------|----------|
| CJK (Chinese, Japanese, Korean) | 1.5B | 800M | Must Have |
| English | 1.5B (incl. L2) | 1.2B | Already Supported |
| Arabic | 400M | 200M | Should Have |
| Cyrillic (Russian, etc.) | 250M | 150M | Should Have |
| Spanish/Portuguese | 700M | 400M | Uses Latin script (existing support) |
| Hindi/Devanagari | 600M | 450M | Could Have |
| Other Scripts | 500M | 300M | Could Have |

### Technical Complexity Ranking

1. **High Complexity**: CJK (tokenization), Thai (no spaces), Arabic (RTL + diacritics)
2. **Medium Complexity**: Devanagari (conjuncts), Vietnamese (tone marks)
3. **Low Complexity**: Cyrillic (similar to Latin), Greek (straightforward stemming)

---

**Document Version**: 1.0
**Date**: 2026-02-01
**Author**: Requirements Analyst
**Status**: Draft for Stakeholder Review
**Next Review**: 2026-02-08
