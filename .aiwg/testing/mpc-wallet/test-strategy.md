# Test Strategy - MPC Wallet & Personal Trust Network

**Document ID:** TEST-STRATEGY-MPC-001
**Status:** Draft
**Created:** 2026-04-09
**Feature:** MPC Wallet with FROST threshold signing, P2P trust attestations, device delegation, Roko temporal bridge

---

## Phase 1: Core Strategy

### 1.1 Project Context

This test strategy covers a cryptographic identity system built on top of the existing matric-crypto PKE infrastructure. The feature introduces:

- **MPC wallet** using FROST threshold signing (2-of-3 default) across secp256k1 and Ed25519 curves
- **P2P trust attestations** signed by MPC wallet, forming a personal trust graph
- **Device delegation** via short-lived certificates commissioned by the MPC wallet
- **Roko temporal bridge** for nanosecond-precision temporal anchoring of trust attestations

The MPC wallet acts as the commissioning authority (root of trust), while device keys serve as runtime signers for day-to-day API operations. The system operates across three curves: secp256k1 (Roko/Ethereum compatibility), X25519 (existing PKE encryption), and Ed25519 (general-purpose signing).

Existing matric-crypto has 108 passing tests covering X25519/AES-256-GCM PKE. All new work must maintain full backward compatibility with the MMPKE01 format and `mm:` address scheme.

### 1.2 Quality Goals

| ID | Goal | Measurement | Priority |
|----|------|-------------|----------|
| QG-1 | Zero key material leaks | Property-based tests scanning memory after drop; no secrets in logs/errors/API responses | Critical |
| QG-2 | 100% coverage on all crypto operations | `cargo-llvm-cov` on encrypt, decrypt, sign, verify, DKG, threshold-sign paths | Critical |
| QG-3 | MPC signing completes <100ms on LAN | `criterion` benchmarks with 3 in-process parties | High |
| QG-4 | All trust attestations verifiable end-to-end | Integration tests covering attest -> store -> query -> verify -> revoke | High |
| QG-5 | Threshold enforcement is absolute | Property-based: t shares sign, t-1 MUST NOT, for all valid (n,t) combinations | Critical |
| QG-6 | Backward compatibility with MMPKE01 | Existing PKE test suite passes unmodified after all phases | High |
| QG-7 | Cross-curve key isolation | secp256k1 key never usable for X25519 ops and vice versa; enforced by type system and tested | Critical |

### 1.3 Test Levels and Scope

#### 1.3.1 Unit Tests (Target: 90% coverage on crypto crates)

All unit tests live inline via `#[cfg(test)] mod tests { ... }` in their respective source files.

**FROST DKG round functions**
- `Round1` package generation produces valid commitments
- `Round2` share distribution is correct for all participants
- Key package extraction yields consistent group public key
- Invalid commitment detection and rejection
- Duplicate participant ID rejection
- Minimum/maximum participant bounds (n >= t >= 2)

**Threshold signing correctness**
- `SigningNonces` generation uses CSPRNG
- `SigningCommitments` round-trip serialization
- Partial signature aggregation produces valid Schnorr signature
- Signature verification against group public key
- Signing with exactly t participants succeeds
- Signing with t+1 through n participants succeeds

**ECDSA secp256k1 sign/verify**
- Deterministic signing (RFC 6979)
- Signature DER and compact encoding
- Public key recovery from signature (ecrecover)
- Domain separation tags in signing context
- Known-answer tests against published test vectors

**Ed25519 sign/verify**
- Standard Ed25519 signing and verification
- Batch verification (when available)
- Known-answer tests against RFC 8032 test vectors
- Context-bound signatures (Ed25519ctx)

**Trust attestation serialization/deserialization**
- CBOR round-trip for TrustAttestation struct
- Field presence validation (attestor, subject, scope, timestamp, signature)
- Scope enum coverage (Identity, Data, Device, Recovery)
- Malformed input rejection (truncated, extra fields, wrong types)

**Device certificate generation and validation**
- DeviceCert struct creation with all required fields
- Expiry calculation from issuance time + TTL
- Signature verification of cert against MPC group public key
- Certificate chain: MPC root -> device cert -> API request
- Revocation flag handling

