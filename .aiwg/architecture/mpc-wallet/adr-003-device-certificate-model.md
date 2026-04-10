# ADR-003: Device Certificate Model

| Field | Value |
|-------|-------|
| **Decision ID** | ADR-003 |
| **Status** | Proposed |
| **Date** | 2026-04-09 |
| **Deciders** | MPC Wallet Architecture Team |
| **Relates to** | ADR-001 (MPC Protocol Selection), ADR-002 (Trust Attestation Format), ADR-005 (Key Recovery Strategy) |

---

## Reasoning

The MPC wallet is a commissioning authority that rarely signs directly. Day-to-day operations (API authentication, note signing, trust attestation creation, peer communication) are performed by device keys. Each device (phone, laptop, hardware token) generates its own keypair and receives a certificate from the MPC wallet authorizing it to act on behalf of the user's identity. This certificate is the bridge between the high-security threshold key and the low-friction device key. The certificate model determines the security boundary, revocation semantics, and verification overhead for every authenticated operation in the system.

---

## Context

In the Personal Trust Network architecture, the MPC wallet public key is the user's root identity. However, the MPC wallet requires coordination between multiple devices (2-of-3 threshold signing per ADR-001) and is reserved for high-value operations:

- Enrolling a new device
- Issuing or revoking trust attestations
- Rotating MPC shares (key refresh)
- Signing device certificates

Device keys handle all frequent operations:
- Authenticating API requests to Fortemi
- Signing notes and attachments
- Creating ephemeral trust proofs ("I'm device X, authorized by wallet Y")
- Participating in P2P encrypted communication with other users' devices

The device certificate must answer: "Is this device key authorized to act for this MPC wallet identity, and what can it do?"

Design constraints:
- **Verification speed is critical**: Every API request, note signature, and trust query involves checking a device certificate. Target: < 1ms verification.
- **Offline verification**: Devices must verify each other's certificates without contacting the issuing wallet or a central authority.
- **Minimal size**: Certificates are transmitted with every authenticated request (like a bearer token, but cryptographic).
- **Revocation**: A compromised device must be revocable without regenerating the MPC wallet.
- **No institutional PKI**: There are no CAs, OCSP responders, or CRL distribution points. The MPC wallet is the only authority.
- **Scope limitation**: Different devices may have different permissions (e.g., a phone can sign notes but not enroll new devices).

The format chosen in ADR-002 (CBOR/COSE) sets a precedent for binary, standards-aligned encoding that this decision should consider.

---

## Evaluation Criteria

| # | Criterion | Weight | Description |
|---|-----------|--------|-------------|
| 1 | **Simplicity** | 30% | Conceptual simplicity of the model, ease of implementation, minimal code surface. A device cert is a small, well-defined object — the model should reflect that. |
| 2 | **Verification speed** | 25% | Time to verify a device certificate. This is on the hot path of every authenticated operation. Target: < 1ms including signature verification. |
| 3 | **Expressiveness** | 20% | Ability to encode permissions, scopes, temporal bounds, and device metadata. Must support differentiated device roles (full, read-only, signing-only). |
| 4 | **Standards alignment** | 15% | Alignment with established certificate/credential standards. Reduces implementation risk and enables third-party tooling. |
| 5 | **Size** | 10% | Wire size of the certificate. Transmitted with API requests and P2P messages. Should not dominate request overhead. |

---

## Options

### Option 1: Lightweight Custom DeviceCert

**Description**: A minimal, purpose-built certificate structure containing only the fields needed for device authorization. Encoded as CBOR (consistent with ADR-002) and signed with the MPC wallet's FROST threshold signature. Fixed set of required fields, optional extension map for future needs.

