# ADR-RTP-001 — Architectural Pattern for Real-Time Provider Integration

**Status:** Proposed (draft) • Revised 2026-05-22 (v2 — standards-first framing per operator direction)
**Epic:** [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
**Companions:** ADR-RTP-002, ADR-RTP-003, ADR-RTP-004

## Context

Fortemi must integrate with real-time providers (Twilio, LiveKit, Agora, SIP gateways, video providers) to support live audio/video pipelines. The architectural question has two layers:

1. **Internal abstraction** — what shape do the core types and traits inside Fortemi take? Provider-specific (Twilio-shaped JSON envelopes, LiveKit room IDs, etc.) or standards-shaped (RTP frames, SIP-style call lifecycle, codec descriptors)?
2. **External integration topology** — for any given provider, does the provider connect directly to a Fortemi endpoint (provider-direct), or do we sit behind a media server (SFU-mediated)?

Operator direction (2026-05-22): **do not bind to any specific provider**. Build the internal abstraction to **VoIP standards** (RTP / SIP / codec descriptors per RFC 6716 etc.) and treat every provider — Twilio, LiveKit, Vonage, SIP-direct, video — as a **swappable adapter** at the boundary.

## Decision

**Adopt a standards-first adapter pattern.**

The internal abstraction is shaped in VoIP-standard terms:

- **Call session** — a typed state machine modeled on SIP call lifecycle (`Ringing`, `EarlyMedia`, `Active`, `OnHold`, `Ended`)
- **Media transport** — a stream of `MediaFrame { payload_type, codec, sample_rate, channels, timestamp_rtp, payload }` records, conceptually equivalent to RTP packets minus the wire format
- **Codec descriptors** — `Codec::PcmuG711`, `Codec::Opus { sample_rate, channels }`, `Codec::L16 { sample_rate, channels }`, etc., aligned with IANA media-type registry
- **Control events** — `CallControlEvent::CallStarted`, `RecordingAvailable { url }`, `Dropped { reason }`, etc., shaped on SIP signaling concepts

Every concrete provider integration is an **adapter** that translates the provider's specific wire format into this internal abstraction. The rest of the system (transcript emitter, session manager, outbox writers, consumers) speaks **only** the standards-shaped types. Provider-specific identifiers (Twilio `CallSid`, LiveKit `room_name`, SIP `Call-ID`) live inside the adapter and are surfaced only through a `provider_call_id: String` field on the call session.

**Integration topology is per-provider**, chosen by the adapter:

- **Twilio Voice adapter (milestone 1)** — provider-direct over WebSocket (Twilio dials a Fortemi-owned WSS endpoint)
- **LiveKit adapter (milestone 2)** — media-server-mediated (Fortemi participates as a server-side participant in a LiveKit room)
- **SIP-direct adapter (milestone 3+)** — provider-direct over SIP/RTP (Fortemi terminates SIP, ingests RTP)
- **Vonage / Telnyx / generic SIP-trunk adapters** — slot into either pattern depending on the provider

All adapters implement the same `CallTransport` trait. The trait's surface is standards-shaped; the adapter handles wire-format translation.

## Rationale

### Why standards-first internal abstraction

| Failure mode | What it looks like | How standards-first prevents it |
|---|---|---|
| Twilio-isms leak into core | `CallSession::twilio_call_sid()` accessor, `if matches!(provider, Provider::Twilio)` branching elsewhere | Core code never sees Twilio-specific types; can't know which provider is in use |
| Swap costs are high | Adding LiveKit means rewriting downstream consumers because they assumed Twilio's event shape | Downstream consumers depend on the standard abstraction; new adapters slot in without touching them |
| Multi-provider deployments fragment | Some calls have Twilio shape, some have LiveKit shape, code branches everywhere | Every call has the same standards-shaped events regardless of upstream provider |
| Standards drift inside the codebase | Codec strings stored as freeform `"mulaw_8000"`; sample rates as `i32` without context | Typed `Codec` enum; sample rates carry units in their type signature |

### Why an adapter pattern (not direct provider SDK use)

- **Future providers don't require core changes** — adding Vonage means writing one adapter module
- **Testability** — a `MockAdapter` exercises the same trait surface as Twilio without touching the network
- **Operational flexibility** — operators can enable only the adapters they need; disabled adapters carry zero runtime cost
- **Standards compliance is enforceable** — the trait surface is the contract; CI lints can verify adapters never bypass it

### Why Twilio still ships first (consequence, not architectural anchor)

ADR-RTP-004 selects Twilio Programmable Voice as the *first concrete adapter implementation*. This is a sequencing decision, not an architectural one. The adapter pattern means:

- The first adapter validates the abstraction against a well-documented, widely-used provider
- Subsequent adapters (LiveKit, Vonage, SIP-direct) exercise the same trait — if the abstraction holds, they slot in cleanly
- If the abstraction *doesn't* hold, the second adapter will surface the leak, and the trait can be refined while the surface is still small

This is the "first concrete implementation of a contract" pattern, not the "first provider that defines the contract" pattern.

### Integration topology is an adapter decision, not a global one

Different providers fit different topologies:

| Provider | Best topology | Why |
|---|---|---|
| Twilio Voice (PSTN) | Provider-direct WS | Twilio dials our endpoint; no benefit to inserting an SFU |
| LiveKit (WebRTC) | Media-server-mediated | WebRTC needs SDP negotiation + ICE; LiveKit handles that |
| SIP-direct (enterprise PBX) | Provider-direct SIP/RTP | We terminate SIP directly; no third party |
| Twilio SIP Trunking → Voice | Provider-direct WS (via Twilio) | Twilio's existing infrastructure handles SIP-to-WS |
| Vonage Voice WebSocket | Provider-direct WS | Same shape as Twilio MS |
| Agora | Media-server-mediated | Similar shape to LiveKit |

ADR-RTP-001's decision is not "everyone is provider-direct" or "everyone is SFU-mediated" — it's that **the choice belongs to the adapter**, and the core never knows which one is in use.

## Rationale

| Factor | Provider-direct | Media-server-mediated |
|---|---|---|
| Operational footprint | Zero (no SFU to run) | Either run an SFU or pay for SaaS |
| Engineering complexity | Per-provider WS handler (~500 LOC each) | One SDK integration covers all providers it supports |
| Codec handling | Per-provider in normalizer | Often handled by SFU |
| Scaling story | Each Fortemi instance handles all provider traffic for its calls | SFU scales independently |
| Audio quality | Direct path from provider; no extra hops | Extra hop adds 10–30 ms |
| Vendor lock-in | Per-provider integration; portable | Tied to chosen SFU |
| Twilio Media Streams | Natural fit (Twilio dials a Fortemi WSS URL) | Possible but awkward |
| WebRTC browser/mobile | Doesn't fit cleanly | Natural fit |
| First-class Rust SDK | None for major providers | LiveKit-rust exists |

The two patterns are complementary, not exclusive. **Milestone 1 is Twilio Voice** (per ADR-RTP-004), which is unambiguously a provider-direct fit and has the most-documented integration surface. Adding media-server-mediated later is straightforward because the call session manager already abstracts the ingestion boundary.

## Consequences

### Positive

- Adding a new provider is a contained engineering task (one adapter module, no core changes)
- Multi-provider deployments work without code branching outside adapters
- The internal abstraction stays aligned with VoIP standards — engineers familiar with RTP/SIP can read the core without learning Twilio's idioms first
- Testing is provider-agnostic — `MockAdapter` exercises the full pipeline without external dependencies
- Operators choose which adapters to compile / enable; lock-in is zero
- Architectural pivot to SFU-mediated for a given provider is local to that adapter

### Negative

- Designing a good adapter trait requires understanding multiple providers up front (more design work before the first line of provider code)
- Twilio's particularly-rich feature set (DTMF, mark/clear semantics, bidirectional audio) needs to be expressible through the trait or escaped via per-adapter capability flags
- The first adapter must be implemented carefully — it sets the precedent; pre-emptive refactoring is expected as the second adapter (LiveKit) reveals gaps

### Implications for code organization

- `crates/matric-rtp/` — new crate housing the standards-shaped types, traits, and the call session manager (extraction from `matric-api` once trait surface stabilizes; may start as a module under `matric-api/src/realtime/` for milestone 1)
- `crates/matric-rtp/src/types.rs` — `MediaFrame`, `Codec`, `CallSession`, `CallControlEvent` (the standards-shaped abstraction)
- `crates/matric-rtp/src/transport.rs` — `CallTransport` trait + `CallSessionManager`
- `crates/matric-rtp/src/adapters/twilio/` — first concrete adapter (milestone 1)
- `crates/matric-rtp/src/adapters/livekit/` — second adapter (milestone 2)
- `crates/matric-rtp/src/adapters/sip/` — third adapter (milestone 3+)
- `crates/matric-rtp/src/adapters/mock/` — test fixture
- `crates/matric-rtp/src/asr/` — ASR backend trait + impls (Deepgram, AssemblyAI, whisper-streaming) — also adapter-shaped per ADR-RTP-003

## Trait Surface (Sketched)

The adapter trait is the contract. Sketched here so future ADRs and the architecture sketch reference a consistent shape:

```rust
/// Wire-format-agnostic media frame. Conceptually equivalent to an RTP packet
/// minus the network framing. Adapters translate provider-specific frames into
/// (or out of) this shape.
pub struct MediaFrame {
    pub codec: Codec,
    pub timestamp_rtp: u32,        // RTP timestamp semantics (codec-rate ticks)
    pub sequence: u32,             // Monotonic per-session
    pub marker: bool,              // RTP marker bit (often = start of talkspurt)
    pub payload: Bytes,            // Raw codec payload (μ-law, Opus frame, L16 PCM, etc.)
}

/// Codec identification aligned with IANA media-type registry.
pub enum Codec {
    PcmuG711 { sample_rate: u32 },    // μ-law; sample_rate normally 8000
    PcmaG711 { sample_rate: u32 },    // A-law
    Opus { sample_rate: u32, channels: u8 },
    L16 { sample_rate: u32, channels: u8 },  // Linear PCM (uncompressed)
    Telephone { event_code: u8 },     // DTMF / RFC 4733 named events
}

/// SIP-style call lifecycle state.
pub enum CallState {
    Ringing,
    EarlyMedia,
    Active,
    OnHold,
    Ended { reason: EndReason },
}

/// Adapter contract. Every concrete provider implements this trait.
#[async_trait]
pub trait CallTransport: Send + Sync {
    fn adapter_name(&self) -> &str;             // "twilio", "livekit", "sip-direct"
    fn provider_call_id(&self) -> &str;          // adapter-specific opaque ID

    /// Inbound media frames from the provider.
    fn frames(&mut self) -> impl Stream<Item = MediaFrame>;

    /// Outbound media frames toward the provider (TTS, hold music, etc.).
    /// Optional — some adapters are read-only initially (milestone 1 Twilio).
    async fn send_frame(&mut self, frame: MediaFrame) -> Result<()> { Err(NotSupported) }

    /// Control events (state transitions, recording-available, DTMF, etc.).
    fn control_events(&mut self) -> impl Stream<Item = CallControlEvent>;

    /// Initiate call teardown.
    async fn end_call(&mut self, reason: EndReason) -> Result<()>;
}
```

The codec normalizer + ASR adapter operate on `MediaFrame` streams from any `CallTransport` impl. Provider-specific behavior never reaches them.

## Alternatives Considered

### Alternative A — Media-server-mediated only

Run LiveKit OSS in the bundle; route every provider through it. Twilio Voice would dial into LiveKit's telephony gateway, which would expose the audio to Fortemi via LiveKit's protocol.

Rejected because:
- Adds substantial operational footprint to the bundle (LiveKit's process model, codec workers, TURN server)
- Twilio Voice → LiveKit telephony is a roundabout path with extra cost
- LiveKit OSS deployment is non-trivial for edge-tier users
- Doesn't reduce engineering work meaningfully — we'd still need Twilio-specific config

### Alternative B — Provider-direct only (no media-server pattern, ever)

Implement per-provider handlers for every WebRTC provider too.

Rejected because:
- WebRTC requires SDP negotiation, ICE candidate gathering, DTLS-SRTP — substantial work to roll our own
- Browser/mobile clients fundamentally need a media server to negotiate with; provider-direct doesn't fit
- LiveKit OSS handles all this for free with a Rust SDK

### Alternative C — Adopt OpenAI Realtime API as the universal upstream

Use OpenAI's Realtime API as the unified upstream; Fortemi only integrates with one provider; OpenAI handles voice in/out.

Rejected because:
- Vendor lock-in to a single (preview) API
- OpenAI Realtime doesn't (currently) accept arbitrary RTP/SIP/Twilio audio — clients must connect via OpenAI's specific endpoints
- Inverts the architecture (Fortemi becomes a thin wrapper over OpenAI); reduces what Fortemi uniquely provides

## References

- `../streaming-realtime/research-synthesis.md` §3 (provider matrix)
- `../streaming-realtime/architecture-sketch.md` §1 (two-plane separation), §3 (one-call walkthrough)
- ADR-RTP-002 (transport choice — direct outgrowth of this decision)
