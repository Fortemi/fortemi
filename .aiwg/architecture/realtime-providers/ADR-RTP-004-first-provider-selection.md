# ADR-RTP-004 — First Concrete Adapter

**Status:** Proposed (draft) • Revised 2026-05-22 (v2 — re-framed as first concrete adapter, not architectural anchor)
**Epic:** [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
**Companions:** ADR-RTP-001 (adapter pattern), ADR-RTP-002 (transport bindings), ADR-RTP-003 (ASR backend)

## Context

Per ADR-RTP-001 the architecture is **adapter-based with a standards-shaped core**. Per ADR-RTP-002 we ship three transport bindings over time (WebSocket → WebRTC → SIP). Each binding gets at least one concrete adapter implementation. This ADR picks **which concrete adapter ships first**.

The choice is not an architectural decision — the core abstraction (ADR-RTP-001) is provider-agnostic. The choice is a sequencing decision: which provider integration validates the trait surface earliest with the lowest engineering risk.

## Decision

**Twilio Programmable Voice is the first concrete adapter (milestone 1).** It is **not** the architectural anchor — `MediaFrame`, `Codec`, `CallTransport`, and the rest of the core are standards-shaped and Twilio-agnostic per ADR-RTP-001. Twilio is the first provider integration the abstraction proves out against; it must not (and architecturally cannot) define the shape.

Adapter sequencing:

| Milestone | Adapter | Transport binding | Purpose |
|---|---|---|---|
| 1 | Twilio Programmable Voice | WebSocket (ADR-RTP-002) | First concrete adapter; validates `CallTransport` trait against a well-documented provider |
| 2 | LiveKit | WebRTC (ADR-RTP-002) | Second adapter; surfaces leaks in the trait that single-adapter milestone 1 couldn't catch; adds browser/mobile + WebRTC capability |
| 3 | SIP-direct (Fortemi terminates SIP) + Twilio SIP Trunking shim | SIP/RTP (ADR-RTP-002) | Self-hosted enterprise telephony; PSTN without Twilio Voice |
| 4+ | Vonage Voice, Agora, Daily.co, Mux video, etc. | Per provider | Continued ecosystem expansion |

The `MockAdapter` (compile-time + test-only) ships **at milestone 1 alongside Twilio** so the trait surface is exercised by two implementations from day one — Twilio for production validation, Mock for testing and trait-shape sanity.

Twilio Messaging (SMS/WhatsApp/webhooks) is explicitly **not** in scope for this epic. Those are HTTP webhook callbacks handled by Phase B receivers (#817 / #818–#823). Listed here for clarity.

## Rationale

### Why Twilio as the first concrete adapter

| Criterion | Twilio Voice | LiveKit | SIP-direct |
|---|---|---|---|
| Documented protocol surface | Most complete in the industry (Media Streams spec, Voice webhooks, well-known idioms) | Good but smaller; Rust SDK abstracts the WebRTC details | RFC-level docs; less practitioner-friendly outside telephony specialists |
| Setup-to-first-call | <1 day (sign up, dial number, point TwiML to URL) | ~1–2 days (account, room create, SDK integration) | Days/weeks (SIP trunk, gateway, NAT) |
| Engineering match for milestone-1 transport (WS per ADR-RTP-002) | Perfect — Twilio dials our WSS URL | Requires WebRTC binding (milestone 2 per ADR-RTP-002) | Requires SIP/RTP binding (milestone 3) |
| Market reach | Vast — telephony is the foundational voice shape | Growing — WebRTC is browser-native | Vast for enterprise; needs gateway |
| User-facing value at milestone 1 | "Call your knowledge base on the phone" — concrete, demonstrable | "Embed Fortemi voice in a website" — abstract until HotM voice UI exists | "PBX integration" — niche near-term audience |
| Rust SDK | None (HTTP API + WSS direct via tokio-tungstenite/axum — acceptable) | livekit-rust (first-party) — better, but doesn't help if the binding isn't ready yet | None production-grade |
| Latency profile | Excellent (Media Streams ≈ 50 ms ingress) | Excellent (WebRTC ≈ 30 ms ingress) | Variable |
| Validates the abstraction against a non-trivial provider | Yes — Twilio's JSON envelope, mark/clear semantics, DTMF, recording lifecycle all exercise the trait | Yes — different from Twilio (WebRTC offer/answer, room participation); good second adapter | Yes — but third adapter is more useful than first for surfacing trait gaps |

### What "milestone 1" includes

The first iteration ships:

1. **Standards-shaped core abstraction**: `MediaFrame`, `Codec`, `CallTransport`, `CallSession`, `CallControlEvent` types in `crates/matric-rtp` (or `matric-api/src/realtime/`). Independent of any provider; CI lint can verify no Twilio-specific identifiers leak outside `adapters/twilio/`.
2. **Twilio Programmable Voice adapter**: `adapters/twilio/` translates Twilio Media Streams JSON envelopes into `MediaFrame` records; translates Twilio Voice webhooks into `CallControlEvent`s.
3. **Mock adapter**: `adapters/mock/` exercises the trait with deterministic frame streams for integration tests. **Ships alongside Twilio in milestone 1 so the trait has two implementations from day one.**
4. **Inbound call walkthrough end-to-end**: caller dials a Fortemi-managed Twilio number → Twilio adapter ingests → standards-shaped frames flow through codec normalizer → ASR adapter (Deepgram per ADR-RTP-003) → transcript emitter → outbox → consumers
5. **Control-plane webhooks**: Twilio fires `call.initiated`, `call.completed`, `recording.completed` to Phase B receivers (#817 family) with a Twilio-specific schema; the Twilio adapter translates those into standards-shaped `CallControlEvent`s
6. **Post-call batch transcript**: when `recording.completed` flows through, queue the existing `AudioTranscriptionHandler` against the recording URL — reuses batch pipeline
7. **Call session persistence**: `call_sessions` and `transcript_segments` tables populated; queryable via REST. Schema is provider-agnostic; provider name + opaque `provider_call_id` stored alongside the standards-shaped fields.
8. **Configuration**: env vars for Twilio account/auth, Deepgram API key, default greeting text, default voice. Adapter selection is config-driven (`REALTIME_ADAPTERS=twilio,mock`).

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
