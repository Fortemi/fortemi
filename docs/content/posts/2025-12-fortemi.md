---
template: post
title: "Fortémi — Origins (2025)"
date: 2025-12-01
author: Fortémi Team
summary: "Before its first public release, Fortémi was built as the memory engine inside a local-first notes app called HotM. This is the origin story: how the search-by-meaning server, its smart tags, and its agent tools came to be during 2025 — under a different name, not yet released."
tags: [report, fortemi, "2025-12", agent-memory, inception]
---

# Fortémi — Origins (2025)

*Fortémi is the agent-memory server. It keeps notes for an AI agent and lets the agent search them by meaning, not just by exact words. You run it yourself, on your own machine, with one Docker command. This is the full server. HotM is a desktop app that uses it. fortemi-react is the same server built to run in a web browser (see its own report).*

*This is an origin report. Everything here predates Fortémi's first public release (January 2026). In 2025 the server did not yet carry the Fortémi name — it lived inside a local-first notes app called **HotM**, was pulled out into its own project at the start of 2026 under the internal codename **matric-memory**, and was named **Fortémi** shortly after. Nothing described here was publicly released in 2025; this is the story of what was **built** before the doors opened.*

## TL;DR

Fortémi did not start as a standalone product. It started as the engine inside **HotM** — a local-first, keep-your-own-data notes and analysis app. In a concentrated build in **August 2025**, the core of what would become Fortémi took shape: a Rust server backed by PostgreSQL that could search notes by meaning as well as by exact words, organize them with smart tags, and expose everything to AI agents through a standard tool protocol. HotM's guiding idea — **your originals are never changed, only enhanced** — became one of Fortémi's lasting principles. Through the rest of the year the project was put on a proper footing: a clean database schema, comprehensive tests, and a formal design-and-build process. At the very end of 2025 the memory engine was ready to leave the nest. In **early January 2026** it was extracted into its own project (codename *matric-memory*), renamed **Fortémi**, and released to the public — the story the January report picks up.

## By the numbers

| What's public | Value |
|---|---|
| What it was | The memory engine inside HotM, a local-first notes app you run yourself |
| Status in 2025 | Pre-public — built and tested, **not yet released** under any name |
| Where it lived | Inside `HotM` (Rust/Axum server + Tauri desktop app); extracted to its own project in January 2026 |
| The name | Chosen later — "Fortémi" was settled in early 2026; in 2025 it was simply HotM's engine |
| Built for | Search-by-meaning notes, smart tags, and AI agents over a standard tool protocol |
| Coverage window | August 2025 through December 2025 (the HotM era) |
| What came next | January 2026 — extracted, renamed Fortémi, first public release (see the January report) |

## Highlights

**1. The idea: search by meaning, from day one.**
What it is: from its earliest days the engine combined three ways to find a note — matching the exact words you type (full-text search), finding notes that *mean* the same thing even in different words (semantic search), and a blend of the two merged into one ranked list using a method called Reciprocal Rank Fusion (RRF). The meaning part was powered by pgvector, an add-on to the PostgreSQL database that stores and compares meaning as numbers.
How you'd use it: search for "how we handle logins" and get back the note about "authentication," even though you never typed that word.
Why it helps: this was the founding bet — that a memory should find what you *meant*, not just what you spelled — and it shipped into the very first prototype.

**2. Originals are never changed, only enhanced.**
What it is: HotM's core principle was that your original note is kept immutable — the system can add an AI-cleaned revision, a summary, or tags alongside it, but it never overwrites what you wrote.
How you'd use it: capture a rough note; let the engine tidy and tag a copy; your original words stay exactly as you left them.
Why it helps: you can trust the memory with your raw thoughts, because nothing you write can be silently lost or rewritten. This principle carried straight into Fortémi.

**3. Smart tags and a knowledge graph.**
What it is: notes were organized with a real vocabulary — tags with broader/narrower/related relationships, collections, and links between notes — plus a record of where each note and revision came from (its provenance).
How you'd use it: tag a note "database" and let the system relate it to "infrastructure" nearby; follow links between connected notes.
Why it helps: the memory became a map of how your knowledge fits together, not just a flat list — the seed of Fortémi's SKOS tagging and graph features.

