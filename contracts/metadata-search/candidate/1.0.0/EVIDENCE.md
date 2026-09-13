# Candidate Search Evidence Binding

Authority: Fortemi #1091. Declared consumer: Core #405. This is unpublished
candidate work, not a compatibility advertisement, REST response revision,
Knowledge Shard change or static-index schema. Suite NO-GO remains.

## Locator Semantics

`evidence-locator.schema.json` governs each locator. Unknown fields are rejected;
no raw external key, filesystem path, URL, snippet or arbitrary source metadata
is permitted. `source`, when present, contains only the existing source-upsert
namespace, scoped identity hash, import-run ID and source-schema version. These
are selected from already-visible source rows, not copied from request metadata.

- `note_id` is the native note identity in the request's archive context.
- `unit.kind=current|title` identifies that note's exact current body or title;
  the unit ID is the native note ID and its index is zero.
- `unit.kind=embedding` identifies the actual stored embedding row, its native
  ID and stored nonnegative chunk index. Its text is that row's exact embedding
  input, not an unrelated current body or snippet. Retain this identity through
  ranking and fusion. A row without reproducible text cannot supply evidence.
- `unit.kind=attachment` identifies the native attachment ID with index zero.
  Its text is the exact extracted text of a completed, visible, nondeleted
  attachment, not the attachment filename or the binary blob contents.
- `content_digest` is lowercase `sha256:` over the full unit's raw UTF-8 text.
  Do not trim, normalize Unicode or line endings, remove a BOM, or process HTML.
  This is text identity, not the BLAKE3 binary-attachment checksum.
- `span` has half-open `[start,end)` byte offsets in that exact unit, explicitly
  labeled `utf8-bytes`. Both ends must be code-point boundaries, including for
  an empty range. A matching unit can be cited as a whole unit; a partial match
  must use coordinates derived from raw text, never from rendered highlights.
- Units are bounded to 16 MiB of UTF-8 text. Out-of-budget text cannot be silently
  truncated and advertised as complete evidence. Schema integers, including JSON
  `1.0`, have the same meaning in Rust and TypeScript. Unit IDs remain native
  strings (not universally UUIDs) to cover PGlite and other declared consumers.

The digest plus unit identity distinguishes replaced text. It does not guarantee
that old text is retained or authorize fetching it. Multiple evidence units and
multiple selected source identities must survive fusion without inventing a
single synthetic chunk or dropping the winning semantic source. The candidate
per-hit envelope below defines ordering, deduplication and response limits.

## Per-Hit Envelope

`evidence-set.schema.json` defines the optional candidate `evidence` field on a
search hit: `{version: "1.0.0", locators: [...], omissions: [...]}`. It is separate
from legacy partial `locators` fields. Absence does not advertise evidence support.
This is a per-hit evidence sub-contract, not the complete producer REST SearchHit
or search-response authority. Every locator must identify the enclosing hit's
native note, with no unknown fields. An empty list must have an omission reason.

There are at most64 locators per hit. Canonical order is kind priority (embedding,
title, current, attachment), native unit ID, numeric index, content digest, numeric
span start/end, source absence before presence, then source namespace/hash/run/
schema version. Strings compare by Unicode scalar order, not UTF-16 or locale.
Exact duplicates are removed; distinct text snapshots, spans and source tuples
remain distinct. Check every input's note identity before deduplication so a
foreign note cannot hide behind an otherwise identical locator.

Fusion merges already-bound locators, reorders and deduplicates them, retaining
all omission reasons. It must not rebuild coordinates from rendered snippets or
replace the winning embedding evidence with a current-body pointer. Truncation
keeps the canonical first64, prioritizing semantic units, and adds `locator-limit`.
`unavailable-unit` reports unavailable/unrepresentable text or source projection,
including no single reproducible unit for a combined-text match. Reasons are
unique and ordered unavailable-unit then locator-limit; no raw input values or
private source fields are included. Neither an empty omissions list nor locally
available evidence is a suite-wide capability or retention guarantee.

