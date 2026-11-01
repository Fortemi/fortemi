---
template: post
title: "Fortémi — October 2026"
slug: "2026-10-fortemi"
date: "2026-10-31"
project: "Fortemi"
author: Fortémi Team
summary: "October brought external sign-in, flexible search vectors, consistent memory exports, and fixes across Fortemi, fortemi-auth, and HotM."
hero: https://docs.fortemi.com/server/assets/blog/2026-10-fortemi.png
heroAlt: "Screenprint illustration in rust, ochre and slate: a keycard at a reader opens one drawer of a tall archive cabinet while the others stay shut, and a stream of lines flows from the open drawer to a clock, with fallen leaves at its base."
tags: [report, fortemi, "2026-10", agent-memory]
status: published
---

Fortemi can now use your team's sign-in service on a server owned by one account. October also added imports for search vectors made elsewhere and memory exports that stay consistent while writes continue. Fortemi is the release name for matric-memory, the agent-memory server.

## TL;DR

`v2026.10.2` added external sign-in and access tokens, credentials that let apps act with a user's rights. It also added more control over how you store and import search vectors. In external sign-in mode, `v2026.10.6` requires a startup upgrade to put existing content under the configured account. `v2026.10.7` widened plain-language searches when strict matching finds too little. The suite work includes shared sign-in checks in fortemi-auth and connection fixes in HotM, which had no versioned release this month.

## At a glance

| Area | October status |
| --- | --- |
| Sign-in | External identity service, user records, personal access tokens |
| Search | Flexible vector sizes, external profile imports, adaptive keyword matching |
| Export | Full and incremental snapshots with a saved change mark |
| Deployment | Kubernetes setup options; Linux amd64 and arm64 images |
| Suite | fortemi-auth sign-in rules; HotM connection and interface fixes |

## Highlights

### Connect agents through your sign-in service

If your team already uses Keycloak, Fortemi can check its access tokens on a server owned by one account. OpenID Connect, or OIDC, is the standard used to sign in this way. A claim policy maps trusted group or role claims, fields in the token, to scopes: rights such as read or write. Unmatched values grant nothing.

`v2026.10.2` creates user records from the sign-in source and the ID it gives each user. Notes and jobs can then record who created them. For a script that cannot complete browser sign-in, a user can create a personal access token that expires. Set the `FORTEMI_PAT_PEPPER` secret to enable these tokens. Their rights stay within the user's allowed scopes, and each use checks that the user is still enabled.

### Bring the search vectors your data needs

Embeddings are lists of numbers, called vectors, used to find similar content. In `v2026.10.2`, you can choose their stored size and import profile vectors made outside Fortemi. A profile represents an entity as a whole, so you can compare entities through `find_similar_entities`.

Each set of vector settings can record a space contract. This states the rules for those vectors. The server gives those rules an ID, called `space_id`. An app can filter sets by that ID before it searches them.

### Export a consistent view while notes change

An app may need a full copy of memory, then just the changes since its last run. October's memory export supports both. It reads the data as it stood at one point and saves a change mark. The next export of changes includes changed records and records of deleted data from that mark.

Use `POST /api/v1/memory/export` or `export_memory_snapshot` through MCP, the Model Context Protocol for agent tools. Keep the full export and its mark together. After restoring a database backup, start again with a fresh full export.

### Search with a phrase from the task

In `v2026.10.7`, a plain-language keyword query starts with strict matching. If results are scarce, or the query is long, Fortemi can widen the search within set bounds. Queries with explicit `AND` or `OR` operators stay strict.

## Features shipped

### Search sets and portable data

Knowledge Shards are files for moving Fortemi data. The `2.1.0/full-v1` format preserves vector sizes and types, including vectors stored with less precision. Import checks reject vectors whose shape does not match their stated settings.

External profile imports track each item by its source ID and keep a run record. You can retry the same upload. Normal background jobs leave those external sets alone. Rebuilding vectors for one set also leaves other sets' vectors in place.

