---
artifact: feature-plan
project: fortemi
cluster: realtime-providers
epic: Fortemi/fortemi#837
milestone: 1 — Twilio Voice + Deepgram + outbox
status: draft-v1
last-updated: 2026-05-22
---

# Milestone 1 — Twilio Voice + Deepgram Streaming + Outbox

## Goal

Ship the first end-to-end real-time provider integration: caller dials a Fortemi-managed Twilio number, audio streams via Media Streams WSS to Fortemi, Deepgram produces live transcripts, transcripts flow through the outbox to UI consumers, recording is processed in batch post-call for high-quality final transcript.

## Acceptance

- A user can dial a configured Twilio number and have their call audio captured by Fortemi
- Live partial + final transcripts appear via SSE within ~1 second of speech
- After the call, the recording is auto-fetched and processed via the existing `AudioTranscriptionHandler` for a high-quality transcript
- All transcript data is queryable: `GET /api/v1/calls/{call_id}` returns session + segments
- Operational metrics surface on `/api/v1/health/streaming`
- Documentation: end-to-end setup guide from "I have a Twilio account" to "first call transcribed"

## Construction Order

```
M1.1 RealtimeIngress trait + call session manager
      └─> M1.2 Twilio Media Streams ingress handler ──┐
                                                       │
M1.3 StreamingASRBackend trait                        │
      └─> M1.4 Deepgram backend impl ─────────────────┤
                                                       │
                                                       ▼
                                          M1.5 Transcript emitter
                                                  (writes to outbox)
                                                       │
M1.6 call_sessions + transcript_segments              │
       migrations ─────────────────────────────────────┤
                                                       │
M1.7 GET /api/v1/calls/{id} REST                      │
                                                       │
M1.8 Phase B receiver schemas for Twilio              │
       webhooks (control plane) ─────────────────────  │
                                                       │
M1.9 Post-call batch transcript trigger                │
       (recording.completed → AudioTranscriptionJob)   │
                                                       │
M1.10 Setup documentation + sample TwiML              │
M1.11 Health metrics + cost-estimate gauge            │
M1.12 Integration tests (mocked Twilio + Deepgram)
```

## Child Issues to File

Each becomes an issue under #837. Listed with sizing (S/M/L), priority, dependencies. **Filing happens after this scoping (#838) is approved.**

### M1.1 — `RealtimeIngress` trait + call session manager

- **Title:** `feat(api): RealtimeIngress trait + call session manager (milestone 1 foundation)`
- **Size:** M
- **Priority:** P1
- **Depends on:** none
- **Body summary:** New module `crates/matric-api/src/realtime/` with the trait, session manager, and call-session state. Trait per ADR-RTP-001 §2.2.

### M1.2 — Twilio Media Streams ingress handler

- **Title:** `feat(api): Twilio Media Streams WebSocket ingress handler`
- **Size:** M
- **Priority:** P1
- **Depends on:** M1.1
- **Body summary:** `crates/matric-api/src/realtime/ingress/twilio.rs`. Accept `wss://.../realtime/twilio/{call_sid}` connections; parse Twilio's JSON-wrapped media frames; decode μ-law → PCM 16 kHz; emit to ASR adapter.

### M1.3 — `StreamingASRBackend` trait

- **Title:** `feat(api): StreamingASRBackend trait + session lifecycle`
- **Size:** S
- **Priority:** P1
- **Depends on:** M1.1
- **Body summary:** Trait shape per ADR-RTP-003 §Consequences. Allows pluggable backends.

### M1.4 — Deepgram streaming backend implementation

- **Title:** `feat(api): Deepgram Streaming ASR backend (default per ADR-RTP-003)`
- **Size:** M
- **Priority:** P1
- **Depends on:** M1.3
- **Body summary:** `crates/matric-api/src/realtime/asr/deepgram.rs`. WebSocket client to Deepgram's streaming API; emit partial/final transcript events; reconnect on drop; failover via configured secondary backend.

### M1.5 — Transcript event emitter

- **Title:** `feat(api): transcript event emitter (writes transcript_partial / transcript_final to outbox)`
- **Size:** M
- **Priority:** P1
- **Depends on:** M1.4, **#592** (outbox helpers)
- **Body summary:** Listens to ASR adapter events; writes `transcript_partial` and `transcript_final` events to `event_outbox` (or bypasses for partials per R-RTP-012 mitigation). New event types registered.

### M1.6 — `call_sessions` + `transcript_segments` migrations

- **Title:** `feat(db): call_sessions and transcript_segments tables`
- **Size:** S
- **Priority:** P1
- **Depends on:** none
- **Body summary:** Two new tables per architecture-sketch §7. Migration under `migrations/`.

### M1.7 — `GET /api/v1/calls/{id}` REST endpoint

- **Title:** `feat(api): GET /api/v1/calls/{id} — session + segment query`
- **Size:** S
- **Priority:** P1
- **Depends on:** M1.6
- **Body summary:** Read endpoint for completed call data.

### M1.8 — Twilio webhook receiver schemas