The Rust SearchEvidenceSet and TypeScript search-evidence-set implementations
validate the envelope, take immutable copies, and construct/merge canonical sets.
The shared36case corpus exercises strict parsing, kind/source/Unicode/numeric
ordering, wrong-note rejection before deduplication, bounded loss, semantic-first
retention and fusion of multiple sources. Cycle75 connects producer scoped SQL,
SearchHit and RRF/RSF/dedup to these sets, with MMR retention tests. A pure merge
helper alone is not the evidence for that integration.

## Validation And Resolution

Cycle79 connects the pure primitive to current producer storage through
`search_evidence_resolution::resolve_on_connection` and POST
`/api/v1/search/evidence/resolve`. The handler validates the bounded JSON body
before lookup, reuses the normalized GET-note policy/audit decision, and selects
text on the verified tenant/archive transaction. An explicit missing-transaction
guard prevents hosted fallback to a personal pool. Shared SQL computes the same
source identity digest during both search projection and resolution.

The data statement checks note deletion, archive-flag selection, optional typed
metadata, native unit identity/index and current source namespace/hash/run/schema.
Completed attachments use extracted text; a removed row cannot resolve. Exact
stored text is withheld above the 16 MiB budget, then the pure helper rechecks
the full hash and UTF-8 boundaries. Neither an unchanged digest nor permission
to call search grants access to the target note. Unit/source checks share one
statement snapshot; backing-note authorization is a preceding read on the same
request transaction. This does not promise retained history or atomic external
policy revocation across a concurrent request.

The fixture-auth test uses a non-superuser/non-RLS-bypass role and one-connection
pool, two tenants and public/archive tables. It covers current/title/embedding/
attachment text, BOM/astral/combining/CRLF ranges, exact same-UUID cross-archive
source collisions, metadata types, changed/missing sources, wrong tenant,
note-policy denial despite route admission, missing hosted context, archived and
deleted notes, changed/oversized text, attachment status/removal, hard-deleted
rows, invalid JSON/media/size and no-store responses. SQL fixture deletion is
not lifecycle-worker purge/crash/restore acceptance. JWT/JWKS cryptography,
installed EE policy behavior, live/released consumer calls and capability
promotion remain separate gates. Cycle80 adds Core remote endpoint adoption
through a distinct resolution receipt and the shared 21-case wire corpus.
Core validates serialized requests before header lookup/I/O, preserves configured
auth/archive context and requires bounded no-store JSON responses with exact
UTF-8 span length. The producer still owns current policy and full-text digest
validation; partial returned text does not independently prove that digest.
Candidate package/unit evidence is not launched producer-to-consumer or released
acceptance. Existing GET/evidence receipts and complete capability false remain.

The Rust `search_evidence` and TypeScript `search-evidence` modules implement pure
schema validation, text binding and resolution. They are not database resolvers,
access-control middleware, note history stores or response serializers.

Malformed wire shape or reversed bounds return `SEARCH_EVIDENCE_INVALID` without
input values. Resolution accepts only a snapshot selected by the caller after
its tenant/archive/auth/deletion/purge checks. Missing, changed, mismatched,
out-of-budget and invalid UTF-8 boundary snapshots all return the same
`SEARCH_EVIDENCE_UNAVAILABLE`, without disclosing why a source is unavailable.
The same selected source projection must still be present when resolving. A
missing snapshot represents no authorized text, not a request to perform I/O.

TypeScript rejects lone surrogates instead of TextEncoder's replacement behavior;
Rust's UTF-8 strings cannot represent them. TypeScript takes immutable copies of
validated wire fields; Rust's validated wrapper has private immutable fields.
Rust Debug omits IDs, digests, source values and text. No source values enter
error messages. The text API itself intentionally returns only the cited range.

## Verification And Remaining Work

