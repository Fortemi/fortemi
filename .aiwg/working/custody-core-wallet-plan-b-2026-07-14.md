# Plan B — CustodyCore Wallet Provider Rework (ROKO + EVM + MPC)

Date: 2026-07-14. Supersedes the pkcs12-first sequencing in
`wallet-provider-remaining-roadmap-2026-05-24.md` and the Phase-A ordering in
`custody-core-wallet-integration-audit-2026-07-14.md` (same directory).

Directives driving this plan:
- Rethink `pkcs12-local`; schema changes are acceptable.
- Target chains: **ROKO network and EVM**.
- CustodyCore must expose **MPC wallet capability**.
- Settlement happens **on ROKO** — the "settlement on Ethereum, Roko orders" line in
  `roko_network/docs/strategy/roko-network-value-proposition.md:215` is outdated and should be fixed.

---

## 1. Ground truth (deep review, 2026-07-14, HEAD `fbe6060`)

**Substantially complete components:**
- **FROST MPC crypto** — `custodycore-crypto/src/mpc.rs` (1,392 lines): real DKG
  (`run_in_process_dkg`), replay-safe nonce reservation, partial signing, coordinator
  sessions + aggregation, group verification, resharing with continuity proofs and share
  epochs. Curves: `Secp256k1` (**FROST Schnorr/BIP-340/Taproot**) and `Ed25519`. ~18 test fns
  + criterion benches. Zeroize + redacting Debug throughout.
- **Single-key ECDSA primitives** — `signing/secp256k1.rs`: recoverable ECDSA over 32-byte
  digests (r,s,v incl. 27/28 handling), BIP-340 verify; `signing/address.rs`: **Ethereum H160
  derivation** (`ethereum_address`, `0x…` format, `mw:` identity); `signing/domain.rs`
  domain tags. Known-answer tested.
- **ROKO receipt verification** — wired end-to-end: `core:1621` recovers ECDSA over
  Keccak-256 receipt digests and checks the ROKO authority set.
- **MPC persistence** — `custodycore-db`: full schema + `SqliteMpcStore` (wallets, share
  metadata, ceremonies, participant status, nonce counters, evidence, recovery/resharing).
  Secret shares never in DB — only `encrypted_share_ref` (device-local storage reference).
  CHECK constraints enforce curve/threshold invariants. ~38 tests.
- **Consent/policy state machine** — `service_impl.rs`: prepare → consent → execute with
  scope enforcement, idempotency/replay, CSRF/origin, fail-closed profiles. Wired + tested.
- **WalletProvider seam** — trait/registry/payload/envelope complete as a contract
  (Keccak-256 payload hash, domain tags, replay challenge, idempotency, structured
  verification states).
- **Design corpus** — ADR-001 (FROST, 2-of-3, user devices as shareholders), ADR-006
  canonical crypto profile (explicitly excludes threshold ECDSA at launch), DKG state-machine
  diagrams, phase-1 module contract, ADR-016 provider boundary, ADR-004 ROKO bridge.

**The missing middle (nothing produces a real wallet signature through the API today):**
- `execute_operation` (`service_impl.rs:315`) returns fixture JSON for every typed operation —
  `MpcDkg`, `SigningApprove`, `SigningDeriveAddress` never touch crypto or DB.
- `custodycore-crypto::mpc` is consumed by **no other crate**. `SqliteMpcStore` is never
  instantiated by the service.
- MPC HTTP paths (`MPC_DKG_INIT_PATH`, `MPC_SIGNING_REQUEST_PATH`, …) exist as constants +
  OpenAPI but are **not mounted** in the axum router (`local_runtime.rs:114-168`).
- `CustodyCoreWalletProvider.sign` returns `"custodycore-typed-operation:{id}"` — the
  placeholder is the whole path, and the provider advertises `ed25519`/`ecdsa-secp256k1`
  capabilities it cannot fulfill.