**Structure**:
```
DeviceCert {
  version: u8,                    // Format version (1)
  wallet_pubkey: [u8; 32],       // Issuing MPC wallet Ed25519 pubkey
  device_pubkey: [u8; 32],       // Device's Ed25519 public key
  device_id: [u8; 16],          // Unique device identifier (UUID)
  capabilities: u32,             // Bitfield: SIGN_NOTES | SIGN_TRUST | ENROLL_DEVICE | ADMIN
  issued_at: i64,               // Unix timestamp
  expires_at: i64,              // Unix timestamp
  extensions: Map<i32, Value>,   // Optional CBOR map for future fields
  signature: [u8; 64],          // FROST Ed25519 threshold signature
}
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Simplicity | 5 | Minimal fields, no abstraction layers, no schema resolution. A single Rust struct with serde derives. The entire implementation fits in one file. No external specification to comply with — the struct IS the specification. |
| Verification speed | 5 | Single Ed25519 signature verification (~50-80us). No certificate chain traversal, no extension parsing on the hot path. Deserialize CBOR (~5us) + verify signature (~70us) = ~75us total. |
| Expressiveness | 4 | Capability bitfield covers the known permission model (sign notes, sign trust, enroll devices, admin). Extension map allows adding fields without version bump. Limitation: bitfield caps at 32 capabilities without extending to u64. |
| Standards alignment | 2 | Proprietary format. Not recognizable by any standard certificate tooling. Cannot be inspected with `openssl`, imported into keystores, or validated by generic verifiers. |
| Size | 5 | ~185 bytes with CBOR encoding (no extensions). Smallest possible representation for the required fields. |

**Weighted Score**: (5 x 0.30) + (5 x 0.25) + (4 x 0.20) + (2 x 0.15) + (5 x 0.10) = 1.50 + 1.25 + 0.80 + 0.30 + 0.50 = **4.35**

### Option 2: X.509 Certificates

**Description**: Use standard X.509v3 certificates where the MPC wallet acts as a self-signed CA and issues device certificates. Device permissions are encoded as X.509 extensions (custom OIDs or standard key usage extensions). The certificate chain is: MPC Wallet (self-signed root) -> Device Certificate.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Simplicity | 1 | X.509 is enormously complex. ASN.1/DER encoding, extension criticality rules, name constraints, basic constraints, key usage flags, authority/subject key identifiers. The `x509-cert` and `rcgen` Rust crates help but still expose significant complexity. Overkill for a system with exactly one CA (the wallet) and one level of delegation (devices). |
| Verification speed | 3 | DER parsing is fast (~100-200us) and signature verification is the same Ed25519 operation. However, X.509 mandates checking extension criticality, basic constraints, validity periods, and (technically) revocation status. A compliant implementation adds overhead. Shortcutting checks defeats the purpose of using X.509. |
| Expressiveness | 5 | X.509v3 extensions can encode arbitrary permissions, constraints, and metadata. Key Usage, Extended Key Usage, and custom OIDs provide a rich permission model. Name constraints can limit delegation scope. The most expressive option by far, though most expressiveness is unused. |
| Standards alignment | 5 | The universal certificate standard. Every TLS library, hardware security module, keystore, and credential management system understands X.509. Maximum interoperability with existing infrastructure. |
| Size | 2 | Minimum ~500-800 bytes for a basic X.509 certificate with Ed25519. DER encoding overhead, distinguished names, extension structures, and OID encoding all contribute. 3-4x larger than the custom format. |

**Weighted Score**: (1 x 0.30) + (3 x 0.25) + (5 x 0.20) + (5 x 0.15) + (2 x 0.10) = 0.30 + 0.75 + 1.00 + 0.75 + 0.20 = **3.00**

### Option 3: CBOR Web Token (CWT) with Device Claims

**Description**: Encode device certificates as CWT (RFC 8392) tokens signed with COSE (RFC 9052). Uses registered CWT claims for standard fields (issuer, subject, expiry) and private claims for device-specific fields (capabilities, device_id). This aligns with ADR-002's choice of CBOR/COSE for trust attestations, creating a unified encoding layer across the trust system.

**Example (diagnostic notation)**:
```
COSE_Sign1 {
  protected: {1: -8},  // alg: EdDSA
  payload: {
    1: h'<wallet_pubkey>',          // iss (wallet)
    2: h'<device_pubkey>',          // sub (device)
    6: 1744156800,                  // iat
    4: 1775692800,                  // exp
    7: h'<device_uuid>',            // cti (token ID = device ID)
    -65540: 0x0000000F,             // capabilities bitfield
    -65541: "Pixel 9 Pro",          // device_name (human-readable)
    -65542: "android/14",           // device_platform
  }
}
```

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Simplicity | 4 | CWT is a thin standard — just CBOR claims in a COSE envelope. Same serialization as trust attestations (ADR-002). One encoding approach for both attestations and device certs. Slightly more ceremony than raw custom struct (claim number registry, COSE header construction) but well worth the standardization. |
| Verification speed | 5 | COSE_Sign1 verification: deserialize CBOR (~5us) + reconstruct Sig_structure (~2us) + Ed25519 verify (~70us) = ~77us. Identical performance to custom format because the cryptographic operation dominates. No chain traversal or extension parsing. |
| Expressiveness | 4 | CWT claim map accepts arbitrary key-value pairs. Capability bitfield in a private claim. Device metadata (name, platform, OS version) in additional private claims. Limitation: no built-in concept of "scoped permissions" — must define our own claim semantics, same as custom format. |
| Standards alignment | 4 | IETF RFC 8392 (CWT) and RFC 9052 (COSE). Same standards family as ADR-002's trust attestation format. Not as universally recognized as X.509 but increasingly adopted in IoT, WebAuthn, and mobile credential ecosystems. COSE verification libraries exist in all major languages. |
| Size | 4 | ~220-280 bytes for a typical device certificate. COSE envelope adds ~80 bytes over raw CBOR claims. ~35-50% larger than custom binary but with self-describing structure and standard framing. |

**Weighted Score**: (4 x 0.30) + (5 x 0.25) + (4 x 0.20) + (4 x 0.15) + (4 x 0.10) = 1.20 + 1.25 + 0.80 + 0.60 + 0.40 = **4.25**

### Option 4: Macaroons with Caveats

**Description**: Use macaroons (Birgisson et al., 2014) as device authorization tokens. The MPC wallet creates a root macaroon for a device, and caveats attenuate its permissions. Macaroons support delegation chains (a device can further restrict its own macaroon for a sub-context) and third-party caveats (requiring external verification). HMAC-based by default; can be adapted for public-key signatures.

**Evaluation**:

| Criterion | Score (1-5) | Rationale |
|-----------|-------------|-----------|
| Simplicity | 3 | Macaroon concept is elegant (chained HMAC caveats) but the delegation and attenuation model adds conceptual complexity beyond what device certificates need. First-party vs. third-party caveats, caveat verification chains, and the HMAC chaining model require careful understanding. Public-key adaptation (vs native HMAC) is non-standard. |
| Verification speed | 3 | Verification requires checking each caveat in the chain. For a simple device cert with 2-3 caveats, this is fast (~100-200us). But the model encourages adding caveats (time bounds, capability restrictions, context limits), and verification time grows linearly. Also, native macaroons use HMAC, not public-key signatures — adapting to Ed25519 loses the elegant chaining property. |
| Expressiveness | 5 | Macaroons excel at fine-grained, attenuatable permissions. A device could further restrict its own macaroon for a specific operation ("sign this note, but only in archive X, before 5pm"). Third-party caveats could require Roko temporal proof as a verification condition. Most expressive option. |
| Standards alignment | 1 | Macaroons are an academic construction (Google Research, 2014) with limited standardization. No RFC, no IANA registry. Implementations exist but vary in format and semantics. The `macaroon` Rust crate exists but is lightly maintained. Niche technology with small community. |
| Size | 3 | A basic macaroon with 3 caveats: ~300-500 bytes. Caveat strings are verbose (predicate encoding). Grows with each caveat added. Not as compact as CBOR alternatives for the same information content. |

**Weighted Score**: (3 x 0.30) + (3 x 0.25) + (5 x 0.20) + (1 x 0.15) + (3 x 0.10) = 0.90 + 0.75 + 1.00 + 0.15 + 0.30 = **3.10**

---

## Comparison Matrix

| Criterion | Weight | Custom DeviceCert | X.509 | CWT/COSE | Macaroons |
|-----------|--------|-------------------|-------|----------|-----------|
| Simplicity | 30% | **5** (minimal) | 1 (complex) | 4 (thin std) | 3 (elegant but layered) |
| Verification speed | 25% | **5** (~75us) | 3 (~200-300us) | **5** (~77us) | 3 (~150-200us) |
| Expressiveness | 20% | 4 (bitfield+ext) | **5** (X.509v3 ext) | 4 (CWT claims) | **5** (caveats) |
| Standards alignment | 15% | 2 (proprietary) | **5** (universal) | 4 (IETF) | 1 (academic) |
| Size | 10% | **5** (~185B) | 2 (~600B) | 4 (~250B) | 3 (~400B) |
| **Weighted Total** | | **4.35** | 3.00 | **4.25** | 3.10 |

---

## Decision

**Adopt the Lightweight Custom DeviceCert (Option 1)** encoded as CBOR for consistency with ADR-002, using the COSE_Sign1 envelope for the MPC wallet's threshold signature.

### Rationale

The custom DeviceCert scores highest (4.35) driven by its dominance in simplicity (30% weight) and verification speed (25% weight) — the two most critical criteria for a structure that sits on every authenticated request's hot path.

The close runner-up, CWT/COSE (4.25), was seriously considered and its standards alignment is valuable. The decision to favor the custom format over CWT is narrow and rests on a specific architectural insight: **device certificates are internal to the Fortemi system**. Unlike trust attestations (ADR-002), which are exchanged between users and potentially verified by third parties, device certificates are only created by a user's MPC wallet and verified by that same user's Fortemi instance (or their peers' instances, which all run Fortemi). There is no third-party verification scenario that benefits from CWT's standardization.

However, we adopt CWT's encoding approach in practice: the custom DeviceCert is serialized as CBOR with integer keys and wrapped in a COSE_Sign1 envelope. This means the implementation shares code paths with trust attestation verification (ADR-002). The difference is semantic, not structural — we define our own claim semantics rather than mapping to CWT registered claims.

The capability bitfield (u32) provides 32 discrete permissions, which is sufficient for the foreseeable permission model:

| Bit | Capability | Description |
|-----|-----------|-------------|
| 0 | `SIGN_NOTES` | Sign notes and attachments |
| 1 | `SIGN_TRUST` | Create trust attestations |
| 2 | `ENROLL_DEVICE` | Enroll new devices (requires MPC co-sign) |
| 3 | `REVOKE_DEVICE` | Revoke other device certificates |
| 4 | `ADMIN` | Full administrative access |
| 5 | `READ_ONLY` | Read access without signing capability |
| 6 | `ROKO_ANCHOR` | Submit temporal anchoring requests to Roko |
| 7-31 | Reserved | Future capabilities |

---

## Consequences

### Positive

- **Sub-100us verification**: Single Ed25519 signature check with minimal deserialization. Every API request, note verification, and trust query pays only ~75us for device authentication. At scale (1000 requests/sec), this is 75ms of CPU time — negligible.
- **Minimal attack surface**: The DeviceCert has ~200 lines of implementation code. No ASN.1 parser, no extension criticality logic, no certificate chain traversal. Fewer code paths = fewer bugs = fewer vulnerabilities.
- **CBOR/COSE code reuse**: Shares serialization and signing infrastructure with trust attestations (ADR-002). The `coset` crate handles COSE_Sign1 for both. One signing code path, one verification code path, parameterized by payload type.
- **Capability bitfield is auditable**: A single u32 encodes all permissions. Permission checks are bitwise AND operations (~1ns). The full permission set is visible in a single hex value. Easy to log, easy to debug, easy to reason about.
- **Compact bearer token**: At ~185-220 bytes (base64: ~250-300 chars), a DeviceCert fits comfortably in an HTTP header (`Authorization: DeviceCert <base64>`). Smaller than most JWTs.

### Negative

- **Proprietary format**: Third-party systems cannot validate DeviceCerts without Fortemi's verification code. If the trust network grows beyond Fortemi's ecosystem, a standardized credential wrapper (VC or CWT) will be needed for external-facing operations.
- **No delegation chains**: Unlike macaroons, a device cannot attenuate its certificate for a sub-context. If fine-grained, context-specific authorization is needed later (e.g., "this session can only access archive X"), a separate authorization layer must be added on top of the DeviceCert.
- **Revocation is out-of-band**: The DeviceCert itself contains no revocation status. Verifiers must check a revocation list (maintained by the user's Fortemi instance). This means revocation propagation has latency — a revoked device cert remains valid until verifiers refresh their revocation list. ADR-005 addresses recovery, but operational revocation checking needs a separate design.

### Neutral

- **Expiry enforces rotation**: The `expires_at` field ensures device certificates have a bounded lifetime. Recommended default: 90 days. Devices must periodically request re-certification from the MPC wallet, which serves as a natural key rotation trigger.
- **Extension map provides escape hatch**: The CBOR extension map (`Map<i32, Value>`) allows adding fields without a format version bump. New capabilities beyond the u32 bitfield, device attestation data (TPM quotes), or platform-specific metadata can be added as extensions. Parsers that don't understand an extension simply skip it.
- **No on-chain storage**: DeviceCerts are not stored on Roko. They are local credentials verified against the MPC wallet public key (which may be anchored on Roko). This keeps the certificate model independent of blockchain storage costs and latency.

---

## Implementation Notes

### Crate Structure

```
crates/matric-core/src/mpc/
  device_cert.rs       -- DeviceCert struct, CBOR serialization, capabilities
  device_cert_issuer.rs -- Certificate issuance (MPC wallet signs)
  device_cert_verifier.rs -- Certificate verification (any party)
