# Candidate Metadata Search Predicates

Authority work: [Fortemi #1091](https://git.integrolabs.net/Fortemi/fortemi/issues/1091).
Declared consumer: [Core #405](https://git.integrolabs.net/Fortemi/fortemi-react/issues/405).

This directory is a candidate, not a published search capability. The schema and
truth cases establish proposed input and scalar semantics. They do not establish
released HTTP support, complete authorization, evidence locators, released
consumer compatibility, or suite portability. Suite NO-GO remains. Candidate
database and retrieval execution evidence is described below, not advertised
as delivered capability.

## REST Authority Candidate

Cycle79 adds the separate `evidence-resolution.schema.json` authority and 21
shared wire cases for POST `/api/v1/search/evidence/resolve`. A JSON body carries
`locator`, optional typed `metadata_predicates`, and optional `include_archived`
(default false). The body is capped at 65,536 bytes; caller tenant/archive fields
are rejected. The existing authenticated memory header selects an accessible
archive, never grants access. The route requires read scope and the handler also
performs the installed normalized note-read policy/audit check for that exact ID.

The database resolver uses the same request connection and shared candidate
scope builder. One data statement checks deletion, archive selection, exact native
unit and current source tuple before returning at most 16 MiB of raw UTF-8 text
for digest/range validation. It never uses inference, historical storage, the
search cache or display snippets. Success is `{ "text": "exact cited range" }`;
malformed input returns bounded `SEARCH_EVIDENCE_INVALID` (400), and missing,
changed or note-policy-denied evidence returns `SEARCH_EVIDENCE_UNAVAILABLE`
(404). Authentication and route denials retain their ordinary 401/403 behavior.
Handler responses are no-store. Core's earlier GET search receipt is historical;
it does not certify this new endpoint. Cycle80 adds Core remote resolution using
a distinct receipt and these unchanged wire cases. Serialized request validation,
bounded no-store UTF-8 responses, a 30-second deadline and caller abort belong to
that consumer operation. The producer still checks current policy and full-text
digest; partial returned text cannot independently prove the full digest. Launched
producer-to-consumer and released gates remain open, with complete evidence
capability false and suite NO-GO unchanged.

Cycle78 clarifies object types and conditional property declarations so the
same schema compiles under strict Ajv and Rust JSON Schema validation. These
declarations make already-implied constraints explicit, not new wire semantics.
Core's remote adapter consumes the full response shape through a separate
candidate REST receipt, with validation before projection or detail I/O. Its
request surface remains q/mode/limit/tags and the existing100-hit local cap;
schema adoption does not enable every server option. The older evidence-only
receipt stays unchanged. Other consumers and live/released acceptance remain.

`search-rest.schema.json` defines the decoded GET query and actual REST response,
including EnhancedSearchHit, legacy chain display fields and explicit degradation.
The query is not a POST body: `strict_filter` and `metadata_predicates` use JSON
query content; other values use ordinary query encoding. Unknown legacy modes
still select hybrid. The server limit is 0..1000, default20, not a consumer's
smaller local limit. `total` is the returned count, not all matching records.

`crates/matric-api/src/search_contract.rs` embeds this authority and relocates
all search references into the generated OpenAPI document. Conditional and
unevaluated-property constraints remain intact without network resolution or
lossy generated-schema conversion. Twenty shared request/response cases are in
`search-rest-vectors.json`; native Rust tests additionally exercise real captured
HTTP response shapes and cross-field/evidence serialization invariants.

Finite score/diversity guards prevent silent NaN-to-null JSON and invalid MMR
inputs. Exact totals, same-note evidence/chain identity, degradation consistency,
metadata range order and the aggregate strict-filter count are semantic checks,
not claims that JSON Schema alone enforces every constraint. Nullable legacy
`min_tag_count` remains accepted. Chain indices/counts remain heuristic display
values, not native evidence coordinates.

This is an unpublished producer candidate. Core#405 and every declared adapter
still need adoption receipts and clean live/released acceptance for this full
REST authority. Existing evidence-only consumer receipts retain their historical
scope and bytes. No immutable revision or complete capability is promoted.

## Input Rules

Predicates form an AND conjunction of at most eight clauses. Only the six named
top-level paths are allowed; arbitrary JSON paths and unknown fields are rejected.
Membership contains at most 32 scalars. Strings have at most 256 Unicode scalar
characters and cannot contain NUL. Numeric request values must fall within
[-9007199254740991, 9007199254740991]; NaN and infinities are not JSON numbers.
The source-identity `import_run_id` path accepts only nonempty strings of at most
200 characters for equality, membership, and range values.

Schema violations return `METADATA_PREDICATES_INVALID`. After schema validation,
reversed range bounds return `METADATA_RANGE_INVALID`. Errors must not include
predicate values, unrecognized paths, or validator diagnostics containing input.

## Match Rules

- `eq` preserves JSON scalar type. Numeric 2 does not match string "2". Numeric
  representations such as 1 and 1.0 compare equally.
- JSON null matches a present JSON null, never a missing key.
- `in` uses the same typed equality. An empty membership matches no rows.
- `range` requires at least one inclusive bound. Both bounds, when supplied,
  have the same type, either number or string. Other stored types do not match.
- Numeric ranges use numeric order. Strings use Unicode scalar order, equivalent
  to UTF-8 byte order for valid Unicode strings, independent of database locale.
- `exists` defaults to true and tests key presence, including null, object, and
  array values. False tests absence. An empty conjunction matches every otherwise
  eligible candidate, not every row regardless of authorization.
- Five metadata paths read author metadata in `note.metadata`, not generated
  `note_revised_current.ai_metadata`. `import_run_id` reads scoped source identity,
  never a same-named metadata key. Positive import-run clauses must all hold for
  one same-tenant, same-schema source identity. Absence means no such identity
  has an import-run value. SQL uses correlated EXISTS, not a result-multiplying
  join. The SQL corpus covers multiple identities, foreign-tenant identities and
  separate archive tables; HTTP verified-claim and locator integration remain.

PostgreSQL JSONB compares numbers numerically but strings using database
collation. Production SQL must choose and index explicit deterministic string
ordering, rather than assuming JSONB comparison alone is cross-runtime stable.
See [PostgreSQL JSON types](https://www.postgresql.org/docs/16/datatype-json.html).

## Verification And Remaining Delivery

`cargo test --offline -p matric-core --lib metadata_search` consumes the shared
corpus with the Rust schema-first parser and a test-only reference matcher. The
matcher is not a production scan fallback. No new dependency is required.

`metadata_predicates::MetadataPredicateQueryBuilder` in matric-db compiles the
validated wrapper into parameterized SQL. Migration20260912000000 adds five
three-column expression indexes on author metadata, plus import-run and
note-identity indexes. The first expression classifies JSON type. The numeric
key clamps numbers to +/-1e16 and truncates to18 fractional digits (booleans map
to0/1 under a separate type constraint). The string key stores at most256 Unicode
characters under C collation. These monotone keys only narrow candidates; exact
numeric/JSONB or C-collated text comparisons recheck every equality/range. They
never alter stored metadata or discard a legitimate match through rounding.
Null and missing use the indexed JSON type expression directly.

This bounds index tuple size even for existing large strings or numbers with
thousands of digits. Versioned key functions must not change in place after
indexes are built. Existing archives receive forward indexes; future archives
inherit them through LIKE INCLUDING ALL. The key/index order follows
[PostgreSQL multicolumn index rules](https://www.postgresql.org/docs/16/indexes-multicolumn.html).

The shared `sql-scope-vectors.json` adds15 source-quantification and bounded-key
recheck cases. `cargo test --offline -p matric-db --test metadata_predicates_sql_test`
requires DATABASE_URL and uses SQLx's isolated test database, not the supplied
database's tables. It executes the exact migration against large preexisting
values,122 actual SQL cases across public/existing/new archive-shaped tables,
three natural selective index plans, and role/RLS isolation checks. The same
corpus passes isolated PGlite. These are database-layer tests, not full server
migration, HTTP, verified-claim, top-k or released-consumer qualification.

Before promotion, implement and verify all of the following together:

Cycle43 integration candidate: `search_candidates` composes typed metadata with
deletion, archive, strict tags, legacy tags/time/collection and embedding-set
membership before lexical/vector limits. `HybridSearchEngine::search_on_connection`
and `SearchRequest::execute_on_connection` retain a caller-owned connection through
retrieval and MMR. The REST prototype accepts a JSON-string `metadata_predicates`
query argument, validates before embedding dispatch, resolves tags/profile on the
request transaction and bypasses the shared FTS cache for hosted requests.
The notation-only resolver cache and per-archive search pools are not used on
that path. Cycle44 router acceptance and its limits are described below.

`matric-search --test scoped_retrieval_test` uses the real migration runner after
the deployment's PostGIS prerequisite, real note writes, synthetic 768-dimensional
vectors and a non-bypass role. Its75 checks cover six registered paths, FTS/vector/
hybrid, English/simple/emoji-trigram/CJK fallback, MMR, pre-limit rejection of
stronger wrong-metadata/tag/set/time/deleted/archived matches and tenant exclusion.
It rolls back its role and rows and runs only inside the temporary native cluster.
This is actual retrieval evidence, not a live HTTP or multi-archive qualification.
Native pg_bigm execution is not proved when the extension is absent; the CJK
fallback remains covered. Earlier SQL-layer122 cases remain a separate corpus.

Remaining promotion gates:

- Production HTTP/JWT/JWKS qualification and enabled-cache isolation; the
  fixture-authenticated in-process middleware stack passes the Cycle44 matrix.
- Identical scoped candidate predicates before ranking and limits in FTS, vector,
  and hybrid retrieval, including authorization, archive, and deleted-state cases.
- Forward index migrations on the actual queried fields, query-plan evidence,
  and bounded handling of existing large metadata values.
- Citation-safe note/chunk/span/source locators without raw external identifiers.
- Core schema adoption, typed SQL, forward index correction, shared actual-SQL
  truth cases, and capability rejection on unsupported adapters.
- Multi-source identity deduplication and scope tests, producer/consumer receipts,
  authority and consumer pins, SAD/ADR updates, and clean-destination acceptance.

The existing consumer's text-coercing predicates and generated-metadata indexes
are known defects, not qualified implementations of this candidate.

Cycle44 executes72 requests through the real auth, request-transaction, archive,
authorization and search-handler stack. The test-only authenticator emits fixed
identities; a local synthetic embedding endpoint proves semantic/hybrid paths
did not degrade to FTS. A one-connection non-bypass runtime pool detects accidental
unbound checkouts. Two tenants can use identical set/configuration names; a
forward migration corrects global uniqueness without changing UUIDs or wire
schemas. Old public/archive catalogs, future clones, same-tenant uniqueness and
row preservation pass a separate migration test. NULL vectors are excluded from
semantic top-k and MMR, not deleted from storage. GET search admission, auth
denial, metadata rejection before inference, archive invisibility, context reset
and local resource cleanup pass. All ten regression groups pass. Enabled Redis,
cryptographic JWT/JWKS, launched production HTTP and real model inference remain
outside this fixture receipt.

Cycle45 routes both public HybridSearch trait methods through the same scoped
pipeline with or without metadata predicates. The builder and trait convenience
methods keep one pool connection's installed context; only request-bound callers
can supply the authenticated transaction. No method establishes authorization.
The old post-limit/set/MMR helper implementations are removed. A reproduced
no-metadata FTS failure now passes with 54 actual entry-point checks covering
three modes, four strategies, strict/unified/legacy tags, set membership,
deletion/archive flags, NULL-vector MMR and tenant exclusion. Three real captured
entry-point spans exclude query/filter fixture values; pipeline count/timing
metrics are retained. This does not qualify arbitrary repository helper calls,
all downstream logs, enabled Redis, JWT/JWKS, real inference or Core backends.
