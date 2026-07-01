---
template: post
title: "Fortémi — February 2026"
date: 2026-02-01
author: Fortémi Team
summary: "The month Fortémi grew up: keep separate memories that never mix, search in more of the world's languages, and let your agent read your pictures, audio, video, and 3D files — plus a tidier knowledge graph and live updates."
tags: [report, fortemi, "2026-02", agent-memory]
---

# Fortémi — February 2026

*Fortémi is the agent-memory server. It keeps notes for an AI agent and lets the agent search them by meaning, not just by exact words. You run it yourself, on your own machine, with one Docker command. This is the full server. HotM is a desktop app that uses it. fortemi-react is the same server built to run in a web browser.*

## TL;DR

February was a huge month. Fortémi learned to keep **separate memories** that never mix, so one project's notes stay apart from another's. Search got smarter about **more languages** — including Chinese, Japanese, and Korean writing (often shortened to "CJK"), plus emoji and symbols. Biggest of all, Fortémi can now **read your files**: it describes your pictures, writes out what people say in audio and video, and even looks at 3D models. It can tell apart who said what in a recording. The knowledge graph — the web of links between related notes — got a big cleanup, so notes now group into clear topics. And apps can watch changes as they happen. All of this shipped across a run of releases, from **v2026.2.0** to **v2026.2.13**, plus two new helper images.

## By the numbers

| What's public | Value |
|---|---|
| What it is | A self-hosted agent-memory server you run yourself |
| How to run it | One Docker command (the all-in-one bundle) |
| Released this month | v2026.2.0, v2026.2.9, v2026.2.10, v2026.2.11, v2026.2.12, and v2026.2.13, plus the GLiNER and speaker-labeling helper images (sidecar-gliner-v1, sidecar-pyannote-v1) |
| Built for | Agents (over the standard agent-tool protocol), custom apps, and team or offline setups |
| License | Free for personal use, learning, and trying it out; a commercial license covers production use |
| Source · image · docs | github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server |
| Desktop app · browser build | HotM (a desktop client) · fortemi-react (the browser build) |

## Highlights

**1. Keep separate memories that never mix.**
What it is: you can now create more than one memory. Each one is walled off from the others, so notes in one don't leak into another.
How you'd use it: keep a "work" memory and a "personal" memory apart, or give each project its own. Your agent picks which one to use, and can even search across several at once when you want.
Why it helps: your notes stay clean and organized, and a busy project never crowds out the rest.

**2. Search that understands more of the world.**
What it is: search now handles more writing systems — Chinese, Japanese, and Korean (CJK), plus emoji and symbols like ⭐ and arrows.
How you'd use it: search in the language you actually write in, or look up a note by an emoji you tagged it with.
Why it helps: you find what you meant, even when it isn't plain English.

**3. Your agent can now read your files.**
What it is: Fortémi can look at what you upload and turn it into notes it can search. It describes pictures, writes out the words in audio and video, and studies 3D models from several angles. It can also pull text out of emails, spreadsheets, and zip archives.
How you'd use it: drop in a recording, a photo, a slide deck, or a video, and let Fortémi make it searchable — no typing it up by hand.
Why it helps: your memory grows from the real files you already have, not just text you write.

**4. Tell apart who said what in a recording.**
What it is: after Fortémi writes out a recording, it can label the speakers — Speaker 1, Speaker 2, and so on. This is called "diarization." You can rename the speakers to real names.
How you'd use it: upload a meeting or interview and get captions that show who was talking.
Why it helps: a transcript is far more useful when you can follow the conversation.

**5. A tidier knowledge graph.**
What it is: the links between related notes used to pile up into one big, noisy blob. Now Fortémi groups notes into clear topics, trims the extra links, and keeps the ones that matter.
How you'd use it: explore your notes by topic and actually see the shape of what you know.
Why it helps: related ideas are easy to spot instead of buried in clutter.

**6. Big uploads that resume, and live updates.**
What it is: large files now upload in a way that can pick up where it left off if the connection drops. And apps can subscribe to a live feed of changes as they happen.
How you'd use it: upload a long video over a shaky link without starting over, and let your app show new notes the moment they land.
Why it helps: uploads finish reliably, and what you see stays fresh on its own.