The shared 55-case corpus includes raw ASCII/astral/combining/BOM/CRLF text,
empty ranges, every kind, safe source projection, unknown/raw fields, numeric
and size bounds, digest format, wrong identity, content replacement, missing
source and ranges inside code points. Both runtimes validate and resolve all
cases; valid cases also bind to the exact authority locator bytes. This is pure
binding evidence, not proof of database authorization or search integration.

Core's separate `SearchRepository.resolveEvidence` candidate now checks current
PGlite storage in one statement: explicit local tenant/archive/visibility/metadata
scope, note deletion, exact unit identity and selected source tuple. Its bounded
SQL UTF-8 hex projection preserves a leading BOM that ordinary PGlite text decoding
strips. Focused tests exercise changed/deleted/purged/unavailable text and real
terminal purge receipts. This is consumer-local resolution evidence, not producer
database resolution, hosted authorization, cache or historical-retention acceptance.
There is no complete locator capability promotion.

Core's clean-installed candidate now executes all55binding and36envelope cases
through its installed public entry, comparing packaged schemas/corpora against
the candidate receipt. Eight actual storage checks cover three search modes and
adapter forwarding, exact Unicode unit resolution, changed text and deletion.
This is local candidate package evidence with synthetic vectors, not producer
database, hosted authorization, inference or released cross-runtime acceptance.
The existing capability negotiation and registered shard package checks remain
separate gates. The source portable inventory passed as27files in three serial
bounded jobs after the unpartitioned attempt timed out; no test was dropped.

The six actual PGlite citation regressions from Cycle71 now use the explicit
candidate evidence field and resolve their locators to real stored text. Historical
RED receipts remain sealed; assertions were strengthened, not skipped. PGlite
projects whole matched title/body/completed-attachment units in the lexical
ranking query, and the exact selected stored embedding ID/index/text in vector
ranking. Hybrid fusion retains both legs' evidence without later rebinding.
PostgreSQL's built-in [SHA-256 and UTF-8 conversion functions](https://www.postgresql.org/docs/current/functions-binarystring.html)
bind the full raw unit in the statement snapshot; only bounded locator JSON is
returned, not the underlying text. A65th candidate records a64-locator limit;
oversized text is withheld rather than hashed as an invented truncated unit.
Native hit IDs use JSON transport to retain leading BOMs. Hybrid display hydration
rechecks deletion and the original note/metadata filters after ranking.

PGlite and its adapter expose the new optional field; the tool returns the same
repository response. Complete evidenceLocators capability remains false. The
Core remote candidate now validates present evidence against this sub-contract
and the enclosing note before any detail enrichment, then forwards immutable
sets across all search/semantic paths without display-text rebinding. Absent
evidence stays absent; null/malformed/foreign evidence fails the whole response
with bounded diagnostics. Synthetic response regressions and a clean-installed
public-entry gate cover this adapter, not fresh live-producer or published
cross-runtime acceptance. No remote database resolver or auth grant is implied.
The producer now validates SearchHit input and output, projects matched units in
request-scoped native SQL, and preserves them through fusion/MMR/dedup. Its
native source digest matches source-upsert's tenant/schema/namespace/key byte
sequence; projected source tuples satisfy the same import-run conjunction used
for note eligibility. Completed attachment-only matches participate in ranking.
Raw repository convenience methods still omit evidence and are not the canonical
filtered search API. Legacy chain_info heuristics are not citation coordinates.

Cycle78 adds strict cross-validator authoring declarations without changing
instance semantics. Core now registers all schema resources locally, including
the predicate file-location alias for its distinct canonical $id, and rejects
malformed REST responses before any detail read. Its new REST receipt is separate
from the unchanged predicate/evidence-only receipt. This candidate adoption
does not qualify fresh live HTTP, other declared clients or released runtimes.

