# ADR-004: Roko Temporal Bridge Architecture

| Field | Value |
|-------|-------|
| **Decision ID** | ADR-004 |
| **Status** | Proposed |
| **Date** | 2026-04-09 |
| **Deciders** | MPC Wallet Architecture Team |
| **Relates to** | ADR-001 (MPC Protocol Selection), ADR-002 (Trust Attestation Format) |

---

## Reasoning

The Roko Network provides nanosecond-precision temporal receipts via its Proof of Authenticated Time (PoAT) consensus. Fortemi needs to anchor trust attestations and key lifecycle events (device enrollment, key rotation, revocation) to Roko's timeline for non-repudiable temporal proof. The bridge architecture determines how Fortemi communicates with Roko, what trust assumptions are made about Roko's responses, and how failures in one system affect the other. This is a cross-system integration with different failure modes, latency characteristics, and security models.

---

## Context

### Roko Network Characteristics

- **Consensus**: Proof of Authenticated Time (PoAT) with ECDSA secp256k1 signatures
- **Block time**: 100ms ticks with nanosecond-precision timestamps
- **RPC interface**: Substrate JSON-RPC via `jsonrpsee` (WebSocket and HTTP)
- **Light client**: Substrate provides `smoldot` for browser/embedded light client verification
- **Chain state**: Temporal receipt extrinsics produce events with receipt hashes
- **Finality**: Grandpa finality gadget (probabilistic finality within a few blocks, absolute finality within ~10 blocks / 1 second)

### Fortemi's Integration Needs

1. **Submit temporal anchor**: Hash of a trust attestation or key event -> Roko extrinsic -> temporal receipt
2. **Verify temporal receipt**: Given a receipt hash, confirm it exists on Roko at the claimed timestamp
3. **Monitor events**: Track receipt confirmations, finality, and any chain reorganizations
4. **Key binding**: Bind MPC wallet public key to a Roko on-chain identity

### Operational Constraints

- Fortemi runs as a single-instance Docker bundle (typical deployment)
- Roko node may be remote (different host, different network)
- Network partitions between Fortemi and Roko should not break Fortemi's core functionality
- Temporal anchoring is an enhancement, not a prerequisite — Fortemi must degrade gracefully without Roko
- The bridge must handle Roko chain upgrades (runtime upgrades change extrinsic formats)

---

## Evaluation Criteria

| # | Criterion | Weight | Description |
|---|-----------|--------|-------------|
| 1 | **Trust model** | 30% | What trust assumptions does the bridge make? Can a compromised intermediary forge temporal proofs? Can Fortemi verify Roko state independently? |
| 2 | **Latency** | 25% | End-to-end time from "submit anchor" to "confirmed receipt." Includes network round-trips, block inclusion, and finality. Target: < 2 seconds for confirmed receipt. |
| 3 | **Operational complexity** | 20% | Deployment burden, configuration, monitoring, and maintenance. Additional services, ports, dependencies. |
| 4 | **Resource usage** | 15% | CPU, memory, disk, and network bandwidth consumed by the bridge. Fortemi targets edge hardware (8GB RAM, 6GB VRAM GPU). |
| 5 | **Resilience** | 10% | Behavior during Roko unavailability, network partitions, chain reorganizations. Graceful degradation without data loss. |

---

## Options

### Option 1: Direct RPC Client

**Description**: Fortemi's Rust backend calls Roko node's JSON-RPC endpoints directly using `jsonrpsee`. Extrinsic submission, receipt verification, and event monitoring are all done through RPC calls from within the existing Fortemi process. Connection configuration is an environment variable (`ROKO_RPC_URL`).