**Address derivation**
- `mm:` address from X25519 public key (existing, must not regress)
- `mm:` address from Ed25519 public key (new, different derivation path)
- `0x` Ethereum address from secp256k1 public key (keccak256, last 20 bytes)
- Known-answer tests for all three derivation paths
- Address parsing round-trip for all formats

**Receipt payload hashing**
- TemporalReceiptPayload serialization to canonical bytes
- SHA-256 hash of payload matches expected
- NanoMoment precision preservation (no truncation to millis)
- Payload ordering determinism (sorted fields)

#### 1.3.2 Integration Tests (Target: 80% coverage)

Integration tests in `crates/*/tests/` directories.

**Full DKG ceremony (3 simulated parties in-process)**
- Three `tokio::spawn` tasks simulate independent parties
- Round 1: all parties generate and broadcast commitments
- Round 2: all parties compute and distribute shares
- All parties derive the same group public key
- Group public key is a valid secp256k1/Ed25519 point

**Threshold signing with 2-of-3 quorum**
- Select 2 of 3 parties, perform signing ceremony
- All 3 possible 2-party subsets produce valid signatures
- Signatures from different subsets verify against same group key
- Concurrent signing sessions do not interfere

**Trust attestation lifecycle**
- Create attestation (MPC-signed) -> store in DB -> query by attestor -> query by subject -> verify signature -> revoke -> verify revocation
- Mutual trust: A trusts B, B trusts A, graph shows bidirectional edge
- Trust scope filtering: attest with scope=Data, query with scope=Identity returns empty

**Device certificate lifecycle**
- Commission device (MPC-signed cert) -> use cert for API auth -> cert approaches expiry -> auto-renew -> revoke -> auth fails
- Multiple devices for same MPC wallet
- Device cert cannot be used to commission other devices (no delegation chaining)

**PKE backward compatibility**
- Generate MMPKE01 encrypted blob with current code
- Decrypt with new code (after all phases applied)
- Generate MMPKE01 encrypted blob with new code
- Decrypt with logic identical to pre-MPC code path
- Address format `mm:` unchanged for X25519 keys

**Database migration tests**
- New tables (`mpc_share_metadata`, `trust_attestations`, `device_certs`, `temporal_receipts`) created successfully
- Existing tables unmodified
- Indexes created (no `CONCURRENTLY` -- sqlx transaction constraint)
- Enum types added correctly
- Rollback path verified (down migrations)

#### 1.3.3 Property-Based Tests (Critical for Crypto)

Using the `proptest` crate. These are the highest-priority tests after basic correctness.

```rust
// Threshold enforcement: t shares sign, t-1 cannot
proptest! {
    #[test]
    fn threshold_enforcement(
        n in 3u16..=7,
        t in 2u16..=n,
        message in prop::collection::vec(any::<u8>(), 1..256),
    ) {
        let (key_packages, group_key) = simulate_dkg(n, t);
        
        // t shares CAN sign
        let subset: Vec<_> = key_packages.iter().take(t as usize).collect();
        let sig = threshold_sign(&subset, &message).unwrap();
        assert!(group_key.verify(&message, &sig).is_ok());
        
        // t-1 shares CANNOT sign
        let insufficient: Vec<_> = key_packages.iter().take((t - 1) as usize).collect();
        assert!(threshold_sign(&insufficient, &message).is_err());
    }
}
```

**Signature correctness invariant**
- For any message and valid key package set, a threshold signature verifies with the group public key
- For any message and any OTHER group public key, verification fails

**Zeroization (memory scanning)**
- After `PrivateKey::drop()`, scan the memory region for key bytes -- must be zeroed
- After DKG share drop, share bytes are zeroed
- After signing nonce drop, nonce bytes are zeroed
- Implementation: use `zeroize` crate with `ZeroizeOnDrop` derive, validate with `unsafe` memory read in test

**DKG ordering invariance**
- Shuffle party ordering before DKG -- group public key is identical regardless of order
- Shuffle share distribution ordering -- group public key is identical

**Trust attestation round-trip**
- For any valid TrustAttestation, serialize -> deserialize produces identical struct
- For any modification to serialized bytes, deserialization either fails or signature verification fails

**Temporal receipt verification**
- For any valid receipt from Roko, `ecrecover(hash, signature)` returns the expected authority address
- Receipts with modified timestamps fail verification
- Receipts with modified payloads fail verification

