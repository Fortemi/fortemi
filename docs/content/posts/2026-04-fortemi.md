---
template: post
title: "Fortémi — April 2026"
date: 2026-04-01
author: Fortémi Team
summary: "An inference-focused month: bring your own AI key, pick a local or cloud provider, and watch answers stream back word by word — on the first big catch-up release, now running on modest graphics cards."
tags: [report, fortemi, "2026-04", agent-memory]
---

# Fortémi — April 2026

*Fortemi is the agent-memory server. It keeps notes for an AI agent and lets the agent search them by meaning, not just by exact words. You run it yourself, on your own machine, with one Docker command. This is the full server. HotM is a desktop app that uses it. fortemi-react is the same server built to run in a web browser (see its own report).*

## TL;DR

April was the inference month. Fortemi got much better at working with AI models — the part of the system that reads, writes, and answers questions. You can now hand it your own AI key for a single request, and it uses that key without ever saving it. You can pick which AI answers you: one running on your own machine, or a cloud service. When you ask a question, the answer can now arrive a few words at a time instead of all at once. The server also checks on its AI helper in the background, so if the helper drops and comes back, Fortemi notices on its own — no restart needed. All of this landed across three releases: **v2026.4.0**, **v2026.4.1**, and **v2026.4.2**. The first of the three was a big catch-up release. It also modernized the built-in model and made the whole thing run on a modest graphics card.

## By the numbers

| What's public | Value |
|---|---|
| What it is | A self-hosted agent-memory server you run yourself |
| How to run it | One Docker command (the all-in-one bundle) |
| Released this month | v2026.4.0, v2026.4.1, and v2026.4.2 — an inference-focused run of releases |
| Built for | Agents (over the standard agent-tool protocol), custom apps, and team or offline setups |
| License | Free for personal use, learning, and trying it out; a commercial license covers production use |
| Source · image · docs | github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server |
| Desktop app · browser build | HotM (a desktop client) · fortemi-react (the browser build) |

## Highlights

**1. Bring your own AI key.**
What it is: an "AI provider" is the service that actually writes the answers. It might run on your own machine, or it might be a cloud service. You can now hand Fortemi your own cloud key for a single request. Fortemi uses it that one time and never stores it.
How you'd use it: point an app at Fortemi, pass your key in the request, and get an answer back — no need to save your key on the server.
Why it helps: you keep control of your own key. Nothing sensitive sits on the server waiting to leak.

**2. Watch the answer stream in, word by word.**
What it is: when you ask for a completion, the reply can now arrive a few words at a time. This uses a simple one-way live feed from the server (called SSE, or "server-sent events"). Each piece of text — roughly a word — is called a token, and Fortemi can send one token at a time.
How you'd use it: ask a question and start reading right away, instead of waiting for the whole reply to finish.
Why it helps: it feels fast and alive, and you can tell it's working.

**3. Pick your AI — local or cloud.**
What it is: Fortemi now speaks to several AI services. You can use one on your own machine (Ollama or llama.cpp) or a cloud one (OpenAI or OpenRouter). You choose per request.
How you'd use it: run a private model at home for everyday work, and switch to a cloud model when you want more power — same server, same notes.
Why it helps: you're not locked to one AI. You match the model to the job, and you can go fully offline if you want.

**4. The server heals itself when your AI comes back.**
What it is: if your AI service is down when Fortemi starts, Fortemi used to mark it "offline" and stay stuck there until you restarted. Now it quietly re-checks in the background. When the service returns, Fortemi flips back to "online" on its own — and it can tell connected apps the moment that happens.
How you'd use it: nothing to do — start your AI service whenever, and Fortemi picks it up within about half a minute.
Why it helps: no more restarting the server just because a helper blinked out for a bit.

**5. Tell Fortemi exactly which AI steps to run on a note.**
What it is: when you save a note, Fortemi normally runs several AI steps on it — cleaning it up, giving it a title, tagging it, and more. Now you can pick which steps run, or skip them all.
How you'd use it: save a note and ask for tagging only, or save it raw with no AI touching it at all.
Why it helps: you get exactly the processing you want. No surprises, no wasted work, and you can store something untouched when that's what you need.

**6. Runs on a modest graphics card now.**
What it is: the first April release added ready-made setups for different sizes of graphics card, and switched the built-in model to a newer one that handles text and images in a single load.
How you'd use it: pick the setup that matches your card — a common 6–8 GB card is enough for the default — and start.
Why it helps: you don't need expensive hardware to run your own memory server.

## Features shipped