- **Title:** `feat(config): Twilio Voice webhook receiver schemas for Phase B (#817)`
- **Size:** S
- **Priority:** P1
- **Depends on:** **#817** Phase B family (specifically #819 receiver registration, #821 schema validation)
- **Body summary:** Register `twilio-voice-events` slug in Phase B receiver registry. Schema captures `CallSid`, `CallStatus`, `RecordingUrl`, etc. No new endpoint code — purely configuration.

### M1.9 — Post-call batch transcript trigger

- **Title:** `feat(api): trigger AudioTranscriptionHandler on Twilio recording.completed webhook`
- **Size:** S
- **Priority:** P1
- **Depends on:** M1.8
- **Body summary:** When the `recording.completed` event flows through the outbox, queue an `AudioTranscription` job pointing at the Twilio recording URL. Reuses existing batch pipeline; no new transcription code.

### M1.10 — Setup documentation

- **Title:** `docs: real-time provider setup guide (Twilio + Deepgram)`
- **Size:** M
- **Priority:** P1
- **Depends on:** M1.1–M1.9 (functionally complete)
- **Body summary:** End-to-end guide: Twilio account setup, number provisioning, TwiML configuration, Deepgram API key, Fortemi env vars, first-call walkthrough. Cross-link from SETUP.md (#583).

### M1.11 — Health metrics + cost estimate gauge

- **Title:** `feat(metrics): rtp_* metrics + estimated-cost gauge on /health/streaming`
- **Size:** S
- **Priority:** P2
- **Depends on:** M1.1–M1.4 (need metric injection points)
- **Body summary:** Per architecture-sketch §10. Metrics include cost estimate per session and rolling cost (R-RTP-003 mitigation).

### M1.12 — Integration tests

- **Title:** `test(realtime): integration tests with mocked Twilio + Deepgram`
- **Size:** M
- **Priority:** P2
- **Depends on:** M1.1–M1.5
- **Body summary:** Mock Twilio WebSocket server + mock Deepgram WebSocket server; verify end-to-end flow including reconnect, failover, codec normalization. Should run in CI without external dependencies.

### M1.13 (operational risk mitigation, fold into M1.10 or file separately) — Consent disclosure docs

- **Title:** `docs: jurisdiction-specific consent disclosure guidance for call recording`
- **Size:** S
- **Priority:** P2
- **Depends on:** M1.10
- **Body summary:** Risk R-RTP-005. Document the regulatory landscape, provide a configurable greeting that includes recording disclosure, link to authoritative resources (no legal advice but pointers to it).

## Sizing Summary

| Size | Count |
|---|---|
| S | 6 (M1.3, M1.6, M1.7, M1.8, M1.9, M1.11, M1.13) — 7 with consent docs |
| M | 5 (M1.1, M1.2, M1.4, M1.5, M1.10, M1.12) |
| L | 0 |

13 children for milestone 1. Roughly 2–4 weeks of engineering effort once dependencies (Phase B receivers #817 family, outbox #591/#592) land.

## Dependencies on Other Work

| Dependency | What blocks here |
|---|---|
| **#591** (event_outbox migration) | M1.5 — transcript events write to outbox |
| **#592** (outbox helpers) | M1.5 — `outbox_insert(tx, event)` |
| **#817 / #819 / #821** (Phase B receivers) | M1.8 — Twilio control-plane webhooks |
| **#593** (publisher) | Live transcript delivery to consumers (technically blocks the value-add, not the implementation) |

Milestone 1 of #837 should not be considered shippable until #586 Phase 1 is at least partially complete. Recommended sequencing:

1. **First**: #591 + #592 land
2. **Parallel**: M1.1 (trait + session manager) and #817 family (Phase B receivers)
3. **Then**: M1.2 + M1.3 + M1.4 (ingress + ASR) and M1.6 + M1.7 (DB + REST)
4. **Then**: M1.5 + M1.8 + M1.9 (event emission + control plane)
5. **Then**: M1.10 + M1.11 + M1.12 + M1.13 (docs, metrics, tests)

## What's NOT in Milestone 1

Per ADR-RTP-004:

- Outbound dialing
- TTS responses / IVR
- Conferencing
- WebRTC (milestone 2)
- LiveKit (milestone 2)
- Direct SIP (milestone 3+)
- Video providers (separate sub-epic later)
- WhatsApp / SMS (Phase B #817 handles separately)
- Self-hosted whisper-streaming (opt-in via env, not enabled by default per ADR-RTP-003)

## Milestone 2 Preview (filed after M1 lands)

Adds WebRTC via LiveKit:

- LiveKit ingress handler (consumes from LiveKit room as a server-side participant)
- LiveKit Cloud or OSS deployment guide
- Browser-side voice capture (HotM coordination — separate repo)
- WebRTC codec handling (Opus 48 kHz → PCM 16 kHz)

## Milestone 3 Preview

Adds direct SIP / video / self-hosted streaming as separate scoping work surfaces real demand.

## References

- ADRs: `../../../architecture/realtime-providers/ADR-RTP-001..004.md`
- Synthesis: `research-synthesis.md`
- Architecture sketch: `architecture-sketch.md`
- Risk register: `risk-register.md`
- Parent epic: [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
- Scoping issue: [#838](https://git.integrolabs.net/Fortemi/fortemi/issues/838)