## Features shipped

**Separate memories (v2026.2.9).** Fortémi can hold many memories at once, each fully walled off from the others. Your agent chooses one per request, or searches several together in a single query. You can copy a memory, back one up on its own, and restore it cleanly. How many you can run scales with your hardware.

**Search in more languages (early February releases).** Search now handles Chinese, Japanese, and Korean writing (CJK), and it finds emoji and symbols — including stars and arrows that used to be missed. Single-character searches in CJK work too.

**Reading your files (v2026.2.9 and v2026.2.12).** This is the big theme of the month. Fortémi turns your uploads into searchable notes:
- **Pictures** — it writes a plain-language description of each image.
- **Audio** — it transcribes recordings into text, with timestamps.
- **Video** — it grabs key moments, describes them, and lines them up with what's being said.
- **3D models** — it renders the model from several angles and describes what it sees.
- **Email, spreadsheets, and archives** — it pulls text out of `.eml`/`.mbox` email, turns each spreadsheet sheet into a table, and lists and reads the contents of zip and tar files.

**Speaker labeling (v2026.2.12).** After a recording is transcribed, Fortémi can tell the speakers apart and label them in the captions. You can rename them later. This runs in a helper container (a "sidecar" — a small program that runs alongside the main server) and is optional.

**A cleaner knowledge graph (v2026.2.10).** Fortémi now groups your notes into topics automatically, gives each group a readable label drawn from your tags, and prunes redundant links so the important connections stand out. It refreshes this on its own after new notes come in.

**Faster, smarter tagging (v2026.2.10).** A small, fast helper called GLiNER spots names and topics in your text in under a third of a second, and it runs on a plain processor — no graphics card needed. If it isn't sure, the work moves up to bigger models only when needed. That keeps tagging quick and cheap for most notes.

**Live updates (v2026.2.10).** Apps can subscribe to a live feed of what's changing — new notes, finished jobs, and more. Reconnecting apps can catch up on anything they missed, and the feed slows itself down under heavy load so nothing is lost.

**Media playback helpers (v2026.2.12).** When you upload video or audio, Fortémi can pre-build web-friendly versions so they stream smoothly, make small preview clips, and create seek-bar thumbnail strips for video. Large files can also be downloaded in pieces.

**Job controls (v2026.2.10).** You can pause and resume background work — for the whole server or one memory at a time — and the setting sticks across restarts.

**Plug in different AI providers (v2026.2.10).** You can point Fortémi at different model backends by name, and ask it which ones are available.

## Fixes

Search fixes made the biggest difference for everyday use. Emoji searches for stars, arrows, and similar symbols now work. Single-character searches in Chinese, Japanese, and Korean now return results. A `limit=0` request now returns an empty list instead of everything.

Reliability got safer too. The server now recovers jobs that got stuck, so work no longer stalls forever. It reads old, quirky PDFs (the kind from decades-old software) without choking on them. Video description jobs that used to be quietly dropped when no image model was set up now wait patiently and run once one is available. And a naming mix-up in audio transcript results was cleaned up so apps read them consistently.

## Performance & reliability

Reliability was a real theme this month. Background jobs now wake up the moment there's work instead of checking on a timer, so things happen sooner. The server reaps stuck jobs on startup and retries them. Heavy work is split into tiers, so simple jobs stay on a fast, cheap path and only hard ones reach the big models. The fast tagging helper runs on a plain processor, keeping most work off the graphics card. Big uploads resume after a drop, and large downloads can arrive in pieces.

## Breaking changes & migrations

Database updates run on their own when you start the new version — you don't have to do anything. A few things are worth knowing:

- The fast tagging helper (GLiNER) is on by default. To turn it off, set its address to empty.
- Speaker labeling needs its own helper container and is only active when you set it up.
- Apps that read the live update feed should switch to the new, versioned format. The old format is replaced.
- The default number of topics pulled from each note was lowered to keep tags tighter.
- The web framework under the hood was upgraded (axum 0.8). This is internal only — it does not change anything for apps that call Fortémi.

