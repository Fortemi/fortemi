# Selected Memory Context Contract

## Cycle97 Browser Job Ownership Receipt

Three fixed fixture-only document-type jobs are enqueued through bounded
browser handshakes, using predeclared tenant/archive/note identities and the
real worker. No producer runtime, wire contract, privileges or Core package
changes. HotM strict owner-qualified cache, endpoint invalidation and buffered
event fencing pass500focused/2177full UI tests, typecheck/build and17actual
browser controls. Live job content and reload, tenant/archive replacement,
foreign denial/logout clearing and cursor reconnect pass. Core220groups/
293requests and655durable audit rows pass. Both owned HTTP fixtures stop/remove
their services with zero PostgreSQL slot refusals. A preceding wrong-cache
launch was intentionally stopped; all evidence is preserved.

Next mobile clipped controls, other persisted stores, live default administration,
nonempty replay response, native, remaining handlers/lifecycle/every-consumer/
CI/delivery/release gates. Source/authority pins and suite NO-GO are unchanged.
See suite Cycle97 phase2-summary.json; no shared inference or signing action.

## Cycle96 Note-Selection Consumer Receipt

Producer runtime/OpenAPI unchanged; two harness files change. The browser gets a
redacted producer-owned note identity fixture. Exactly one audited browser detail
read per note is checked before Core, then exactly four Core enrichment reads are
required. Earlier four-total assumption failure is preserved, not suppressed.
HotM keyed view/request/gate fencing passes400focused/2133full UI and eleven real
browser controls. Core220groups/293requests/635durable audit rows pass. External job
cache/mobile clipped controls and full lifecycle/consumer/CI/delivery remain; NO-GO.

## Cycle95 Browser Consumer Receipt

Producer runtime and generated OpenAPI are unchanged. The harness invokes an
optional bounded real-browser driver after worker acceptance and records its
process exit. HotM notifications replace streams on auth/tenant/selection changes.
Actual browser passes six stream controls; Core220groups/293requests and625durable
audit rows pass. No metadata/auth substitutes or privilege widening.
Application isolation remains incomplete: HotM job cache is unscoped and the
browser note list remains from tenant A after switching to B. Remaining unmigrated
archive/inference GETs return503. Native/full lifecycle/delivered pins remain.

Status: Cycle93 producer candidate for HotM#1 / Fortemi#1091 / Core#405.
Authority: Fortemi generated OpenAPI, ADR-090 and upstream HotM
ADR-MOBILE-001 Decision6; suite contract-authority/profile ADR remains controlling.
This is live persistence metadata, not a Knowledge Shard or static-index change.

## Motivation

Cycle92's real HotM resolver fails before SSE on archive inventory GET503.
Inventory and detail handlers still use raw pool repositories; inventory also
requires admin. Reader realtime admission must not depend on inventory management
or privileged physical storage statistics. Native scoped archive routing already
resolves the visible name/default within the authenticated tenant transaction.

## Candidate Contract

GET `/api/v1/memory/context` requires the existing read scope and returns exactly
`name` and `schema_name` as strings. `X-Fortemi-Memory` selects an archive by its
visible name. When absent, routing chooses the tenant's configured default or
the built-in public fallback. Explicit public routing follows the existing
middleware behavior. The response is a snapshot, not an access capability;
subsequent requests must still pass current authorization and visibility checks.

Responses use no-store. Hosted requests need both the request-owned tenant scope
and resolved archive context; missing context, invalid metadata and dependency
errors fail closed. No new pool checkout, process cache, schema-name derivation,
raw error disclosure, inventory, description, note counts or physical byte-size
metrics are introduced. Unsupported methods remain outside the hosted migration
allowlist. Legacy archive/memory inventory stays admin-only and unmigrated.

## Tenant Default Constraint

`20260913000100_tenant_archive_default.sql` replaces the legacy deployment-wide
partial unique index with uniqueness on tenant_id where is_default is true.
Existing rows and nil-tenant personal default remain unchanged. Independent
tenants can choose defaults, but one tenant cannot have two. Existing physical
schema identity and archive name constraints are not changed by this migration.
Migration runs transactionally under the normal release/migration policy.

Do not roll back by clearing tenant defaults or blindly reinstating global
uniqueness after independent defaults exist. The old application can retain the
compatible tenant-qualified index; otherwise use a separately reviewed recovery
procedure. Consumer rollback must fail closed when the new endpoint is absent,
not return to admin inventory or guessed public/schema mappings.

## Consumers And Delivery

Cycle94 update: HotM candidate now adopts this endpoint through its normal HTTP
client with4096byte bounded JSON/exact-field decoding. Its server fixture uses
that source-bound real resolver/client/parser without request-name translation.
Generated-schema agreement and actual-server metadata/live/replay receipts are
recorded in Cycle94; results do not imply launched browser/native qualification,
full lifecycle, immutable delivered consumer pins or releases. The historical
Cycle93 paragraph below describes the producer-only checkpoint at that time.

HotM is the direct selected-context/realtime consumer. Its Cycle92 resolver still
uses archive list/detail and is NOT yet switched to this endpoint. Next update
the typed bounded response decoder, real request path, explicit/default fixtures,
application tests and actual-daemon client/SSE acceptance against this authority.
Core does not call this new endpoint; its existing package/wire pins remain
unchanged. Inventory clients remain bound to their existing operation contracts.

The additive endpoint needs canonical producer OpenAPI, consumer authority/pin
reconciliation, negative/source-bound receipts and delivered CI/release identities.
Generating the producer artifact does not authorize a consumer pin to an
uncommitted source or qualify an unchanged released server. This compatibility
change remains incomplete until every required producer/consumer gate passes.

## Verification Scope

The bounded native fixture uses real middleware and PostgreSQL FORCE RLS with
NOSUPERUSER/NOBYPASSRLS, one runtime connection, two tenant defaults and the
unchanged nil-tenant default. It exercises explicit/default/public selection,
foreign/unknown memories, invalid headers, authentication/scope negatives,
missing scope/context, dependency failure and same-tenant duplicate rejection.
Generated success schema rejects unknown fields and noncanonical schema names.
Fixture authenticator identities are not a new JWT/issuer/active-tenant receipt.
The suite Cycle93 checkpoint owns exact results, source and cleanup identities.

Full hosted handlers/follow-ups/lifecycle/outbox/every-consumer/CI/delivery/release
gates remain. No inference model or shared-service access is needed; suiteNO-GO,
public readiness false, all20 Lane B issues and prior holds remain unchanged.
