---
artifact: research-synthesis
project: fortemi
cluster: realtime-providers
epic: Fortemi/fortemi#837
status: draft-v1
last-updated: 2026-05-22
authors:
  - claude-opus-4-7 (orchestrator)
---

# Real-Time Provider Integration — Research Synthesis

## 1. Purpose

This document is the literature/landscape backbone for [Fortemi/fortemi#837 — Real-time provider integration](https://git.integrolabs.net/Fortemi/fortemi/issues/837). It maps the provider ecosystem (Twilio, WebRTC, SIP, video), identifies the streaming-ASR options available, and frames the architectural constraints that any implementation must navigate. Companion to [`architecture-sketch.md`](architecture-sketch.md) (boundary lines + flow) and the 4 ADRs under `.aiwg/architecture/realtime-providers/`.

This is **green-field research** from Fortemi's perspective: `roctinam/research-papers` contains no prior sources for real-time transcription, WebRTC, SIP/RTP, or media-server architecture (verified by query 2026-05-22). The candidate sources surfaced in §6 are net-new and will be filed as `[INDUCT]` issues.

## 2. Problem Statement

Fortemi today processes audio in **batch**: an attachment is uploaded (one-shot), `AudioTranscriptionHandler` calls the Speaches (faster-whisper) container's OpenAI-compatible `/v1/audio/transcriptions` endpoint, fans out long files into chunks, stores the transcript, and triggers diarization. The pipeline is **file-shaped**, not **stream-shaped**.

Real-time provider integration requires:

1. **Continuous audio frames** flowing in (PCM 16 kHz typical) — not whole-file uploads
2. **Sub-second partial transcripts** — incremental, updated as more audio arrives
3. **Speaker diarization mid-call** — incremental, not post-call
4. **Live transcript events** emitted to the outbox so downstream consumers (UI, alerts, agent-assist) see them in real time
5. **Provider control-plane signals** (call started, call ended, recording available) — these are webhook POSTs that fit Phase B (#817) of the incoming-streams roadmap

The current pipeline cannot do (1)–(3) without significant new work. (4) and (5) sit on top of Phase 1/Phase B of #586's roadmap and are tractable once the data plane is solved.

## 3. Provider Landscape

### 3.1 Categories of provider

| Category | What it provides | Examples | Key technical surface |
|---|---|---|---|
| **Telephony providers (PSTN/SIP)** | Phone calls (in/out), live audio streams of the call, control-plane webhooks, recording delivery | Twilio Programmable Voice, Vonage Voice, Plivo, Telnyx | Twilio Media Streams (WebSocket of PCM frames); Vonage NCCO + Voice WebSocket; SIP trunking |
| **Messaging providers** | SMS, WhatsApp, MMS webhooks | Twilio Messaging, Vonage, Plivo, Telnyx | Webhook POSTs (signed) — fits Phase B (#817) directly |
| **WebRTC SaaS** | Browser/mobile WebRTC sessions, server-side media access via SFU | LiveKit Cloud, Agora, Daily.co, Twilio Video, 100ms.live | WebRTC offer/answer; server-side SDK to consume tracks |
| **WebRTC OSS SFUs** | Self-hosted media server; you own the deployment | mediasoup, Janus, LiveKit OSS, Pion, ion-sfu | Same WebRTC protocols; you operate the SFU |
| **Video ingest providers** | Live video streaming endpoints (RTMP push, RTSP pull, HLS), recording-end webhooks | Mux, Cloudflare Stream, AWS IVS, recording aggregators | RTMP/RTSP/HLS; webhook for "recording available" |
| **Hosted real-time ASR APIs** | Streaming transcription as a service — replaces whisper for the live path | Deepgram Streaming, AssemblyAI Streaming, Speechmatics, Rev AI, OpenAI Realtime API (preview) | WebSocket streaming of audio; receive partial + final transcripts as JSON events |

Most production deployments combine categories. For example: Twilio Programmable Voice (telephony) **plus** Deepgram (hosted ASR) **plus** an OSS SFU isn't unusual; pick provider per concern.

### 3.2 Provider matrix — features relevant to Fortemi

| Provider | Live audio frames | Live video | Control webhooks | Recording webhooks | Server-side SDK (Rust) | Notes |
|---|:---:|:---:|:---:|:---:|:---:|---|
| Twilio Programmable Voice | Yes (Media Streams WS, μ-law/PCM) | No (Voice = audio only) | Yes (TwiML) | Yes | No — Twilio has no Rust SDK; HTTP API + WebSocket directly | Best-documented telephony streaming API in the industry |
| Twilio Video | Yes (via SFU) | Yes | Yes | Yes | No | WebRTC under the hood |
| Twilio Messaging | N/A | N/A | Yes (signed webhooks) | N/A | No | Phase B (#817) handles this with HMAC receivers |
| Vonage Voice | Yes (Voice WebSocket) | No | Yes (NCCO) | Yes | No | Similar shape to Twilio |
| LiveKit Cloud | Yes (WebRTC) | Yes | Yes | Yes | **Yes** (`livekit-rust` SDK exists) | Most Rust-native option |
| LiveKit OSS | Yes (WebRTC) | Yes | Yes | Configurable | Yes | Self-hosted SFU; ops burden |
| Agora | Yes | Yes | Yes | Yes | No | Larger, Chinese-founded; popular for global low-latency |
| Daily.co | Yes | Yes | Yes | Yes | No | Developer-friendly; smaller scale |
| Deepgram Streaming | N/A (consumer of) | N/A | N/A | N/A | No (HTTP/WS) | Drop-in streaming ASR; faster than self-hosted whisper |
| AssemblyAI Streaming | N/A | N/A | N/A | N/A | No (HTTP/WS) | Similar to Deepgram |
| OpenAI Realtime API | Yes (audio I/O) | No | Yes | No | No | Closed beta as of 2026-05; tight integration with GPT models |
| Mux | No (consumer-side) | Yes (RTMP ingest, HLS out) | Yes | Yes | No | Live video ingest + processing |

**Key takeaway:** **LiveKit is the only provider with a first-party Rust SDK**. Every other integration is HTTP + WebSocket directly. That's a significant point of decision for ADR-RTP-001 (architecture) and ADR-RTP-004 (first-provider).

## 4. Streaming-ASR Options

Live audio frames must be converted to text. Three architectural choices, each with sub-options:

### 4.1 Self-hosted streaming whisper (extending #576's sidecar)

| Option | What it is | Pros | Cons |
|---|---|---|---|
| **faster-whisper streaming** | Use the existing speaches container's "streaming" mode (recent faster-whisper supports streaming inference via VAD-based chunking) | Reuses existing infra (#576 work) | Latency is still 2–5 s typical; not "live" feeling |
| **whisper-streaming (Macháček et al., 2023)** | Open-source streaming wrapper around whisper using LocalAgreement-2 | Sub-second partials possible | Higher GPU load; less production-tested |
| **WhisperX with live VAD** | WhisperX with voice-activity-detection-driven incremental decoding | Better word-level timestamps, speaker labels | More moving parts; need recent versions |
| **NVIDIA Riva (ASR)** | NVIDIA's enterprise streaming ASR service | Production-grade; sub-100ms latency | Heavy infra dependency; NVIDIA-only |

### 4.2 Hosted streaming ASR API

| Service | Latency (typical) | Cost (typical Q1 2026) | Notes |
|---|---|---|---|
| Deepgram Nova-3 Streaming | <300 ms partial | $0.0036/min streaming | Industry-leading latency; good multi-language |
| AssemblyAI Streaming | <500 ms partial | $0.005/min | Strong English; speaker diarization streaming |
| Speechmatics | <500 ms | Enterprise pricing | EU-friendly; strong real-time |
| Rev AI Streaming | <1 s | $0.012/min | Quality-focused; slightly higher latency |
| OpenAI Realtime API | <500 ms (model variable) | Closed beta pricing | Bundled with GPT-4o reasoning; voice in/out |

### 4.3 Hybrid

- Use hosted ASR for the **live path** (partials shown in UI immediately)
- Re-process the recording with self-hosted whisper after the call ends (higher-quality final transcript, speaker labels, diarization)
- Best of both: low live latency + high final quality, at extra cost

### 4.4 Recommendation grounds for ADR-RTP-003

The choice depends on three factors:

1. **Latency requirement.** If "typing-style" sub-500 ms partials are required (agent-assist UX), hosted ASR wins decisively.
2. **Cost sensitivity.** Self-hosted whisper is free (compute owned); hosted ASR is $0.004–$0.012/min — adds up at scale.
3. **Edge-tier capability.** Whisper sidecar (#576) on edge can do streaming with the right wrapper but VRAM contention with Ollama (the original #576 problem) makes the edge case hard. Hosted ASR sidesteps this.

**Tentative recommendation (to be confirmed in ADR-RTP-003):**
- **Edge tier (6–8 GB VRAM):** hosted ASR only (whisper sidecar can't share GPU with Ollama for streaming)
- **Mid tier (12–16 GB):** hosted ASR by default; self-hosted whisper-streaming opt-in via env flag
- **High-end (24 GB+):** self-hosted by default; hosted ASR available as fallback

## 5. Architectural Constraints

### 5.1 Latency budget

For "live typing" UX, the end-to-end budget from audio capture to text appearing in the consumer UI:

| Stage | Budget | Notes |
|---|---|---|
| Provider → Fortemi (network ingress) | 50–150 ms | Twilio Media Streams ≈ 50 ms typical; SIP ≈ 100 ms |
| Frame buffering / VAD | 50–200 ms | Depends on chunking window |
| ASR inference (partial) | 100–500 ms | Hosted ASR < 300 ms; self-hosted whisper-streaming 500–1500 ms |
| Outbox write + Redis Stream publish | 5–20 ms | Negligible compared to ASR |
| Consumer (SSE) → UI render | 50–100 ms | Fortemi outbound stream + browser |
| **Total budget for "live" feeling** | **< 1500 ms** | Industry standard for "typing-style" UX |

This budget excludes the deeper-quality final transcript path (hybrid model). Self-hosted whisper on the edge tier may exceed 1500 ms; hosted ASR is reliably under.

### 5.2 GPU contention (links to #576)

The existing GPU sidecar lifecycle (#576) stops whisper/pyannote during Ollama tiers to free VRAM. For live transcription this can't happen — whisper has to be **always-on** during an active call. This implies:

- On single-GPU systems, **live transcription and AI revision can't happen concurrently**. Either:
  - Pause job worker during active calls (block tier transitions)
  - Or use hosted ASR for the live path
- High-end systems (multi-GPU or 24 GB+) can run both

ADR-RTP-003 must capture this trade-off explicitly.

### 5.3 Codec compatibility

Providers send audio in different codecs/formats:

| Provider | Audio format |
|---|---|
| Twilio Media Streams (μ-law) | μ-law 8 kHz PCM (default) or L16 16 kHz PCM (with `<Stream>` parameters) |
| Twilio Media Streams (Opus) | Opus 48 kHz (paid feature) |
| LiveKit / WebRTC | Opus 48 kHz typical; G.711 for SIP gateways |
| Deepgram Streaming | Accepts WAV, PCM, FLAC, Opus, MP3 (auto-detect or specify) |
| AssemblyAI Streaming | PCM 16 kHz mono (re-encode upstream) |
| Whisper (any variant) | Expects 16 kHz mono PCM internally; format conversion required |

**Implication:** every provider needs a codec-conversion shim before audio hits the ASR backend. For Twilio μ-law → PCM 16k → whisper: ~10 ms overhead per chunk; cheap. Document the conversion path in each provider's child issue.

### 5.4 Outbox integration (links to #586 Phase 1)

Every partial and final transcript event MUST write to `event_outbox` (the integration boundary established in #586). New event types:

- `transcript_partial` — { call_id, speaker?, text, is_final: false, timestamp }
- `transcript_final` — { call_id, speaker?, text, is_final: true, segment_id, timestamp }
- `call_event` — { call_id, event: 'started' | 'ended' | 'recording_available', metadata }

This means: **real-time provider integration depends on #591 (event_outbox) being live**. Phase A (#811) of incoming streams doesn't have this dependency, but the real-time provider epic does. Sequence accordingly.

### 5.5 Control-plane vs data-plane separation

A clean architectural pattern:

- **Control plane** (provider → Fortemi via HTTP webhooks): "call started", "call ended", "recording available". Goes through Phase B receivers (#817). HMAC-signed; idempotent; per-provider schema in the receiver registry (#820, #821).
- **Data plane** (provider → Fortemi via WebSocket/WebRTC): audio frames. Goes through a new media-handling layer — not Phase B.

This separation lets us **reuse Phase B fully for the control plane** and only build new infrastructure for the data plane. ADR-RTP-001 should make this explicit.

## 6. Candidate Sources for Induction

The following are gaps in `roctinam/research-papers` to be filled by `[INDUCT]` issues. Each will be filed as a separate INDUCT in the research-papers repo with a brief abstract.

### Standards / RFCs (high-priority — these are stable foundations)

1. **RFC 3550 — RTP: A Transport Protocol for Real-Time Applications** (Schulzrinne et al., 2003). The bedrock of all real-time media transport.
2. **RFC 3261 — SIP: Session Initiation Protocol** (Rosenberg et al., 2002). Telephony signaling.
3. **RFC 6716 — Definition of the Opus Audio Codec** (Valin et al., 2012). Default WebRTC codec.
4. **RFC 8825 — Overview: Real-Time Protocols for Browser-Based Applications** (Alvestrand, 2021). WebRTC umbrella spec.
5. **RFC 8829 — JavaScript Session Establishment Protocol (JSEP)** (Uberti et al., 2021). WebRTC offer/answer model.
6. **RFC 8825/8826/8827 family** — security considerations.

### Streaming-ASR research (high-priority — informs ADR-RTP-003)

7. **Whisper (Radford et al., 2022)** — `arXiv:2212.04356`. The base model Fortemi's pipeline already uses.
8. **whisper-streaming (Macháček et al., 2023)** — `arXiv:2307.14743`. The LocalAgreement-2 streaming wrapper.
9. **WhisperX (Bain et al., 2023)** — `arXiv:2303.00747`. Word-level timestamps + speaker diarization.
10. **Streaming RNN-T (Graves, 2012)** — `arXiv:1211.3711`. The Transducer architecture that hosts most production streaming ASR.
11. **Faster-Whisper engineering notes** — the C++ CTranslate2-backed inference engine used in the Fortemi speaches container.

### Provider documentation (high-priority — implementation surface)

12. **Twilio Media Streams documentation** — defines the WebSocket protocol for live audio from Programmable Voice. Includes audio format options, bidirectional support, mark/clear semantics. `www.twilio.com/docs/voice/twiml/stream`
13. **Twilio Voice webhook signing reference** — for HMAC verification in Phase B receivers.
14. **LiveKit Server SDK (Rust) reference** — `docs.livekit.io/server-sdk-rust`. The only first-party Rust SDK in the provider matrix; baseline option for ADR-RTP-001.
15. **Deepgram Streaming API reference** — `developers.deepgram.com/reference/listen-live` (or current location). Industry-leading hosted streaming ASR.
16. **AssemblyAI Streaming v3 reference** — comparable hosted alternative.

### OSS media servers (medium-priority — operations background)

17. **mediasoup architecture documentation** — `mediasoup.org/documentation/v3/`. SFU design + Rust integration notes.
18. **Janus architecture** — `janus.conf.meetecho.com/docs/`. Plug-in model.
19. **Pion (Go WebRTC)** — `github.com/pion/webrtc`. Different language but conceptually instructive.

### Industry benchmarks / comparisons (medium-priority — informs decisions)

20. **WebRTC vs Media Streams cost benchmark studies** — practitioner blog posts comparing TCO of Twilio Media Streams vs LiveKit OSS vs WebRTC self-hosting (search for 2024–2025 case studies).
21. **Streaming ASR latency benchmarks (independent)** — Picovoice / Voicegain etc. publish comparison tables. Useful for sanity-checking provider claims.

### Architecture papers (low-priority — design context)

22. **Real-time agent assist architectures (Salesforce, Genesys, etc.)** — case-study material on integrating live transcripts into agent workflows. Mostly white-paper grade.

### Total

22 candidate sources. Pareto: items 1–16 (16 sources) cover 90% of the design decisions. Items 17–22 are background reading for confidence rather than blockers.

**Plan:** file all 22 as `[INDUCT]` issues in `roctinam/research-papers` in a single batch with shared parent label `domain:realtime-providers`. INDUCT issues will be linked from #838's closure comment.

## 7. Open Questions for the Architecture Sketch

These flow into the architecture sketch (`architecture-sketch.md`) and ADRs:

1. **First-class provider for first iteration?** Twilio Programmable Voice (most-documented, largest market) vs LiveKit (Rust SDK, simpler integration)?
2. **Media-server vs direct provider integration?** Twilio Media Streams gives raw audio to a Fortemi-owned WebSocket endpoint — no media server needed. WebRTC providers require either client-side capture or a server-side SFU (LiveKit covers both).
3. **Hosted vs self-hosted ASR per tier?** Cost vs quality vs latency vs GPU contention.
4. **Where does the transcription pipeline live?** Inline in `matric-api` (Rust async task), or as a new sidecar service (`matric-rtp` or similar)?
5. **Diarization streaming vs batch?** Pyannote's streaming mode is rudimentary; offline diarization on the recording (post-call) gives better quality. Hybrid is likely.

## 8. Cost Picture (Preliminary)

Real-time provider integration has **non-trivial standby cost** because connections to providers/SFUs are long-lived. Approximate per-tier picture:

| Tier | Standby cost (when no calls) | Per-call active cost |
|---|---|---|
| Edge (6–8 GB VRAM) | Hosted ASR adapter idle: ~5 MB RAM. WebSocket listeners: nominal. | Per-minute hosted ASR charge ($0.004–0.012); no incremental GPU |
| Mid (12–16 GB) | Same + optional whisper-streaming warm-pool: 1.5 GB VRAM if enabled | Hosted ASR or self-hosted whisper; whisper consumes GPU for duration of call |
| High-end (24 GB+) | Self-hosted whisper warm-pool: 2.5 GB VRAM | Self-hosted whisper streaming; Ollama unaffected if >24 GB total |

**Defaults that respect the #586 cost gate principle:**
- `REALTIME_PROVIDERS_ENABLED=false` by default on all tiers (opt-in)
- `REALTIME_ASR_BACKEND=hosted` default when providers are enabled (hosted ASR has no GPU cost)
- Self-hosted whisper streaming is `REALTIME_ASR_BACKEND=whisper-streaming` opt-in

## 9. Conclusions / Direction for ADRs (v2 — standards-first framing)

**Operator direction (2026-05-22): do not bind to any specific provider; build to VoIP standards with adapter pattern for swappability.** The synthesis now points to:

1. **The deliverable is the abstraction.** The standards-shaped core (`MediaFrame`, `Codec`, `CallTransport`, `CallSession`, `CallControlEvent`) is the architectural anchor of milestone 1. Twilio is the **first concrete adapter**, not the architectural anchor. A Mock adapter ships alongside Twilio so the trait surface is exercised by two implementations from day one — preventing accidental Twilio-shaping of the abstraction.

2. **Milestone 1 stack (the first proof-of-abstraction):**
   - Standards-shaped core (`crates/matric-rtp/`)
   - Twilio Programmable Voice adapter (WebSocket binding, per ADR-RTP-002)
   - Mock adapter (test/CI binding)
   - Deepgram streaming ASR (hosted, per ADR-RTP-003 — sidesteps #576 GPU contention)
   - Outbox emission of standards-shaped `transcript_partial` and `transcript_final` events
   - Hybrid pattern: live transcripts via Deepgram, batch re-transcription via existing `AudioTranscriptionHandler` post-call

3. **Milestone 2: LiveKit adapter (WebRTC binding).** Second concrete adapter; surfaces leaks in the trait that single-adapter milestone 1 couldn't catch. Adds browser/mobile + WebRTC capability. First-party Rust SDK (livekit-rust) lowers integration risk.

4. **Milestone 3: SIP/RTP binding (two adapters).** SIP-direct adapter (Fortemi terminates SIP/RTP — true standards alignment, no vendor dependency) plus Twilio SIP Trunking adapter (SIP-to-Twilio-Voice shim that reuses milestone 1 WS binding). Both ship to address PSTN-without-Twilio-Voice use cases.

5. **Milestone 4+ (as demanded):** Vonage Voice WS, Agora WebRTC, Daily.co, video providers (Mux, IVS), self-hosted whisper-streaming for cost-sensitive deployments. Each is a single adapter slotting into the existing trait — no core changes.

This phasing makes each milestone shippable independently, treats SIP/RTP as a peer transport (not deferred indefinitely), and respects the cost gate (everything opt-in, hosted ASR by default). Most importantly: **the trait surface is the deliverable** — adapters can be added forever without revisiting the architecture.

## 10. References (so far)

This synthesis cites no `REF-XXX` sources because the `roctinam/research-papers` corpus is empty on this topic. The candidate sources in §6 will be filed as INDUCT issues; once inducted they will be referenced by REF number in subsequent revisions of this synthesis.

## 11. Open issues for the Fortemi-side artifact set

After this synthesis, the next artifacts to produce (covered in subsequent tasks of #838):

- `architecture-sketch.md` — boundary lines, dataflow diagram, integration points
- `risk-register.md` — provider lock-in, codec compatibility, GPU contention, cost variance, regulatory (call recording consent laws vary by jurisdiction)
- `feature-plan.md` — construction-ready plan for milestone 1 (Twilio + Deepgram + outbox)
- 4 ADRs under `.aiwg/architecture/realtime-providers/`:
  - ADR-RTP-001: Architectural pattern (provider-direct vs media-server-mediated)
  - ADR-RTP-002: Transport choice for live audio (WebSocket vs WebRTC vs SIP)
  - ADR-RTP-003: ASR backend strategy (hosted vs self-hosted vs hybrid, per tier)
  - ADR-RTP-004: First-provider selection (Twilio Voice as recommended first milestone)
- Child-issue plan for milestone 1 implementation under #837
