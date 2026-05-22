# ADR-RTP-002 — Transport Choice for Live Audio

**Status:** Proposed (draft) • Revised 2026-05-22 (v2 — re-framed as per-adapter transport binding)
**Epic:** [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
**Companions:** ADR-RTP-001 (adapter pattern), ADR-RTP-003 (ASR backend), ADR-RTP-004 (first provider)

## Context

Three viable transport protocols carry live audio between a provider and Fortemi:

1. **WebSocket (WSS)** — long-lived TCP+TLS connection; provider streams binary frames. Used by Twilio Media Streams, Vonage Voice WebSocket, AWS IVS, hosted ASR APIs.
2. **WebRTC (UDP-based)** — browser-native real-time protocol; SDP negotiation, ICE for NAT traversal, DTLS-SRTP for encryption. Used by LiveKit, Agora, Daily.co, browser/mobile clients.
3. **SIP / RTP** — IETF telephony standard; signaling via SIP (RFC 3261), media via RTP/SRTP (RFC 3550). Used by PSTN, enterprise PBX, SIP trunking, FreeSWITCH/Asterisk gateways.

Per ADR-RTP-001 the **internal abstraction** is standards-shaped (`MediaFrame`, `Codec`, `CallTransport`) and transport choice is a **per-adapter** decision — the core never knows which transport delivered a given frame. This ADR captures **which transports we implement adapters for and in what order**, not "what's the one transport everyone uses."

## Decision

**Implement multiple transport bindings, each behind a `CallTransport` adapter:**

- **Milestone 1**: WebSocket binding — Twilio Media Streams adapter (provider-direct WS)
- **Milestone 2**: WebRTC binding — LiveKit adapter (SFU-mediated)
- **Milestone 3**: SIP/RTP binding — SIP-direct adapter (Fortemi terminates SIP, ingests RTP) **AND** Twilio SIP Trunking adapter (SIP → Twilio → existing Media Streams binding)

All three bindings feed the same `MediaFrame` stream into the same downstream pipeline (codec normalizer → chunk buffer → ASR adapter → transcript emitter). They differ only at the adapter layer.

The order is **WS → WebRTC → SIP**, but the abstraction lands at milestone 1, so each subsequent binding plugs in without core changes.

## Rationale

### Comparison of transport bindings

| Factor | WebSocket | WebRTC | SIP / RTP |
|---|---|---|---|
| Engineering complexity (adapter) | Low (axum already supports WS upgrade) | Medium (SDP offer/answer, ICE, DTLS-SRTP — bulk handled by LiveKit SDK) | High (own SIP stack OR gateway dependency; RTP packetization, SRTP, DTMF, transport negotiation) |
| NAT traversal | Not needed (server-initiated TLS) | Required (STUN/TURN servers) | Provider/gateway concern; sometimes Fortemi-side |
| Audio quality | Good (Opus 48 kHz available; μ-law 8 kHz default for Twilio) | Excellent (Opus 48 kHz native) | Variable (G.711 default — narrowband; Opus possible) |
| Latency | ~50–150 ms typical | ~30–100 ms typical | ~100–200 ms typical |
| Standards alignment | Each provider defines its own JSON envelope on top — adapter does translation | Native WebRTC standard; adapter is thin over the SDK | Native IETF standard (RFC 3261 / 3550) — adapter is direct |
| Provider ecosystem | Twilio Voice, Vonage Voice WS, AWS IVS, plus all hosted ASR (Deepgram, AssemblyAI) | LiveKit, Agora, Daily, browser-native | PSTN, enterprise PBX, FreeSWITCH/Asterisk, Twilio SIP Trunking |
| First-party Rust support | tokio-tungstenite, axum WS | livekit-rust SDK | rsip / rsipstack (immature as of Q1 2026); FreeSWITCH FFI is an alternative |
| Implementation risk | Low | Medium | High |

### Why WebSocket binding ships first (sequencing)

1. **Lowest implementation risk** of the three; axum WS support is mature; Twilio Media Streams is the most-documented WS-based telephony protocol
2. **Validates the `CallTransport` trait against a real provider** — without a working adapter, the trait is hypothetical
3. **Hosted ASR (per ADR-RTP-003) also speaks WebSocket** — milestone 1 has WS on both edges, minimizing transport variety in the first iteration
4. **No infrastructure footprint** — no STUN/TURN, no SIP server, no SFU

### Why WebRTC binding is second

LiveKit's first-party Rust SDK abstracts most of the WebRTC complexity. The marginal engineering cost over WebSocket is medium (codec negotiation, room participation, ICE), not high. Browser/mobile direct capture becomes possible — strategically important once HotM grows a voice UI.

### Why SIP/RTP is third, not deferred indefinitely

SIP is the actual IETF standard for voice — operationally, every PBX, every SIP trunk, every Asterisk/FreeSWITCH deployment speaks it. Deferring SIP indefinitely means Fortemi can't be self-hosted for enterprise telephony without a vendor in the loop. The standards-first framing in ADR-RTP-001 makes SIP a peer transport, not an afterthought.

Two SIP-binding options will ship in milestone 3:

- **SIP-direct adapter** — Fortemi terminates SIP/RTP itself. Higher engineering cost (Rust SIP stack maturity is the bottleneck); enables fully self-hosted deployments.
- **Twilio SIP Trunking adapter** — SIP-to-Twilio-Voice gateway path. Lower engineering cost (reuses milestone-1 WS infrastructure with a SIP-to-Twilio shim); requires Twilio dependency but solves the PSTN-without-Twilio-Voice case.

Both are needed long-term; the first is the better strategic deliverable, the second is the better near-term unblock.

### What does NOT change between bindings

All three bindings produce `MediaFrame` records that flow into the same codec normalizer, the same chunk buffer, the same ASR adapter, the same transcript emitter. The standards-first abstraction in ADR-RTP-001 means transport-binding work is contained in the adapter; nothing downstream needs to know.

## Consequences

### Positive

- Milestone 1 reuses existing axum WS infrastructure
- No new operational dependencies (STUN/TURN, SIP servers, SFUs) until milestone 2+
- Hosted ASR backends speak the same WS protocol — milestone-1 transport variety is minimal
- Each subsequent binding (WebRTC, SIP) plugs into the existing `CallTransport` trait without core changes
- Standards-first framing means SIP/RTP is a peer transport, not a second-class afterthought — important for enterprise deployments

### Negative

- Direct browser/mobile audio capture must wait for WebRTC (milestone 2)
- Direct PSTN must wait for milestone 3 (or use Twilio Voice as a paid gateway via the WS binding)
- WebSocket has slightly higher latency than WebRTC (TCP vs UDP)
- The team must avoid letting Twilio's WS envelope shape (JSON-wrapped media frames) define the trait — that's the failure mode the standards-first framing exists to prevent

### Implementation notes (WS binding, milestone 1)

- Use `axum::extract::ws::WebSocketUpgrade` for the ingress endpoint
- Read-side: `socket.recv().await` yields `Message::Binary(bytes)`; for Twilio, parse the JSON envelope and extract `media.payload` (base64 μ-law)
- The Twilio adapter is responsible for **translating** Twilio's JSON envelope into `MediaFrame { codec: Codec::PcmuG711 { sample_rate: 8000 }, timestamp_rtp, payload, ... }`. The trait never sees Twilio's JSON shape.
- Per-frame codec normalization: μ-law decode is trivial (lookup table, ~10 ns/sample). The normalizer takes `MediaFrame` and produces PCM 16 kHz mono regardless of input codec.
- Write-side rarely used for milestone 1; leave the WS half-duplex unless a provider requires bidirectional. The trait's `send_frame()` default impl returns `NotSupported`.

## Alternatives Considered

### Alternative A — WebRTC first

Skip WebSocket and start with LiveKit WebRTC integration. Gives us browser/mobile capability immediately.

Rejected because:
- Twilio Voice (milestone 1 provider) doesn't speak WebRTC for the audio path; Media Streams is WebSocket
- WebRTC adds significant scope: SFU operational concerns, ICE configuration, DTLS-SRTP correctness, codec negotiation correctness
- LiveKit SDK abstracts a lot but still requires understanding the protocol when debugging
- We can ship Twilio Voice in milestone 1 in a fraction of the time

### Alternative B — SIP first

Build a Fortemi-native SIP stack. Eliminates the Twilio dependency.

Rejected because:
- SIP is significantly more complex than WebSocket (signaling stack, RTP packetization, SRTP key exchange, DTMF handling, transport negotiation)
- Rust SIP libraries are not production-grade as of Q1 2026
- Operational burden of running SIP servers (firewall config, ALG concerns) is substantial
- Twilio SIP Trunking gives us SIP-to-WS for ~$0.015/min — cheap given the engineering savings

### Alternative C — HTTP long-polling

Use HTTP/2 chunked responses to stream audio over a long-poll connection. Avoids both WebSocket and WebRTC complexity.

Rejected because:
- No major provider supports this for audio
- Audio framing semantics are awkward over HTTP
- Re-invents what WebSocket already does
- Latency is worse than WebSocket

## References

- `../streaming-realtime/research-synthesis.md` §3.2 (provider matrix), §5.3 (codec compatibility)
- ADR-RTP-001 (architectural pattern)
- ADR-RTP-004 (Twilio Voice as first provider)
- Twilio Media Streams docs: `www.twilio.com/docs/voice/twiml/stream`
- RFC 6455 — The WebSocket Protocol
