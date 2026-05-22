# ADR-RTP-001 — Architectural Pattern for Real-Time Provider Integration

**Status:** Proposed (draft) • 2026-05-22
**Epic:** [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
**Companions:** ADR-RTP-002, ADR-RTP-003, ADR-RTP-004

## Context

Fortemi must integrate with real-time providers (Twilio, LiveKit, Agora, SIP gateways, video providers) to support live audio/video pipelines. There are two viable architectural patterns:

1. **Provider-direct**: Fortemi exposes per-provider WebSocket / WebRTC endpoints; the provider connects directly to Fortemi. No intermediate media server.
2. **Media-server-mediated**: All providers route through an OSS or SaaS SFU (e.g., LiveKit). Fortemi consumes from the SFU using a single API.

The choice affects everything downstream: operational footprint, codec handling, scaling story, and the cost picture.

## Decision

**Adopt provider-direct architecture for milestone 1.** Add media-server-mediated pattern (LiveKit) for milestone 2 to cover WebRTC use cases that don't fit provider-direct.

The two patterns coexist: each provider gets the integration shape that best fits it. Twilio Voice → provider-direct. LiveKit room → media-server-mediated (Fortemi as LiveKit participant). SIP → media-server-mediated via a SIP gateway (LiveKit Cloud Telephony or similar).

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

- Milestone 1 ships with zero new operational dependencies (no SFU to deploy)
- Twilio Media Streams pattern is well-trodden; Fortemi engineering risk is contained
- The architecture supports both patterns long-term — no future architectural pivot required

### Negative

- Per-provider integration code accumulates over time (Twilio handler, Vonage handler, etc.)
- WebRTC support is deferred to milestone 2
- We don't get the operational benefits of one-SFU-handles-all until milestone 2

### Implications for code organization

- `crates/matric-api/src/realtime/ingress/twilio.rs` (milestone 1)
- `crates/matric-api/src/realtime/ingress/livekit.rs` (milestone 2, when added)
- `crates/matric-api/src/realtime/session.rs` (call session manager — shared across all providers)
- Per-provider handlers implement a common `RealtimeIngress` trait so the rest of the system is agnostic

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
