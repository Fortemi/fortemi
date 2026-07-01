---
template: post
title: "Fortémi — January 2026"
date: 2026-01-01
author: Fortémi Team
summary: "Fortémi's first public release: a self-hosted agent-memory server that searches your notes by meaning, tags them with a smart vocabulary, keeps data strictly separated, works with AI agents out of the box, and runs with one Docker command."
tags: [report, fortemi, "2026-01", agent-memory]
---

# Fortémi — January 2026

*Fortémi is the agent-memory server. It keeps notes for an AI agent and lets the agent search them by meaning, not just by exact words. You run it yourself, on your own machine, with one Docker command. This is the full server. HotM is a desktop app that uses it. fortemi-react is the same server built to run in a web browser (see its own report).*

## TL;DR

This is the month Fortémi went public. The first release, **v2026.1.0**, shipped on January 24. It's a memory server for AI agents that you host yourself. Out of the box it can search your notes by meaning, organize them with smart tags, keep separate sets of data strictly apart, and talk to AI agents through a standard plug-in. It can also encrypt notes so you can share them safely. Later in the month, a run of small releases made it easier to install and more solid — including an all-in-one setup you start with a single Docker command. The project's license also moved to a source-available model that stays free for personal use, learning, and trying it out.

## By the numbers

| What's public | Value |
|---|---|
| What it is | A self-hosted agent-memory server you run yourself |
| How to run it | One Docker command (the all-in-one bundle) |
| Released this month | v2026.1.0 — the first public release (refined by same-month point releases) |
| Built for | Agents (over the standard agent-tool protocol), custom apps, and team or offline setups |
| License | Free for personal use, learning, and trying it out; a commercial license covers production use |
| Source · image · docs | github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server |
| Desktop app · browser build | HotM (a desktop client) · fortemi-react (the browser build) |

## Highlights

**1. Search by meaning, not just exact words.**
What it is: Fortémi has three ways to search. One matches the exact words you type (this is called full-text search, or FTS). One finds notes that mean the same thing even if they use different words (this is called semantic search). And a blend of the two combines their results into one ranked list. That blend uses a method named Reciprocal Rank Fusion, or RRF — a simple way to merge two "best" lists into one.
How you'd use it: ask for "how we handle logins" and get back the note that talks about "authentication," even though you never typed that word.
Why it helps: you find what you meant, not just what you spelled. The blend is the default, so you get the best of both without choosing.

**2. Smart tags that understand relationships.**
What it is: tags in Fortémi aren't just flat labels. They follow a shared standard called SKOS — a W3C way to organize a vocabulary so tags can be broader than, narrower than, or related to one another. You can group tags into named vocabularies and mark each one as a work-in-progress, approved, or retired.
How you'd use it: tag a note "database," and Fortémi knows it sits under "infrastructure" and next to "caching."
Why it helps: your tags become a real map of your knowledge, not a pile of sticky notes.

**3. Keep separate data strictly apart.**
What it is: a strict filter that runs *before* the fuzzy search. You can say a note must have certain tags, may have any of a set of tags, must not have others, or must come only from a chosen vocabulary. Because the filter runs first, the results can never spill outside what you asked for.
How you'd use it: keep one client's notes, or one project's notes, completely walled off from the rest — all in one database.
Why it helps: you get guaranteed separation for privacy and compliance, without running a separate server for each group.

**4. Works with AI agents out of the box.**
What it is: Fortémi ships with a server that speaks MCP — the Model Context Protocol, the standard way AI agents plug into outside tools. It came with a large set of ready-made tools for reading, writing, searching, tagging, exporting, and backing up your notes.
How you'd use it: point an AI assistant at Fortémi and it can save what it learns and look things up later, all on its own.
Why it helps: your agent gets a real memory it can use without custom glue code.