#### 1.3.4 E2E / System Tests

Full user journey tests exercising the API layer.

**Complete user journey**
1. Generate MPC wallet (DKG with 3 parties) via `POST /api/v1/mpc/dkg/init`
2. Commission a device via `POST /api/v1/devices/commission`
3. Attest trust to a peer via `POST /api/v1/trust/attest`
4. Encrypt data to trusted peer using their public key
5. Verify temporal receipt for the trust attestation via `POST /api/v1/temporal/verify`
6. Revoke device via `DELETE /api/v1/devices/{id}`
7. Verify revoked device cannot authenticate

**API endpoint tests**
- All `/api/v1/mpc/*` endpoints: init DKG, get status, sign
- All `/api/v1/trust/*` endpoints: attest, query graph, revoke
- All `/api/v1/devices/*` endpoints: commission, list, revoke
- All `/api/v1/temporal/*` endpoints: verify, get receipt
- Error responses: invalid parameters, unauthorized, conflict states

**MCP tool tests**
- New MCP tools for trust management produce correct JSON
- MCP tools handle error cases gracefully (no panics, structured errors)

#### 1.3.5 Performance Tests

Using `criterion` crate for benchmarks.

| Benchmark | Target | Notes |
|-----------|--------|-------|
| FROST 2-round signing (2-of-3, secp256k1) | <100ms LAN | 3 in-process parties, measure wall clock |
| FROST 2-round signing (2-of-3, Ed25519) | <50ms LAN | Ed25519 is faster than secp256k1 |
| DKG ceremony (3-party) | <500ms | One-time setup cost |
| DKG ceremony (5-party) | <2s | Scaling test |
| Trust attestation verification | >10,000/sec | Single-thread throughput |
| Temporal receipt verification (ecrecover) | >5,000/sec | secp256k1 recovery |
| Device cert validation | >50,000/sec | Signature verify + expiry check |
| secp256k1 sign | >5,000/sec | Single key, deterministic |
| Ed25519 sign | >20,000/sec | Single key |
| Ethereum address derivation | >100,000/sec | keccak256 + slice |

Benchmarks run nightly in CI, not on every push (to avoid flaky timing on shared runners).

#### 1.3.6 Security Tests

These tests verify that security invariants hold under adversarial conditions.

**Threshold enforcement (negative testing)**
- With t-1 shares, signing MUST fail (not produce a weak/partial signature)
- With t-1 shares plus a fabricated share, signing MUST fail
- Replaying a previous signing round's commitments MUST fail
- Using nonces from a different signing session MUST fail

**Key material leak prevention**
- `Debug` impl for all secret types prints `[REDACTED]`, not key bytes
- `Display` impl for all secret types prints `[REDACTED]`
- `Serialize` is NOT derived for secret types (compile-time enforcement)
- API error responses containing crypto failures do not include key material
- Structured logging (`tracing`) does not emit key bytes at any log level
- Test: grep API response bodies and log output for known key byte patterns

**Replay attack resistance**
- Signing round IDs are unique (UUID per session)
- Resubmitting a previous round's nonce commitment is rejected
- Resubmitting a previous round's partial signature is rejected

**Cross-curve isolation**
- Attempting to use a secp256k1 private key in an X25519 ECDH operation fails at the type level (compile error) and at runtime if raw bytes are smuggled
- Attempting to use an Ed25519 key for secp256k1 signing fails
- Key storage format includes curve identifier; loading a key for the wrong curve fails

### 1.4 Automation Strategy

All tests run in CI/CD via Gitea Actions workflows.

| Test Category | Trigger | CI Gate? | Runner |
|---------------|---------|----------|--------|
| Unit tests | Every push, every PR | Yes (merge blocker) | matric-builder |
| Integration tests | Every push, every PR | Yes (merge blocker) | matric-builder (PostgreSQL container) |
| Property-based tests | Every push, every PR | Yes (merge blocker) | matric-builder |
| E2E / API tests | Every push to main | Yes (merge blocker) | matric-builder (PostgreSQL container) |
| Performance benchmarks | Nightly schedule | No (advisory) | matric-builder |
| Security tests | Every push, every PR | Yes (merge blocker) | matric-builder |
| Coverage report | Every push to main | Yes (>90% crypto, >80% API) | matric-builder (cargo-llvm-cov) |

