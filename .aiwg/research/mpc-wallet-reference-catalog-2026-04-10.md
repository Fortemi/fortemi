# MPC Wallet Reference Catalog

**Created:** 2026-04-10  
**Purpose:** Curated, citable source base for designing Fortemi MPC wallet, device delegation, trust attestations, and secure key handling.

## Selection Policy

- Prefer **primary sources** (standards, RFCs, original papers, maintainer docs).
- Include **secondary sources** only when they add practical validation (audits, major workshop material).
- Include **vendor docs** for platform key storage/deployment behavior.
- Include **professional discussions** and **GitHub issues** for implementation pitfalls and ecosystem signals.

---

## 1) Primary Sources (Highest Authority)

### Threshold signatures / FROST
- RFC 9591 — The FROST Protocol (IRTF CFRG):  
  https://www.rfc-editor.org/rfc/rfc9591
- IETF datatracker entry for RFC 9591 (includes discussion links/metadata):  
  https://datatracker.ietf.org/doc/html/rfc9591
- Original FROST paper (IACR ePrint 2020/852):  
  https://eprint.iacr.org/2020/852

### Signature formats and curves
- BIP-340 — Schnorr Signatures for secp256k1 (Final):  
  https://bips.dev/340
- RFC 8032 — EdDSA (Ed25519/Ed448):  
  https://www.rfc-editor.org/rfc/rfc8032
- RFC 7748 — X25519/X448 (Elliptic Curves for Security):  
  https://www.rfc-editor.org/rfc/rfc7748

### Key derivation / password hardening
- RFC 5869 — HKDF:  
  https://www.rfc-editor.org/rfc/rfc5869
- RFC 9106 — Argon2:  
  https://www.rfc-editor.org/rfc/rfc9106

### Token/cert encoding
- RFC 9052 — COSE structures and process:  
  https://datatracker.ietf.org/doc/html/rfc9052
- RFC 8392 — CWT:  
  https://www.rfc-editor.org/rfc/rfc8392

---

## 2) Project Maintainer Documentation (Primary Practical)

### Zcash Foundation FROST implementation
- Repository (includes `frost-secp256k1-tr` and status notes):  
  https://github.com/ZcashFoundation/frost
- Project book/documentation portal:  
  https://frost.zfnd.org

### Rust crypto libraries used by this architecture
- `k256` Schnorr module docs (BIP-340):  
  https://docs.rs/k256/latest/k256/schnorr/
- `x25519-dalek` crate docs:  
  https://docs.rs/x25519-dalek/
- `ed25519-dalek` moved location notice (important for tracking active issue location):  
  https://github.com/dalek-cryptography/ed25519-dalek
- Active dalek monorepo (current issue/discussion locus):  
  https://github.com/dalek-cryptography/curve25519-dalek

---

## 3) Secondary Sources (Validated Expert Material)

- NCC Group public audit report of Zcash Foundation FROST implementation:  
  https://www.nccgroup.com/research/public-report-zcash-frost-security-assessment/
- NIST MPTS talk entry for FROST (Chelsea Komlo):  
  https://csrc.nist.gov/presentations/2020/mpts2020-1b3
- ICE-FROST paper (robustness/identifiable cheating extension context):  
  https://eprint.iacr.org/2021/1658

---

## 4) Vendor Documentation (Operational Security Behavior)

### Apple
- Secure Enclave key protection guidance:  
  https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave

### Android
- Android Keystore system guidance:  
  https://developer.android.com/privacy-and-security/keystore

### Microsoft / Windows
- CNG DPAPI / DPAPI-NG overview:  
  https://learn.microsoft.com/windows/win32/seccng/cng-dpapi
- TPM fundamentals (Windows security):  
  https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/tpm-fundamentals

---

## 5) Professional Discussions / Community Channels

- RFC 9591 datatracker page (entry point to mailing-list discussion metadata):  
  https://datatracker.ietf.org/doc/html/rfc9591
- BIP-340 includes Post-History with bitcoin-dev thread references:  
  https://bips.dev/340
- Zcash Foundation FROST repository discussions:  
  https://github.com/ZcashFoundation/frost/discussions
- dalek ecosystem discussions (current repository):  
  https://github.com/dalek-cryptography/curve25519-dalek/discussions

---

## 6) GitHub Issues to Track (Implementation Risk Signals)

### FROST ecosystem
- ZcashFoundation/frost #238 (trusted dealer key-share generation demo epic; helpful for implementer flow artifacts):  
  https://github.com/ZcashFoundation/frost/issues/238

### dalek / dependency and platform behavior
- x25519-dalek #92 — strict `zeroize` pin compatibility problem:  
  https://github.com/dalek-cryptography/x25519-dalek/issues/92
- curve25519-dalek #355 — large stack frames on BPF target:  
  https://github.com/dalek-cryptography/curve25519-dalek/issues/355

---

## 7) Suggested Citation Priority for Fortemi Design Docs

Use this citation order in ADRs/specs:

1. RFCs / standards / original papers
2. Maintainer repository docs for the exact crate/protocol version in use
3. Public audit reports
4. Vendor platform docs
5. Professional discussion threads / GitHub issues

---

## 8) Gaps to Fill Next

- Add concrete issue/discussion links from:
  - `RustCrypto/elliptic-curves` (`k256`-specific issue threads)
  - `ZcashFoundation/frost` security or nonce-management discussion threads
- Add reproducible “known-good version set” references (crate versions + changelog links) before implementation freeze.

