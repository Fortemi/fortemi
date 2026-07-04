---
template: post
title: "Fortémi — June 2026"
date: 2026-06-01
author: Fortémi Team
summary: "A big building month for the memory server: data can now stream IN — live chat replies, signed webhooks, resumable uploads, and outside event feeds — and a wide security and privacy pass made the logs and errors stop leaking sensitive details."
hero: https://docs.fortemi.com/server/assets/blog/2026-06-fortemi.png
tags: [report, fortemi, "2026-06", agent-memory]
---

# Fortémi — June 2026

*Hero image: AI-generated with ChatGPT from a brand-specified prompt; no text or logos are AI-rendered.*

*Fortemi is the agent-memory server. It keeps notes for an AI agent and lets the agent search them by meaning, not just by exact words. You run it yourself, on your own machine, with one Docker command. This is the full server. HotM is a desktop app that uses it. fortemi-react is the same server built to run in a web browser (see its own report).*

## TL;DR

June was a busy building month for the server. The headline: Fortemi got much better at taking data **in**. You can now chat with your memory and watch the reply appear word by word. Other apps can send updates straight in through a secure drop-box. A busy agent can pour in a long stream of notes without losing any. Big file uploads pick up where they left off if the connection drops. Fortemi can even listen to outside event feeds and fold them into your memory on its own. All of this shipped in one release, **v2026.6.0**. After that, the team did a wide safety pass. The server's logs and error messages no longer leak private details. Sensitive actions now leave an audit trail. And the server refuses to start if it's set up in an unsafe way.

## By the numbers

| What's public | Value |
|---|---|
| What it is | A self-hosted agent-memory server you run yourself |
| How to run it | One Docker command (the all-in-one bundle) |
| Released this month | v2026.6.0 — the "data coming in" milestone |
| Built for | Agents (over the standard agent-tool protocol), custom apps, and team or offline setups |
| License | Free for personal use, learning, and trying it out; a commercial license covers production use |
| Source · image · docs | github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server |
| Desktop app · browser build | HotM (a desktop client) · fortemi-react (the browser build) |

## Highlights

**1. Chat that types its answer as it goes.**
What it is: when you ask your memory a question, the answer now arrives a few words at a time instead of all at once at the end.
How you'd use it: ask a question and start reading right away — you don't wait for the whole reply to finish.
Why it helps: it feels fast and alive, like a person typing back to you, and you can tell it's working.

**2. A secure drop-box other apps can post to.**
What it is: you can set up a "receiver" that other tools send updates to. Fortemi checks each message is genuine, checks it's the right shape, and ignores accidental repeats.
How you'd use it: point a tool you already use — say a phone or chat service — at your receiver, and its events land in your memory on their own.
Why it helps: your memory stays current without you copying things over by hand, and only real, well-formed messages get in.

**3. Pour in a long stream of notes — nothing gets lost.**
What it is: a busy agent can send notes as one long stream. Fortemi confirms each one, and if the agent sends too fast, Fortemi tells it to slow down so nothing is dropped.
How you'd use it: let an agent run for hours and stream what it learns straight into memory; if the link breaks, it picks up where it left off.
Why it helps: large, long-running jobs finish safely instead of failing partway and losing work.

**4. Big uploads that resume after a drop.**
What it is: large files — long videos, recordings — upload in a way that can resume if the connection cuts out.
How you'd use it: start a multi-gigabyte upload over a shaky link; if it drops, it continues from where it stopped instead of starting over.
Why it helps: no more re-uploading a huge file from scratch because of one hiccup.

**5. Listen to outside event feeds.**
What it is: Fortemi can now tune in to event streams from other systems and add what they carry to your memory.
How you'd use it: connect a feed of events you care about, and Fortemi folds them in automatically — even catching up after a restart.
Why it helps: your memory can grow from live sources, not just things you add by hand.

**6. A wide safety and privacy pass.**
What it is: the team went through the whole server and made sure its logs, error messages, and even its examples stop showing private details like file paths, keys, and the contents of your notes.
How you'd use it: nothing to do — you just get a server that's careful about what it reveals.
Why it helps: if a log or error is ever seen by the wrong person, it no longer hands them your secrets.

## Features shipped

**Data coming in, four ways (v2026.6.0).** This release was all about getting information *into* the server smoothly and safely.

- **Live chat replies.** There's a new way to chat with your memory where the answer streams back a few words at a time. It shares one graphics card fairly. A live chat holds the card only while it's answering. If the card is busy, it politely says "try again in a moment." That way it never blocks the background work that keeps your memory fresh. If you reconnect mid-answer, it can pick up from the last words you saw.
- **Incoming webhooks (the secure drop-box).** You can register a receiver that other apps post to. Each message is checked three ways. A signature proves it really came from the sender. A shape-check makes sure it has the right fields. And a "do-this-once" key means an accidental repeat won't be stored twice. Every accepted message is saved to a shared, durable log so nothing slips through.
- **Streaming notes in, with pacing.** A new channel lets an agent send notes as a long, line-by-line stream. Fortemi confirms each line and reports progress. When the sender is too fast, it pushes back so nothing overflows. If the stream breaks, the sender resumes from the last confirmed line. Each note that comes in is saved together with its log entry, so the count always matches.
- **Resumable big uploads.** File uploads now follow a well-known resumable standard, so multi-gigabyte media can continue after an interruption instead of restarting.
- **Outside event sources.** Fortemi can pull from external event streams — common ones like Redis and, if you turn it on, Kafka — and from other live feeds. It handles each event at least once and remembers its place across restarts. Anything it can't handle goes to a holding area, so one bad event doesn't stop the rest. These connectors are off by default, so they cost nothing until you switch one on.