- **No chain-transaction layer at all**: no RLP/EIP-155/EIP-1559, no extrinsic builder, no
  alloy/ethers/subxt deps. ROKO submission is `submit_fake_roko_anchor` only.
- Device-share encryption-at-rest (the thing `encrypted_share_ref` points to) is unbuilt; no
  inter-device round transport (`channel.rs` from the phase-1 contract).
- `Pkcs12Local` is an enum variant only — there is no implementation to "replace"; we are
  free to redefine the slot.

**Chain facts (roko_network):** Substrate + Frontier; accounts are **H160 with ECDSA
secp256k1** (`EthereumSignature`); EVM chain ID **442**; standard txs are auto-timestamped by
validators (PoAT signed-extensions removed); anchor proof retrieved via `temporal_*` RPCs;
testnet live, mainnet not launched; `roko-indexer-sidecar` (subxt→Postgres) is the read path.

**MPC-ECDSA research verdict:** FROST cannot produce ECDSA. For native threshold ECDSA the
only credible open Rust choice is **Lockness `cggmp24`** (MIT/Apache, Kudelski-audited,
production at dfns; gaps: no identifiable aborts, no in-protocol key refresh). DKLs23 is
technically strong but commercially licensed. ZenGo multi-party-ecdsa is unmaintained — do
not use. A **dual-gate** pattern (FROST quorum authorizes → policy-enforced single ECDSA key
in keystore/HSM/enclave signs) is a legitimate transitional v1 (Turnkey-style), with a
documented single-point-of-key-compromise regression. Never derive Schnorr and ECDSA keys
from the same MPC key material (BIP-340 domain-separation assumptions; separate DKGs).

---

## 2. Target architecture

### 2.1 Provider lineup (schema change: `WalletProviderKind`)

| Kind | Replaces | Role | Key scheme(s) |
|---|---|---|---|
| `Noop` | (keep) | test-only, degraded/unsigned semantics | — |
| `CustodyCore` | (keep) | typed-operation custody path — made **real** (executes FROST/ECDSA via service) | per operation |
| `LocalKeystore` | **`Pkcs12Local`** | encrypted local keystore signer — dev default AND v1 chain signer | `ecdsa-secp256k1` (recoverable), `ed25519` |
| `Mpc` | (new) | threshold wallet — FROST Schnorr + ed25519 now; ECDSA path per §2.3 | `schnorr-secp256k1`, `ed25519`; later `ecdsa-secp256k1` |

Rationale for dropping PKCS#12: it is an x509/TLS container with no ecosystem fit for
EVM-style keys. `LocalKeystore` = Web3-secret-storage/EIP-2335-style encrypted JSON
(Argon2id/scrypt KDF + AEAD), holding k256 and ed25519 keys. It reuses the exact acceptance
criteria of old #203 (fail-closed load, secret redaction, deterministic fixtures, capability
metadata) with a chain-relevant container. The same keystore code doubles as the
**device-local encrypted share storage** backing `encrypted_share_ref` — one AEAD/KDF
implementation serves both needs.

### 2.2 One chain path for ROKO + EVM

Because ROKO is Frontier with H160/ECDSA accounts, implement the chain layer **once** as
EVM: EIP-1559 typed transactions + `eth_*` JSON-RPC submission. ROKO = `eip155:442`;
Ethereum/other EVM chains = additional chain refs. Substrate-side submission (subxt) is not
needed for v1; the `temporal_*` RPCs are read-side only. Anchor tx = standard tx carrying the
content hash (validators auto-timestamp); anchor proof read back via
`temporal_getTransactionTimestamp` / `temporal_getBlockMetadata`.

New module/crate `custodycore-chain`:
- EIP-1559 tx builder + RLP (prefer `alloy-consensus`/`alloy-primitives`; fall back to a
  minimal hand-rolled encoder if the dependency footprint is unacceptable — workspace
  currently has zero chain deps).
- Keccak digesting → provider `sign()` with **decoded-tx policy check before signing**
  (no blind digest signing: the signer must see chain_id, to, value, calldata).
