# CustodyCore → ROKO Wallet Integration — Audit & Completion Plan (2026-07-14)

Scope: what is needed to complete the custody-core integration to allow wallet use on the
roko_network project — issue audit across `roko/CustodyCore`, `roko/roko_network`,
`Fortemi/fortemi`, `roctinam/agentic-sandbox`; code-state verification of both local repos;
and research synthesis on wallet-system best practices for signing onchain artifacts
(receipts, execution records) and offchain data (datasets).

---

## 1. Where things stand

### Phase 6 — injectable wallet provider (CustodyCore): half done, and the done half is verified

| Issue | Title | State | Code verification |
|---|---|---|---|
| #201 | WalletProvider trait + registry | closed | Confirmed — `custodycore-port/src/wallet_provider.rs:94-116` (bind/sign/verify, capabilities, health); registry fails closed (`:522-530`) |
| #202 | Canonical payload + signature envelope | closed | Confirmed — `WalletProviderPayload` w/ domain tag, idempotency key, replay challenge, audit correlation id; Keccak-256 payload hash; envelope validation enforces domain-tag + hash match |
| #204 | No-op provider, unsigned/degraded semantics | closed | Confirmed — degraded evidence; production-profile rejection in `validate_for_profile`; `enforce_required_cryptographic_proof` rejects unsigned |
| #205 | CustodyCore adapter | closed | Confirmed — prepare → consent → execute path; structured verification states. **Caveat:** emits placeholder proof strings, not real signature bytes |
| #206 | Fortemi policy/scope mapping | closed | Confirmed — `sign_trust`/`sign_notes`/`read_only` mapping; `*`/`admin`/`all`/`catch_all`/dotted-wildcard scopes fail closed |
| #203 | pkcs12-local provider (default real signer) | **open** | **No implementation code at all** — only the config enum variant + registry fixtures. This is the gating dependency (per `planning/wallet-provider-remaining-roadmap-2026-05-24.md`) |
| #208 | Audit/observability events | **open** | Partial — `audit_correlation_id` plumbing exists end-to-end, but **no audit event is emitted** on provider selection/execution; no result-state sink |
| #207 | Provider compatibility suite + flowgate evidence | **open** | Only inline per-provider unit tests (~16); no shared cross-provider matrix; gate report still systems-design PASS. The roadmap's `include_str!` blocker is already resolved (commit `d0325cf`) — the real blocker is #203 |

**Untracked gap found by inspection:** the `WalletProviderRegistry` is **never constructed
outside unit tests** — nothing in `custodycore-api/local_runtime.rs` or service startup wires
it. Provider selection is unreachable through the live service. No issue exists for this.

### ROKO settlement/anchoring (Phase 1 of `roctinam/strategy` → `partnership/roko-settlement-architecture.md`)

| Item | Repo/Issue | State |
|---|---|---|
| Energize live ROKO wire (testnet → mainnet) | CustodyCore #209 | Verification logic (`verify_temporal_receipt()`) done; `FakeLocal` transport done. **No chain client exists** — no subxt/jsonrpsee/reqwest dependency anywhere; `RokoSubmissionStatus::Submitted` is defined but never constructed; `LiveTestnet`/`LiveMainnet` unconditionally return `DependencyDegraded` |
| Direct-settlement primitive (pay-on-verified-completion) | CustodyCore #210 | **Zero code, zero planning artifacts** |
| Escrow / controlled exchange | CustodyCore #211 | Phase 2, explicitly deferred |
| Signed completion artifact (`result_hash`) at `mission.completed` | agentic-sandbox #586 | open (cross-repo Phase-1 dependency) |
| Anchor provenance via `event_outbox` → CustodyCore | Fortemi/fortemi #1007 | open (cross-repo Phase-1 dependency) |

### roko_network chain side (greenfield for custody)

- **No CustodyCore references exist in the chain repo.** Integration is entirely new on that side.
- Chain: Substrate + Frontier hybrid. **Accounts are Ethereum-style H160 with ECDSA/secp256k1
  signatures** (`node/primitives/src/lib.rs:51-85`), EVM chain ID **442**, token ROKO (18 dec).
  SS58 rendering exists (prefix 42) but the key scheme is ECDSA, not sr25519/ed25519.
- Anchoring surface: custom `temporal-transactions` pallet + `temporal_*` RPC namespace
  (`temporal_getTransactionTimestamp`, `temporal_getBlockMetadata`, `temporal_getConsensusTime`, …).
  Clients submit **standard** transactions; validators auto-timestamp (PoAT signed-extensions were
  **removed** — a temporal-aware client must not register them).
- Temporal receipts are ECDSA-signed (`TemporalSignature = sp_core::ecdsa::Signature`).
- Read path: `sidecar/` = `roko-indexer-sidecar` (subxt against an archive node → Blockscout Postgres).
- Testnet live (v3 chain spec, ~6s blocks); mainnet runtime builds but is not launched; no public
  endpoints in docs — CustodyCore needs endpoint URLs (archive node for the indexer).