Property-based tests use `PROPTEST_CASES=256` in CI (vs default 100 locally) for broader coverage on merge-blocking runs.

### 1.5 Frameworks and Tools

| Tool | Purpose | Crate/Package |
|------|---------|---------------|
| `#[tokio::test]` | Async test runtime | `tokio` |
| `cargo test --workspace` | Test runner | built-in |
| `proptest` | Property-based testing | `proptest` |
| `criterion` | Benchmarking | `criterion` |
| `cargo-llvm-cov` | Code coverage | `cargo-llvm-cov` |
| `cargo-audit` | Dependency vulnerability scanning | `cargo-audit` |
| `zeroize` | Secret zeroization (and test verification) | `zeroize` |
| `tempfile` | Temporary directories for key storage tests | `tempfile` |

No mocking frameworks for crypto operations -- all crypto tests use real primitives. Database tests use real PostgreSQL test containers (per project convention: never skip DB tests, never mock the DB).

---

## Phase 2: Methods & Environment

### 2.1 Test Data Management

**Generated keypairs per test**: Every test generates its own keypairs using `Keypair::generate()` (CSPRNG-backed). No shared key fixtures across tests.

**UUID-tagged isolation**: All database records created during tests use UUID identifiers (`uuid::Uuid::new_v4()`) to prevent collisions when tests run in parallel. Never use timestamp-based identifiers (per project convention -- parallel tests can collide on millis).

**Known-answer test vectors**: For address derivation and signature verification, use published test vectors:
- secp256k1: Bitcoin BIP-340 test vectors
- Ed25519: RFC 8032 Section 7 test vectors
- Ethereum addresses: EIP-55 checksum test vectors
- FROST: RFC 9591 Appendix test vectors

**DKG simulation helper**: A shared test utility `fn simulate_dkg(n: u16, t: u16) -> (Vec<KeyPackage>, GroupPublicKey)` runs a complete in-process DKG and returns key packages for use in signing tests. Lives in `crates/matric-crypto/src/mpc/test_helpers.rs` (compiled only under `#[cfg(test)]`).

**Trust attestation builders**: Test builder pattern for constructing TrustAttestation structs with sensible defaults:
```rust
TestAttestation::builder()
    .attestor(alice_group_key)
    .subject(bob_group_key)
    .scope(TrustScope::Identity)
    .build_and_sign(&alice_key_packages)
```

### 2.2 Test Environment

**Local development**:
- `cargo test --workspace` runs all unit + integration + property-based tests
- PostgreSQL via `docker run` or system install (connection: `postgres://matric:matric@localhost/matric`)
- No external services required (Roko bridge tests use recorded responses or known-good receipt fixtures)

**CI environment** (Gitea Actions on `matric-builder`):
- PostgreSQL test container with pgvector + PostGIS (from `build/Dockerfile.testdb`)
- `POSTGRES_USER=matric` is the superuser in testdb image
- Extensions (vector, postgis, pg_trgm) auto-created by `build/init-extensions.sh`
- Port collision avoidance: `base + (GITHUB_RUN_ID % 1000)`
- `--test-threads=1` for tests requiring serial execution (MPC ceremony tests with shared state)

**No mocks for database**: Per project convention, all database tests hit real PostgreSQL. The test container runs migrations on startup. Tests that need isolation use UUID-tagged records and clean up after themselves.

**Roko bridge test data**: Since Roko testnet may not always be available, temporal receipt verification tests use a fixture set of known-good receipts with pre-computed hashes and signatures. A separate `#[cfg(feature = "roko-live")]` flag enables live testnet verification for manual runs.

### 2.3 Defect Management

- All test failures filed as Gitea issues with label `bug`
- Crypto-related failures additionally labeled `security`
- Property-based test failures include the failing seed for reproducibility (`PROPTEST_SEED=...`)
- Performance regression issues labeled `performance`

### 2.4 Test Execution Order

Some MPC tests require serial execution due to shared ceremony state:

```
# In CI workflow
cargo test --workspace -p matric-crypto -- --test-threads=1 mpc::
cargo test --workspace --exclude matric-crypto
```

All non-MPC tests run in parallel (default `cargo test` behavior).

---

## Phase 3: Governance & Improvement

### 3.1 Quality Gates

