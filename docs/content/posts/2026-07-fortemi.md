---
template: post
title: "Fortémi — July 2026"
slug: "2026-07-fortemi"
date: 2026-07-31
author: Fortémi Team
summary: "July made Fortémi safer to upgrade and easier to trust. The server got better backups, clearer releases, verified Knowledge Shards, attachment sidecars, and stronger platform checks."
hero: https://docs.fortemi.com/server/assets/blog/2026-07-fortemi.png
heroAlt: "Illustration of a self-hosted Fortémi server packaging a verified knowledge shard with receipts, attachment blobs, and platform checks."
tags: [report, fortemi, "2026-07", agent-memory]
status: published
---

## TL;DR

July was a trust month for the Fortémi server. Upgrades got safer. Releases got more checks. Knowledge Shards got clearer receipts. Files can now move with a shard when you ask for that. Fortémi also gained a better Intel Arc / XPU setup path.

## By the numbers

| What's public | Value |
|---|---|
| What it is | A self-hosted agent-memory server |
| Release series | `2026.7.x`, published through July |
| Key capabilities | memory search, API and MCP access, Knowledge Shards, files, local model paths, self-hosted bundle |
| Public image | `ghcr.io/fortemi/fortemi` |
| Docs | docs.fortemi.com/server |

## Highlights

**1. Safer upgrades from older installs.**
What it is: Fortémi now has a clearer path from older February databases to current builds.
How you would use it: update an existing bundle and let the server check backup safety first.
Why it helps: old installs are less likely to fail mid-update.

**2. Knowledge Shards got real receipts.**
What it is: Fortémi now checks shard shape, checksums, links, and import rules more strictly.
How you would use it: export a knowledge pack, import it somewhere else, and inspect what survived.
Why it helps: “portable” means more when the package proves what it contains.

**3. Files can travel with the package.**
What it is: shard exports can include file bytes as sidecars.
How you would use it: move notes with their files, not only text records.
Why it helps: proof is less likely to become a broken filename after export.

**4. Intel Arc / XPU deployment became a documented path.**
What it is: Fortémi added a host-vLLM setup for Intel Arc / XPU systems.
How you would use it: run text generation through a vLLM server on the host while Fortémi keeps search vectors separate.
Why it helps: self-hosted memory can fit more local hardware setups.

**5. Release publishing became stricter.**
What it is: release jobs now check image pushes, file checksums, proof files, and signing keys.
How you would use it: pull a public image or sidecar with more trust that the release made it.
Why it helps: a green build should mean the public artifact really exists.

**6. Tested system claims became scoped.**
What it is: Fortémi records which system paths passed, and keeps wider claims bounded.
How you would use it: read the proof as “this path passed,” not “everything is done.”
Why it helps: the project can be precise without overclaiming support.

## Features shipped

**Safer upgrades and bundle recovery.** July started with safer updates. Fortémi can clean up older February database history before new schema work runs. It also repairs restore states that would fail on newer database tools. The bundle now stops if it cannot make the backup first. That is safer than changing data without a fallback.

**Intel host-vLLM support.** The server now has a documented Intel Arc / XPU path. The Compose overlay removes NVIDIA assumptions. It points generation at a host vLLM endpoint and keeps embeddings on their own provider. This helps users who want local inference without the usual NVIDIA stack.

**Knowledge Shard checks.** July’s largest server theme was moving knowledge with proof. Fortémi now has stronger checks for named shard profiles across the server, browser, AIWG, and HotM paths. The server checks shape, missing parts, checksums, tree links, repeat imports, bad input, and failed imports that must write nothing.

**File sidecars.** Fortémi can export file bytes as `blobs/<digest>` entries when the caller asks for them. Import checks those files before writing. Missing sidecars can still stay as references. That keeps old shards valid while adding a path for all-in-one packages.