**Architecture**:
```
[Fortemi API] --jsonrpsee--> [Roko Node RPC]
     |                             |
     |--- submit_anchor() -------->|
     |<-- temporal_receipt --------|
     |--- verify_receipt() ------->|
     |<-- confirmation ------------|
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Trust model | 3 | Trusts the Roko node to return honest responses. A compromised or malicious RPC endpoint could return fake receipts or suppress submissions. Fortemi has no way to verify that the node's responses reflect actual chain state without independent validation. Mitigated by connecting to a trusted node (user's own or known operator). |
| Latency | 5 | Minimal overhead: one WebSocket connection, direct extrinsic submission. Submit -> block inclusion (~100ms) -> finality (~1s) = ~1.1s typical. No intermediary processing. Lowest possible latency for the integration. |
| Operational complexity | 5 | Zero additional services. One environment variable (`ROKO_RPC_URL`). Fortemi's existing async Tokio runtime handles the WebSocket connection. No separate process to monitor, restart, or update. |
| Resource usage | 5 | A single WebSocket connection: ~50KB memory, negligible CPU, minimal bandwidth (a few KB per anchor request). No disk usage. Fits easily within edge hardware constraints. |
| Resilience | 3 | If the Roko node is unreachable, anchoring fails. Fortemi can queue anchor requests and retry, but there's no alternative path. WebSocket disconnects require reconnection logic. No local state to verify past receipts — depends on the node being available for verification. |

**Weighted Score**: (3 x 0.30) + (5 x 0.25) + (5 x 0.20) + (5 x 0.15) + (3 x 0.10) = 0.90 + 1.25 + 1.00 + 0.75 + 0.30 = **4.20**

### Option 2: Event Subscription (Pub/Sub)

**Description**: Fortemi subscribes to Roko block events via the Substrate `subscribe_newHead` and `subscribe_events` RPC methods. Anchor submissions are still direct RPC calls, but receipt confirmation and verification happen asynchronously through event processing. A local event log tracks all Roko events relevant to Fortemi's anchors.

**Architecture**:
```
[Fortemi API] --submit extrinsic--> [Roko Node RPC]
[Fortemi Event Processor] <--subscribe_events-- [Roko Node RPC]
     |
     |--- process_block_events()
     |--- update_anchor_status()
     |--- store_event_log()
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Trust model | 3 | Same trust assumptions as direct RPC — the node is trusted. Event subscription adds no independent verification. However, the local event log provides an audit trail: if events are later contradicted, the discrepancy is detectable (though not preventable). |
| Latency | 4 | Submission latency is identical to direct RPC. Confirmation is event-driven rather than polled, which is slightly more efficient but adds processing latency (~10-50ms for event deserialization and matching). Net effect: ~1.2s typical, marginally slower than direct RPC. |
| Operational complexity | 4 | Still runs within Fortemi's process, but adds an event processing pipeline: subscription management, event deserialization, anchor matching, local event log storage. More code paths, more failure modes (subscription drops, event backpressure). |
| Resource usage | 4 | Event subscription receives all block events, not just Fortemi-related ones. Depending on Roko's activity, this could be significant bandwidth and processing. Event log storage in PostgreSQL adds disk usage (~1KB per event, bounded by retention policy). |
| Resilience | 4 | Local event log survives Roko disconnections — past receipts are verifiable from local state. Subscription reconnection with replay from last known block provides catch-up capability. Better than direct RPC for handling transient outages. |

**Weighted Score**: (3 x 0.30) + (4 x 0.25) + (4 x 0.20) + (4 x 0.15) + (4 x 0.10) = 0.90 + 1.00 + 0.80 + 0.60 + 0.40 = **3.70**

### Option 3: Light Client Embedding

**Description**: Embed the Substrate light client (`smoldot`) directly into Fortemi's Rust process. The light client syncs block headers and can verify state proofs without trusting any single RPC node. Extrinsic submission goes through the light client's transaction pool. Receipt verification uses Merkle proof verification against synced headers.

