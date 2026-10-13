---
template: post
title: "Fortémi — September 2026"
slug: "2026-09-fortemi"
date: "2026-09-30"
project: "Fortemi"
author: Fortémi Team
summary: "September added repeatable note imports, deletion previews, and fixes that preserve unselected data during restore."
hero: https://docs.fortemi.com/server/assets/blog/2026-09-fortemi.png
heroAlt: "Three blue glass supports with fitted pads carry a heavy white stone slab above calm water, beside a clear inspection prism and a small amber tile."
tags: [report, fortemi, "2026-09", agent-memory]
status: published
---

September gave Fortemi users more control over how notes enter, leave, and return to memory. Imports can be repeated without adding extra copies. You can preview a purge before it runs. Restore fixes keep data outside the chosen set.

## TL;DR

`v2026.9.1` added imports that track each note by its source. You can retry the same batch and get the same note IDs back.

`v2026.9.10` added purge previews and records of finished cleanup. It also fixed scoped restores so they keep records outside the chosen set and retain their IDs.

The month also brought manual note links, bounded dataset runs, and fixes for upgrades from older databases.

## By the numbers

| Area | September status |
| --- | --- |
| Note imports | Source identity, exact replay, dry-run, and whole-batch writes |
| Deletion | Preview, resumable cleanup, and content-free receipts |
| Restore | Preserved tag strings, retained identities, and unselected state |
| Agent tools | Versioned dataset execution with explicit limits |
| Auth integration | Added certificate trust settings for the hosted verifier |

## Highlights

### Retry an import without adding copies

What changed: `v2026.9.1` added a way to create or update notes by a stable source ID.

How it works: each note has a source name and an ID from that source. An exact batch replay returns the prior note IDs without adding rows.

Why it matters: say a document import stops before your app gets the reply. You can send the same batch again. The source name and document ID let the server match each item to its stored note. For example, use a stable source name for a set of documents. Keep each document’s ID the same on the next run. If the text has changed, choose the change policy before you send the batch.

### Check what a purge will remove

What changed: `v2026.9.10` added a purge process with a preview.

How it works: the server removes the chosen rows in one step. File and search cleanup can resume. The final receipt records the result without keeping the erased content.

Why it matters: before you remove a body of work, you can inspect the planned scope. You also get a record of the cleanup result. Keep the preview with the task you plan to run. Check that the preview selects only the data you mean to remove. After the purge, check the final receipt for the file and search cleanup result. Removing the rows alone does not finish file and search cleanup.

### Restore chosen notes and keep the rest

What changed: `v2026.9.10` fixed scoped Knowledge Shard restores. Knowledge Shards are files used to move Fortemi data.

How it works: a scoped restore keeps the IDs of retained records. It also keeps records outside the chosen set. A clash with a record outside that set causes the change to fail and roll back.

Why it matters: say you restore one set of notes while keeping another. The chosen set defines what can change. A link to another record alone does not let a restore delete it. Before starting, check that the restore scope includes only the notes you mean to restore. The fix keeps a shared link from turning a small restore into a wider removal. Keep a backup as well, so you have a known point to return to.

### Give agent data runs a clear lifecycle

What changed: `v2026.9.3` shipped a versioned dataset tool through MCP, the Model Context Protocol used by agent tools.

How it works: `manage_dataset_execution` supports preview, run, status, checkpoint, cancel, and resume. It checks the chosen contract and limits before making an API call.

Why it matters: an agent can check what a run will do before it starts. If work stops, the agent can check its status and resume from a checkpoint.

### Link notes with stable retry behavior

What changed: `v2026.9.0` added the manual note-link route.

How it works: you choose a source note and a target note. Exact retries return the same stored link ID. A conflicting score or metadata returns a conflict.

Why it matters: a lost reply should not leave you guessing whether a link exists. Your app can retry the request and use the stored result.

## Features shipped

The import tool accepts batches of 1–500 items. Each batch commits as a whole. A dry-run makes no lasting changes. For changed content, you choose whether to create a version, replace it, or report a conflict. Use the conflict choice when you want to review changed text before accepting it. Use a new version when the change should become part of the note’s history.

The API route is `POST /api/v1/notes/source-upsert`. Agents can use `upsert_external_notes` for the same contract.

Dataset runs have bounds on input size, record count, time, and work running at once. Their receipts leave out source content and its logical IDs. They keep hashes, checkpoints, and reason codes that a tool can check.

Manual links use the same access checks for the source and target notes. Tag paths now follow one set of checks as notes are stored, read, and linked. Tag paths still support five parts.

## Fixes

A shard restore now keeps tag strings that meet the shard’s rules. It no longer rewrites those tags through the rules used when writing a note. When you write or edit a note, the tag rules stay the same.