- **Nonce allocator** owned in DB (per signing account, incremented in the same transaction
  as the submission-intent record — transactional outbox; reconcile on startup;
  replace-at-same-nonce for stuck txs).
- Idempotent submission: caller idempotency key → recorded tx hash; "already known" /
  finalized-at-nonce = success.
- Receipt polling: `eth_getTransactionReceipt` + ROKO `temporal_*` enrichment; feeds the
  existing `verify_temporal_receipt()`.

### 2.3 MPC wallet capability — two stages

- **Stage 1 (ship with Phase B below): FROST-native.** Expose the existing FROST stack
  through the `Mpc` provider for Schnorr-secp256k1 and ed25519 signing (offchain artifacts,
  receipts, execution records — the "general data" surface). This is mostly *wiring*, not
  new crypto.
- **Stage 2 (chain ECDSA): decision required — recommend dual-gate v1, cggmp24 v2.**
  - **v1 dual-gate:** FROST quorum consent (existing ceremony machinery) authorizes release
    of a policy-enforced `LocalKeystore`/HSM ECDSA key that signs the chain tx. Aligns with
    ADR-006's "no threshold ECDSA at launch"; document the single-point-of-key-compromise
    regression explicitly.
  - **v2 native threshold ECDSA:** add `cggmp24` as a peer MPC backend (separate DKG per
    scheme — never cross-derive), presignature pools, policy gates the online round.
    Requires revising ADR-006. Resharing = fresh DKG + wallet-record re-point (cggmp24 has
    no in-protocol refresh).
  - Future (non-blocking): BIP-340 Schnorr precompile in ROKO's Frontier runtime would let
    FROST keys authorize ROKO smart accounts natively — file as a roko_network research
    issue, not a custody dependency.

### 2.4 Schema changes (acceptable per directive)

1. `WalletProviderKind`: `Noop | CustodyCore | LocalKeystore | Mpc` (rename/remove
   `Pkcs12Local`; config-serde migration note — kebab-case values change).
2. `WalletProviderCapability` gains:
   - `key_scheme: KeyScheme` (`schnorr-secp256k1` | `ecdsa-secp256k1` | `ed25519`) —
     algorithms stop being free strings.
   - `chains: Vec<String>` — CAIP-2 refs (`eip155:442` = ROKO testnet EVM, `eip155:1`, …)
     plus `offchain` for artifact signing.
3. `WalletProviderPayload`: keep as-is for offchain artifacts (domain tag + Keccak canonical
   bytes already fit). Add a **typed chain-tx body** variant: `body` carries the unsigned
   EIP-1559 envelope with `domain_tag = custodycore.chain.tx.v1` and a `chain_ref` field so
   envelope validation binds signature → chain.
4. `SignatureEnvelope` gains: `key_scheme`, `chain_ref: Option<String>`,
   `signature_encoding` (e.g. `rsv-hex-65`), and optional `derived_address` evidence
   (H160 / `mw:`) so `SigningDeriveAddress` results are carried in-band.
5. `TypedOperation`: add `AnchorSubmit` and `SettlementExecute` (needed by #209/#210;
   the consent/policy machine and scope mapping then govern them like any sensitive op —
   new scopes `sign_anchor`, `sign_settlement` in the Fortemi mapping, wildcards still
   fail closed).
6. Config: `RokoSubmissionConfig` gains signer binding (`provider_id`, `key_id`,
   `chain_ref`); new `MpcProviderConfig` (curve/scheme, threshold, participant set) and
   `LocalKeystoreConfig` (path, KDF params) — validated per runtime profile like Noop today.

---

## 3. Delivery phases

### Phase 0 — decisions & doc hygiene (small, do first)
- **ADR-017 (new): provider lineup rework** — pkcs12→local-keystore, `Mpc` kind, schema
  changes above, chain model (EVM path for ROKO+EVM).
- **ADR-006 amendment**: record the ECDSA strategy (dual-gate v1 → cggmp24 v2) and the
  per-scheme separate-DKG rule.