`v2026.10.6` uses each set's query prompt across model services. This covers models that need a specific prompt around a search query. If this step fails, search falls back to text matching and tells you it has done so. Shard format `2.2.0` keeps those prompts; export rejects older targets that would lose them.

### Deployment and service health

Helm and Kustomize, tools for setting up apps on Kubernetes, now provide files with separate parts for web requests, jobs, agent tools, and database upgrades. You can scale background work on its own. These setups use external PostgreSQL 18 and Redis, an in-memory data store. The all-in-one bundle can also run without root access; see the volume requirements below.

The bootstrap command sets up a tenant, the account that owns the data, and its first operator. Run it again with the same settings and it reports no change. A text-only setup can turn off attachments. File routes and jobs then reject attachment work.

You can opt into OpenTelemetry, a standard way to track work and measure the service. Traces follow a request into queued jobs and model calls. Metrics cover request time, queue depth, and model-call time. Export stays off until you set it up.

### fortemi-auth: shared sign-in checks

The shared auth library checks sign-in for Fortemi's external mode. Its `v2026.10.0` changes map groups and roles to scopes and distinguish people from service callers. `v2026.10.1` checks token type, intended service, lifetime, and scope limits for each client. It also limits key downloads and refresh attempts.

`v2026.10.2` can read key lists from identity services that publish both signing and encryption keys. It keeps only valid signing keys. `v2026.10.3` passes on checked client and email details to record who acted. Email does not grant access. Apps that use the library must change their code to use these new contracts, the rules for data passed between the library and app.

### HotM: connection and interface changes

HotM is a client app for Fortemi. Its unreleased October work lets the app and agent proxy read the server's sign-in details for external OIDC and single-tenant OAuth, which grants apps access through tokens. The app still blocks unknown sign-in modes and token-check contracts. This work adds no sign-in screen or token-handling flow.

An archive is a separate memory space within Fortemi. Events keep archive names and schemas, the names used to set archives apart in the database. HotM keeps hosted tenant filtering and replay checks. Its unreleased changelog says live events for external OIDC and personal access tokens stay blocked until Fortemi exposes a verified tenant identity. Fortemi added that identity in `/api/v1/me`, but this alone does not establish that a given HotM build can use it.

The interface can load while workspace services are offline, and the mobile sidebar received a fix. This does not mean you can read stored notes offline. HotM's `hotm-latest` is a rolling app build; these changes are not a versioned October release.

## Fixes

`v2026.10.5` fixed the shipped Keycloak setup and claim-policy example. The reference setup was tested end to end against Keycloak 26.7.3.

In `v2026.10.6`, the required startup upgrade moves content ownership to the configured tenant in external OIDC mode. Default-archive note creation works after that change. You need admin rights to manage archives, while readers get an archive picker and live events filtered to allowed archives. Streams end when their access tokens expire.

Search now retains the chosen order for varied results after removing duplicate chains, a step enabled by default. The `diversity` setting therefore changes result order instead of losing that order to score sorting.

`v2026.10.10` fixes valid embedding-set loader requests that failed tenant checks. `v2026.10.11` keeps note-profile matches within the chosen tenant and archive, excluding hidden or deleted notes. It also preserves imported profile vectors without the need for extra set-membership records. Graph reads through `GET /api/v1/graph/{id}` now follow the same tenant and archive checks and skip hidden or deleted notes. A missing root, or one in another tenant, returns `404` once access is checked; roles without access still get `403`. Embedding-import status stays readable while a run is applying, and a second active run with the same ID gets `409`.

The same release fixes the global rate limiter, which now refills at the configured requests per period and keeps its burst capacity. At 600 requests per 60 seconds, one request returns every 100 milliseconds.

The same patch scopes saved `fortemi-login` sessions to the full issuer address and client ID. This sign-in helper stores tokens in the operating system's keychain. Old sessions that do not match are refused; sign in again. You must still opt in to store tokens as plain text in a file.

## Performance & reliability

Vector search indexes now build through background jobs. For a bulk load, you can defer the index build until the data is in place. Index health reports let an admin check the index and look for drift in its rows.