- roko_network open issues (#33-35 packaging, #28 faucet, #26 infra) are not wallet blockers, but
  **no issue exists for the settlement transaction type** the strategy doc assigns to ROKO L1.

### Drift and risks surfaced

1. **Receipt-shape drift risk:** CustodyCore's `RokoTemporalReceiptEnvelope` (authority set/epoch/
   key, convergence, …) was designed against ADR-004 assumptions; roko_network has since removed
   PoAT signed-extensions and moved to validator-side timestamping with a different retrieval path
   (`temporal_*` RPCs). The envelope must be reconciled against real RPC responses before #209.
2. **Signature-scheme mismatch vs plan:** generic Substrate custody guidance favors ed25519 (HSM
   support), but ROKO account signatures are **secp256k1/ECDSA** — CustodyCore's ROKO transaction
   signer must be ECDSA (which is, conveniently, the best-supported curve in HSM/KMS/MPC tooling).
3. **Settlement-model ambiguity:** `roko_network/docs/strategy/roko-network-value-proposition.md:215`
   says "Settlement happens on Ethereum — Roko provides ordering"; the settlement architecture doc
   and #209/#210 say settle directly on ROKO. Needs an explicit decision before #210 is built.
4. **Adapter placeholder signatures:** `CustodyCoreWalletProvider` returns
   `signature: "custodycore-typed-operation:{id}"` with `degraded_mode=false` — evidence of a typed
   operation, not verifiable cryptography. Fine if documented; misleading otherwise.
5. **FROST-MPC claim:** the strategy doc describes a "FROST-MPC signing root" as existing; nothing
   in the repo inspection surfaced an MPC implementation — treat as aspirational until verified.

---

## 2. Research synthesis (research-team findings)

### Onchain signing (receipts, execution records, anchoring/settlement txs)

- **Custody tiers behind one interface:** PKCS#12 file (dev tier) → PKCS#11/HSM or cloud-KMS-bound-
  to-TEE-attestation (prod tier) → MPC/threshold (CGGMP21/DKLs23 for ECDSA; FROST RFC 9591 for
  Schnorr) as a later tier. Never hand-roll threshold crypto. Mature signer APIs (Fireblocks,
  Web3Signer, Vault transit) converge on: isolated signing process, narrow sign/verify API, shared
  anti-replay DB with locking, and a **policy-before-sign callback** that fails closed.
- **Payload design:** EIP-712-style domain separation for offchain artifacts (name, version,
  chain/genesis id, purpose tag); for extrinsics use the native chain signing payload verbatim
  (era, nonce, genesis hash, spec/tx version) — don't invent a parallel envelope for transactions.
- **Replay/idempotency:** mortal transactions by default; **own the nonce** (per-account allocator
  in your DB, incremented in the same transaction as the submission record — transactional outbox);
  caller idempotency keys stored transactionally; treat "already known"/finalized-at-nonce as
  success. Exactly-once is impossible — build at-least-once + idempotent processing.
- **Ops:** NIST SP 800-57 audit lifecycle (log key id, payload hash, policy verdict, requester,
  timestamp — never key material); defined cryptoperiods + rotation runbooks; verify-after-sign on
  every signature (catches Tink-class nonce-derivation bugs — cf. the 2026 $2.4M Cardano wallet
  exploit from an unaudited signer); audited crypto crates only; zeroize; per-purpose derived keys
  (anchoring vs settlement).

### Offchain data signing (datasets, execution/work records)

- **Envelope: DSSE** (sign `PAE(payloadType, payload)`) — injective length-prefixed encoding, no
  canonicalization in the verify path, and `payloadType` cryptographically domain-separates dataset
  signatures from execution records from transactions. CustodyCore's existing custom
  newline-delimited canonical encoding + domain tags is defensible; DSSE alignment buys tool interop.
- **Content: in-toto Statements** (subject = artifact digests; typed predicates per record kind;
  SLSA Provenance v1 fields where they fit) — free interop with cosign/rekor tooling.
- **Datasets: manifest-of-digests** (hash files, sign the manifest; Merkle root for selective
  inclusion proofs; OpenSSF Model Signing v1.0 compatible; sign Croissant metadata as a subject).
- **Anchor the envelope hash, not just the payload hash** — the anchor then proves the *signature*
  existed at time T, which is what makes signatures survive key rotation/revocation. Optional
  RFC 3161 TSA countersignature for legal recognition.
- **Key history:** never delete rotated public keys (did:web or x509 with validity windows) so
  verifiers evaluate keys as-of signing time; embed the verification bundle (CAdES-LTA principle)
  for long-term verifiability.

---

## 3. What is needed to complete the integration (ordered)