**Security and privacy hardening.** After the release, the team carried out a broad safety pass across the whole server, now landing on the main code line.

- **Stop leaking details.** Debug logs, error replies, and even the code examples in the docs were all cleaned. They no longer print sensitive things like file paths, tokens, passphrases, or the contents of what you store.
- **An audit trail for sensitive actions.** Important actions now leave a clear record: signing in, changing an encryption key, uploading an attachment, controlling the job queue, and changing tags or categories. You can see what happened and when.
- **Refuse to start when unsafe.** If the server is set up in an incomplete or unsafe way, it now stops at startup instead of running in a risky state.
- **Safer responses.** Replies carry standard safety headers, and errors come back in a clean, predictable form instead of exposing raw internal messages.

## Fixes

The most visible fix restored the server's public Docker image. The build that publishes the image had quietly stopped working months earlier. Worse, it was reporting success anyway. Now it publishes again, and it fails loudly if a push ever fails, so a broken release can't hide. Beyond that, most of the month's fixes were part of the safety pass above. They tightened what the server prints in logs and errors across nearly every part of the system.

## Performance & reliability

Reliability was a real theme. Live chat shares the graphics card fairly. A chat can never starve the background jobs that keep your memory current. When the card is busy, chat waits its turn. Streaming notes in has built-in pacing, so a fast sender is asked to slow down. Long streams and big uploads can resume after a drop. The outside-feed connectors keep their place across restarts. They set aside anything they can't process, so one bad event doesn't halt the flow.

## Breaking changes & migrations

None. Everything this month is additive. The new ways to send data in sit alongside what was already there, and older calls keep working. New storage tables — for resumable uploads and the holding area — are created on their own when you upgrade. One thing to know: the new live-chat and streaming channels carry your data up front. So a web page reads them with a normal fetch reader, not the browser's simplest built-in one.

## Releases

- **v2026.6.0** (Jun 15) — the "data coming in" milestone: live streaming chat, secure incoming webhooks, resumable streaming note-ingest, resumable big-file uploads, and pluggable outside event sources. Also restored the public Docker image build. Published as the server's Docker image.

The wide security and privacy pass described above landed on the main code line later in the month and is set to roll into the next release.

## Dependencies & security

Security was the second big story of the month. The safety pass cleaned sensitive details out of logs, errors, and docs. It added an audit trail for sensitive actions. It made the server refuse to start when it's set up wrong. And it added standard safety headers to responses. On top of that, the riskier or heavier add-ons are off by default. The outside-feed connectors only run when you switch them on. The Kafka connector is locked down twice: it isn't even built into the normal package unless you ask for it. That keeps the default server lean and reduces what could go wrong.

## Docs & developer experience

The server's documentation lives at **docs.fortemi.com/server**. The v2026.6.0 release shipped with full, plain-English release notes covering all four parts of the "data coming in" work. The project's front page now makes the choice clear. Want a ready-to-use app on your own computer? Get **HotM**. Want a headless server for agents, custom apps, teams, or offline use? Run Fortemi itself. A new check in the build also watches the docs for examples that shouldn't ship, part of the same safety pass.

## Tests & CI

New tests cover each of the month's features: streaming chat, streaming note-ingest, the incoming-webhook checks, and the outside-feed connectors, including the optional Kafka path. A full test run against a real database checks the trickier promises. A duplicate webhook isn't stored twice. The count of notes in always matches the log. And a half-finished stream rolls back cleanly. The release pipeline itself was fixed so it can no longer report a false success, and leftover code warnings were cleared.

## Cross-project impact

- **HotM** (the desktop app) shipped its own June release. It made fresh installs smoother and more reliable. Linux installs now set up speech-to-text by default. And desktop video uploads use a sturdier path with visible progress. HotM bundles the very same Fortemi server inside it, so the server's work flows straight into the desktop app. The desktop side of the new live-chat feature is the next step there.
- **fortemi-react** (the browser build of this server) also had a big June — same memory model, running fully in a web browser. See its report.
- **Agents and the wider stack** read and write memory through Fortemi's standard agent tools. So the new "data coming in" channels give agents richer, safer ways to feed and grow their memory.
- **Pagenary** (the publishing tool) builds the Fortemi docs site.

## Known issues & open threads

- The desktop app (HotM) doesn't use the new live, word-by-word chat yet. The server side shipped this month; the desktop side is the next piece.
- Real-time voice and video connections (phone, web calling, live video) are planned as a separate, larger effort and are not part of this month's "data coming in" work.
- The wide security and privacy pass is still landing on the main code line and will roll into the next release; expect more of it.
- The outside-feed connectors are off by default. As more people turn them on for large feeds, expect more tuning of how they pace and recover.

## What's next

Connect the desktop app to live streaming chat so HotM can show answers as they're typed. Finish rolling the security and privacy pass into a release. Begin the separate real-time voice and video effort. And keep the three surfaces — the server, the browser build, and the desktop app — moving in step.

## Appendix

- **What it is:** a self-hosted agent-memory server, built in Rust, run with one Docker command.
- **Released this month:** v2026.6.0 — the "data coming in" milestone, published as the server's Docker image.
- **License:** free for personal use, learning, and trying it out; a commercial license covers production use.
- **Source · image · docs:** github.com/Fortemi/fortemi · ghcr.io/fortemi/fortemi · docs.fortemi.com/server · window: all of June 2026.
- **Related surfaces:** HotM (desktop client) · fortemi-react (browser build).