Cycle77 adds `search-rest.schema.json` and20 shared REST vectors. The producer
bundles request/result/EnhancedSearchHit, locator, envelope and predicate schemas
into generated OpenAPI with local references, retaining strict conditionals.
Query parameters use their actual GET encodings; explicit total/degradation and
finite-number guards protect Rust serialization. Legacy captured HTTP response
shapes and native evidence survive the offline checks. This replaces the API's
unresolved evidence reference, not the candidate's unpublished authority status.
Consumer adoption receipts and full REST live/released fixtures remain open, as
does the producer's authorized current-storage resolver. Then qualify deletion/purge, cache,
hosted JWT/JWKS, every declared adapter and clean-installed/released consumers.
Only after that work may immutable producer revisions replace candidate hashes
or complete evidence-locator capabilities become true. No migration, release
or acceptance closure is implied by adding these primitives.

Rollback removes the candidate evidence field, helpers and their consumers together; it
must not change already-published schemas or historical receipts. No stored
text, native IDs, source identities or applied migration is rewritten here.
## Cycle81 Installed HTTP Cell

The `scoped_search_tests::hosted_installed_core_search_resolution_http` test is
explicitly ignored unless a clean-installed Core fixture is supplied. Cycle81
uses the hash-pinned Cycle80 candidate, native Node fetch and actual Axum TCP
listeners inside the bounded private-network runner. No response mock, custom
fetch or unsupported search-parameter rewrite is used. Production search,
note-detail and resolution handlers retain the verified-context middleware and
non-bypass tenant role. Separate note-denial policy runs on its own listener.

Five phases cover current search/resolution, stale text, archived inclusion,
tombstones and SQL hard deletion across two tenants and an archive. All four
text units, embedding index7, exact BOM/astral/combining/CRLF bytes, partial and
empty ranges, numeric-versus-string resolution predicates and scope/auth/policy
denials are checked. Exact HTTP traffic and four persisted detail accesses per
note prove resolution does not perform hidden detail enrichment.

This is a named fixture-auth TCP cell, not a launched production binary, real
JWT/JWKS, model-quality, external policy-revocation atomicity or lifecycle-worker
purge/crash/restore cell. The candidate package is not published, historical
receipts remain unchanged and complete capability stays false. Reproduce through
the suite Cycle81 containment plan and bounded verifier; never use shared services.

## Cycle82 Production JWT HTTP Cell

The explicit JWT fixture profile replaces FixtureIdentity only for the installed
consumer listeners. It invokes production build_clerk_authenticator with the
pinned fortemi-auth v2026.9.0 implementation, real HTTPS discovery/JWKS, a
disposable extra CA and PgTenantStore over the non-bypass runtime pool. No mock
HTTP client or TLS-verification bypass is used. The unchanged Cycle80 package
continues through actual search/detail/resolution handlers and native Node fetch.

The base five-phase matrix remains intact. Additional cases cover sixteen bad
claim/signature/JOSE/scope forms on all three routes, untrusted TLS, cold JWKS
outage/recovery, warm-key outage, discovery failure/redirect rejection, and
database tenant active/suspended/soft_deleted/re-admitted states with the same
already-minted tokens. An unaffected tenant remains admitted. TLS/JWT keys are
disposable test identities; tokens and private keys never enter durable receipts.

The pinned verifier caches JWKS for600seconds and fetches discovery per request.
The observed same-URI rotation behavior retains the cached old key and rejects a
new kid without refreshing; a changed JWKS URI fetches the new key. This is an
explicit limitation, not immediate key revocation, TTL-expiry acceptance or an
endorsed rotation policy. Discovery outages fail closed even with cached keys.

Cycle82 source-bound receipts qualify this private router/installed-package cell
only. Production binary/config bootstrap, external EE policy atomicity, full
cache invalidation/expiry, lifecycle workers, models, performance, other consumers
and delivered/released revisions remain separate gates. No schema, dependency,
consumer receipt or full capability is promoted. Suite NO-GO remains in force.
