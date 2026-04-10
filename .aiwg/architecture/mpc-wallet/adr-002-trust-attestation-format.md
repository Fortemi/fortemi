# ADR-002: Trust Attestation Format

| Field | Value |
|-------|-------|
| **Decision ID** | ADR-002 |
| **Status** | Proposed |
| **Date** | 2026-04-09 |
| **Deciders** | MPC Wallet Architecture Team |
| **Relates to** | ADR-001 (MPC Protocol Selection), ADR-003 (Device Certificate Model), ADR-004 (Roko Temporal Bridge) |

---

## Reasoning

Trust attestations are the currency of the Personal Trust Network. A user's MPC wallet signs attestations that declare trust relationships: "I trust Alice's wallet for domain X with confidence Y, attested at time Z." These attestations are stored in Fortemi's knowledge base, exchanged P2P between users, and optionally anchored to Roko for temporal proof. The format must balance compactness (attestations accumulate over time and are exchanged frequently), extensibility (new trust dimensions will emerge), and interoperability (third-party systems may want to verify trust claims).

---

## Context

The Personal Trust Network is a decentralized web-of-trust where users issue signed attestations about other users' wallets. Unlike institutional PKI (where a CA issues certificates), trust attestations are peer-to-peer, bidirectional, and domain-scoped. A user might trust Alice for "code review" but not for "financial advice."

Attestation lifecycle:
1. **Creation**: User's MPC wallet signs an attestation about a peer's wallet public key
2. **Storage**: Attestation stored as a Fortemi note attachment (or standalone record)
3. **Exchange**: Sent to the attested peer (who stores it as incoming trust)
4. **Verification**: Any party can verify the signature against the attester's MPC wallet public key
5. **Anchoring** (optional): Hash submitted to Roko for temporal receipt (proves attestation existed at time T)
6. **Revocation**: Attester issues a revocation attestation referencing the original

Key constraints:
- Attestations must be compact — a user with 500 trust relationships should not bloat their Fortemi instance
- The format must be self-describing and versioned — attestations will outlive any single software version
- Verification must be fast — batch verification of trust paths involves checking chains of attestations
- The format must carry structured claims (trust domain, confidence level, temporal bounds) without being a general-purpose container
- Fortemi already uses a binary format precedent: MMPKE01 magic bytes for encrypted notes

