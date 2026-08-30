---
template: post
title: "Fortémi — August 2026"
slug: "2026-08-fortemi"
date: "2026-08-28"
project: "Fortemi"
author: Fortémi Team
summary: "August tightened the hosted Fortémi path with stronger tenant, audit, credential, policy, quota, recovery, and key-handling gates."
hero: https://docs.fortemi.com/server/assets/blog/2026-08-fortemi.png
heroAlt: "Sunlit glacier-blue glass forms rising from water, representing clear boundaries and durable memory receipts."
tags: [report, fortemi, "2026-08", agent-memory]
status: published
---

Fortemi spent August tightening the server base for memory, agent access, and future paid team use. Release files became easier to trust. Background work became easier to bound and recover. The server also gained more tenant, key, audit, and outbound-rule work for future enterprise and commercial releases.

## TL;DR

Fortemi's August server line centers on two public releases: `v2026.7.22` and `v2026.8.0`.

`v2026.7.22` fixed a runtime contract publication path. It also improved background jobs, recovery, redacted job checks, and the `h2` dependency.

`v2026.8.0` began the August CalVer line. It fixed release files by keeping release handoff tied to the signed tag. It kept the app contract stable. It also fixed text chunking, MCP restart behavior, startup log redaction, and migrations.

The larger theme is product readiness. Today, Community Edition gives users a local-first server with Docker setup, API access, MCP access for agents, auth controls, background ingest jobs, and source-linked memory.

The paid team path adds stricter shared-use controls. That means tenant context, stored keys, request checks, audit proof, startup key checks, outbound rules, billing gates, and provider proof.

## By the numbers

| Area | August status |
| --- | --- |
| Public server releases cited | `v2026.7.22` and `v2026.8.0` |
| Currently available path | Self-hosted Community Edition through the Docker bundle |
| Enterprise/commercial direction | Tenant-aware shared use, stored keys, audit receipts, key checks, outbound rules, billing gates, and provider proof |
| Public install surfaces | Fortemi README, GitHub docs, Docker Compose bundle, and Fortemi site |
| Related client surfaces | HotM desktop app context and fortemi-react package surfaces |

## Highlights

### Release files stayed tied to the signed tag

What changed: `v2026.8.0` corrected the release-file path for the August server line.

How it works: release handoff now keeps the full tag reference. The release stays bound to the signed tag it started from.

Why it matters: operators need to know which build they run. This narrows the gap between the public tag, the release check, and the file they install.

### Paid team controls became clearer

What changed: Fortemi advanced controls for tenant scope, user keys, request checks, audit receipts, startup key checks, and outbound rules.

How it works: shared or commercial requests need to show who is asking. They need to show which tenant owns the work. They need to show which credentials may be used. They pass checks before sensitive work starts.

Why it matters: shared memory cannot rely on local trust. These controls can be tested and audited. They can also connect to plans and provider behavior.

### Background jobs became easier to recover

What changed: the `v2026.7.22` line bounded job handlers. It added safer failure reports. It also recovered jobs left in a running state.

How it works: jobs now move toward done, retry, or failed. Job checks explain stages without exposing payloads.

Why it matters: memory ingest and embedding work often runs after the first request. Users need those jobs to finish, fail clearly, or recover after restart.

### Text chunking handled real documents better

What changed: the August server line fixed chunking for mixed line endings and non-ASCII text.

How it works: chunk spans now keep UTF-8 byte positions across LF, CRLF, and other line forms.

Why it matters: search quality depends on stable spans. Fortemi should point back to the right source bytes, even when text has mixed line endings or non-ASCII text.

### MCP clients got a clearer restart path

What changed: unknown Streamable HTTP session IDs now return a clear 404 after server restart.

How it works: a conforming MCP client can treat that response as a signal to start a fresh session.

Why it matters: agent tools should recover from a server restart without making users guess if the old session still works.

## Features shipped

Fortemi's public server work in August focused on two tracks: the current self-hosted server and the base for future paid team releases.

The self-hosted path remains the normal public entry point. The README documents a Docker bundle flow. It starts PostgreSQL, Redis, the API, the MCP server, and the local stack. The config docs keep auth on by default for protected API use. They also require clear choices before a deployment is exposed beyond local use.

The paid team path moved in a stricter direction. Tenant context, request checks, stored keys, audit proof, startup key checks, outbound rules, billing gates, and provider proof are treated as product gates. In user terms, Fortemi is moving shared use away from broad server trust. The server should be able to prove that a tenant, user, key, model, plan, and destination passed policy.

The MCP path also improved. Fortemi exposes an MCP server for agent tools. August tightened restart behavior. Old session IDs can be rejected clearly. Clients can then open a new session.

## Fixes

`v2026.8.0` corrected the release path for the August line. The public release notes state that app behavior and the `core-v1` Knowledge Shard app contract stayed unchanged while corrected artifacts were published.

The same line fixed chunking for mixed line endings and non-ASCII text. That keeps document spans aligned with the original bytes. It also prevents one ingest failure from blocking embedding work.

MCP startup logs were also narrowed. Generated registration values are redacted from bundle boot logs. Raw registration responses are not printed there.