**4. Built for AI agents from the start.**
What it is: the engine shipped with schemas for the Model Context Protocol (MCP) — the standard way AI agents plug into outside tools — alongside a normal web API described with OpenAPI, and local NLP prompt templates for revision, summarization, and tagging run through Ollama on your own machine.
How you'd use it: point a local AI assistant at the engine and let it read, write, search, and tag notes on its own — with the model running on your hardware.
Why it helps: "a memory an agent can actually use" was a requirement from the first week, not an afterthought — which is why Fortémi launched agent-ready.

**5. Local-first and yours.**
What it is: everything ran on your own machine — a Rust (Axum) server on PostgreSQL, a Tauri desktop app with a React interface, a system-tray presence, and a global hotkey for quick capture. Your data and your AI compute both stayed local.
How you'd use it: hit a hotkey, jot a thought, and have it captured, enhanced, and searchable — without anything leaving your computer.
Why it helps: the privacy-first, self-hosted stance that defines Fortémi today was there in the original design.

## What was built (before the first public release)

**The memory engine (August 2025).** In a focused burst, the foundation everything later builds on came together inside HotM:

- **Hybrid search.** Keyword search, meaning-based search (via Ollama embeddings + pgvector), and a blended RRF ranking — with basic filters — in one query.
- **Tags, collections, links, and provenance.** The beginnings of a structured vocabulary and a knowledge graph, plus a record of where each note came from.
- **Immutable originals with AI enhancement.** Originals preserved; revisions, summaries, and tags added alongside by local NLP.
- **An agent-and-API surface.** MCP tool schemas for agents, a web API with an OpenAPI description, and local prompt templates.
- **A local-first app shell.** A Rust/Axum backend and a Tauri + React desktop UI with quick capture, a health banner, tag/collection editing, a system-tray icon, and a global hotkey.

**Put on a solid footing (December 2025).** Late in the year the project moved from prototype to something built to last:

- **A clean, greenfield database schema** — soft-delete consolidated into one clear model, dev-era migrations cleared away.
- **Comprehensive test coverage** — a formal construction milestone focused on tests, so the foundation was verified, not just written.
- **A formal design-and-build process** — the engine's inception and construction phases were set up with a real SDLC baseline, and the development tooling was modernized.

## Timeline

- **August 2025** — the memory engine is scaffolded inside HotM: Rust/Axum on PostgreSQL, Ollama + pgvector embeddings, hybrid FTS + vector + RRF search, tags/collections/links/provenance, MCP tool schemas, an OpenAPI web API, and a Tauri + React desktop shell. The founding principles — search by meaning, immutable originals, agent-ready, local-first — are all present.
- **December 2025** — the engine is put on a firm footing: a clean greenfield schema, comprehensive tests, and a formal SDLC baseline; the project is established in its home repository at year's end.
- **Early January 2026** — the memory engine is extracted from HotM into its own project under the internal codename *matric-memory*, renamed **Fortémi**, and made public. That is where the [January 2026 report](2026-01-fortemi.md) begins.

*No public releases were made in 2025. Early `v0.1`–`v0.2` tags on HotM were pre-release builds of the desktop app, not releases of the memory server.*

## Cross-project impact

- **HotM** was the cradle. The memory engine was born inside it, and after the engine was extracted, HotM continued as a desktop app that embeds the same Fortémi server. The two share a lineage and, to this day, a design philosophy.
- **fortemi-react** (the browser build) and the wider agent stack inherit the same founding model — search by meaning, structured tags, agent-first tools — because it was set from the very first commits.
- **Pagenary** (the publishing tool) later builds the Fortémi docs site where reports like this one are published.

## What's next

The origin ends where the public story begins. In January 2026 the engine leaves HotM, takes the name Fortémi, and ships its first public release — one Docker command, a memory server for AI agents, free for personal use. Everything after this report is covered month by month, starting with **January 2026**.

## Appendix

- **What it was:** the memory engine inside HotM — a local-first notes app — built in Rust on PostgreSQL, not yet named or released.
- **Built in 2025:** search by meaning (FTS + vector + RRF), immutable originals with local AI enhancement, structured tags and a knowledge graph, MCP agent tools and an OpenAPI web API, and a Tauri desktop shell — followed by a clean schema, comprehensive tests, and a formal build process.
- **The name:** "Fortémi" was chosen in early 2026; in 2025 this was HotM's engine, briefly codenamed *matric-memory* on extraction.
- **Source · docs:** github.com/Fortemi/fortemi · docs.fortemi.com/server · window: August–December 2025 (pre-public origin).
- **Related surfaces:** HotM (the desktop app it was born inside) · fortemi-react (the browser build).