- **roko_network doc fix**: correct the outdated "settlement on Ethereum" line in
  `docs/strategy/roko-network-value-proposition.md` (settlement is on ROKO).

### Phase A — wire the missing middle (highest leverage; no new crypto)
1. `execute_operation` → real branches: `MpcDkg` → `run_in_process_dkg` + `SqliteMpcStore`
   persistence; `SigningApprove` → FROST session (reserve nonce → partial → aggregate →
   `verify_group_signature`) → real `SignatureEnvelope`; `SigningVerify` → group verify;
   `SigningDeriveAddress` → `ethereum_address` / `mw:` derivation.
2. Mount the `MPC_*` and `SIGNING_*` routes in `local_runtime.rs` per the existing OpenAPI.
3. Wire `WalletProviderRegistry` into runtime startup from validated config (today it exists
   only in tests).
4. Replace the adapter placeholder — `CustodyCoreWalletProvider` returns real envelopes or
   stops advertising `ecdsa-secp256k1`/`ed25519` it can't produce.
5. **#208 audit events** while wiring: provider-selection + operation events (provider id,
   key id, payload hash, policy verdict, correlation id; never key material) — NIST 800-57
   style; degraded/no-op visibly auditable.
6. Share encryption at rest: AEAD+KDF wrap for device-local shares (the same keystore code
   as `LocalKeystore`), making `encrypted_share_ref` real.

### Phase B — providers (re-scoped #203 + new)
7. **`LocalKeystore` provider** (re-scope #203): encrypted keystore load (fail-closed,
   redacted), k256 recoverable ECDSA + ed25519, verify-after-sign, capability metadata with
   `key_scheme`/`chains`, H160 derivation.
8. **`Mpc` provider (Stage 1, FROST-native)**: bind/sign/verify over the wired FROST stack;
   ceremony lifecycle surfaced through existing typed operations; health reflects
   quorum/participant availability.
9. **#207 compatibility matrix**: shared contract harness across
   `noop`/`local-keystore`/`mpc`/`custodycore`; negative tests (domain tag, hash mismatch,
   replay, idempotency, unknown key, unsupported scheme/chain, noop-in-production); flip the
   flowgate to implementation-PASS; close epic #2.

### Phase C — chain layer + live ROKO wire (#209)
10. `custodycore-chain`: EIP-1559 builder, nonce allocator + transactional outbox,
    idempotent `eth_sendRawTransaction` client, receipt polling with `temporal_*`
    enrichment; decoded-tx policy check before every signature.
