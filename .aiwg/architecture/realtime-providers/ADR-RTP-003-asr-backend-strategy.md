# ADR-RTP-003 — Streaming ASR Backend Strategy

**Status:** Proposed (draft) • 2026-05-22
**Epic:** [#837](https://git.integrolabs.net/Fortemi/fortemi/issues/837)
**Companions:** ADR-RTP-001 (architectural pattern), ADR-RTP-002 (transport), ADR-RTP-004 (first provider)

## Context

Live audio frames must be converted to text in near-real-time (<1.5 s end-to-end budget). Three approaches:

1. **Hosted streaming ASR APIs** — Deepgram, AssemblyAI, Speechmatics, Rev AI, OpenAI Realtime
2. **Self-hosted streaming whisper** — whisper-streaming wrapper, WhisperX, Faster-Whisper streaming mode
3. **Hybrid** — hosted for the live path (low latency, no GPU contention), self-hosted batch re-processing of the recording for higher-quality final transcript

This decision cuts across cost, latency, hardware-tier capability, and the long-standing GPU contention concern from #576.

## Decision

**Default to hosted ASR (Deepgram Streaming) on all tiers.** Self-hosted whisper-streaming is opt-in via env flag on mid/high-end tiers. Hybrid (live=hosted, batch re-process=self-hosted whisper) is the recommended pattern for production deployments that need both low live latency and high final-transcript quality.

Per-tier defaults:

| Tier (VRAM) | Default live ASR | Default batch re-process | Opt-in alternatives |
|---|---|---|---|
| Edge (6–8 GB) | Deepgram | Self-hosted whisper (existing batch pipeline) | None — self-hosted streaming will OOM with Ollama |
| Mid (12–16 GB) | Deepgram | Self-hosted whisper | `REALTIME_ASR_BACKEND=whisper-streaming` (with Ollama pause during calls) |
| High-end (24 GB+) | Deepgram | Self-hosted whisper | Same as mid; multi-GPU deployments can keep both concurrent |

The ASR adapter is a trait (`StreamingASRBackend`); the call session manager picks the implementation at session start based on environment and tier.

## Rationale

### Why hosted is the default

| Factor | Hosted (Deepgram) | Self-hosted whisper-streaming |
|---|---|---|
| Latency (partial) | <300 ms | 500–1500 ms |
| Latency (final) | <800 ms | 1000–3000 ms |
| Cost | $0.0036/min ≈ $5/100hr | "Free" — but GPU compute |
| Setup cost | API key only | Container, model weights, GPU warmth |
| Multi-language | 30+ languages, no setup | Supported but requires model swap |
| GPU contention with Ollama | None | Severe (links back to #576) |
| Failure modes | API down → failover | OOM, model crash, sidecar lifecycle bugs |
| Edge-tier viability | Yes — no GPU needed | No — VRAM contention with Ollama |
| First-call latency | Constant | Cold start adds 5–30s |

Hosted ASR sidesteps the entire GPU-contention problem that #576 solved for batch. For live calls, **whisper has to be always-on**, and on a single-GPU system that conflicts with everything else GPU-bound. Hosted ASR removes this conflict completely.

### Why self-hosted remains opt-in

- **Cost sensitivity at scale**: A high-volume deployment (1000s of calls/day) racks up hosted ASR costs faster than buying more GPU
- **Data sovereignty**: Some users will refuse to send audio to a third party
- **Air-gapped deployments**: No internet → no hosted ASR
- **Self-host quality is improving**: whisper-streaming and successors are converging on hosted-ASR latency

### Why hybrid is the recommended production pattern

Live path needs low latency; final transcript needs high quality. These tensions resolve cleanly:

```
During call:    audio frames → hosted ASR → live partials + finals → outbox
After call:     recording.mp3 → AudioTranscriptionHandler → high-quality batch transcript → DB
```

The batch pipeline (existing `AudioTranscriptionHandler` + faster-whisper) already does this for uploaded audio. The new pipeline just adds the live path and routes the recording back to the batch handler when the call ends.

## Consequences

### Positive

- Edge tier is viable for live transcription (no GPU contention)
- Latency budget is met comfortably (~500 ms total via hosted)
- Cost grows with usage (variable, not capital expenditure)
- #576's batch pipeline unchanged; no rework of existing job handlers
- Multi-language support out of the box on hosted providers

### Negative

- Per-minute hosted cost can exceed self-hosted at very high volume — operators must understand this trade-off
- Adds a new outbound dependency (Deepgram WSS endpoint must be reachable)
- API key management for Deepgram (and any fallback providers)
- We now have two ASR pipelines to maintain (live + batch)

### Trait shape

```rust
#[async_trait]
pub trait StreamingASRBackend: Send + Sync {
    /// Open a per-call streaming session. Returns a handle for sending frames.
    async fn open_session(&self, config: SessionConfig) -> Result<Session>;
}

#[async_trait]
pub trait Session: Send + Sync {
    /// Push PCM 16kHz mono frame. Non-blocking.
    async fn send_frame(&mut self, pcm: &[i16]) -> Result<()>;
    /// Stream of recognized text events (partials + finals).
    fn events(&mut self) -> &mut (impl Stream<Item = TranscriptEvent> + Unpin);
    /// Close the session cleanly.
    async fn close(self) -> Result<()>;
}

pub enum TranscriptEvent {
    Partial { text: String, ts: f64 },
    Final { text: String, speaker: Option<String>, start_ts: f64, end_ts: f64, confidence: Option<f32> },
    Error { reason: String },
}
```

Concrete implementations: `DeepgramBackend`, `AssemblyAIBackend`, `WhisperStreamingBackend`. Session manager loads the configured backend at startup.

### Failover

If the primary hosted backend is unreachable, attempt failover to a secondary (configured) backend. Failover is per-session: a new session starts with the secondary; an active session attempts reconnect with the primary first (200 ms timeout) before failing over.

```
REALTIME_ASR_BACKEND=deepgram              # primary
REALTIME_ASR_BACKEND_FALLBACK=assemblyai   # secondary; optional
```

## Alternatives Considered

### Alternative A — Self-hosted only (no hosted)

Build everything on whisper-streaming. Avoids vendor dependency.

Rejected because:
- Edge-tier deployment becomes impossible without GPU upgrades
- Latency on commodity GPUs is too high for "live typing" UX
- #576's GPU contention reappears at every active call

### Alternative B — Hosted only (no self-hosted option)

Force everyone to Deepgram/equivalent. Simpler operationally.

Rejected because:
- Air-gapped deployments can't function
- Data-sovereignty-sensitive deployments will refuse
- Operators with existing whisper investment lose that value

### Alternative C — OpenAI Realtime API as the universal backend

Use OpenAI's Realtime API; it bundles ASR with the LLM.

Rejected because:
- Closed beta (Q1 2026); production readiness uncertain
- Lock-in to a single vendor
- Pricing not finalized; may not match Deepgram's cost
- ASR-only deployments don't need (or want) the LLM tied in

### Alternative D — Browser-side ASR (Whisper-WASM, web speech API)

Push ASR to the client.

Rejected because:
- Server-side has authoritative transcripts; client-side fragments them
- Multi-participant calls (each client has different ASR results) → reconciliation nightmare
- Browser ASR quality varies wildly

## References

- `../streaming-realtime/research-synthesis.md` §4 (ASR options), §5 (latency budget, GPU contention), §8 (cost picture)
- ADR-RTP-001, ADR-RTP-002, ADR-RTP-004
- #576 — GPU-exclusive sidecar lifecycle (the contention problem this ADR sidesteps for live)
- Deepgram Streaming API docs (to be inducted)
- whisper-streaming (Macháček et al., 2023, arXiv:2307.14743) (to be inducted)