| Gate | Criterion | Enforcement |
|------|-----------|-------------|
| Crypto coverage | >90% line coverage on `matric-crypto` | `cargo-llvm-cov` in CI, fail if below threshold |
| API coverage | >80% line coverage on `matric-api` MPC/trust/device endpoints | `cargo-llvm-cov` in CI |
| Zero key leaks | All property-based zeroization tests pass | CI merge blocker |
| Threshold enforcement | All (n,t) property tests pass with 256 cases | CI merge blocker |
| Backward compatibility | All existing `matric-crypto` tests pass unmodified | CI merge blocker (existing test suite) |
| No `#[ignore]` | No new `#[ignore]` attributes added | Code review policy |
| No `std::env::set_var` | No `set_var`/`remove_var` in test code | Code review policy (per project convention) |
| Performance baseline | FROST signing <100ms in benchmark suite | Nightly CI advisory (manual review if regressed) |

### 3.2 Risk-Based Testing Priority

| Risk | Likelihood | Impact | Testing Response |
|------|-----------|--------|-----------------|
| Threshold signing allows t-1 quorum | Low (FROST is well-studied) | Critical (complete security failure) | Property-based tests with exhaustive (n,t) combinations |
| Key material leaked in logs/errors | Medium (easy to accidentally log) | Critical (key compromise) | Grep-based tests on log output; `Debug`/`Display` redaction tests |
| DKG produces inconsistent group keys | Low | Critical (wallet unusable) | Property-based ordering invariance; integration test with 3 parties |
| Temporal receipt forged | Low (secp256k1 ecrecover is sound) | High (fake timestamps) | Known-answer tests against Roko test vectors |
| Device cert expiry bypass | Medium | High (stale device access) | Unit tests with mocked clock; integration test with short TTL |
| Cross-curve key confusion | Medium (multiple curves in one system) | Critical (wrong algorithm) | Type-system enforcement; runtime curve-ID checks; property tests |
| PKE backward compatibility broken | Low | High (existing encrypted data unreadable) | Full regression suite on MMPKE01 format |
| MPC ceremony timeout/hang | Medium (network-dependent) | Medium (degraded UX) | Integration tests with timeout assertions; cancellation tests |

### 3.3 Compliance and Standards

| Standard | Relevance | Test Coverage |
|----------|-----------|---------------|
| RFC 9591 (FROST) | DKG and threshold signing protocol | Known-answer tests from RFC appendix; property-based threshold enforcement |
| RFC 6979 | Deterministic ECDSA (secp256k1) | Known-answer tests against published vectors |
| RFC 8032 | Ed25519 signing | Known-answer tests against Section 7 vectors |
| EIP-55 | Ethereum address checksum | Known-answer tests; round-trip property tests |

Audit trail: All DKG ceremonies and threshold signing operations produce structured log entries (ceremony ID, participant count, threshold, timestamp) for post-hoc review. Tests verify these log entries exist without containing key material.

### 3.4 Continuous Improvement

**After each phase delivery**:
1. Review coverage report -- identify untested paths
2. Review property-based test failure seeds from CI -- add as explicit regression tests
3. Update benchmark baselines if hardware changes
4. Add any discovered edge cases as named test cases (not just property-based)

**Quarterly**:
1. Run `cargo-audit` and update dependencies
2. Review FROST library upstream for security advisories
3. Re-run property-based tests with `PROPTEST_CASES=10000` for deeper exploration
4. Review CI timing -- split slow test suites if total exceeds 10 minutes

### 3.5 Test Traceability Matrix

| Feature | Unit | Integration | Property | E2E | Perf | Security |
|---------|------|-------------|----------|-----|------|----------|
| secp256k1 sign/verify | X | | X | | X | |
| Ed25519 sign/verify | X | | X | | X | |
| FROST DKG | X | X | X | X | X | |
| Threshold signing | X | X | X | X | X | X |
| Trust attestations | X | X | X | X | | |
| Device certificates | X | X | | X | X | |
| Temporal receipts | X | X | X | X | X | |
| Address derivation | X | | X | | X | |
| PKE backward compat | | X | | | | |
| Key zeroization | | | X | | | X |
| Threshold enforcement | | | X | | | X |
| Cross-curve isolation | X | | | | | X |
| Replay resistance | | | | | | X |