After startup, hosted readiness tracks the service that holds the keys used to encrypt data. Brief outages report degraded health. If the current key cannot be used, the readiness check fails. If an older key is missing, that request fails. The rest of the service stays ready.

## Breaking changes & migrations

MCP now needs an explicit API address. Set `MATRIC_API_URL` or `FORTEMI_URL`, or choose `MCP_API_LAYOUT=bundle` or `MCP_API_LAYOUT=sidecar`. These layouts use a local API: bundle runs the services together, while sidecar runs MCP alongside the API. Without one of these settings, MCP exits at startup. If you set only `ISSUER_URL` before, add `FORTEMI_URL`. The old fallback could send API calls and bearer tokens to the default hosted address.

MCP HTTP access now needs a valid token by default. Local access without sign-in requires both `REQUIRE_AUTH=false` and `I_UNDERSTAND_NO_AUTH=true`. Setting only the first stops startup. Hosted mode refuses anonymous access.

OAuth clients may request only their registered scopes. Re-register old MCP clients with the scopes they need, such as `mcp read write`, then reconnect.

Dynamic client registration, which lets apps register themselves, is now disabled by default in hosted mode. Its setting is `FORTEMI_OAUTH_DYNAMIC_REGISTRATION`. Registration replies no longer include `registration_access_token` or `registration_client_uri`; update clients that expect those fields. `/oauth/authorize` checks requests before any redirect. Unknown clients and unregistered redirect addresses get a local `400` error.

`FORTEMI_ALLOW_LOCAL_ISSUER` no longer allows plain HTTP for a public sign-in host. Move that issuer to HTTPS.

Hosted mode now closes routes that were public: `/api/v1/ws`, `/api/v1/ingest/stream`, and the knowledge diagnostics under `/api/v1/health/*`. They return `401` without a valid bearer token and `503` with one, pending tenant-scoped support. The basic health and readiness probes stay public.

