---
artifact: risk-register
project: fortemi
cluster: realtime-providers
epic: Fortemi/fortemi#837
status: draft-v1
last-updated: 2026-05-22
---

# Real-Time Provider Integration — Risk Register

Risks identified during scoping (#838). Each entry: impact, likelihood, mitigation, owner. Reviewed at milestone gates.

Scale: **L** = low, **M** = medium, **H** = high, **C** = critical.

## R-RTP-001 — Provider vendor lock-in

| Field | Value |
|---|---|
| Impact | H |
| Likelihood | M |
| Risk | Building deep Twilio-specific integration creates lock-in; switching providers later requires rewriting the ingress layer |
| Mitigation | `RealtimeIngress` trait abstracts the provider; per-provider implementations behind the trait. Twilio's idioms (TwiML, CallSid) stay in the Twilio module; the rest of the system speaks in trait language |
| Residual risk | Medium — abstractions leak; some Twilio-isms will end up in shared code unless watched |
| Owner | Architecture |

## R-RTP-002 — Hosted ASR vendor outage

| Field | Value |
|---|---|
| Impact | H (live transcripts stop) |
| Likelihood | L (Deepgram SLA 99.9%) |
| Risk | Deepgram outage takes live transcription offline; user-visible during the outage |
| Mitigation | Configure a secondary backend via `REALTIME_ASR_BACKEND_FALLBACK`; auto-failover per ADR-RTP-003. Continue capturing audio during the outage; produce a batch transcript when the recording is available |
| Residual risk | Low — failover covers it for active sessions; new sessions during outage attempt primary first, fall back on first error |
| Owner | Engineering |

## R-RTP-003 — Cost variance at scale

| Field | Value |
|---|---|
| Impact | M (financial, not technical) |
| Likelihood | H if usage is uncapped |
| Risk | Per-minute hosted ASR cost can grow faster than self-hosted compute; operators may be surprised by bills |
| Mitigation | Document costs prominently in setup guide. Provide `REALTIME_ASR_RATE_LIMIT_MINUTES_PER_DAY` and `REALTIME_ASR_RATE_LIMIT_MINUTES_PER_HOUR` config to cap usage. Surface running-cost estimate metric: `rtp_estimated_asr_cost_dollars_total` |
| Residual risk | Low — visibility + caps prevent runaway |
| Owner | Engineering + Documentation |

## R-RTP-004 — GPU contention with Ollama on single-GPU systems

| Field | Value |
|---|---|
| Impact | H (one of the two pipelines stalls) |
| Likelihood | M (only when both an active call and a long revision job collide) |
| Risk | Live calls and AI revision compete for VRAM; one stalls (or OOMs) on small-GPU systems |
| Mitigation | Default ASR backend is hosted (ADR-RTP-003) — no GPU usage by the call. Self-hosted whisper-streaming requires opt-in on mid/high-end. Pause Ollama tier during active calls when self-hosted is enabled |
| Residual risk | Low — covered by default config |
| Owner | Architecture |

## R-RTP-005 — Call recording consent / regulatory variance

| Field | Value |
|---|---|
| Impact | C (legal exposure for operators) |
| Likelihood | M (varies by jurisdiction; some U.S. states + EU require two-party consent) |
| Risk | Operators may deploy Fortemi for recording without complying with applicable consent laws (e.g., California, Florida, EU GDPR, Illinois BIPA for voice biometrics) |
| Mitigation | Document jurisdiction-specific guidance prominently in the milestone 1 release notes and SETUP.md. Provide a configurable "recording disclosure" greeting prepended to inbound calls. Make recording opt-in per call (default off in some tier or env) |
| Residual risk | Operator responsibility ultimately; Fortemi can only provide tools, not enforcement |
| Owner | Documentation + Architecture |

## R-RTP-006 — Audio quality on lossy networks

| Field | Value |
|---|---|
| Impact | M (transcript quality degrades) |
| Likelihood | M (mobile networks, international callers, congested links) |
| Risk | Audio frames dropped or delayed cause transcript gaps and lower confidence |
| Mitigation | Capture per-frame metrics (jitter, drop rate); emit metric. Configure VAD chunking to be drop-tolerant (don't require contiguous frames). Display confidence per segment so users see when quality is low |
| Residual risk | Inherent to lossy networks; documentation matters more than mitigation |
| Owner | Engineering |

## R-RTP-007 — Codec/encoding mismatch

| Field | Value |
|---|---|
| Impact | M (call works but transcript is garbage) |
| Likelihood | L (per-provider codec is well-documented) |
| Risk | Provider sends a codec we don't normalize correctly (e.g., Opus 48 kHz sample rate mismatch with whisper's expected 16 kHz) |
| Mitigation | Per-provider test fixture with known-good audio samples; CI validates round-trip codec normalization. Codec normalizer logs unsupported codec gracefully and emits metric |
| Residual risk | Low — covered by test fixtures |
| Owner | Engineering |

## R-RTP-008 — WebSocket connection drops mid-call

| Field | Value |
|---|---|
| Impact | M (partial transcript continuity broken) |
| Likelihood | L–M (network issues happen) |
| Risk | Provider WebSocket closes mid-call (timeout, network hiccup); we lose audio until reconnect |
| Mitigation | Reconnect logic with exponential backoff. Twilio Media Streams supports reconnect via `connect` mark; LiveKit handles via its own reconnect protocol. Emit `call_event { event: 'dropped' }` so consumers know |
| Residual risk | Medium — reconnect adds latency and gaps; some audio inevitably lost |
| Owner | Engineering |

## R-RTP-009 — Hosted ASR latency drift

| Field | Value |
|---|---|
| Impact | M (UX degrades from "live" to "delayed") |
| Likelihood | M (provider performance varies regionally and over time) |
| Risk | Deepgram latency climbs over time or in some regions; "live" UX no longer feels live |
| Mitigation | Monitor `rtp_asr_partial_latency_ms` histogram. Alert when p99 > 1000 ms sustained. Failover to secondary provider on sustained latency, not just on errors |
| Residual risk | Low with active monitoring |
| Owner | Engineering |

## R-RTP-010 — Twilio account misconfiguration / billing

| Field | Value |
|---|---|
| Impact | C (no calls flow) |
| Likelihood | M (account misconfiguration is common in first deployments) |
| Risk | Operator's Twilio account misconfigured (TwiML wrong, number unprovisioned, billing suspended); calls fail or don't reach Fortemi |
| Mitigation | Documentation walkthrough for milestone 1 setup. Health endpoint validates Twilio API key on startup and surfaces errors prominently. Surface inbound call attempts (even failed) in metrics so operators see the issue |
| Residual risk | Low with good docs |
| Owner | Documentation + Engineering |

## R-RTP-011 — Compounding GPU contention with whisper-streaming + Ollama + new revision pipeline

| Field | Value |
|---|---|
| Impact | H (multi-pipeline OOMs on mid-tier GPUs) |
| Likelihood | M (only when self-hosted streaming is enabled on mid-tier with active jobs) |
| Risk | Self-hosted whisper-streaming (opt-in per ADR-RTP-003) competes with Ollama AND with batch revision; OOM or GPU thrashing under combined load |
| Mitigation | When `REALTIME_ASR_BACKEND=whisper-streaming`, also gate the job worker's GPU tier on call session state. New env: `REALTIME_PAUSES_JOB_TIER=true`. Document the trade-off |
| Residual risk | Medium — multi-pipeline GPU scheduling is hard; users with mid-tier hardware are at risk of unexpected interactions |
| Owner | Architecture |

## R-RTP-012 — Outbox throughput under live-call load

| Field | Value |
|---|---|
| Impact | M (transcripts lag the call) |
| Likelihood | L on edge tier (low volume); M at scale |
| Risk | A long call generates many `transcript_partial` events; outbox publisher (#593) backlog grows; live UX degrades |
| Mitigation | `transcript_partial` events are intentionally ephemeral — consider not writing them to outbox at all and instead pushing directly to per-call SSE channel (bypassing the outbox for partials only; finals still go through). Trade-off: partials are not durable, but they're not meant to be |
| Residual risk | Low if architecture takes the bypass path for partials |
| Owner | Architecture (ADR-RTP-001 may need a sub-decision) |

## R-RTP-013 — Speaker diarization mid-call quality

| Field | Value |
|---|---|
| Impact | M (speaker labels wrong) |
| Likelihood | M (real-time diarization is harder than offline) |
| Risk | Pyannote's streaming/online mode is rudimentary; speaker labels can flip-flop during a call |
| Mitigation | Disable streaming diarization for milestone 1 (transcripts emit without speaker labels). Run offline diarization on the recording post-call for the final transcript. Document the trade-off |
| Residual risk | Low — explicit milestone 1 scope decision |
| Owner | Architecture |

## R-RTP-014 — Security: WebSocket endpoint exposed publicly

| Field | Value |
|---|---|
| Impact | H (potential abuse: anyone hitting the WSS endpoint could send fake audio) |
| Likelihood | M without proper auth |
| Risk | The Twilio Media Streams WSS URL is technically publicly reachable; without per-call auth, any attacker could connect and stream fake audio |
| Mitigation | Validate that an inbound WSS connection has a corresponding prior "ringing" control-plane event for the same `CallSid`. Reject WSS connections without a matching live session. Apply per-session-token check (Twilio's Stream parameters can include a Fortemi-issued session token) |
| Residual risk | Low with the session-token check; window of vulnerability is between control-plane "ringing" event and Twilio dialing the WSS |
| Owner | Security + Engineering |

## R-RTP-015 — Per-call session resource leaks

| Field | Value |
|---|---|
| Impact | M (memory/file descriptor growth over time) |
| Likelihood | L–M (every async task leak compounds) |
| Risk | Long-lived call sessions that don't clean up (WebSocket task, ASR session, codec state) leak resources |
| Mitigation | Per-session structured task with `Drop` impl that closes WS, closes ASR session, frees codec state. Integration test: open 100 sessions, close them, assert no leaks. Metric: `rtp_active_session_count` should match expected |
| Residual risk | Low with disciplined task lifecycle |
| Owner | Engineering |

## Summary

| Severity | Count |
|---|---|
| Critical | 2 (R-005, R-010) |
| High | 4 (R-001, R-002, R-004, R-011, R-014) |
| Medium | 8 (R-003, R-006, R-007, R-008, R-009, R-012, R-013, R-015) |

Critical risks (R-005 regulatory, R-010 Twilio config) are operational/documentation concerns, not engineering — but they can sink milestone 1 in production if not addressed in the release. Both have mitigation in documentation.

High-engineering risks (R-001, R-002, R-004, R-011, R-014) all have concrete mitigations in the ADRs.

## Open mitigations to file as child issues under #837

- Implement `RealtimeIngress` trait (R-RTP-001 mitigation) — covered by milestone 1 implementation
- Configure ASR failover (R-RTP-002 mitigation) — milestone 1 default config
- Implement rate-limit env vars + cost metric (R-RTP-003 mitigation) — milestone 1
- Document jurisdiction-specific consent requirements (R-RTP-005 mitigation) — release notes
- Pause Ollama tier when self-hosted streaming active (R-RTP-011 mitigation) — milestone 1 or 2
- Session-token validation for WSS handshake (R-RTP-014 mitigation) — milestone 1 security gate
