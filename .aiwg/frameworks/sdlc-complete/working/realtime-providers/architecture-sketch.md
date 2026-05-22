---
artifact: architecture-sketch
project: fortemi
cluster: realtime-providers
epic: Fortemi/fortemi#837
status: draft-v1
last-updated: 2026-05-22
authors:
  - claude-opus-4-7 (orchestrator)
---

# Real-Time Provider Integration — Architecture Sketch

Companion to `research-synthesis.md`. Captures the boundary lines and the live-audio dataflow. The decisions named here (`<--TBD-->` markers) will be locked by the 4 ADRs.

## 1. Two Planes

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  CONTROL PLANE (HTTP webhooks — reuses Phase B receivers #817)              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Provider                                                                  │
│   (Twilio,    ─── POST /api/v1/webhooks/incoming/{slug} ───┐                │
│    LiveKit,                       (HMAC-signed, idempotent) │                │
│    SIP gw)                                                  │                │
│                                                             ▼                │
│                                                       event_outbox          │
│                                                             │                │
│                                                             ▼                │
│                                                       Redis Stream          │
│                                                       (call_event)          │
│                                                             │                │
│                                                             ▼                │
│                                                       Consumers             │
│                                                       (SSE, WS, webhooks)   │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  DATA PLANE (audio frames — NEW infrastructure)                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Provider                                                                  │
│   (Twilio    ─── WSS audio frames ──┐                                       │
│    Media                            │                                       │
│    Streams,                         ▼                                       │
│    LiveKit,            ┌─────────────────────────┐                          │
│    SIP)                │  Media ingress layer    │                          │
│                        │  (Twilio MS handler /   │                          │
│                        │   LiveKit consumer /    │                          │
│                        │   SIP gateway)          │                          │
│                        └────────────┬────────────┘                          │
│                                     │ PCM 16kHz mono frames                 │
│                                     ▼                                       │
│                        ┌─────────────────────────┐                          │
│                        │  Codec normalizer       │  μ-law → PCM,           │
│                        │  + chunk buffer (VAD)   │  Opus → PCM, etc.       │
│                        └────────────┬────────────┘                          │
│                                     │                                       │
│                                     ▼                                       │
│                        ┌─────────────────────────┐                          │
│                        │  Streaming ASR adapter  │  <--TBD ADR-RTP-003-->  │
│                        │  hosted | self-hosted   │  hosted = Deepgram      │
│                        └────────────┬────────────┘  self  = whisper-stream │
│                                     │                                       │
│                                     │ {partial, final, speaker?, ts}        │
│                                     ▼                                       │
│                        ┌─────────────────────────┐                          │
│                        │  Transcript emitter     │  Writes to outbox        │
│                        └────────────┬────────────┘  in PG transaction       │
│                                     │                                       │
│                                     ▼                                       │
│                                event_outbox                                 │
│                                     │                                       │
│                                     ▼                                       │
│                              Redis Stream                                   │
│                              (transcript_partial /                          │
│                               transcript_final)                             │
│                                     │                                       │
│                                     ▼                                       │
│                              Consumers (SSE → UI,                           │
│                                         agent assist,                       │
│                                         downstream jobs)                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key invariant:** both planes converge on `event_outbox`. Whether an event originates from a webhook (control) or a live audio stream (data), it lands in the same outbox table and flows through the same Phase 1 publisher (#593) and consumers (#594–#596). No new event bus.

## 2. Component Map

### 2.1 Control plane — already covered by Phase B

Existing infrastructure under Phase B (#817):
- `POST /api/v1/webhooks/incoming/{slug}` (#818)
- Receiver registry (#819) — per-provider config with HMAC secret + schema
- HMAC verification (#820)
- Schema validation (#821)
- Idempotency (#822)
- Outbox write

**New work for this epic:** register per-provider receiver schemas. Each provider gets a slug, e.g., `twilio-voice-events`, `livekit-room-events`. Schema captures the provider's specific event shape (Twilio's `CallSid` vs LiveKit's `room_name`). **No new endpoint code** — purely configuration.

### 2.2 Data plane — new infrastructure

Six new components, all under `crates/matric-rtp/` (new crate) or within `crates/matric-api/src/realtime/` (new module — preferred for first iteration to avoid crate-graph churn):

| Component | Responsibility | Crate / Module |
|---|---|---|
| **Media ingress handlers** | Per-provider WebSocket / WebRTC endpoints | `matric-api/src/realtime/ingress/` |
| **Codec normalizer** | μ-law / Opus / G.711 → PCM 16 kHz mono | `matric-rtp/src/codec.rs` |
| **Chunk buffer with VAD** | Buffer frames; emit chunks at silence boundaries or fixed-window | `matric-rtp/src/buffer.rs` |
| **Streaming ASR adapter** | Trait + impls for Deepgram, AssemblyAI, whisper-streaming | `matric-rtp/src/asr/` |
| **Transcript emitter** | Write `transcript_partial` / `transcript_final` events to outbox | `matric-rtp/src/emitter.rs` |
| **Call session manager** | Map provider-specific call IDs to internal `call_id`; manage per-call task lifecycle | `matric-rtp/src/session.rs` |

**Recommendation:** start as `crates/matric-api/src/realtime/` (a module, not a new crate) for milestone 1. Extract to `crates/matric-rtp` once the trait surface stabilizes (likely milestone 2 or 3).

### 2.3 Why a new crate eventually

- **Compile-time isolation:** real-time work has tight latency budgets and shouldn't be slowed by `matric-api`'s much larger crate
- **Dependency hygiene:** real-time provider SDKs (livekit-rust, twilio HTTP clients) shouldn't bleed into the API surface
- **Testability:** a dedicated crate makes integration tests against mock providers cleaner

But: premature crate-extraction adds friction. The module-first approach lets iteration happen in `matric-api` until the surface is right, then extract.

## 3. Dataflow — One Call End-to-End

A walkthrough of a Twilio inbound call with Deepgram streaming ASR (milestone 1 reference):

```
1. Caller dials Fortemi-owned Twilio number
2. Twilio fires HTTP POST → /api/v1/webhooks/incoming/twilio-voice-events
   Body: { CallSid: "CA123", From: "+1...", To: "+1...", CallStatus: "ringing" }
   → Phase B HMAC verification + schema validation
   → outbox row: call_event { event: 'ringing', call_id: 'CA123', ... }

3. Fortemi responds with TwiML pointing to:
   wss://fortemi.example.com/realtime/twilio/CA123
   (TwiML <Connect><Stream url="..."/></Connect>)

4. Twilio opens WebSocket to that URL
   → matric-api `/realtime/twilio/{call_sid}` handler accepts
   → Session manager creates call session: call_id = CA123, asr_backend = Deepgram

5. Audio frames flow:
   Twilio (μ-law 8kHz) → media ingress handler
   → codec normalizer (μ-law → PCM 16kHz)
   → chunk buffer (VAD; emit on 500ms silence or 1.5s max window)
   → ASR adapter (Deepgram streaming WS)
   → transcript events received from Deepgram
   → transcript emitter writes to outbox in PG tx

6. Outbox publisher (#593) picks up events:
   → Redis Stream `transcript_partial:CA123`
   → SSE consumer (#594) pushes to subscribed UI clients
   → WS dispatcher (#595) pushes to active WebSocket sessions
   → Webhook dispatcher (#595) fires outbound webhooks if registered

7. Caller hangs up
   → Twilio closes WebSocket
   → Twilio fires control webhook: CallStatus=completed
   → Phase B receiver writes outbox row
   → Session manager closes the call session
   → Optional: schedule batch re-transcription of the recording (higher quality)
```

## 4. Boundary Lines (where new code stops and existing code begins)

| Boundary | New code on this side | Existing code on this side |
|---|---|---|
| **Provider webhooks → outbox** | Per-provider schema registration | Phase B receivers (#817 family — already filed) |
| **Provider audio WSS → media ingress** | Per-provider ingress handler (new) | n/a — this is the new layer |
| **Audio frames → ASR adapter** | Codec normalization, VAD chunking, ASR adapter trait | n/a |
| **ASR output → outbox** | Transcript emitter (new) | `event_outbox` table (#591) — uses existing insert helpers (#592) |
| **Transcript events → consumers** | Per-call_id event-type registration | Outbox publisher (#593), SSE (#594), WS (#595), metrics (#596) — already designed |

## 5. Where the Whisper Sidecar (#576) Fits

`#576` introduced GPU-exclusive sidecar lifecycle: stop whisper/pyannote during Ollama tiers to free VRAM. For live transcription that lifecycle pattern **does not apply** because whisper needs to be always-on during an active call.

Three resolutions, in order of preference:

1. **Hosted ASR for the live path** (recommended for milestone 1): bypasses the GPU contention entirely. `#576` continues to operate unchanged for the batch path; live path uses Deepgram.
2. **Pause Ollama tier during active calls**: when a call session is active, the job worker pauses tier transitions and keeps the whisper sidecar warm. Recovery: ends-of-call event triggers resumption. Higher-engineering cost; only viable on mid/high-end tiers.
3. **Multi-GPU**: dedicate one GPU to whisper, another to Ollama. Only viable on high-end deployments.

ADR-RTP-003 will lock the per-tier choice. The architecture supports all three because the ASR adapter is a trait — the call session manager picks the backend at session start based on configuration.

## 6. Call Session Identity

A persistent ID space:

| ID | Source | Lifetime | Purpose |
|---|---|---|---|
| `call_id` (UUID) | Generated by Fortemi on session start | Forever (stored with transcript records) | Internal stable identifier |
| `provider_call_id` (string) | Provider-assigned (e.g., Twilio's `CallSid`) | Provider-dependent | Lookup, debugging |
| `session_token` (opaque) | Generated by Fortemi for each WebSocket handshake | Until call ends | Auth for the WebSocket connection |

The session manager maintains a `(provider, provider_call_id) → call_id` lookup. Control-plane events (Phase B) carry `provider_call_id`; data-plane events carry `call_id`. The session manager bridges them.

## 7. Storage

New tables (migration filed under milestone 1):

| Table | Purpose |
|---|---|
| `call_sessions` | One row per call: `call_id`, `provider`, `provider_call_id`, `started_at`, `ended_at`, `asr_backend`, `metadata` |
| `transcript_segments` | One row per final transcript segment: `id`, `call_id`, `speaker_label?`, `text`, `start_ts`, `end_ts`, `confidence?` |

Partials (`transcript_partial` events) are **NOT persisted** — they live only in the outbox/stream and are intentionally ephemeral (the next partial supersedes them). Only `transcript_final` segments land in `transcript_segments`.

Recordings (when available — Twilio provides them post-call) are processed by the existing attachment + `AudioTranscriptionHandler` pipeline. This produces the high-quality batch transcript that supplements the live one.

## 8. Authentication / Authorization

Two distinct auth surfaces:

### 8.1 Inbound (provider → Fortemi)

- **Control plane (webhooks)**: HMAC per the Phase B receiver registry (#817 family). Each provider's signing format documented per-slug.
- **Data plane (WebSocket / WebRTC)**: per-provider mechanism.
  - Twilio Media Streams: the WSS URL embeds the `CallSid` and Fortemi validates it against the prior control-plane "ringing" event (must have arrived first). No additional auth required because Twilio dialed the URL.
  - LiveKit: server-to-server tokens (JWT) issued by Fortemi for the LiveKit room.
  - SIP: TLS + SIP digest auth via gateway.

### 8.2 Outbound (Fortemi → providers)

For control commands (e.g., dial out, end call):
- Per-provider API key stored in `inference_provider_credentials` table or equivalent
- Loaded via `OLLAMA_BASE`-style env vars per provider (`TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN`, `LIVEKIT_API_KEY`, `LIVEKIT_API_SECRET`)
- Per `token-security` rule: never logged, never committed, file-mode 600

## 9. Failure Modes

| Failure | Detection | Recovery |
|---|---|---|
| Provider WebSocket drops mid-call | WSS close event | Emit `call_event { event: 'dropped' }`; client/provider initiates reconnect; resume with last partial |
| ASR backend unreachable (hosted) | HTTP error / WebSocket close | Failover to secondary ASR backend (e.g., AssemblyAI if Deepgram down); if no secondary, emit `transcript_error` event and continue capturing audio for post-call batch |
| ASR backend high latency | Per-frame latency exceeds threshold | Emit metric; consider failover; never block the audio buffer |
| Codec mismatch / decode failure | Codec normalizer returns error | Log + skip frame; emit metric; do not crash the session |
| Outbox write fails | PG error | Buffer events in memory for N seconds; retry; if buffer overflows, drop oldest and emit metric |
| Pyannote crash mid-call | Process exit | Continue ASR without speaker labels; emit metric; resume on next call |

## 10. Metrics (per call session)

| Metric | Type | Purpose |
|---|---|---|
| `rtp_call_active_count` | Gauge | Active call sessions |
| `rtp_audio_frame_latency_ms` | Histogram | Provider → ingress latency |
| `rtp_codec_decode_failures_total` | Counter | Codec error rate |
| `rtp_asr_partial_latency_ms` | Histogram | Audio → first partial round-trip |
| `rtp_asr_failover_total` | Counter | Backend failover events |
| `rtp_outbox_write_failures_total` | Counter | Outbox write errors |
| `rtp_session_duration_seconds` | Histogram | Per-call duration |

Surface on `/api/v1/health/streaming` (coordinated with #596 and Phase A #814).

## 11. Open Architecture Decisions (drive the 4 ADRs)

1. **ADR-RTP-001 — Architectural pattern**: Provider-direct (Fortemi owns the WebSocket from Twilio) vs media-server-mediated (LiveKit SFU between provider and Fortemi). Provider-direct is simpler; media-server-mediated handles many WebRTC providers uniformly. **Tentative: provider-direct for milestone 1; media-server pattern when WebRTC is added in milestone 2.**

2. **ADR-RTP-002 — Transport choice**: WebSocket (Twilio Media Streams) vs WebRTC (LiveKit / Agora / direct browser) vs SIP (PSTN, enterprise). **Tentative: WebSocket first (lowest engineering risk); WebRTC second.**

3. **ADR-RTP-003 — ASR backend strategy**: Hosted (Deepgram default) vs self-hosted (whisper-streaming) vs hybrid (live = hosted, batch re-process = self-hosted). **Tentative: hosted default on all tiers; self-hosted opt-in via env on mid/high-end.**

4. **ADR-RTP-004 — First-provider selection**: Twilio Voice vs LiveKit vs WebRTC-direct. **Tentative: Twilio Voice for milestone 1 (most-documented API, no SFU required, gives raw audio over WSS that Fortemi owns).**

## 12. References

- Companion: `research-synthesis.md` (this directory)
- Parent epic: [Fortemi/fortemi#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
- Scoping issue: [Fortemi/fortemi#838](https://git.integrolabs.net/Fortemi/fortemi/issues/838)
- Phase B receivers: [Fortemi/fortemi#817](https://git.integrolabs.net/Fortemi/fortemi/issues/817)
- Outbox dependency: [Fortemi/fortemi#591](https://git.integrolabs.net/Fortemi/fortemi/issues/591)
- GPU sidecar lifecycle: [Fortemi/fortemi#576](https://git.integrolabs.net/Fortemi/fortemi/issues/576) (closed)
- Incoming streams roadmap: `../streaming-realtime/incoming-streams-roadmap.md`