For external OIDC deployments, `v2026.10.6` runs a required ownership upgrade at startup. It moves existing data to the configured tenant as one change that either completes or rolls back. Running it again does not repeat the change. Stop all old writers first. Use separate migration credentials and a runtime database role that neither owns the tables nor bypasses row access checks. When no database URL is set, the bundle creates the restricted runtime login itself. Follow the [ownership upgrade guide](https://github.com/Fortemi/fortemi/blob/main/docs/content/external-oidc-upgrade.md). Managed database setups require PostgreSQL 18.

To run the bundle without root, use user and group IDs `999:999`. `PGDATA`, the database's data directory, must belong to that user; another owner makes startup fail. Use a subdirectory of the volume as directed by the startup error. The first non-root start on a volume used by root registers a new MCP client because the old credentials file belongs to root.

If you set `RATE_LIMIT_PERIOD_SECS=1` to work around the old rate limiter, set it back to `60` in the same rollout as `v2026.10.11`. Leaving `1` in place allows 600 requests per second.

Direct fortemi-auth consumers must update `AuthContext`, the record of a checked caller, for contracts `2.0.0`, `3.0.0`, and `4.0.0`. The changes cover caller kind, scope grants, dropped scopes, and token attribution. Contract `3.0.0` also adds configuration fields and requires an opt-in for loaded rules that trust token scopes.

## Releases

The early server versions differ in which files are available.

| Server version | Date | Contents and delivery status |
| --- | --- | --- |
| `v2026.10.0` | October 9 | Tagged; no images or release assets |
| `v2026.10.1` | October 9 | Images and assets published; images unsigned |
| `v2026.10.2` | October 9 | Signed correction, external sign-in, and vector work; no native binaries |
| `v2026.10.3` | October 10 | Test-fixture correction; signed images, no native binaries |
| `v2026.10.4` | October 10 | Agent-tool scope fix and native binaries for this server line |
| `v2026.10.5` | October 12 | Keycloak reference setup fixes |
| `v2026.10.6` | October 16 | Required ownership upgrade, archive access, events, and query instructions |
| `v2026.10.7` | October 19 | Adaptive keyword search and dependency updates |
| `v2026.10.8` | October 21 | Test discovery for the pinned suite authority revision |
| `v2026.10.9` | October 21 | Bash 3.2 network setup fix in tests |
| `v2026.10.10` | October 23 | Embedding-set access fixes |
| `v2026.10.11` | October 31 | Rate-limit refill, profile-search and graph-read scope, import status, login sessions, and build-cache work |

The fortemi-auth changelog dates `v2026.10.0` and `v2026.10.1` to October 9, `v2026.10.2` to October 12, and `v2026.10.3` to October 14. Fortemi records consuming those versions. The `v2026.10.4` entry, dated October 28, is a prepared source version. Its signing and release steps still have to pass.

## Dependencies & security

The server and fortemi-auth updated the libraries used for secure connections. These are `quinn-proto` 0.11.19 and `rustls` 0.23.46. The latter addresses Rust security advisories RUSTSEC-2026-0346 through RUSTSEC-2026-0349.

Archive ingestion moved to `zip` 9, with rules to limit the older libraries still allowed. HotM's October source work updates libraries in the app and proxy, including `qs` 6.16.1 for safer text output.

Image signing starts with the `v2026.10.2` correction; `v2026.10.0` and `v2026.10.1` supplied no signed images. The new signing workflow includes a software bill of materials, which lists image components, and records of how images were built. In `v2026.10.0`, those controls moved from deferred to pending activation; that status did not mean signed images were available. Check signatures against the published key before moving an image into your registry.

## Docs & DX

The [server docs](https://docs.fortemi.com/server/) now cover external sign-in setup, managed PostgreSQL, Kubernetes deployment, vector index operations, and trace export. The connection guide includes `fortemi-login` and a reference Keycloak setup. The image-promotion guide explains how to verify and copy signed images.

## Tests & CI

Continuous integration, or CI, gained checks for keyword relevance, archive access, and external sign-in. HotM tests cover malformed server metadata, archive events, and replay through mocked streams.

The arm64 API and bundle images are tested under QEMU, which emulates that hardware. There is no native arm64 test runner.

Server build work reuses trusted compiler caches with checks on the source and output. Release builds keep their no-cache and publication checks. HotM also added bounded build caches. These are build changes, with no claimed gain in app speed.

## Cross-project impact

Fortemi `v2026.10.5` consumes fortemi-auth `v2026.10.2`; `v2026.10.6` consumes `v2026.10.3` and reports claim contract `4.0.0`, the versioned rules for checked identity fields. HotM accepts external OIDC metadata for verifier contracts `3.0.0` and `4.0.0`, which define the token checks it understands. Check both sides when choosing a server and client build.

## Known issues & open threads

Hosted mode still does not declare readiness to serve multiple tenants. The routes listed under migrations stay closed until they have tenant-scoped support. The public archive policy defaults still allow the full catalog; the new checks do not by themselves set a custom access policy.

The dependency policy still allows the withdrawn `zip` 2.4.2 and 3.0.0 versions, only on paths that read archives without extracting files. `calamine` and `utoipa-swagger-ui` still lack releases built on `zip` 9. The `cargo-audit` security check still fails on advisories for those older versions. `v2026.10.7` also disables the `ops-autoheal` profile, which had access to the Docker socket, pending a reviewed replacement.

Embedding imports that are interrupted mid-run still lack lasting recovery; that work remains open.

HotM's mocked event tests do not establish that all live setups work. fortemi-auth `v2026.10.4` still needs fresh checks of the library and the apps that use it.

## What's next

The open work covers the remaining hosted routes and release checks for the prepared auth version. That version also needs tests in apps.

## Appendix

Report window: October 1–31, 2026.

- [Fortemi release notes](https://github.com/Fortemi/fortemi/releases)
- [Fortemi changelog](https://github.com/Fortemi/fortemi/blob/main/CHANGELOG.md)
- [fortemi-auth source and changelog](https://github.com/Fortemi/fortemi-auth)
- [HotM source](https://github.com/Fortemi/HotM)
