# ADR-RTP-002 — Transport Choice for Live Audio

**Status:** Proposed (draft) • 2026-05-22
**Epic:** [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
**Companions:** ADR-RTP-001 (architectural pattern), ADR-RTP-003 (ASR backend), ADR-RTP-004 (first provider)

## Context

Three viable transport protocols for getting live audio frames into Fortemi:

1. **WebSocket (WSS)** — long-lived TCP+TLS connection; provider streams binary frames. Used by Twilio Media Streams, Vonage Voice WebSocket, AWS IVS, hosted ASR APIs.
2. **WebRTC (UDP-based)** — browser-native real-time protocol; SDP negotiation, ICE for NAT traversal, DTLS-SRTP for encryption. Used by LiveKit, Agora, Daily.co, browser/mobile clients.
3. **SIP / RTP** — telephony standard; signaling via SIP, media via RTP/SRTP. Used by PSTN, enterprise PBX, SIP trunking providers.

The choice affects engineering effort, NAT-traversal complexity, audio quality, and which providers are reachable.

## Decision

**Use WebSocket as the primary transport for milestone 1.** Add WebRTC support in milestone 2 via LiveKit. Treat SIP as a separate milestone (milestone 3) routed through a SIP gateway that bridges to either WebSocket or WebRTC.

Architecturally, the call session manager doesn't care which transport delivered the audio frames — only that PCM 16 kHz mono arrives at the chunk buffer. Transport is a per-provider ingress concern.

## Rationale

| Factor | WebSocket | WebRTC | SIP |
|---|---|---|---|
| Engineering complexity | Low (axum already supports WS upgrade) | High (SDP offer/answer, ICE, DTLS-SRTP) | Very high (separate stack; gateway recommended) |
| NAT traversal | Not needed (server-initiated TLS) | Required (STUN/TURN servers) | Provider/gateway concern |
| Audio quality | Good (Opus 48 kHz available; μ-law 8 kHz default for Twilio) | Excellent (Opus 48 kHz native) | Variable (G.711 default — narrowband) |
| Latency | ~50–150 ms typical | ~30–100 ms typical | ~100–200 ms typical |
| Ecosystem | Twilio Media Streams, Vonage Voice WebSocket, Deepgram, AssemblyAI | LiveKit, Agora, Daily, browser-native | PSTN, enterprise telephony |
| Provider availability | Yes — Twilio, Vonage | Yes — LiveKit, Agora, Daily | Yes via gateway (Twilio SIP, LiveKit Telephony, FreeSWITCH) |
| First-party Rust support | tokio-tungstenite, axum WS | livekit-rust SDK (for LiveKit) | None (rsip, rsipstack exist but immature) |
| First-iteration risk | Low | Medium | High |

WebSocket is the right choice for milestone 1 because:

1. **Twilio Media Streams (milestone 1's provider per ADR-RTP-004) is WebSocket-only** for the audio path
2. **axum already handles WS upgrade** — the ingress endpoint is ~50 lines of new code
3. **No NAT traversal infrastructure** required (Fortemi accepts inbound connections; provider initiates)
4. **Hosted ASR (Deepgram/AssemblyAI per ADR-RTP-003) is also WebSocket** — same transport on both edges

WebRTC adds the ability to integrate browser/mobile clients directly without a provider. That's strategically valuable but doesn't have to be in milestone 1.

SIP is essential for direct PSTN integration (no Twilio in the loop) but is a substantial engineering investment. Best handled by a SIP-to-WS gateway (Twilio SIP Trunking → Programmable Voice → Media Streams gives us this for free).

## Consequences

### Positive

- Milestone 1 reuses existing axum WS infrastructure
- No new operational dependencies (STUN/TURN, SIP servers, etc.)
- Hosted ASR backends speak the same protocol; we can pipe audio from one WSS to another with minimal buffering
- Future transports plug into the same call session manager via the `RealtimeIngress` trait

### Negative

- Direct browser/mobile audio capture must wait for WebRTC (milestone 2)
- Direct PSTN must wait for milestone 3 (or use Twilio as a paid gateway)
- WebSocket has slightly higher latency than WebRTC (TCP vs UDP)

### Implementation notes

- Use `axum::extract::ws::WebSocketUpgrade` for the ingress
- Read-side: `socket.recv().await` yields `Message::Binary(bytes)`; for Twilio, parse the JSON envelope and extract `media.payload` (base64 μ-law)
- Per-frame codec normalization: μ-law decode is trivial (lookup table, ~10 ns/sample)
- Write-side rarely used for milestone 1 (we receive but don't send audio); leave the WS half-duplex unless a provider requires bidirectional

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