**Working with AI providers (v2026.4.0 through v2026.4.2).** This was the month's main story: giving you more control over the AI that reads and answers.

- **Bring-your-own-key endpoints.** Three new endpoints let an app drive AI completions through Fortemi. One asks for a plain answer, one streams the answer back live, and one lists the AI services Fortemi knows about and whether each needs a key. Keys you pass in are used once and never saved. This also lets a web app reach a cloud AI *through* Fortemi. Browsers normally block a web page from calling another site directly — a safety rule called CORS — so passing the call through Fortemi is exactly what a browser build needs.
- **Real word-by-word streaming.** For a local Ollama model, the streaming endpoint now sends one piece of text at a time as the model writes it, instead of holding the whole reply until the end. Other AI services still send the reply in one piece for now.
- **Pick your provider.** Fortemi now works with Ollama and llama.cpp (on your own machine) and OpenAI and OpenRouter (in the cloud). You can pick one per request. A newer local model, added in the first release, handles both text and images from a single load, so you don't juggle separate models.
- **Direct chat with your memory.** A chat endpoint lets you have a back-and-forth conversation, with history, and choose which model answers. It shares the graphics card carefully so a chat can't crowd out the background work that keeps your memory fresh.
- **Self-healing availability.** A background check re-tests your AI service on a set schedule. The server's health report always shows the true, current state, and connected apps get a live "it's back" (or "it's gone") signal so they can raise or clear an offline banner without constantly asking.
- **Choose the AI steps per note.** Saving a note can now run all of the usual AI steps, just the ones you name, or none at all.

**Media understanding (v2026.4.0).** The big first release also brought richer handling of audio, video, and 3D files.

- **Long audio splits up for transcription.** Long recordings are cut into pieces and transcribed in parallel, then stitched back together.
- **Type-aware cleanup.** When Fortemi tidies a note, the result now fits the kind of thing it is — a meeting gets decisions and action items, a movie gets a synopsis and cast.
- **Better video and 3D understanding.** Video key moments and 3D model views are described one at a time, so each piece can be retried on its own if something goes wrong.

**Running it your way (v2026.4.0, v2026.4.1).** Setup got friendlier.

- **Ready-made hardware setups.** Pick a setup that matches your graphics card. The default targets common 6–8 GB cards.
- **A guided installer.** A set of step-by-step scripts walks you through cloning, configuring, pulling models, checking ports, and starting up. A companion setup file can even prepare several AI providers at once.
- **Bigger, adjustable archive limits.** When Fortemi pulls files out of a ZIP or tar archive, the size limits are now much larger by default and you can set them yourself.

## Fixes

- **No more fake edit history.** Saving a note with AI cleanup turned off used to leave a misleading "edited" record even though nothing was changed. That empty record is gone; the note's searchable text still stays in sync.
- **The server no longer hangs waiting on an optional helper.** If the optional in-memory helper (Redis) wasn't reachable, startup could stall. It now gives up after a few seconds, and the standard bundle simply leaves that helper off.
- **Cleaner streaming.** Some models "think out loud" in a hidden block before answering. That block used to freeze the live stream until it finished. It's now turned off during streaming, so words start flowing right away.
- **The public image stopped being masked.** Everyday development builds were quietly overwriting the "latest" label on the public Docker image, hiding real releases behind unfinished work. That label is now reserved for real releases only.
- **Builds work on more chip types.** New build switches let you leave out parts that don't compile on certain processors (like Arm), so the image builds cleanly across more machines.
- **Sturdier media and 3D output.** Fixes stopped blank or grey 3D thumbnails, corrected AI cleanup on non-default memory archives, and firmed up resumable uploads. A memory limit for one AI helper was raised so it stops running out of room.

## Performance & reliability

Reliability got real attention. The AI connection now recovers on its own: a background check brings a provider back "online" without a restart. The AI helpers — for transcription, speaker labeling, and tagging — gained retries, a cool-off when something keeps failing, and memory limits so they don't overwhelm the machine. Graphics-card work is scheduled one job at a time by default so different tasks don't fight over the card. Startup no longer hangs on an optional helper. And the built-in time limit for long jobs was raised so big videos finish instead of getting cut off.

## Breaking changes & migrations

A few things to know when you upgrade:

- **Graphics-card helpers now run on the processor by default.** Transcription and speaker-labeling helpers moved off the graphics card by default, to leave room for the main AI. If you were running them on the card, pick the matching hardware setup (`COMPOSE_PROFILES=gpu-12gb` or `gpu-24gb`) to keep that behavior.
- **Streaming now sends many small pieces.** The streaming endpoint used to send one chunk; for local models it now sends many, one per word-piece. An app that expected a single chunk should be updated to read the stream piece by piece.
- **Archive size limits changed.** The default limits for pulling files out of an archive are much higher now. If you relied on the old, smaller limits as a safety cap, set them yourself.
- **The "latest" image label changed meaning.** It now points to real releases only. If you pinned "latest" for everyday development builds, switch to the development label instead.

The final April release adds no new database tables, so upgrading it needs no data migration.

## Releases

- **v2026.4.0** (Apr 3) — the big catch-up release and the first in a while. It added direct chat with your memory, a choice of AI providers (local and cloud), runtime AI settings you can change without a restart, ready-made hardware setups for modest cards, a newer text-and-image model, richer audio/video/3D understanding, sturdier AI helpers, and a guided installer.
- **v2026.4.1** (Apr 12) — let you choose which AI steps run on each note, made archive size limits adjustable, added a way to set up several AI providers at once, redesigned the project's front page, added a feature-and-hardware guide, and fixed the startup hang and the fake edit-history record.
- **v2026.4.2** (Apr 22) — added the bring-your-own-key endpoints, real word-by-word streaming for local models, the self-healing provider check with live "online/offline" signals, and reserved the "latest" image label for real releases.

All three were published as the server's Docker image at ghcr.io/fortemi/fortemi.

## Dependencies & security

Security showed up in a few practical ways. Keys you bring for a single request are never stored on the server — they're used once and dropped. Passing cloud AI calls through Fortemi means a web app doesn't have to hold those keys in the browser. The big first release also patched known security problems in its building blocks. And the "latest" image label was locked to real releases, so you can trust what you pull.

## Docs & developer experience

The server's documentation lives at **docs.fortemi.com/server**. April's docs work made getting started clearer. The project's front page was redesigned with a plain problem-and-solution framing, a picture of how a note flows from ingest to search, tables of the available endpoints, and examples for picking a provider. A new feature-and-hardware guide maps each feature to the graphics-card size it needs. The front page also makes the choice clear: want a ready-to-use app on your own computer? Get **HotM**. Want a headless server for agents, custom apps, teams, or offline use? Run Fortemi itself — it's the Docker server; the desktop installer lives with HotM.

## Tests & CI

New tests cover the month's inference work, including the key-passing logic behind the bring-your-own-key endpoints and the contract the desktop app relies on for chat. The build pipeline itself was hardened: the step that verifies the test database was fixed to check against the right database user and to wait for the server to be ready first, and a flaky diagnostic step was removed. The publish step now reserves the "latest" image label for real releases, so an everyday build can't quietly mask one.

## Cross-project impact

- **HotM** (the desktop app) bundles the very same Fortemi server inside it, so April's inference work flows straight into the desktop app: direct chat, the choice of AI providers, and word-by-word answers for local models. The contract that HotM relies on for chat is now covered by tests.
- **fortemi-react** (the browser build of this server) benefits directly from the bring-your-own-key endpoints. Because they let a web page reach a cloud AI *through* Fortemi, they sidestep the browser rule that would otherwise block those calls — exactly what a browser build needs.
- **Agents and the wider stack** read and write memory through Fortemi's standard agent tools, so the new provider choices and streaming give agents more ways to get answers.
- **Pagenary** (the publishing tool) builds the Fortemi docs site.

## Known issues & open threads

- Word-by-word streaming works for the local Ollama model today. Cloud services (OpenAI, OpenRouter) still send the reply in one piece; live streaming for them is the next step.
- Bringing your own key works for plain completions and the local streaming path. Live streaming with a brought key for cloud services is part of that same follow-up.
- The self-healing provider check re-tests on a set schedule (about every half minute by default). On very stable setups you can slow it down to cut overhead.

## What's next

Extend real word-by-word streaming to the cloud AI services, not just the local one. Keep improving the bring-your-own-key experience so apps can prompt for exactly the keys they need. Bring the new streaming and provider choices into the desktop app. And keep the three surfaces — the server, the browser build, and the desktop app — moving in step.

## Appendix

- **What it is:** a self-hosted agent-memory server, built in Rust, run with one Docker command.
- **Released this month:** v2026.4.0, v2026.4.1, and v2026.4.2 — an inference-focused run of releases, published as the server's Docker image.
- **License:** free for personal use, learning, and trying it out; a commercial license covers production use.
- **Source · image · docs:** github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server · window: all of April 2026.
- **Related surfaces:** HotM (desktop client) · fortemi-react (browser build).
</content>
</invoke>