When a note joins an embedding set by rule, the server uses that note’s tenant and memory scope. These sets group notes for vector search. This fix leaves existing set members as they are.

`v2026.9.9` fixed hosted note writes, access checks, and live events to use the verified tenant. A tenant defines which account owns the data.

## Performance & reliability

`v2026.9.6` improved startup during long database upgrades. The bundle waits for the API to be ready while checking that its process remains alive. Agent tools register once the API is ready. If the API is down or sends a bad reply, saved sign-up details are kept. The bundle reports the fault instead of failing as it reads the reply.

A retry can reuse a recent backup only if checks show it still matches the data and upgrade steps. If a new backup fails, the old backup is kept.

## Breaking changes & migrations

The source-import release adds a source-identity migration. Apply it before enabling import callers.

`v2026.9.10` uses forward migrations through the normal migration runner. Migration files that have been applied stay unchanged. Keep a verified backup before upgrading. Going back to an older build alone does not roll back the database.

## Releases

Dates below use the recorded release date. The partial `v2026.9.4` entry uses its changelog date.

`v2026.9.0` — September 2. Manual links and unified tag-path checks.

`v2026.9.1` — September 4. Note imports you can repeat by source ID.

`v2026.9.3` — September 5. Dataset runs with fixed release contract files.

`v2026.9.4` — September 5. Dataset checks and bulk job fixes; the release was partial.

`v2026.9.5` — September 6. Fix for tests that share a database.

`v2026.9.6` — September 7. Upgrade startup and recovery-point fixes.

`v2026.9.7` — September 7. Extra trusted certificates for hosted sign-in checks.

`v2026.9.9` — September 9. Fixes for tenant scope in a revised release.

`v2026.9.10` — September 17. Purge previews and scoped shard restore fixes.

`v2026.9.11` — September 20. A dataset contract record for the React consumer.

For `v2026.9.4`, images and release entries appeared, but required test coverage failed and native builds were skipped.

`v2026.9.11` updates the record of dataset checks. It leaves runtime behavior and dataset maturity unchanged.

## Dependencies & security

`v2026.9.7` added `FORTEMI_AUTH_CA_BUNDLE` for extra trusted certificates when checking the hosted sign-in source and its keys. Bad files stop startup with a clear error. Certificate tests keep hostname, expiry, HTTPS, and redirect checks in place.

The release includes the hosted verifier in standard API and bundle builds. Hosted mode still needs its own settings for keys, usage limits, and data scope.

## Docs & DX

Source imports ship with rules for requests and replies, plus tests you can run.

Dataset runs ship strict rules for requests and receipts. A separate tool checks the results.

For an app author, these files give you test inputs and the results to check. Start with the [server release notes](https://github.com/Fortemi/fortemi/releases) and [configuration guide](https://github.com/Fortemi/fortemi/blob/main/docs/content/configuration.md).

## Tests & CI

September added purge tests for retries that run at the same time, shared files, stale previews, and restore retry after a crash.

Manual-link tests cover repeat requests, conflicts, access scope, and rollback. Source-import tests cover exact replay and whole-batch rollback.

## Cross-project impact

The server’s manual-link contract names the HotM app as the software that uses it.

`v2026.9.7` pins the `fortemi-auth` `v2026.9.0` authority for its hosted verifier.

`v2026.9.11` declares the independent `@fortemi/react` dataset consumer. That record leaves checks of the released package and live service with the consumer.

## Known issues & open threads

Dataset runs remain at alpha stage for live storage.

Source identities are outside the Knowledge Shard export profiles. An export reports `source-identity-outside-profile` when that loss applies.

You choose which data a purge may remove and which data it must keep. The restore fixes do not prove full backup or loss-free transfer across the suite.

## What's next

Test the exact server and app releases you use: an exact import replay should return the same note IDs without duplicates. A scoped restore should keep unselected records and retained IDs, and roll back on a clash outside its scope. For a purge, check that the preview matches your chosen scope and the final receipt records finished file and search cleanup.

## Appendix

Report window: September 1–30, 2026. Public source paths for the release notes and contracts:

- [Fortemi releases](https://github.com/Fortemi/fortemi/releases)
- [Source imports in `v2026.9.1`](https://github.com/Fortemi/fortemi/releases/tag/v2026.9.1)
- [Dataset execution in `v2026.9.3`](https://github.com/Fortemi/fortemi/releases/tag/v2026.9.3)
- [Purge and restore in `v2026.9.10`](https://github.com/Fortemi/fortemi/releases/tag/v2026.9.10)
- [Validation declaration in `v2026.9.11`](https://github.com/Fortemi/fortemi/releases/tag/v2026.9.11)
- [Fortemi repository](https://github.com/Fortemi/fortemi)
- [fortemi-auth repository](https://github.com/Fortemi/fortemi-auth)