Existing ecosystem context:
- Fortemi's PKE system uses custom binary with magic bytes (`MMPKE01`) and versioned headers
- Roko uses SCALE encoding (Substrate's native codec) for on-chain data
- The MCP server exchanges JSON over HTTP
- Fortemi's note content is UTF-8 markdown with YAML frontmatter

---

## Evaluation Criteria

| # | Criterion | Weight | Description |
|---|-----------|--------|-------------|
| 1 | **Compactness** | 25% | Wire size and storage footprint. Attestations accumulate; a user with hundreds of trust relationships needs efficient storage. Target: < 500 bytes for a typical attestation. |
| 2 | **Extensibility** | 20% | Ability to add new fields, trust dimensions, and metadata without breaking existing parsers. Schema evolution without migration. |
| 3 | **Interoperability** | 20% | Ability for third-party systems to parse and verify attestations without Fortemi-specific libraries. Standards alignment with broader identity/credential ecosystems. |
| 4 | **Implementation simplicity** | 20% | Ease of implementation in Rust, availability of mature crates, integration with existing Fortemi code patterns. |
| 5 | **Ecosystem fit** | 15% | Alignment with Fortemi's existing formats (MMPKE01, YAML frontmatter, JSON APIs), Roko's SCALE encoding, and the Rust crypto ecosystem. |

---

## Options

### Option 1: Custom Binary Format (MMPKE01-style)

**Description**: Design a purpose-built binary format with magic bytes (`MMTRUST01`), versioned header, fixed-layout fields for common claims, and a TLV (type-length-value) extension section for optional fields. Follows the precedent set by Fortemi's existing MMPKE01 encrypted note format. The signature covers the entire payload excluding the signature field itself.

**Proposed layout**:
```
[magic: 8B] [version: 2B] [flags: 2B]
[attester_pubkey: 32B] [attestee_pubkey: 32B]
[domain_hash: 16B] [confidence: 1B] [created_at: 8B] [expires_at: 8B]
[roko_anchor_hash: 32B (optional, flagged)]
[tlv_extensions: variable]
[signature: 64B]
Total (no extensions, no anchor): ~169 bytes
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Compactness | 5 | ~169 bytes for a standard attestation. Smallest possible representation. No schema overhead, no string keys, no structural framing. |
| Extensibility | 3 | TLV section allows new fields, but parsers must handle unknown TLV types gracefully. Version bumps for structural changes. No schema negotiation — clients must agree on version. Less flexible than schema-evolving formats. |
| Interoperability | 1 | Completely proprietary. Third parties need Fortemi's parsing library. No standard tooling. Cannot be validated by generic verifiers. A wall against ecosystem integration. |
| Implementation simplicity | 4 | Straightforward `nom` or manual byte parsing in Rust. No external dependencies. Full control over layout. But: must write custom serializer/deserializer, fuzz it, handle endianness, write documentation. |
| Ecosystem fit | 4 | Matches MMPKE01 precedent. Consistent with Fortemi's existing binary format approach. However, diverges from the JSON/YAML patterns used in the API layer and MCP server. |

**Weighted Score**: (5 x 0.25) + (3 x 0.20) + (1 x 0.20) + (4 x 0.20) + (4 x 0.15) = 1.25 + 0.60 + 0.20 + 0.80 + 0.60 = **3.45**

### Option 2: W3C Verifiable Credentials (VC)

**Description**: Encode attestations as W3C Verifiable Credentials using JSON-LD. The VC data model (v2.0) provides a standardized envelope for claims with cryptographic proofs. The credential subject contains trust-specific claims; the proof section carries the FROST threshold signature. VCs can be serialized as JSON-LD (verbose) or JWT (compact but less extensible).

**Example**:
```json
{
  "@context": ["https://www.w3.org/ns/credentials/v2", "https://fortemi.io/trust/v1"],
  "type": ["VerifiableCredential", "TrustAttestation"],
  "issuer": "did:fortemi:mpc:ed25519:<pubkey>",
  "credentialSubject": {
    "id": "did:fortemi:mpc:ed25519:<attestee_pubkey>",
    "trustDomain": "code-review",
    "confidenceLevel": 0.85,
    "validFrom": "2026-04-09T00:00:00Z",
    "validUntil": "2027-04-09T00:00:00Z"
  },
  "proof": { "type": "FrostEd25519Signature2026", "proofValue": "..." }
}
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Compactness | 2 | JSON-LD is verbose. A typical attestation would be 800-1500 bytes. JWT encoding reduces this to ~500-700 bytes but loses JSON-LD's extensibility. Context URLs add overhead. |
| Extensibility | 5 | JSON-LD context mechanism allows adding arbitrary claims without breaking existing parsers. Schema evolution is a first-class feature. New trust dimensions are just new context terms. |
| Interoperability | 5 | W3C standard adopted by governments, enterprises, and decentralized identity ecosystems. Generic VC verifiers can validate structure. DID-based identifiers integrate with broader SSI ecosystem. |
| Implementation simplicity | 2 | JSON-LD processing is complex (context resolution, graph normalization for signing). Rust JSON-LD crates exist but are less mature than serde-based approaches. VC libraries in Rust (`ssi` crate by Spruce) exist but are heavyweight. FROST signature type would need custom proof suite registration. |
| Ecosystem fit | 2 | Fortemi's internal format is not JSON-LD. Would introduce a new data model paradigm alongside existing YAML frontmatter and binary formats. DID resolution adds infrastructure requirements. Overkill for a self-sovereign system that doesn't need institutional interop today. |

**Weighted Score**: (2 x 0.25) + (5 x 0.20) + (5 x 0.20) + (2 x 0.20) + (2 x 0.15) = 0.50 + 1.00 + 1.00 + 0.40 + 0.30 = **3.20**

### Option 3: CBOR-Encoded Claims (CWT/COSE)

**Description**: Use CBOR (Concise Binary Object Representation, RFC 8949) with COSE signing (RFC 9052) to create compact, standardized attestations. Structure follows CWT (CBOR Web Token, RFC 8392) patterns with private claims for trust-specific fields. The signature is a COSE_Sign1 structure wrapping the CBOR-encoded claims. CBOR integer keys replace string keys for compactness while maintaining a registered claim namespace.

**Example (diagnostic notation)**:
```
COSE_Sign1 {
  protected: {1: -8},  // alg: EdDSA
  payload: {
    1: "fortemi:mpc:<attester>",    // iss
    2: "fortemi:mpc:<attestee>",    // sub
    6: 1744156800,                  // iat (unix timestamp)
    4: 1775692800,                  // exp
    -65537: "code-review",          // trust_domain (private claim)
    -65538: 85,                     // confidence (0-100)
    -65539: h'<roko_anchor>'        // roko_temporal_hash
  }
}
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Compactness | 4 | CBOR with integer keys: ~200-350 bytes for a typical attestation. Not as compact as raw binary (CBOR framing overhead ~30-50 bytes) but significantly smaller than JSON. COSE signature envelope adds ~80 bytes. |
| Extensibility | 4 | CBOR maps are inherently extensible — unknown keys are skipped by parsers. Private claim namespace allows Fortemi-specific fields. IANA claim registry provides standardized semantics for common fields. Slightly less flexible than JSON-LD contexts but more practical. |
| Interoperability | 4 | IETF standards (RFC 8949, 9052, 8392). Widely used in IoT, WebAuthn, FIDO2, mDL (mobile driver's license). COSE signature verification is implemented in most languages. However, FROST-specific algorithm identifiers would need IANA registration or private use. |
| Implementation simplicity | 4 | Excellent Rust crates: `ciborium` for CBOR, `coset` for COSE. Serde-compatible. COSE signing integrates cleanly with raw signature bytes from FROST. No graph normalization or context resolution needed. |
| Ecosystem fit | 4 | Binary format aligns with MMPKE01 precedent (compact, versioned). CBOR's self-describing nature improves on raw binary's documentation burden. COSE signing is the same paradigm as Fortemi's existing sign-then-verify pattern. Works well with both Rust (API) and Node.js (MCP server) via `cbor` npm package. |

**Weighted Score**: (4 x 0.25) + (4 x 0.20) + (4 x 0.20) + (4 x 0.20) + (4 x 0.15) = 1.00 + 0.80 + 0.80 + 0.80 + 0.60 = **4.00**

### Option 4: Protobuf Messages

**Description**: Define attestation structure as Protocol Buffer messages with proto3 syntax. Protobuf provides efficient binary serialization with strong schema evolution guarantees (field addition/removal without breaking). The signature covers the serialized protobuf bytes. Schema is distributed as `.proto` files.

**Example**:
```protobuf
message TrustAttestation {
  bytes attester_pubkey = 1;
  bytes attestee_pubkey = 2;
  string trust_domain = 3;
  uint32 confidence = 4;        // 0-100
  int64 created_at_unix = 5;
  int64 expires_at_unix = 6;
  bytes roko_anchor_hash = 7;   // optional
  bytes signature = 15;
}
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Compactness | 4 | Protobuf varint encoding is efficient. ~180-300 bytes for a typical attestation. Comparable to CBOR. Field tags add minimal overhead. |
| Extensibility | 4 | Proto3 handles unknown fields gracefully. New fields can be added with new field numbers. Strong backward/forward compatibility guarantees. However, structural changes (nested messages, oneofs) require careful schema versioning. |
| Interoperability | 3 | Protobuf is widely used in gRPC ecosystems but is not a standard for identity/credential systems. Third parties need the `.proto` file to parse attestations (not self-describing without it). No alignment with identity standards (W3C, IETF). |
| Implementation simplicity | 3 | `prost` crate generates Rust types from `.proto` files. Build-time code generation adds complexity (build.rs, protoc dependency). Protobuf does not handle signing natively — must define signature-excluded serialization manually. Not serde-compatible without additional wrappers. |
| Ecosystem fit | 2 | Fortemi does not use Protobuf anywhere. Would introduce a new serialization paradigm, build dependency (protoc), and code generation step. Diverges from serde-based patterns used throughout the codebase. Roko uses SCALE, not Protobuf. |

**Weighted Score**: (4 x 0.25) + (4 x 0.20) + (3 x 0.20) + (3 x 0.20) + (2 x 0.15) = 1.00 + 0.80 + 0.60 + 0.60 + 0.30 = **3.30**

---

## Comparison Matrix

| Criterion | Weight | Custom Binary | W3C VC | CBOR/COSE | Protobuf |
|-----------|--------|---------------|--------|-----------|----------|
| Compactness | 25% | **5** (169B) | 2 (800-1500B) | 4 (200-350B) | 4 (180-300B) |
| Extensibility | 20% | 3 (TLV) | **5** (JSON-LD) | 4 (CBOR maps) | 4 (proto3) |
| Interoperability | 20% | 1 (proprietary) | **5** (W3C) | 4 (IETF) | 3 (gRPC) |
| Implementation simplicity | 20% | 4 (manual) | 2 (complex) | **4** (ciborium) | 3 (prost/protoc) |
| Ecosystem fit | 15% | 4 (MMPKE01) | 2 (new paradigm) | **4** (binary+serde) | 2 (new dep) |
| **Weighted Total** | | 3.45 | 3.20 | **4.00** | 3.30 |

---

## Decision

**Adopt CBOR-encoded claims with COSE signing (Option 3)** as the trust attestation format, using the `ciborium` and `coset` Rust crates.

### Rationale

CBOR/COSE scores highest overall (4.00) with no score below 4 on any criterion — the most balanced option. It achieves near-custom-binary compactness (~250 bytes typical) while providing the extensibility and standards alignment that a proprietary format lacks.

The key differentiators versus the alternatives:

- **vs. Custom Binary**: CBOR is self-describing and standards-backed (IETF RFC 8949). Unknown fields are naturally skipped rather than requiring custom TLV parsing. Third parties can use any CBOR library to inspect attestations without Fortemi-specific code. The ~80 byte overhead versus raw binary is an acceptable cost for these benefits.
- **vs. W3C VC**: VCs are designed for institutional credential ecosystems with DID resolution, JSON-LD processing, and proof suite registries. This is excessive machinery for a P2P trust network where both parties run Fortemi. If institutional interop becomes important later, a VC envelope can wrap CBOR attestations without changing the core format.
- **vs. Protobuf**: CBOR does not require build-time code generation, protoc tooling, or `.proto` file distribution. It integrates with serde (via ciborium) matching Fortemi's existing serialization patterns. Protobuf's advantages (strong typing, generated code) are less relevant for a small, well-defined message type.

COSE (RFC 9052) provides a standardized signing envelope that cleanly separates protected headers, payload, and signature — the exact structure needed for FROST threshold signatures. The COSE algorithm registry can be extended with FROST-specific identifiers using the private-use range.

---

## Consequences

### Positive

- **Compact and efficient**: ~250 bytes per attestation means a user with 1000 trust relationships stores ~250KB of attestation data. Negligible compared to note content.
- **Standards-based**: IETF RFCs provide stable specifications. CBOR and COSE are used in WebAuthn, FIDO2, and mobile credentials — well-understood by the security community.
- **Serde-compatible**: `ciborium` integrates with serde, allowing the same Rust structs to serialize to CBOR (for signing/storage) and JSON (for API responses) with minimal code.
- **Self-describing**: CBOR's diagnostic notation provides human-readable debugging. Tools like `cbor.me` can inspect attestations without custom parsers.
- **MCP server compatibility**: The `cbor` npm package in Node.js can parse attestations, enabling the MCP server to expose trust data without a Rust FFI bridge.
- **Roko anchor integration**: CBOR byte strings naturally carry the 32-byte Roko temporal hash without encoding overhead.

### Negative

- **Not a standard credential format**: Unlike W3C VCs, CBOR attestations are not directly consumable by identity wallets, credential verifiers, or SSI ecosystems. If Fortemi later needs to interoperate with these systems, a VC wrapper layer must be built.
- **FROST algorithm registration**: COSE's algorithm registry does not include FROST threshold signatures. Private-use algorithm IDs (-65537 range) work but are not globally recognized. IANA registration would be needed for full standards compliance.
- **CBOR canonical form**: Signing requires deterministic serialization. CBOR has a canonical form (RFC 8949 Section 4.2) but `ciborium` must be configured to produce it. Non-canonical CBOR would break signature verification. This is a correctness pitfall that must be caught in testing.

### Neutral

- **Migration from MMPKE01**: The existing PKE format is unaffected. Trust attestations are a new data type — no migration needed. Both formats coexist: MMPKE01 for encrypted notes, CBOR/COSE for trust attestations.
- **Claim namespace**: Using IANA CWT claim numbers for standard fields (iss, sub, iat, exp) and private claims for trust-specific fields (-65537 onward) provides a clean separation. Private claims should be documented in a Fortemi-specific registry.
- **Binary storage in PostgreSQL**: Attestations can be stored as `BYTEA` columns in PostgreSQL, with indexed CBOR fields extracted into separate columns for query performance (attester, attestee, domain, created_at).

---

## Implementation Notes

### Crate Dependencies

```toml
# Cargo.toml additions
ciborium = "0.2"          # CBOR serialization (serde-compatible)
coset = "0.3"             # COSE signing structures
```

### Attestation Structure

```rust
use serde::{Serialize, Deserialize};

/// Trust attestation claims (CBOR-encoded, COSE-signed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustAttestationClaims {
    /// Attester's MPC wallet public key (CWT "iss" equivalent)
    #[serde(rename = "1")]
    pub attester: Vec<u8>,       // 32 bytes Ed25519 pubkey

    /// Attestee's MPC wallet public key (CWT "sub" equivalent)
    #[serde(rename = "2")]
    pub attestee: Vec<u8>,       // 32 bytes Ed25519 pubkey

    /// Unix timestamp of creation (CWT "iat")
    #[serde(rename = "6")]
    pub created_at: i64,

    /// Unix timestamp of expiry (CWT "exp")
    #[serde(rename = "4")]
    pub expires_at: i64,

    /// Trust domain identifier (e.g., "code-review", "financial")
    #[serde(rename = "-65537")]
    pub trust_domain: String,

    /// Confidence level 0-100
    #[serde(rename = "-65538")]
    pub confidence: u8,

    /// Optional Roko temporal anchor hash
    #[serde(rename = "-65539", skip_serializing_if = "Option::is_none")]
    pub roko_anchor: Option<Vec<u8>>,
}
```

### Architecture Integration

```
crates/matric-core/src/trust/
  mod.rs                -- Trust attestation public API
  attestation.rs        -- TrustAttestationClaims struct + CBOR serialization
  cose_signing.rs       -- COSE_Sign1 creation/verification with FROST signatures
  verification.rs       -- Batch attestation verification
  revocation.rs         -- Revocation attestation handling
```

### Database Schema (future migration)

```sql
CREATE TABLE trust_attestations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attester_pubkey BYTEA NOT NULL,          -- indexed
    attestee_pubkey BYTEA NOT NULL,          -- indexed
    trust_domain TEXT NOT NULL,              -- indexed
    confidence SMALLINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    roko_anchor_hash BYTEA,
    raw_cbor BYTEA NOT NULL,                 -- full COSE_Sign1 envelope
    revoked_at TIMESTAMPTZ,
    CONSTRAINT valid_confidence CHECK (confidence BETWEEN 0 AND 100)
);

CREATE INDEX idx_trust_attester ON trust_attestations(attester_pubkey);
CREATE INDEX idx_trust_attestee ON trust_attestations(attestee_pubkey);
CREATE INDEX idx_trust_domain ON trust_attestations(trust_domain);
```

### Signing Flow

1. Construct `TrustAttestationClaims` struct
2. Serialize to canonical CBOR using `ciborium`
3. Wrap in `coset::CoseSign1Builder` with protected header (algorithm: EdDSA / FROST-Ed25519)
4. Sign the `Sig_structure` bytes using FROST threshold signing (ADR-001)
5. Encode final `COSE_Sign1` to CBOR bytes for storage/transmission

### Verification Flow

1. Decode `COSE_Sign1` from CBOR bytes
2. Extract protected header and payload
3. Reconstruct `Sig_structure` per RFC 9052
4. Verify FROST threshold signature against attester's MPC wallet public key
5. Validate claims (expiry, confidence range, domain)
6. Optionally verify Roko temporal anchor (ADR-004)

---

## References

- RFC 8949: Concise Binary Object Representation (CBOR)
- RFC 9052: CBOR Object Signing and Encryption (COSE)
- RFC 8392: CBOR Web Token (CWT)
- `ciborium` crate: https://crates.io/crates/ciborium
- `coset` crate: https://crates.io/crates/coset
- IANA COSE Algorithms Registry: https://www.iana.org/assignments/cose/cose.xhtml
- IANA CWT Claims Registry: https://www.iana.org/assignments/cwt/cwt.xhtml
