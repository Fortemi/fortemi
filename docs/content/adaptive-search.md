# Adaptive keyword search

Search parses explicit syntax before retrieval. `AND`, `NOT`, `OR` (also `or`),
quoted phrases, unary `-`, trailing `prefix*`, and parentheses are supported.
Precedence is NOT, then AND (including adjacent terms), then OR. AND and NOT
keywords must be uppercase; lowercase natural-language words remain analyzer
input. Backslash escapes the next character; escaped syntax disables relaxation.
Hyphens inside a word remain part of that word. Prefix stars must terminate a
nonempty term. Unclosed quotes/groups, dangling operators/escapes, and misplaced
stars return `InvalidInput`; malformed explicit input never becomes a plain query.
Queries are bounded to 8,192 bytes and 128 tokens.

English/simple analysis retains stemming, accents, and stopword handling. Phrases
retain stopword distances. Stopword-only expressions match nothing. Each explicit
term or phrase can match the title, current body, or a completed attachment;
Boolean operations combine those matches at note scope. A NOT therefore excludes
a term wherever it occurs in those units. Phrases cannot span units. Tags remain
scoring inputs, not admission inputs; general weighted-field admission is tracked
separately in #1211. Explicit syntax on a trigram/CJK route uses the simple FTS
analyzer, so substring approximation cannot weaken its constraints.

Explicit expressions constrain lexical and semantic candidates before limits.
They are never relaxed, even with zero results. Plain English/simple queries start
with strict AND using the existing unit-local admission predicate. A second OR
retrieval is allowed when fewer than five strict candidates exist or the query has
at least six distinct analyzed terms. These are product defaults, not learned
language classification. Trigram/bigram searches retain their existing strategy.

| Environment setting | Default | Meaning |
| --- | --- | --- |
| `SEARCH_ADAPTIVE_ENABLED` | `true` | Set `false` for strict retrieval |
| `SEARCH_ADAPTIVE_SCARCITY` | `5` | Relax below this strict candidate count; 0 disables |
| `SEARCH_ADAPTIVE_LONG_QUERY_TERMS` | `6` | Relax at this distinct analyzed term count; 0 disables |
| `SEARCH_ADAPTIVE_MIN_MATCH_RATIO` | `0` | Require `max(1, ceil(ratio * terms))`; ratio clamped to 0–1 |
| `SEARCH_ADAPTIVE_CANDIDATE_LIMIT` | `200` | Cap each candidate list, clamped to 1–1,000 |

The Rust `HybridSearchConfig.query_policy` overrides these defaults per request.
Invalid environment values retain defaults. The scarcity threshold is capped by
the candidate limit. Each list requests twice the response limit (at least the
scarcity threshold), capped at the candidate limit. Strict candidates are retained
alongside expanded candidates: at most two lexical lists plus one semantic list.
Minimum-should-match is checked on this bounded union before fusion; it can
underfill a response when candidates below the retrieval cap would qualify.
Stopwords and repeated analyzed lexemes do not inflate the required term count.

Every full lexical match precedes partial matches after fusion, diversity ranking,
and deduplication. Within a tier, existing ranking is retained; equal fusion ties use ascending note UUID, and deduplication preserves
that incoming order for equal scores. Explicit constraints also apply in
semantic-only search; plain semantic-only search retains semantic ranking.

The `Lexical query policy diagnostics` tracing event includes `policy`, `trigger`,
`required_terms`, `analyzed_terms`, `candidate_limit`, and ordered `match_tiers`.
These diagnostics contain no query text or note identifiers. For explicit Boolean
queries, `required_terms=0` denotes expression-based admission rather than a
scalar minimum; all returned matches are full. Empty results still emit the
policy event. No response contract changes are required.

## Relevance evaluation

Run against a disposable PostgreSQL 18+ database with pgvector and PostGIS:

```sh
CARGO_BUILD_JOBS=6 cargo test --locked -j 6 -p matric-search \
  --test adaptive_query_test -- --test-threads=1 --nocapture
```

Set `DATABASE_URL` to the disposable database. SQLx creates isolated databases and
the test runs the production migration runner. Set `FORTEMI_RELEVANCE_REPORT` to
write JSON output. The report evaluates `SearchRequest::execute_on_connection`,
the production retrieval/fusion/dedup path used by search handlers. It does not
measure HTTP transport, authentication, or provider embedding generation.