### Phase A — finish the wallet-provider package (all tracked, CustodyCore)
1. **#203 pkcs12-local provider** — the gating item. Key load from approved secret source only,
   fail-closed + redaction, deterministic sign/verify fixtures, capability metadata.
   *Research folds in:* verify-after-sign; zeroize; audited crates; per-purpose keys.
2. **#208 audit/observability events** — emit provider-selection and provider-operation events
   (provider id + correlation id + structured result states) to a real sink; plumbing already exists.
   *Research folds in:* NIST 800-57 event fields; never log secret material.
3. **#207 compatibility matrix + gate** — shared contract harness across noop/pkcs12-local/
   custodycore; negative tests (domain tag, hash mismatch, replay, idempotency, unknown key,
   unsupported provider/algorithm); flip gate to implementation-PASS; close epic #2.
4. **NEW (untracked): wire the registry into the live runtime** — provider selection must be
   reachable through service startup/config, not just unit tests.
5. **NEW (untracked): resolve adapter placeholder signatures** — either produce real signature
   bytes or explicitly document typed-operation evidence semantics so `degraded_mode=false` isn't
   misleading.

### Phase B — energize the ROKO wire (#209, plus decisions)
6. **DECISION (ADR): signing path + settlement model.** (a) ECDSA/EVM path (ethers-style, `eth_*`)
   vs Substrate path (subxt) — both hit the same H160 accounts; ECDSA is mandatory either way.
   (b) Does settlement happen on ROKO (per settlement-architecture doc) or on Ethereum with ROKO as
   ordering layer (per value-prop doc)? #210's shape depends on this.
7. **NEW: reconcile the temporal-receipt envelope** with actual `temporal_*` RPC responses
   (post-PoAT-removal chain reality) before building the transport.
8. **#209 LiveTestnet transport** — add the chain client dependency (none exists today), nonce
   allocator + transactional outbox, mortal txs, idempotent submission, retry/stuck-tx replacement;
   gate behind `live_enabled` + ops runbook; then LiveMainnet (mainnet isn't launched yet — testnet
   validation is the near-term target). Needs endpoint URLs (archive node) from the roko team.

### Phase C — settlement (cross-repo)
9. **#210 direct-settlement primitive** — precondition `verify_temporal_receipt()` passes;
   idempotent (consumed-hash/replay-challenge pattern); write the missing planning artifact first
   (none exists).
10. **agentic-sandbox #586** — signed completion artifact (`result_hash`, Ed25519/AgentCard JWS).
11. **Fortemi #1007** — `event_outbox` consumer handing `{entity_type, entity_id, content_hash}`
    to CustodyCore for anchoring; store temporal-receipt ref against the entity.
12. **roko_network: NEW issue if needed** — confirm the settlement transaction type (may be plain
    balance transfer / EVM contract) and publish stable RPC endpoints.
13. **Phase 2 (deferred):** #211 escrow, sandbox #587 metered claims.

### Research-driven backlog candidates (not yet issues)
- DSSE/in-toto alignment for offchain artifact signing (datasets + execution records);
  manifest-of-digests dataset signing; anchor the **envelope** hash.
- Provider tier roadmap: PKCS#11/HSM provider and remote-signer provider after pkcs12-local;
  MPC (FROST) later.
- Policy-before-sign hook (limits/allowlists/quorum) in front of `sign()`, fail-closed.
- Key-history document + rotation runbook (keys evaluated as-of signing time).

---

## 4. Suggested issue actions (pending authorization — none filed)

| Action | Repo | Note |
|---|---|---|
| File: wire WalletProviderRegistry into live runtime | CustodyCore | Untracked gap #4 above |
| File: adapter signature semantics (real bytes vs documented typed-op evidence) | CustodyCore | Drift #4 |
| File: ADR — ROKO signing path + settlement model decision | CustodyCore | Blocks #209/#210 shape |
| File: reconcile temporal-receipt envelope vs live `temporal_*` RPCs | CustodyCore | Drift #1 |
| File: settlement planning artifact for #210 | CustodyCore | No planning docs exist |
| File: settlement tx type + public endpoints for custody service | roko_network | Strategy doc assigns this to ROKO L1; no issue exists |
| Label/link: #209/#210/#211 have no labels or phase-6-style metadata | CustodyCore | Housekeeping |

Sources: issue trackers (CustodyCore #163–#212, roko_network #1–#36, fortemi #1007,
agentic-sandbox #586); local repo inspection (CustodyCore HEAD `fbe6060`, roko_network HEAD
`fc397aa`); `roctinam/strategy` → `partnership/roko-settlement-architecture.md`; research agents'
cited web sources (RFC 9591, EIP-712, Substrate tx format, DSSE/in-toto/SLSA/Sigstore, C2PA,
RFC 8785/3161/4998, NIST SP 800-57, OpenSSF Model Signing).
