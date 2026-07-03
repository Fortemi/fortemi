---
namespace: fortemi
name: fortemi-monthly-report
platforms: [all]
description: Author a Fortémi monthly report as a docsite blog post — gather what shipped that month from git history, CHANGELOG, and release announcements, then write it up FOR USERS in plain language and publish it to the Pagenary blog collection.
commandHint:
  argumentHint: "<YYYY-MM> [--dry-run]"
  allowedTools: Read, Write, Edit, Bash, Glob, Grep
  category: documentation
---

# Fortémi Monthly Report (docsite blog)

You author Fortémi's monthly report as a public blog post on the docs site. It is a
user-facing narrative of what shipped that month — not a changelog dump.

## Natural language triggers

- "write the <month> Fortémi report"
- "create the monthly blog post for <month>"
- "monthly report for 2026-05"

## Where it goes

- **Post file:** `docs/content/posts/{{YYYY-MM}}-fortemi.md` (Pagenary `posts` collection, `route: /blog`, `layout: blog` — see `docs/config.json` → `collections`). Pagenary ≥ 2026.7.11 auto-materializes collection posts for our manifested tenant, so just dropping the file here is enough — no manifest edit needed.
- **Template:** `.aiwg/templates/fortemi-monthly-report.md.tmpl` — copy it and fill every section.
- **Gold-standard reference:** the June post `docs/content/posts/2026-06-fortemi.md` (brought in from the strategy repo `roctinam/strategy:marketing/monthly/2026-06/fortemi.md`). When in doubt, match its structure and voice.

## Steps

1. **Resolve the window** from the `<YYYY-MM>` argument (first day → last day of that month).
2. **Gather what shipped (research input — to understand, not to publish):**
   - Releases + dates: `git for-each-ref --sort=creatordate --format='%(creatordate:short) %(refname:short)' refs/tags | grep 2026-MM` (v-tags only).
   - Release notes: `docs/releases/v<version>-announcement.md` (the authoritative public narrative) and the matching `CHANGELOG.md` `## [<version>]` sections.
   - Commit context: `git log --since=<start> --until=<end>` grouped by conventional type / subsystem — to *understand* the work.
   - Docs touched: `git log --since --until -- docs/`.
3. **Translate to user-facing benefits.** For every feature/theme write **What it is · How you'd use it · Why it helps** in plain words.
4. **Fill the template** into `docs/content/posts/{{YYYY-MM}}-fortemi.md`. Every section present; use "None this month." where empty. A quiet month (no release) is reported honestly.
5. **Verify:** `npx pagenary build:tenants fortemi-docs` builds clean (strictLinks green) and `DOCS_CONTRACT_MODE=blocking npm run docs:contract -- --profile=hosted_strict` returns `new=0`. If `tools/readability.mjs` is available, target Flesch-Kincaid ≤ 8.

## Hard rules (from the report spec)

- **Reading level ~6th grade.** Short sentences, common words, active voice, say "you." Explain jargon (RRF, BM25, pgvector, MCP, KDF, CalVer, HMAC) on first use.
- **Public-facing facts only.** NO commit/PR/issue counts, NO internal `#N` citations (the tracker is internal), NO release totals or "N fixes" tallies, NO "→ latest" ranges. Name released versions + the Docker image; state no totals.
- **Public sources only:** `github.com/Fortemi/fortemi`, `ghcr.io/fortemi/fortemi`, `docs.fortemi.com/server`.
- **No AI attribution** anywhere (commit, post body, or comments).
- **Completeness by default:** never silently omit a section.
- **Frontmatter** is the Pagenary blog form: `template: post`, `title`, `date` (month first day), `author: Fortémi Team`, `summary`, `tags: [report, fortemi, "YYYY-MM", agent-memory]`. Do not set `draft` (docsite posts are published).

## Product framing (reuse)

Fortémi is the agent-memory server: it keeps notes for an AI agent and lets the agent search
them by meaning, not just exact words. You run it yourself with one Docker command. HotM is a
desktop app that uses it; fortemi-react is the same server built to run in a browser.

## References

- `.aiwg/templates/fortemi-monthly-report.md.tmpl` — the section skeleton + rules.
- `docs/content/posts/2026-06-fortemi.md` — gold-standard example.
- `docs/config.json` → `collections` — the blog wiring.
- `docs/releases/` + `CHANGELOG.md` — per-month source material.
