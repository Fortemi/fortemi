---
template: post
title: "Fortémi — May 2026"
date: 2026-05-01
author: Fortémi Team
summary: "A safety-and-listening month for the memory server: it now asks for a key by default, it can turn live phone calls into searchable memory, and a one-command dev setup plus easy AI-engine switching make it simpler to run."
tags: [report, fortemi, "2026-05", agent-memory]
---

# Fortémi — May 2026

*Fortemi is the agent-memory server. It keeps notes for an AI agent and lets the agent search them by meaning, not just by exact words. You run it yourself, on your own machine, with one Docker command. This is the full server. HotM is a desktop app that uses it. fortemi-react is the same server built to run in a web browser (see its own report).*

## TL;DR

May had two big stories. First, the server got safer to run out of the box. It now asks for a key before it answers, unless you clearly turn that off for local use. Second, it learned to listen: Fortemi can now sit on a live phone call and turn what people say into searchable memory, as they speak. On top of those, the month made the server easier to set up and easier to point at the AI engine of your choice. There was also a one-command developer setup that brings Fortemi, its desktop app, and a local model up together. And the very first boot got faster and gentler — no more pinning your graphics card for hours. All of this shipped across a run of releases, from **v2026.5.0** to **v2026.5.13**.

## By the numbers

| What's public | Value |
|---|---|
| What it is | A self-hosted agent-memory server you run yourself |
| How to run it | One Docker command (the all-in-one bundle) |
| Released this month | v2026.5.0 through v2026.5.13 — "safe by default, and it can listen" |
| Built for | Agents (over the standard agent-tool protocol), custom apps, and team or offline setups |
| License | Free for personal use, learning, and trying it out; a commercial license covers production use |
| Source · image · docs | github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server |
| Desktop app · browser build | HotM (a desktop client) · fortemi-react (the browser build) |

## Highlights

**1. Your server now asks for a key by default.**
What it is: before this month, a fresh server would answer anyone who could reach it. Now it wants a Bearer token first — a secret key you attach to each request, like a password on a badge. If you don't want that on your own laptop, you can turn it off, but you have to say so on purpose.
How you'd use it: run the server as before. To reach it, send your key with each request. For a private, single-person setup, you can flip two clearly named switches to allow no-key mode.
Why it helps: a server you can reach over a network shouldn't answer strangers. This makes the safe choice the default, so you can't leave the door open by accident.

**2. Live phone calls become searchable memory.**
What it is: Fortemi can now sit on a live phone call and turn the talking into text as it happens (that's speech-to-text). When the call ends, it also makes a cleaner, higher-quality transcript from the recording.
How you'd use it: connect a phone/voice service and a speech-to-text service, and your calls flow into memory. You get fast, live text during the call and a polished version afterward.
Why it helps: important conversations don't get lost. They become notes you can search by meaning later, not just an audio file you'll never replay.

**3. A one-command developer workstation.**
What it is: a single command brings up Fortemi, its desktop app, and a local AI model together, in containers. It comes with a friendly wrapper and a pre-flight check that catches common setup mistakes.
How you'd use it: if you're building or testing against Fortemi, run one command and get a working stack — no fiddling with several separate setups. If you don't want the desktop app, there's a backend-only mode.
Why it helps: you go from "nothing installed" to "everything running" in minutes, even if you've never touched Docker.

**4. Pick your own AI engine, the easy way.**
What it is: Fortemi can send its thinking to different AI providers. This month a short wizard and a simple settings file made switching between them a 30-second job — no editing container files.
How you'd use it: run the wizard, pick your provider, paste a key if needed. You can even use one provider for chat and a different one for the "search by meaning" part.
Why it helps: you're not locked to one engine. Use a cloud model for chat and a local one for search, or swap engines whenever your needs change.

**5. A faster, gentler first boot.**
What it is: the built-in help documents used to run the full AI pipeline the moment you started the server — which could pin your graphics card for hours on a small machine. Now search works right away, and the heavy work is optional.
How you'd use it: start the server and search the built-in docs immediately. Turn on the deeper "search by meaning" for those docs only when you want it.
Why it helps: the first five minutes with Fortemi feel light and fast, not like a machine grinding in the background.

**6. No more silent file loss.**
What it is: the server stores attachments (like images and recordings) as files. A rare failure could leave a record pointing at a file that was never fully written. That path was hardened.
How you'd use it: nothing to do — you just get a server that's honest about its files. If a file really is missing, you get a clear "this is gone" message instead of a confusing error.
Why it helps: you can trust that what the server says it saved is actually on disk.

## Features shipped

**Safe by default (v2026.5.12).** Authentication is the standard way a server checks who's asking. Fortemi now turns it on by default for its main endpoints. To run without a key — fine for a private setup on your own machine — you must set two clearly named switches together, on purpose. A server built for multiple separate users or teams (that's "multi-tenant") refuses the no-key mode entirely. A few endpoints stay open no matter what, like the health check and the sign-in pages, so the server can still be started and checked.