**Architecture**:
```
[Fortemi API] --> [Embedded smoldot Light Client] --> [Roko P2P Network]
                        |
                        |--- sync_headers()      (background)
                        |--- submit_extrinsic()   (via P2P)
                        |--- verify_state_proof() (local)
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Trust model | 5 | Trustless verification. The light client verifies block headers against the Grandpa finality proofs and validates state proofs (Merkle proofs) locally. No single node can forge temporal receipts. The strongest trust model — Fortemi independently verifies Roko's chain state. |
| Latency | 3 | Light client must sync headers before first use (~5-30 seconds initial sync). Extrinsic submission through P2P network may be slower than direct RPC (peer discovery, relay hops). Ongoing: header sync is near-real-time but adds ~100-200ms latency for state proof generation and verification. Typical end-to-end: ~2-3 seconds. |
| Operational complexity | 2 | Embedding smoldot adds significant complexity: P2P networking (libp2p), chain spec management, header database, warp sync, runtime upgrade handling. smoldot's Rust API is lower-level than jsonrpsee. Must handle chain spec updates when Roko upgrades its runtime. Testing requires a local Roko chain or mocked P2P layer. |
| Resource usage | 2 | Light client maintains a header database (~50-200MB depending on chain age), runs P2P networking (libp2p stack: ~20-50MB RAM, multiple TCP connections), and periodically processes finality proofs (CPU spikes). On edge hardware (8GB RAM), this is a noticeable footprint alongside Fortemi's existing services (PostgreSQL, Redis, Ollama). |
| Resilience | 5 | The light client is inherently resilient — it connects to multiple P2P peers and tolerates individual node failures. No single point of failure. Can verify historical receipts from local header database even during network partitions. Best resilience of all options. |

**Weighted Score**: (5 x 0.30) + (3 x 0.25) + (2 x 0.20) + (2 x 0.15) + (5 x 0.10) = 1.50 + 0.75 + 0.40 + 0.30 + 0.50 = **3.45**

### Option 4: Relay Service

**Description**: Deploy a separate bridge service between Fortemi and Roko. The relay handles RPC communication, event processing, receipt caching, and retry logic. Fortemi communicates with the relay via a simple internal API (gRPC or REST). The relay can optionally run a light client for trustless verification.

**Architecture**:
```
[Fortemi API] --internal API--> [Roko Relay Service] --jsonrpsee--> [Roko Node]
                                       |
                                       |--- receipt_cache (SQLite/Redis)
                                       |--- retry_queue
                                       |--- event_processor
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Trust model | 3 | Same as direct RPC unless the relay embeds a light client (which adds Option 3's complexity). The relay itself becomes a trust boundary — a compromised relay can forge receipts. Adds a new attack surface. |
| Latency | 4 | Extra hop through the relay adds ~1-5ms. Negligible compared to Roko's block time. Relay can pre-cache frequently verified receipts for sub-millisecond lookups. Net: ~1.2s typical, similar to direct RPC. |
| Operational complexity | 1 | Separate service to build, deploy, monitor, restart, and version. New Docker container in the bundle. Health checks, log aggregation, configuration management. Must be updated when Roko's extrinsic format changes. Doubles the maintenance surface for what is fundamentally a thin integration layer. |
| Resource usage | 2 | Separate process: ~50-100MB RAM, its own SQLite/Redis for caching, its own network connections. On edge hardware, every process competes for resources. If using a separate language (e.g., Node.js for rapid development), adds a runtime dependency. |
| Resilience | 4 | Relay can queue and retry during Roko outages. Receipt cache provides local verification for recently anchored items. However, the relay itself is a single point of failure unless deployed redundantly (which further increases operational complexity). |

**Weighted Score**: (3 x 0.30) + (4 x 0.25) + (1 x 0.20) + (2 x 0.15) + (4 x 0.10) = 0.90 + 1.00 + 0.20 + 0.30 + 0.40 = **2.80**

---

## Comparison Matrix

| Criterion | Weight | Direct RPC | Event Sub | Light Client | Relay Service |
|-----------|--------|------------|-----------|--------------|---------------|
| Trust model | 30% | 3 (trusts node) | 3 (trusts node) | **5** (trustless) | 3 (trusts relay+node) |
| Latency | 25% | **5** (~1.1s) | 4 (~1.2s) | 3 (~2-3s) | 4 (~1.2s) |
| Operational complexity | 20% | **5** (zero services) | 4 (event pipeline) | 2 (smoldot) | 1 (separate service) |
| Resource usage | 15% | **5** (~50KB) | 4 (~1MB+log) | 2 (~200MB) | 2 (~100MB) |
| Resilience | 10% | 3 (no fallback) | 4 (event log) | **5** (P2P mesh) | 4 (retry queue) |
| **Weighted Total** | | **4.20** | 3.70 | 3.45 | 2.80 |

---

## Decision

**Adopt Direct RPC Client (Option 1)** as the initial Roko temporal bridge, with an architectural path toward light client verification (Option 3) as a future enhancement.

### Rationale

Direct RPC wins decisively (4.20 vs 3.70 for the next option) because the two highest-weighted criteria after trust model — latency (25%) and operational complexity (20%) — both favor simplicity. For a first integration between Fortemi and Roko, the direct RPC approach delivers the fastest path to working temporal anchoring with zero additional deployment complexity.

The trust model trade-off (score 3 vs light client's 5) is the key concession. We mitigate this through:

1. **Trusted node assumption**: Users connect to their own Roko node or a node operated by a known party. This is reasonable for early adoption where the user population is technical.
2. **Receipt cross-verification**: Fortemi stores the submitted extrinsic hash and the returned receipt. Periodic cross-verification against a second Roko node (or block explorer) detects dishonest nodes after the fact.
3. **Upgrade path**: The `RokoClient` trait abstracts the Roko communication layer. Swapping the direct RPC implementation for a light client implementation requires zero changes to calling code.

The light client (Option 3) becomes the right choice when:
- The Roko network has many independent validators (decentralization increases the value of trustless verification)
- `smoldot`'s Rust API stabilizes for non-browser use cases
- Edge hardware has more headroom (16GB+ RAM deployments)

The relay service (Option 4) is rejected outright: it adds operational complexity without improving the trust model. Every benefit of the relay (caching, retry, event processing) can be implemented within Fortemi's existing process.

---

## Consequences

### Positive

- **Zero operational overhead**: No new services, containers, ports, or configuration beyond `ROKO_RPC_URL`. The bridge is a Rust module within `matric-core`.
- **Lowest latency**: Single WebSocket hop to the Roko node. Temporal anchoring completes in ~1.1 seconds (submission + block inclusion + finality). This is well within the target of < 2 seconds.
- **Familiar technology**: `jsonrpsee` is Substrate's official Rust RPC client library. Well-documented, actively maintained, async/tokio-native. The Fortemi team already works in this ecosystem.
- **Graceful degradation**: If `ROKO_RPC_URL` is not configured, temporal anchoring is simply disabled. All other Fortemi functionality works without Roko. Trust attestations are valid with or without temporal anchors — the anchor is an optional enhancement.
- **Resource-friendly**: A single WebSocket connection consumes negligible resources. No impact on edge hardware deployments.

### Negative

- **Trusts the RPC node**: A malicious or compromised Roko node can return fabricated temporal receipts. Fortemi has no way to independently verify chain state. This is acceptable for trusted-node scenarios but inadequate for adversarial environments.
- **Single point of failure**: If the configured Roko node is down, all temporal anchoring fails. No automatic failover to alternative nodes (though a list of fallback URLs could be configured).
- **No local receipt cache**: Past receipt verifications require re-querying the Roko node. If the node is unavailable, previously verified receipts cannot be re-verified. This is mitigated by storing verification results in PostgreSQL.

### Neutral

- **Extrinsic format coupling**: Fortemi must construct Roko-specific extrinsics (SCALE-encoded, signed with secp256k1). Roko runtime upgrades that change extrinsic formats will require Fortemi code updates. This coupling exists regardless of the bridge architecture.
- **Connection management**: WebSocket connections drop and must be reconnected. `jsonrpsee` handles reconnection, but transient disconnections during extrinsic submission require retry logic. Standard async error handling — not architecturally significant.
- **Future light client migration**: The `RokoClient` trait abstraction means the light client can be added as an alternative backend without changing the bridge API. Feature-flagged: `--features roko-light-client` enables smoldot, `--features roko-rpc` (default) uses direct RPC.

---

## Implementation Notes

### Crate Structure

```
crates/matric-core/src/roko/
  mod.rs              -- Public API, RokoClient trait
  rpc_client.rs       -- Direct RPC implementation (jsonrpsee)
  types.rs            -- Roko extrinsic types, temporal receipt struct
  anchor.rs           -- Anchor submission and verification logic
  queue.rs            -- Retry queue for failed submissions
```

### RokoClient Trait (Abstraction for Future Light Client)

```rust
#[async_trait]
pub trait RokoClient: Send + Sync {
    /// Submit a temporal anchor hash to Roko
    async fn submit_anchor(&self, hash: [u8; 32]) -> Result<TemporalReceipt, RokoError>;

    /// Verify that a temporal receipt exists on-chain
    async fn verify_receipt(&self, receipt_id: &ReceiptId) -> Result<ReceiptStatus, RokoError>;

    /// Get the current Roko block timestamp (nanosecond precision)
    async fn current_timestamp(&self) -> Result<u128, RokoError>;

    /// Check connection health
    async fn health_check(&self) -> Result<(), RokoError>;
}
```

### Configuration

```rust
/// Roko bridge configuration
pub struct RokoConfig {
    /// WebSocket RPC URL (e.g., ws://localhost:9944)
    pub rpc_url: Option<String>,

    /// Maximum retry attempts for failed submissions
    pub max_retries: u32,           // default: 3

    /// Retry backoff base (milliseconds)
    pub retry_backoff_ms: u64,      // default: 500

    /// Receipt verification timeout
    pub verify_timeout: Duration,   // default: 10s

    /// Enable/disable temporal anchoring
    pub enabled: bool,              // default: true if rpc_url is set
}
```

Environment variables:
```
ROKO_RPC_URL=ws://localhost:9944     # WebSocket endpoint (omit to disable)
ROKO_MAX_RETRIES=3                   # Submission retry attempts
ROKO_VERIFY_TIMEOUT_SECS=10         # Verification timeout
```

### Retry Queue

Failed anchor submissions are queued in PostgreSQL and retried by the background job worker (`crates/matric-jobs`):

```sql
CREATE TABLE roko_anchor_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    anchor_hash BYTEA NOT NULL,
    source_type TEXT NOT NULL,           -- 'trust_attestation', 'device_cert', 'key_event'
    source_id UUID NOT NULL,             -- Reference to the anchored entity
    status TEXT NOT NULL DEFAULT 'pending', -- pending, submitted, confirmed, failed
    attempts INT NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMPTZ,
    receipt_id BYTEA,                    -- Populated on confirmation
    receipt_timestamp BIGINT,            -- Nanosecond Roko timestamp
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    confirmed_at TIMESTAMPTZ
);

CREATE INDEX idx_roko_queue_status ON roko_anchor_queue(status) WHERE status IN ('pending', 'submitted');
```

### Integration with Trust Attestations (ADR-002)

When a trust attestation is signed:
1. Compute SHA-256 hash of the COSE_Sign1 attestation bytes
2. Insert into `roko_anchor_queue` with `source_type = 'trust_attestation'`
3. Background job submits to Roko via `RokoClient::submit_anchor()`
4. On confirmation, update the attestation's `roko_anchor` field (ADR-002 CBOR claim `-65539`)
5. Re-sign the attestation with the updated anchor hash (or store anchor separately as metadata)

### Health Endpoint Integration

The existing `/health` endpoint exposes Roko bridge status:

```json
{
  "status": "healthy",
  "capabilities": {
    "roko_temporal_bridge": {
      "enabled": true,
      "connected": true,
      "last_block_timestamp": "2026-04-09T12:00:00.123456789Z",
      "pending_anchors": 2,
      "confirmed_anchors_24h": 47
    }
  }
}
```

### Graceful Degradation

If `ROKO_RPC_URL` is not set:
- `RokoClient` returns `RokoError::Disabled` for all operations
- Trust attestations are created without temporal anchors (the `roko_anchor` CBOR claim is omitted)
- No background jobs are scheduled for anchor submission
- Health endpoint reports `"roko_temporal_bridge": { "enabled": false }`

This ensures Fortemi works as a complete system without Roko, and temporal anchoring is a pure enhancement.

---

## References

- Substrate JSON-RPC specification: https://docs.substrate.io/reference/rpc/
- `jsonrpsee` crate: https://crates.io/crates/jsonrpsee
- `smoldot` light client: https://github.com/smol-dot/smoldot
- Grandpa finality: https://docs.substrate.io/learn/consensus/#grandpa
- SCALE codec: https://docs.substrate.io/reference/scale-codec/