The frozen 15-note/eight-query fixture uses binary judgments exclusively from
`queries[].relevant_notes`: gain 1/0, discount `log2(rank+1)`, nDCG@10, MRR@10,
and relevance Recall@5/10/100. Unjudged notes are nonrelevant; duplicate note IDs
are removed before cutoffs; empty rankings and empty qrels score zero. All queries
contribute equally to macro means. The floating-point relevance maps and
`expected_top_3` fields are not judgments in this contract. UUID order follows
fixture-key order. All three policies run ten times and must return identical IDs.
The strict run disables adaptation; OR explicitly joins client terms; adaptive
uses frozen defaults. Latency includes the engine and database, excluding seeding.

Synthetic-vector constraint tests are functional tests only. Semantic/hybrid
relevance needs independently pinned real-model query/document embeddings and
separate baselines; keyword quality cannot stand in for those measurements.
The independent generated syntax fixture checks operator and ranking behavior;
it is not a held-out natural-language relevance benchmark.

Measured on PostgreSQL 18.6 / pgvector 0.8.7 with this engine fixture:

| Policy | nDCG@10 | MRR@10 | Recall@5 |
| --- | ---: | ---: | ---: |
| Strict | 0.541986 | 0.875000 | 0.452083 |
| Explicit OR diagnostic | 0.956565 | 1.000000 | 0.950000 |
| Adaptive | 0.956565 | 1.000000 | 0.950000 |

These results meet the proposed 0.90 floors and 0.95 nDCG stretch target. They
reproduce the earlier strict/OR SQL diagnostic aggregates through the production
engine, without establishing HTTP or semantic/hybrid relevance baselines. No
parameters were tuned on a development split in this implementation; all eight
golden queries remain the existing regression set.

## CI regression gate

Run the complete keyword gate with one command:

```sh
bash scripts/ci/search-relevance.sh
```

`DATABASE_URL` must select a disposable PostgreSQL 18+ database with pgvector and
PostGIS available. The command runs the database fixtures, ten repetitions of each
strict/OR/adaptive golden query, the independent Python metric recomputation, and
twelve gate tests including deliberate regressions and malformed baselines. No
embedding provider or GPU is used. The report is written to
`$FORTEMI_RELEVANCE_REPORT` when set (a relative path is resolved against the
working directory), otherwise to `${CARGO_TARGET_DIR:-target}/relevance/keyword.json`.
The integration job in `.gitea/workflows/test.yml` runs this command with the
report under the run-scoped `TMPDIR` and uploads it as the `keyword-relevance`
artifact, including failed per-query results when available. The report identifies the fixture SHA-256, baseline SHA-256, source
commit, PostgreSQL/pgvector versions, configuration, route, and evaluator contract.

`keyword_relevance_baseline.json` pins aggregate and per-query metrics for all
three policies. The gate allows absolute epsilon `0.000001`; every adaptive
query and aggregate must meet its accepted baseline, with additional 0.90 nDCG/MRR
release floors. Strict and OR diagnostic aggregates must reproduce their frozen
baselines within epsilon. All eight queries are mandatory, including empty runs.
A fixture/qrels change requires an explicitly reviewed baseline and quality
report; the gate never updates baselines automatically. Latency is reported, not
gated on shared-runner timing. The ANN overlap acceptance gate is unchanged.

Outstanding qualification for #1210: add frozen real-model query/document
embeddings with model/tokenizer/preprocessing revisions and full fixture coverage;
establish semantic and hybrid relevance baselines without degradation; add an
HTTP/provider regeneration lane; add disjoint development/held-out natural-language,
typo, multilingual, and filtered relevance sets. The present gate measures keyword
relevance through the production engine and functional hybrid constraints only.

The Rust and Python evaluator outputs were independently cross-checked against
`ir_measures` 0.4.3 with `pytrec-eval-terrier` 0.5.11: all aggregate and per-query
nDCG@10, MRR@10, and Recall@5/10/100 values agree within `1e-12`, including
empty strict results. This verification dependency is not needed by fast CI.
See the [ir-measures API documentation](https://ir-measur.es/en/latest/getting-started.html).