**5. Share notes safely with encryption.**
What it is: Fortémi can encrypt a note so only the people you choose can read it. This is public-key encryption (PKE): each person has a public "address" you can share and a private key you keep secret. A note can be locked for several people at once, and each private key is itself protected by your passphrase.
How you'd use it: send an encrypted note to a teammate using only their public address; nobody else can open it.
Why it helps: you can share sensitive notes without trusting the network or the server in between.

**6. Run it all with one Docker command.**
What it is: later in the month an all-in-one bundle arrived. It packs the database, the main server, and the agent plug-in server into a single container.
How you'd use it: start one Docker container and you have a working memory server — no piecing parts together.
Why it helps: setup goes from a checklist to a single command.

**7. Choose your own AI engine.**
What it is: Fortémi can use different back-ends to do its thinking. It defaults to Ollama, which runs models on your own machine. It can also talk to OpenAI-compatible services if you prefer.
How you'd use it: keep everything local with Ollama, or point Fortémi at an outside service by setting one option.
Why it helps: you decide where your data and your compute live.

## Features shipped

**The core memory server (v2026.1.0).** The first release brought the foundation everything else builds on.

- **Hybrid search.** Keyword search, meaning-based search, and a blend of the two, all in one query. The meaning-based part is powered by pgvector, an add-on for the PostgreSQL database that stores and compares meaning as numbers.
- **Smart, standards-based tags.** A full SKOS tagging system with broader/narrower/related links, grouped vocabularies, and tag governance (mark a tag as proposed, approved, or retired).
- **Strict separation.** The pre-search filter described above, so one group's notes stay walled off from another's.
- **A memory pipeline for AI.** New notes can be tidied up with help from an AI model, given meaning-based fingerprints for search, given a title from their content, and automatically linked to closely related notes.
- **Folders and templates.** Organize notes into nested collections, and start new notes from reusable templates with fill-in-the-blank fields.
- **History you can trust.** Fortémi keeps both the original and the revised version of a note, so nothing you wrote is lost.
- **A server for agents.** The MCP tool set for agents, plus a normal web API with an OpenAPI description and a built-in browser page (Swagger UI) for trying calls by hand.
- **Encryption built in.** The public-key sharing described above, including locking one note for several recipients.
- **Your choice of AI engine.** Ollama by default, with OpenAI-compatible services as an option.

**Easier to run and manage (later in the month).** A run of small releases smoothed the rough edges.

- **All-in-one Docker bundle (v2026.1.7).** Database, main server, and agent server in one container.
- **Encryption identity management (v2026.1.6).** New agent tools to create, list, switch, export, import, and delete your encryption keysets — so you can manage who you are and back up your keys.
- **Automatic backup folder (v2026.1.6).** The backup tools now create their folder on first use, so backups work right away.
- **Friendlier tag search (v2026.1.5).** Strict search now accepts plain text tags, not only formal SKOS addresses, so simple tagging just works.

## Fixes

The most important fix closed a data-separation gap. At launch, the strict filter was applied to the keyword search but not to the meaning-based search, which meant a meaning-based query could return notes it should have kept out. This was corrected (v2026.1.4) so the filter now covers both kinds of search — your separated data stays separated no matter how you look for it.

Other fixes in the same run: tag lookups against the database were corrected so all tag features behave consistently; updating just one field of a note (like "starred" or "archived") now works; the agent server correctly handles plain-text replies such as version comparisons; and updating a note now returns the full, updated note instead of an empty reply, which is the expected behavior for a web API.

## Performance & reliability

The strict filter was moved so it runs before the search itself, both for keyword and meaning-based queries. That means the separation is guaranteed rather than checked after the fact. Search uses purpose-built database indexes for both keyword and meaning-based lookups, so results come back quickly even as your notes grow.

## Breaking changes & migrations

No code-level breaking changes — this is the first public release, so there's nothing older to break. One change to know about: the project's license moved to the Business Source License 1.1 (v2026.1.9). Personal use, learning, and evaluation stay free; production use needs a commercial license; and the license is set to convert to a fully open-source license (AGPL-3.0) in 2030. See docs.fortemi.com/server for a plain-English explanation.

## Releases

