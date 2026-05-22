# ADR-RTP-004 — First-Provider Selection

**Status:** Proposed (draft) • 2026-05-22
**Epic:** [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
**Companions:** ADR-RTP-001, ADR-RTP-002, ADR-RTP-003

## Context

The real-time provider epic spans Twilio Voice, Twilio Messaging, WebRTC providers (LiveKit, Agora, Daily), SIP/PSTN, and video providers (Mux, IVS). All cannot be built simultaneously. Choosing the first provider commits engineering effort and sets architectural precedent for everything that follows.

## Decision

**Twilio Programmable Voice is the first provider integration (milestone 1).**

LiveKit (Cloud and OSS) is milestone 2. SIP direct (without Twilio) is milestone 3 or later. Video ingest is a separate sub-epic to be filed after milestone 2.

Twilio Messaging (SMS/WhatsApp webhooks) is **not** a milestone of this epic — it's pure Phase B (#817) work and should be filed against that sub-epic instead. Listed here for clarity that it's not in scope.

## Rationale

### Why Twilio Voice first

| Criterion | Twilio Voice | LiveKit | SIP direct |
|---|---|---|---|
| Documented integration | Most complete in the industry | Good but smaller surface | RFC-level docs; less practitioner-friendly |
| Setup-to-first-call | <1 day (sign up, dial number, point TwiML to URL) | ~1–2 days (account, room create, SDK integration) | Days/weeks (SIP trunk, gateway, NAT) |
| Engineering match for milestone 1 architecture | Perfect — Twilio dials our WSS URL (ADR-RTP-001 provider-direct, ADR-RTP-002 WebSocket) | Requires WebRTC support which is milestone 2 | Requires SIP stack |
| Market reach | Vast — telephony is the foundational shape | Growing — WebRTC is browser-native | Vast for enterprise; needs gateway |
| User-facing value at milestone 1 | "Call your knowledge base on the phone" — concrete | "Embed Fortemi voice in a website" — abstract until UI built | "PBX integration" — niche audience |
| Rust SDK | None (HTTP API + WSS direct — acceptable) | livekit-rust (first-party) | None production-grade |
| Latency profile | Excellent (Media Streams ≈ 50 ms ingress) | Excellent (WebRTC ≈ 30 ms ingress) | Variable |

### What "milestone 1" includes

The first iteration ships:

1. **Inbound calls to a Fortemi-managed Twilio number**: caller dials the number, hears a configurable greeting, audio streams to Fortemi via Media Streams WSS, live transcript flows out via SSE to subscribed UI clients
2. **Control-plane webhooks**: Twilio fires `call.initiated`, `call.completed`, `recording.completed` events to Fortemi; Phase B receivers (#817 family) capture them
3. **Live transcripts**: Deepgram streaming ASR (per ADR-RTP-003) produces partial + final transcripts; emitted to outbox; consumers see them
4. **Post-call batch transcript**: when the recording arrives via the `recording.completed` webhook, schedule the existing `AudioTranscriptionHandler` for high-quality offline pass
5. **Call session persistence**: `call_sessions` and `transcript_segments` tables populated; queryable via REST
6. **Configuration**: env vars for Twilio account/auth, Deepgram API key, default greeting text, default voice

### What milestone 1 does NOT include

- Outbound dialing (Fortemi-initiated calls)
- Recording playback / TTS responses (acting on transcripts as it goes)
- IVR / menu navigation
- Conferencing (multi-party calls)
- WebRTC / LiveKit
- Direct SIP
- Video
- WhatsApp / SMS (those go through Phase B with their own slugs)

These are explicitly out of scope to keep milestone 1 small and shippable. Each becomes its own ticket after milestone 1 lands.

### Why LiveKit is milestone 2 and not milestone 1

LiveKit is genuinely attractive — first-party Rust SDK, WebRTC, browser-native. But:

1. **WebRTC is a different transport** (per ADR-RTP-002 we're starting with WSS). Adding WebRTC means SDP negotiation, ICE, DTLS-SRTP — all valuable but additive scope.
2. **The user-facing value requires a UI**. A WebRTC integration without a client app to capture the audio is half-finished. HotM doesn't yet have a "talk to my notes" UI; building that is itself work.
3. **Twilio Voice gives us phone access first** — a more concrete user value: dial a number, talk, transcripts appear.

Milestone 2 will likely pair LiveKit integration with a HotM-side voice-capture component.

### Why SIP direct is later

Direct SIP integration (no Twilio in the loop) requires either:
- A self-hosted SIP server (FreeSWITCH, Asterisk, Kamailio) — substantial operational burden
- A SIP-stack Rust crate that doesn't yet exist at production grade
- A SIP-to-WS gateway hosted somewhere

Twilio SIP Trunking → Programmable Voice → Media Streams gives us SIP-via-Twilio for ~$0.015/min. This is the right path for milestone 1; direct SIP only when a user has a strong reason to bypass Twilio.

## Consequences

### Positive

- Milestone 1 is small, focused, demonstrable
- Engineering risk concentrated on familiar tech (WebSocket, HTTP, hosted ASR)
- "Call your Fortemi number" is a concrete UX story — easy to evaluate
- Reuses Phase B (#817) for control plane — no duplicate code
- Architecturally extends cleanly into LiveKit (milestone 2)

### Negative

- WebRTC users (browser-direct voice) wait for milestone 2
- Direct PSTN users (no Twilio) wait for milestone 3
- We are now committed to operating Twilio account state (number assignment, billing) for the demo deployment

### Operational requirements

For milestone 1 to actually be usable:

- Twilio account with at least one phone number provisioned
- TwiML Bin (or self-hosted TwiML endpoint) pointing inbound calls to Fortemi's Media Streams WSS URL
- Deepgram account with streaming API enabled
- Fortemi instance reachable from the public internet on its WSS endpoint (or via a tunnel like Cloudflare Tunnel, ngrok in dev)
- Outbound webhook endpoint reachable for Twilio call-status callbacks

Documentation must cover all of the above as a setup section in the milestone 1 release.

## Alternatives Considered

### Alternative A — Twilio Messaging first

SMS / WhatsApp via Twilio Messaging webhooks.

Rejected because this is Phase B work, not real-time-provider work. The audio/video focus of #837 is a different problem. Twilio Messaging should be one or more receiver slugs filed under #817.

### Alternative B — LiveKit first

LiveKit's Rust SDK and browser-native WebRTC are compelling.

Rejected because:
- Lacks a UI to demonstrate value without HotM-side work
- WebRTC adds substantial architectural scope before any provider integration
- Twilio gives us a more concrete user story sooner

### Alternative C — Agora or Daily first

Established WebRTC SaaS alternatives to LiveKit.

Rejected because LiveKit's Rust SDK is the only first-party Rust integration in the WebRTC category. If we're picking WebRTC, LiveKit is the natural choice. If LiveKit is rejected for milestone 1 (it is), Agora/Daily have the same problem.

### Alternative D — Mux / Cloudflare Stream (video) first

Video ingest is a clean self-contained problem.

Rejected because:
- Less common user need than voice
- Doesn't exercise the live-ASR path (the most architecturally novel work)
- Better as a follow-up after milestone 1 proves the architecture

## References

- `../streaming-realtime/research-synthesis.md` §3.2 (provider matrix), §7 (first-provider question), §9 (conclusions)
- `../streaming-realtime/architecture-sketch.md` §3 (one-call walkthrough — Twilio reference)
- ADR-RTP-001 (architectural pattern — provider-direct first)
- ADR-RTP-002 (transport choice — WebSocket first)
- ADR-RTP-003 (ASR backend — hosted Deepgram default)
- Twilio Programmable Voice docs: `www.twilio.com/docs/voice` (to be inducted)
- Twilio Media Streams docs: `www.twilio.com/docs/voice/twiml/stream` (to be inducted)