11. `submit_live_roko_anchor`: anchor tx (content hash calldata) signed by configured
    provider (`LocalKeystore` v1; dual-gate MPC when Stage 2 lands), `LiveTestnet` first;
    end-to-end validation: content hash → live anchor → real temporal receipt →
    `verify_temporal_receipt()` passes. Reconcile `RokoTemporalReceiptEnvelope` fields
    against actual `temporal_*` responses (post-PoAT-removal chain reality).
    Ops runbook + `live_enabled` gating. Needs from roko team: archive/RPC endpoints,
    funded testnet account (faucet #28 helps).

### Phase D — settlement + cross-repo (unchanged from audit)
12. **#210 direct settlement**: `SettlementExecute` typed op; precondition = verified
    temporal receipt; idempotent via consumed-hash/replay-challenge; write the missing
    planning artifact first.
13. agentic-sandbox **#586** (signed `result_hash` completion artifact) and Fortemi **#1007**
    (`event_outbox` → CustodyCore anchoring) proceed in parallel once Phase C is live on
    testnet.
14. Phase 2 items unchanged: **#211 escrow**, sandbox #587; **cggmp24 threshold-ECDSA
    upgrade** (Stage 2 v2); ROKO Schnorr-precompile research issue.

---

## 4. Filed (2026-07-14) — ADRs committed + issues filed

**ADRs** (roko/CustodyCore `main`, commit `ee4e275`):
- ADR-017 wallet-provider lineup + chain-signing rework (pkcs12→local-keystore, `mpc` kind, EVM path, schema).
- ADR-018 threshold/authorized ECDSA strategy (Stage 1 dual-gate → Stage 2 cggmp24; amends ADR-006).
- ADR-019 EVM chain-transaction layer + live ROKO submission (amends ADR-004).
- ADR-006 status block amended (threshold ECDSA now in scope for chain signing).

**Issues** (roko/CustodyCore): #203 re-scoped → local-keystore; **#213** wire the missing middle (p0, blocks all);
**#214** mpc provider (Stage 1 FROST); **#215** custodycore-chain EVM tx layer; **#216** Stage 1 dual-gate ECDSA;
**#217** Stage 2 cggmp24. Annotated: epic #2 (pivot summary + ADR links), #207 (4-provider matrix),
#208 (folded into #213), #209 (EVM live-wire re-scope + receipt reconciliation), #210 (settlement-on-ROKO + planning-artifact-first).

**roko_network**: #37 — correct outdated "settlement on Ethereum" doc line + provide custody RPC/archive endpoints,
funded testnet account, and `temporal_*` receipt-contract confirmation.

Sequencing: #213 → #203 + #214 → #207 gate → #215 + #216 (#209 live wire) → #210 settlement → #217 (Stage 2).

### Original issue-action table (for reference)

| Action | Repo | Notes |
|---|---|---|
| Re-scope #203 → `LocalKeystore` provider (EVM-compatible ECDSA + ed25519 encrypted keystore) | CustodyCore | Supersedes pkcs12; same security acceptance criteria |
| File: Phase A "wire the missing middle" (split: execute_operation reality; route mounting; registry runtime wiring; share encryption at rest) | CustodyCore | The deep review's central finding; blocks everything |
| File: `Mpc` WalletProvider (Stage 1 FROST-native) | CustodyCore | Mostly wiring |
| File: ADR-017 provider lineup + schema changes; ADR-006 amendment (ECDSA strategy) | CustodyCore | Phase 0 |
| Update #207 scope to 4-provider matrix | CustodyCore | |
| Re-scope #209 with EVM-path decision + receipt-envelope reconciliation | CustodyCore | |
| File: `custodycore-chain` transaction layer | CustodyCore | New crate/module |
| File: dual-gate MPC-authorized chain signing (Stage 2 v1) + later cggmp24 (v2) | CustodyCore | Two issues, sequenced |
| File: fix outdated settlement-model line in value-prop doc | roko_network | Confirmed outdated |
| File: provide stable RPC/archive endpoints + funded custody account for testnet | roko_network | Phase C dependency |
| File (research, future): BIP-340 Schnorr precompile in Frontier runtime | roko_network | Non-blocking |

## 5. What this changes vs the 2026-05-24 roadmap

- pkcs12-local is **not** the gating item anymore; the **service→crypto→db wiring** is.
  The old roadmap ordered #203 → #208 → #207; Plan B orders **wiring(+#208) → providers
  (#203-rescoped + mpc) → #207 gate → chain layer (#209) → settlement (#210)**.
- The "pkcs12" schema slot is redefined rather than implemented as designed — sanctioned by
  the directive that schema changes are acceptable.
- MPC moves from "future tier" to a **named provider kind with a two-stage crypto plan**,
  reusing the already-built FROST stack immediately.
- The settlement-model ambiguity is resolved (ROKO settles); the receipt-envelope
  reconciliation and endpoint asks on the roko side stand.

Sources: deep code review 2026-07-14 (CustodyCore HEAD `fbe6060`); threshold-ECDSA research
(cggmp21/24, DKLs23, ToB/Kudelski audits, Fireblocks MPC-CMP, Turnkey enclave model, arXiv
2607.08226, BIP-340); prior audit + onchain/offchain research in
`custody-core-wallet-integration-audit-2026-07-14.md`.