`v2026.7.22` addressed job paths that could remain unclear after timeout, panic, abnormal stop, or restart. The release also refreshed runtime contract material. That kept container checks and the published contract aligned.

## Performance & reliability

The biggest reliability work was around bounded background jobs. Fortemi now pushes jobs toward a known outcome. Jobs should finish, retry, or fail. Startup and periodic recovery paths help pick up work that stopped mid-run.

Config also stayed fail-closed. Public docs describe strict parsing for sensitive settings. They also require checks before shared exposure. A memory server should reject a risky config instead of guessing.

Paid inference work added scoped retry behavior behind shared-use gates. This can help future team use handle repeated provider or key failures. It does not change the Community Edition routing model or make a broad uptime promise.

## Breaking changes & migrations

`v2026.7.22` did not require a destructive database step in its public upgrade notes.

`v2026.8.0` includes forward database migrations that normal startup applies. Operators should still back up and verify the database before upgrade. Rollback should use a restored snapshot in a separate place. It should not weaken tenant or access controls.

## Releases

`v2026.7.22` was published on GitHub in August 2026 as a corrective server release. It focused on keeping the runtime contract coherent. It also made job execution easier to bound, recover, and inspect safely.

`v2026.8.0` was published on GitHub in August 2026 as the corrected August server file line. It keeps the app contract stable. It fixes release handoff, chunking spans, MCP restart behavior, startup log redaction, and migration delivery.

No August HotM public GitHub release was used as evidence for this report. HotM remains important client context. Its August work belongs in client notes unless a public HotM release is available.

## Dependencies & security

Fortemi updated `h2` to address `RUSTSEC-2026-0258` in the public server release line.

The security story this month is specific. KMS work means startup-gated key custody for future paid credential paths. It does not mean every Fortemi secret in every deployment is KMS-backed. Credential work means stored, scoped keys. It does not mean Fortemi can recover or delete keys from every outside provider.

The outbound-rule work is also bounded. The goal is to check destinations before adding keys or opening paid inference links. That lowers risk for shared use. It is not a promise that all outbound data movement is impossible.

## Docs & DX

The public README and docs remain the best starting point for operators. The README covers the Docker bundle, local setup scripts, and MCP connection path. The config docs cover auth defaults, shared exposure rules, strict parsing, and MCP examples. The auth docs cover API keys, OAuth flows, scopes, refresh, and MCP auth modes.

Fortemi's public site also points readers to the server, HotM, and fortemi-react surfaces. The npm pages for `@fortemi/core` and `@fortemi/react` remain the public package install surfaces for browser and React work.

The main DX gain this month is trust. Release files are less vague. Upgrade notes are clearer. Config checks are clearer. MCP clients get clearer restart behavior.

## Tests & CI

Release checks now include a guard for keeping release handoff tied to the signed tag. That is the public point. Release checks should verify the source behind the file.

The server work also carried checks for chunking, job recovery, MCP restart behavior, and paid team schema and request paths.

## Cross-project impact

HotM is the main client context for Fortemi's shared-use and route-authority work. Server gates matter because client flows need clear answers. Which route is allowed? Which tenant is admitted? Which actions have proof?

`fortemi-auth` remains related auth-contract groundwork. Its public repo documents shared Rust auth interfaces and provider integrations. This report does not claim an August `fortemi-auth` release.

The fortemi-react package surface remains the browser-side companion. Its details belong in a separate browser report. They should not be folded into server release claims.

## Known issues & open threads

Future enterprise and commercial use remains evidence-bound. Live provider receipts, production key proof, billing and plan gates, and public protocol surfaces remain separate release gates.

Some paid team docs are still operator-facing rather than quick-start material. The public self-hosted path is still the clearer onboarding path today.

## What's next

Next work should keep the Community Edition and paid team stories clear.

For Community Edition, keep the local Docker path simple, documented, and easy to verify.

For enterprise and commercial releases, keep closing evidence gaps around tenant checks, key custody, destination rules, audit receipts, billing gates, and live provider behavior.

For clients, keep wiring Fortemi server proof into HotM and fortemi-react. Users should see clear route authority, session recovery, and action proof instead of hidden server assumptions.

## Appendix

Public sources checked on 2026-08-28:

- [Fortemi releases](https://github.com/Fortemi/fortemi/releases)
- [Fortemi `v2026.8.0`](https://github.com/Fortemi/fortemi/releases/tag/v2026.8.0)
- [Fortemi `v2026.7.22`](https://github.com/Fortemi/fortemi/releases/tag/v2026.7.22)
- [Fortemi repository and README](https://github.com/Fortemi/fortemi)
- [Fortemi config docs](https://github.com/Fortemi/fortemi/blob/main/docs/content/configuration.md)
- [Fortemi auth docs](https://github.com/Fortemi/fortemi/blob/main/docs/content/authentication.md)
- [Fortemi public site](https://fortemi.com/)
- [`@fortemi/core`](https://www.npmjs.com/package/@fortemi/core)
- [`@fortemi/react`](https://www.npmjs.com/package/@fortemi/react)
- [HotM repository](https://github.com/Fortemi/HotM)
- [HotM releases](https://github.com/Fortemi/HotM/releases)
- [fortemi-auth repository](https://github.com/Fortemi/fortemi-auth)