**Live phone calls into memory (v2026.5.12).** This release laid the foundation for real-time voice. It brings the first phone-service adapter (Twilio Voice) and a live speech-to-text engine (Deepgram). During a call, live text is produced word by word, with automatic reconnects and health readouts if the link stutters. When the call's recording is ready, Fortemi pulls it in as an attachment and runs it through the mature audio pipeline for a higher-quality final transcript. The design keeps each phone service's own quirks tucked inside its adapter, so other voice services can be added later without changing everything downstream.

**Pick your own AI engine (v2026.5.4, v2026.5.7–v2026.5.10).** Fortemi gained first-class support for its four inference providers behind a single, tidy catalog. That made several things possible at once:

- **Route chat and search to different engines.** Use one provider for chat and another for the "search by meaning" work. Handy when your chat provider can't do search-style embeddings.
- **Change engines safely.** You can test a new setup without saving it, or have the server check every changed engine first and refuse the switch if any of them fail — so a bad key never takes your server half-down.
- **See what changed.** Every engine change is written to an audit record, and live dashboards get a quiet signal so they can refresh without constant polling.
- **A one-command dev setup with a wizard.** The developer workstation added a short wizard that walks you through five common engines and writes a simple settings file. A follow-up fixed a rough edge with one engine (it needed the name you serve the model under, not the download path) and the docs were threaded through so new users actually find the wizard.

**A faster, gentler first boot (v2026.5.3, v2026.5.5, v2026.5.6).** The built-in help archive got a careful overhaul. On first boot it now loads for exact-word search right away — no waiting on an AI engine, no long grinding pass. The deeper "search by meaning" over those docs is a one-command opt-in. In the Docker bundle, that help archive no longer loads itself unless you ask; that matches how the from-source build always behaved. The server's build now rebuilds this help archive fresh every time, so it can't drift out of date. And imported notes can carry their own titles, so nothing lands untitled.

## Fixes

- **Silent attachment data loss (v2026.5.0).** The path that writes attachment files was hardened so a record can't outlive its file. Leftover half-written files are swept away at startup, and a missing file now returns a clear "this is gone" message instead of a vague error.
- **Help archive had no titles, and pinned the graphics card (v2026.5.6).** Imported help notes now get real titles. And importing that archive no longer fires off a flood of background jobs that could tie up your graphics card for hours on a small machine.
- **A rough edge switching to one AI engine (v2026.5.9).** The engine wizard now asks for the exact name you serve your model under, so the very first chat call works instead of failing.
- **A build-tooling mismatch (v2026.5.0).** An internal build image was updated so it can talk to current Docker hosts, unblocking the test and release pipelines.

## Performance & reliability

Reliability showed up in the small stuff. The first boot no longer runs the heavy AI pass, so exact-word search over the built-in docs works instantly and your machine stays quiet. The live speech-to-text path reconnects on its own and can fall back to a backup engine if its main one drops, with health readouts you can watch. Switching AI engines can be checked before it's committed, so a bad setting can't take the server half-down. And the built-in help archive is rebuilt fresh with every server build, so it never drifts stale.

## Breaking changes & migrations

There is one behavior change to know about.

**Authentication is now on by default.** If you were running an older server that answered without a key, the new server will expect one. If you truly want the old no-key behavior on a private machine, you must set both clearly named acknowledgment switches together; otherwise the server refuses to start rather than run wide open. A server set up for multiple separate users or teams will not allow the no-key mode at all. The plain migration path: create a key for each app that talks to your server, add that key to each request, then remove the no-key switches (or set the "require a key" switch on) and restart.

Two smaller notes. New storage tables — for the incoming call sessions and the message receivers — are created on their own when you upgrade; run migrations as usual. And in the Docker bundle, the built-in help archive no longer loads itself on first boot. If you want it, turn it on with one setting or run the one-command seed on a running server.

## Releases