## Releases

- **v2026.2.0** (Feb 3) — Steadier releases and the start of file attachments. Automated tests were made reliable, and you could attach files to notes with automatic duplicate detection.
- **Early point releases** (early Feb) — A run of small fixes: better tag matching, and the first rounds of emoji and CJK search fixes.
- **v2026.2.9** (Feb 16) — The big one. Separate memories, a database upgrade with stronger password security, and the ability to read your files (pictures, audio, video, and 3D models). It also ships with a built-in help library loaded the first time you start it.
- **v2026.2.10** (Feb 19) — A cleaner knowledge graph, a live update feed, and the fast, low-cost tagging helper (GLiNER).
- **v2026.2.11** (Feb 20) — Reliability: the server recovers stuck jobs on its own and reads old, quirky PDFs without choking.
- **v2026.2.12** (Feb 22) — The full media pipeline: resumable big uploads, speaker labeling, ready-to-stream video and audio, plus reading email, spreadsheets, and zip archives.
- **v2026.2.13** (Feb 23) — The heavy add-on images now build and ship on their own, so regular updates are faster.
- **sidecar-gliner-v1 and sidecar-pyannote-v1** (Feb 23) — The two helper images — fast tagging and speaker labeling — got their own version tags, so you can update them without rebuilding everything.

All published as the server's Docker image at ghcr.io/fortemi/fortemi.

## Dependencies & security

Security got real attention. The database moved up to PostgreSQL 18 with stronger password protection. Fortémi ships closed by default: every private endpoint needs a valid token unless you explicitly opt out for local use. You can create API keys for your own tools. The heavy add-ons — fast tagging and speaker labeling — run as separate helper containers, which keeps the main server lean. The web framework under the hood was also upgraded to a current version.

## Docs & developer experience

The server's documentation lives at **docs.fortemi.com/server**. This month it gained a media integration guide with ready-to-use examples for uploads, streaming playback, captions, and the live update feed, plus a guide for running without a graphics card. Fortémi also comes with a built-in help library loaded the first time you start it, so answers are right there in your own memory. A quickstart makes a zero-config setup easy.

## Tests & CI

A large automated test pass checked the agent tools against a real database, so the trickier promises — like memories staying fully separate — are verified, not assumed. The build system was also split so the heavy machine-learning helper images build and release on their own. That keeps regular releases fast and means a helper update no longer forces a full rebuild.

## Cross-project impact

- **HotM** (the desktop app) bundles this very server inside it, so everything above flows straight into the desktop experience — separate memories, file reading, and live updates included.
- **fortemi-react** (the browser build of this server) shares the same memory model, running in a web browser. See its own report.
- **Agents and the wider stack** read and write memory through Fortémi's standard agent tools. The new file-reading, separate memories, and live updates give agents a richer, tidier memory to work with.
- **Pagenary** (the publishing tool) builds the Fortémi docs site.

## Known issues & open threads

- Speaker labeling needs its own helper container and a graphics card. It's off until you set it up.
- Reading 3D models needs a renderer and an image model configured. Without them, those jobs wait.
- Image and video description jobs also wait until an image model is set up, then run on their own.
- Some of the heaviest features (video, 3D, speaker labeling) work best on a machine with a graphics card. There's a CPU-only guide for lighter setups.

## What's next

Keep improving the knowledge graph so topics stay clear as your notes grow. Keep widening what Fortémi can read from your files. And keep the three surfaces — the server, the browser build, and the desktop app — moving in step so a feature here shows up there too.

## Appendix

- **What it is:** a self-hosted agent-memory server, built in Rust, run with one Docker command.
- **Released this month:** v2026.2.0, v2026.2.9, v2026.2.10, v2026.2.11, v2026.2.12, and v2026.2.13, plus the GLiNER and speaker-labeling helper images (sidecar-gliner-v1, sidecar-pyannote-v1), published as the server's Docker image.
- **License:** free for personal use, learning, and trying it out; a commercial license covers production use.
- **Source · image · docs:** github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server · window: all of February 2026.
- **Related surfaces:** HotM (desktop client) · fortemi-react (browser build).