```

### Rust Types

```rust
use bitflags::bitflags;
use serde::{Serialize, Deserialize};

bitflags! {
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct DeviceCapabilities: u32 {
        const SIGN_NOTES    = 0b0000_0001;
        const SIGN_TRUST    = 0b0000_0010;
        const ENROLL_DEVICE = 0b0000_0100;
        const REVOKE_DEVICE = 0b0000_1000;
        const ADMIN         = 0b0001_0000;
        const READ_ONLY     = 0b0010_0000;
        const ROKO_ANCHOR   = 0b0100_0000;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCert {
    /// Format version
    #[serde(rename = "1")]
    pub version: u8,

    /// Issuing MPC wallet Ed25519 public key
    #[serde(rename = "2")]
    pub wallet_pubkey: [u8; 32],

    /// Device's Ed25519 public key
    #[serde(rename = "3")]
    pub device_pubkey: [u8; 32],

    /// Unique device identifier
    #[serde(rename = "4")]
    pub device_id: [u8; 16],

    /// Capability bitfield
    #[serde(rename = "5")]
    pub capabilities: u32,

    /// Issued at (unix timestamp)
    #[serde(rename = "6")]
    pub issued_at: i64,

    /// Expires at (unix timestamp)
    #[serde(rename = "7")]
    pub expires_at: i64,
}

impl DeviceCert {
    /// Check if this certificate grants a specific capability
    pub fn has_capability(&self, cap: DeviceCapabilities) -> bool {
        DeviceCapabilities::from_bits_truncate(self.capabilities).contains(cap)
    }

    /// Check if the certificate is temporally valid
    pub fn is_valid_at(&self, unix_timestamp: i64) -> bool {
        self.issued_at <= unix_timestamp && unix_timestamp < self.expires_at
    }
}
```

### Issuance Flow

1. Device generates an Ed25519 keypair locally
2. Device sends its public key + device metadata to the user's Fortemi instance
3. User approves enrollment (explicit consent via UI)
4. MPC wallet constructs a `DeviceCert` with appropriate capabilities
5. MPC wallet signs the CBOR-serialized cert using FROST 2-of-3 (requires 2 existing devices to co-sign)
6. Signed `COSE_Sign1(DeviceCert)` is returned to the new device
7. Device stores the certificate alongside its private key in platform-secure storage (Keychain, KeyStore, TPM)

### Verification Flow (Hot Path)

```rust
pub fn verify_device_cert(
    cose_bytes: &[u8],
    expected_wallet: &[u8; 32],
    now: i64,
) -> Result<DeviceCert, CertError> {
    // 1. Decode COSE_Sign1 envelope (~5us)
    let cose = CoseSign1::from_slice(cose_bytes)?;

    // 2. Extract and deserialize DeviceCert from payload (~3us)
    let cert: DeviceCert = ciborium::from_reader(cose.payload.as_deref().ok_or(CertError::NoPayload)?)?;

    // 3. Check wallet identity
    if cert.wallet_pubkey != *expected_wallet {
        return Err(CertError::WalletMismatch);
    }

    // 4. Check temporal validity (~1ns)
    if !cert.is_valid_at(now) {
        return Err(CertError::Expired);
    }

    // 5. Verify FROST Ed25519 threshold signature (~70us)
    let verifying_key = ed25519::VerifyingKey::from_bytes(&cert.wallet_pubkey)?;
    cose.verify_signature(&[], |sig, data| {
        verifying_key.verify(data, &ed25519::Signature::from_slice(sig)?)
    })?;

    Ok(cert)
}
```

### Revocation Design

Device revocation uses a local revocation list stored in PostgreSQL:

```sql
CREATE TABLE revoked_device_certs (
    device_id BYTEA PRIMARY KEY,         -- 16-byte UUID from DeviceCert
    wallet_pubkey BYTEA NOT NULL,
    revoked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reason TEXT
);
```

The verifier checks this table (cached in-memory with 60s TTL) after signature verification. Revocation propagation to peers uses the existing SSE/WebSocket event system.

### API Integration

Device certificates are presented as HTTP headers:

```
Authorization: DeviceCert <base64-encoded COSE_Sign1>
```

The Axum middleware extracts, verifies, and injects the `DeviceCert` into the request extensions, making it available to all handlers via `Extension<DeviceCert>`.

---

## References

- RFC 9052: CBOR Object Signing and Encryption (COSE) — Structures and Process
- RFC 8392: CBOR Web Token (CWT)
- `bitflags` crate: https://crates.io/crates/bitflags
- `coset` crate: https://crates.io/crates/coset
- Birgisson, A. et al. (2014). "Macaroons: Cookies with Contextual Caveats" — NDSS 2014
- FROST Ed25519 verification: compatible with standard Ed25519 verification (same group, same signature format)