- **v2026.5.0** (May 3) — Hardened the attachment file path against silent data loss, fixed a build-tooling mismatch, and put the HotM desktop app front and center in the docs.
- **v2026.5.1** (May 9) — Added a macOS Intel build of the API and wired up the docs site's own build-and-publish pipeline.
- **v2026.5.2** (May 9) — Deployment comfort: a bundled llama.cpp engine option, a tiny "just make it run" overlay for small hosts (about 2 GB idle), and "bring your own LLM" recipes in the README.
- **v2026.5.3** (May 10) — Reworked the built-in help archive so first boot is exact-word-search-ready right away, with no long AI pass; the archive is now rebuilt fresh on every server build.
- **v2026.5.4** (May 10) — First-class support for all four AI providers behind one catalog: route chat and search to different engines, test or safely check changes before committing, and get an audit record of every engine change.
- **v2026.5.5** (May 10) — In the Docker bundle, the built-in help archive is now opt-in on first boot, matching the from-source build.
- **v2026.5.6** (May 10) — Imported notes can carry their own titles, and importing the help archive no longer floods the machine with background jobs.
- **v2026.5.7** (May 18) — A one-command developer workstation: Fortemi, its desktop app, and a local model brought up together, with a friendly wrapper and a pre-flight check.
- **v2026.5.8** (May 18) — A short wizard and a simple settings file to pick your AI engine — five common options — with no container-file editing.
- **v2026.5.9** (May 18) — Fixed a rough edge with one engine so the first chat call works, and expanded the example settings.
- **v2026.5.10** (May 18) — Threaded the engine wizard and settings through the main docs so new users find them.
- **v2026.5.11** (May 18) — A clean maintenance tag that lands identically on both code mirrors. No behavior change.
- **v2026.5.12** (May 25) — The realtime milestone: live phone-call transcription (Twilio Voice + Deepgram), a higher-quality post-call transcript, secure message receivers, and authentication on by default.
- **v2026.5.13** (May 25) — A security maintenance release that updated a few underlying libraries flagged by advisories.

Every version this month is published as the server's Docker image.

## Dependencies & security

Security was a real theme. The headline is authentication on by default, described above. On top of that: incoming call and message data is checked with signatures to prove it really came from the sender; the live speech-to-text engine handles its API key securely; and every change to an AI engine is written to an audit record. A late-month release also updated a few underlying libraries that security advisories had flagged, and new checks were added to the build so those advisories get caught before code merges. One responsibility stays with you, the operator: recording and transcribing calls has consent and legal rules, and the setup guide walks through them.

## Docs & developer experience

The server's documentation lives at **docs.fortemi.com/server**. This month the docs site got its own build-and-publish pipeline, so changes go live cleanly and broken links get caught before they ship. New guides landed for the real-time phone setup and for the one-command developer workstation, and the "bring your own LLM" recipes make it easy to point Fortemi at the engine you prefer. There's also a plain, step-by-step setup file aimed at AI-driven installs.

## Tests & CI

New tests cover the month's big pieces: the real-time call transport, the live speech-to-text path (including reconnects and fallback), the message receivers, and the call-session records. The build now runs security-advisory checks for both the Rust and the JavaScript parts, so known-risky libraries get flagged before merge. And the built-in help archive is rebuilt inside the pipeline on every publish, with a size sanity check that fails loudly rather than shipping a stale archive.

## Cross-project impact

- **HotM** (the desktop app) is bundled straight into the new developer workstation, so the server's work flows right into the desktop experience for anyone building against it.
- **Agents and the wider stack** read and write memory through Fortemi's standard agent tools. The new real-time voice path means a live phone call can now become memory an agent can search.
- **fortemi-react** (the browser build of this server) shares the same memory model, running fully in a web browser. See its report.
- **Pagenary** (the publishing tool) builds the Fortemi docs site, which gained its own pipeline this month.

## Known issues & open threads

- The real-time work ships with its first phone-service adapter (Twilio Voice). More voice services can be added on the same foundation over time.
- You can store a per-workspace AI-engine setting today, but live per-workspace routing at request time is a planned follow-up.
- The no-key local mode is meant only for a private machine. On any server others can reach, keep authentication on.
- Real-time voice is a new area. Expect more tuning as more people run live calls through it.

## What's next

Build on the real-time foundation: keep the live phone path steady and open the door to more voice services. Continue the "pick your own engine" work toward live per-workspace routing. And keep the three surfaces — the server, the browser build, and the desktop app — moving in step. The next month leans hard into getting data *into* the server smoothly and safely.

## Appendix

- **What it is:** a self-hosted agent-memory server, built in Rust, run with one Docker command.
- **Released this month:** v2026.5.0 through v2026.5.13, each published as the server's Docker image.
- **License:** free for personal use, learning, and trying it out; a commercial license covers production use.
- **Source · image · docs:** github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server · window: all of May 2026.
- **Related surfaces:** HotM (desktop client) · fortemi-react (browser build).
</content>
</invoke>