**Schema 2 work.** The server added readers for Knowledge Shard schema `2.0.0`. Exact `2.0.0/full-v1` export is opt-in and bound to a receipt. The default export stays on the stable path unless a caller asks for the exact route.

**Release authority and proof.** Fortémi moved release and CI credentials behind OpenBao-backed flows. Release tags and normal commits use different signing keys. Native sidecars and public release files now get checksum and proof checks.

**Usage records.** The server added durable usage records for requests, model calls, streams, files, ingest, vectors, speech text, and queue work. This gives Fortémi a cleaner base for quotas and future managed surfaces.

## Fixes

Several fixes made the server safer in real use. Bundle images now make database secrets for each install. Host Ollama exposure is constrained. Autoheal rules are explicit. Readiness and shutdown improved. Job retries are bounded. Rate-limit responses now carry `Retry-After` where expected.

Search and vector work also tightened up. Query vector contracts now resolve correctly. Failed vector writes roll back. Full-text search cache state is isolated. File ingest is bounded before checks. Managed bytes are gated on malware scan results.

## Performance & reliability

Reliability was a core July theme. The backup gate, safer restore path, bounded shard import, staged sidecars, release checks, and system test paths all reduce hidden failure. Later July work also made Linux arm64 and macOS sidecar builds more stable.

## Breaking changes & migrations

No broad user-facing API break landed in July. The main note is for upgrades. Bundled updates now require a successful backup before current schema work runs. If the backup cannot be made, the server stops before changing the database.

## Releases

The public July server series includes:

- `2026.7.0` — February-to-current update safety and release recovery.
- `2026.7.1` — upgrade reliability, Intel Arc / XPU host-vLLM support, and OpenBao-backed CI credentials.
- `2026.7.10` — data movement and Knowledge Shard contract checks.
- `2026.7.11` — stable publication recovery for the shard release.
- `2026.7.12` — documentation shard and scan corrections.
- `2026.7.13` — exact Knowledge Shard `2.0.0` readers, receipt-bound full-profile export, and tested-system proof.
- `2026.7.14` through `2026.7.19` — follow-up releases for native sidecar and tested-system release reliability.

## Dependencies & security

Security work touched runtime rules and publishing. The MCP server dependency tree was updated to remove a high-severity `fast-uri` advisory and other current advisories covered by safe updates. Fortémi also tightened Docker exposure, made safer bundle secrets, protected API inventory output, hardened rate-limit contracts, and moved CI and release credentials through OpenBao.

## Docs & developer experience

Docs gained a February-to-current upgrade runbook, an Intel host-vLLM guide, shard sidecar notes, backup guidance, and updated API contracts. The docs also gained July topic articles about memory inboxes and knowledge receipts. Those are topic pieces. This page is the monthly server report.

## Tests & CI

July added checks around upgrades, shard import and export, sidecar staging, OpenAPI responses, release files, system cells, and consumer paths. The public point is simple: more release claims are tied to repeatable receipts instead of trust in a build log.

## Cross-project impact

fortemi-react benefits from the same Knowledge Shard work, especially named profiles and receipt-backed movement. HotM benefits from the tested-system path and server stability. AIWG benefits from stronger shard movement and proof trails. Pagenary continues to publish the docs and blog surfaces.

## Known issues & open threads

Windows proof remains deferred. Full-profile movement is still scoped and receipt-bound. It is not a blanket claim. Some follow-up July releases were about release setup, not product behavior. They did not change shard formats or tested-system proof.

## What's next

The next work is to keep closing the gap between self-hosted memory, browser memory, and desktop consumers without overstating parity. Expect more receipt-backed movement, more tested-system proof, and safer bundle ops.

## Appendix

- **Published artifact:** `ghcr.io/fortemi/fortemi`.
- **Releases:** the `2026.7.x` server series, published through July.
- **Source / docs:** github.com/Fortemi/fortemi · docs.fortemi.com/server · window: all of July 2026.