- **v2026.1.0** (Jan 24) — the first public release. Hybrid search (keyword + meaning + blended), SKOS smart tags, strict data separation, the AI memory pipeline, folders and templates, note history, the agent tool server plus a web API, built-in encryption for sharing, and a choice of AI engines.
- **v2026.1.4** (Jan 29) — closed the data-separation gap so meaning-based search also honors the strict filter, plus tag-database fixes.
- **v2026.1.5** (Jan 29) — plain-text tags accepted in strict search; single-field note updates fixed; plain-text replies handled correctly; the encryption tool deployed.
- **v2026.1.6** (Jan 30) — updating a note now returns the full note; the backup folder is created automatically; new tools to manage your encryption identities.
- **v2026.1.7** (Jan 30) — the all-in-one Docker bundle: database, main server, and agent server in one container.
- **v2026.1.8** (Jan 30) — build-and-test pipeline hardened for reliable, repeatable builds.
- **v2026.1.9** (Jan 30) — license moved to the Business Source License 1.1, with a full check that every dependency was compatible.
- **v2026.1.10** (Jan 31) — build pipeline consolidated into one consistent path, with the test database wired in properly.
- **v2026.1.11** (Jan 31) — fixed a build clash that could happen when two builds ran at the same time.

Published as the server's Docker image at ghcr.io/fortemi/fortemi.

## Dependencies & security

Two safety-minded things stand out this month. First, the data-separation fix (v2026.1.4) made sure meaning-based search can't leak notes past the strict filter. Second, when the license moved to the Business Source License 1.1 (v2026.1.9), the team checked every one of the project's several hundred dependencies to confirm they were all compatible and carried no license conflicts. Encryption is built in from day one: notes are sealed with well-known methods (X25519 for key exchange, AES-256-GCM for the content), and your private key is protected by your passphrase using Argon2id, a method designed to make guessing that passphrase slow and costly.

## Docs & developer experience

The server's documentation lives at **docs.fortemi.com/server**. The launch shipped with an operators guide, an architecture guide, an integration guide, an API reference, and encryption and tagging guides. The web API comes with an OpenAPI description and a built-in browser page for trying calls by hand. When the license changed, a plain-English licensing guide was added so you can tell at a glance what's free and what needs a commercial license.

## Tests & CI

Fortémi launched with an automated build-and-test pipeline that checks formatting, catches common mistakes, and runs the test suite — including tests that need a real database and tests that use a graphics card. Over the last days of the month that pipeline was made more reliable: it was consolidated into one consistent path, its test database was wired in properly, and a clash that could occur when two builds ran at once was fixed. New tests were added specifically to confirm that the strict filter keeps separated data apart.

## Cross-project impact

- **HotM** (the desktop app) and **fortemi-react** (the browser build) are the two surfaces that build on this server. The desktop app bundles the same Fortémi server inside it, and the browser build is the same memory model made to run in a web browser. January's launch is the foundation both of them stand on.
- **Agents and the wider stack** talk to Fortémi through its standard agent tools, so anything that speaks that protocol can start using Fortémi as its memory.
- **Pagenary** (the publishing tool) builds the Fortémi docs site.

## Known issues & open threads

None called out this month. As the first public release, the focus was on shipping the core and then smoothing installation and reliability; the license change is the main thing to be aware of, and it's covered above.

## What's next

Make the server faster and more scalable as your notes grow — including a caching layer and options for larger, tiered storage. Keep adding AI back-end choices and improving automatic backups. And keep the three surfaces — the server, the browser build, and the desktop app — moving in step.

## Appendix

- **What it is:** a self-hosted agent-memory server, built in Rust, run with one Docker command.
- **Released this month:** v2026.1.0 — the first public release, refined by same-month point releases and published as the server's Docker image.
- **License:** free for personal use, learning, and trying it out; a commercial license covers production use.
- **Source · image · docs:** github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server · window: all of January 2026.
- **Related surfaces:** HotM (desktop client) · fortemi-react (browser build).
