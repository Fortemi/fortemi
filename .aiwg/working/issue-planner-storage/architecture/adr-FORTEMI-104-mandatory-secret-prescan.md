# ADR-FORTEMI-104: Pre-Ingest Secret Detection Is Mandatory; Combined Path-Denylist + Content-Regex; No Opt-Out in v1

**Status**: Proposed
**Date**: 2026-05-21
**Issue**: fortemi/fortemi#736
**Phase**: Elaboration (commissioned by Phase 3 SDLC corpus generation)
**Source**: Synthesis §3 Decision 5, §3 Decision 7 (defaults), §2.2 Disagreement 3, §5 R-1

## Context

Indexing user-owned directories in place creates a new failure mode that did not exist for Managed archives: **users will point Fortemi at directories that contain secrets they didn't intend to expose**. The most likely cases:

- A developer points Fortemi at their `~/projects/myapp/` directory; the directory contains `.env` with production database credentials.
- A team points Fortemi at a shared `~/srv/code/` repository; one developer committed an AWS access key six months ago in a config file, fixed it, but the key still appears in git history (and could be in any current checkout).
- A user points Fortemi at their `~/.ssh/` directory by mistake; `id_rsa` private key gets BLAKE3-hashed and the file contents end up chunked and embedded into pgvector.
- A user points Fortemi at a directory containing `.kube/config` or `.aws/credentials`.

Stream B documents real incidents from code-indexing tools that omitted this defense (synthesis §5 R-1). The blast radius is significant: once a secret's content is chunked and embedded, it lives in the pgvector store; a future semantic search for "database password" or "API key" can surface it; and even after the source file is fixed and the archive is rescanned, the old chunks must be invalidated (synthesis §3 Decision 5 — invalidation is automatic because content_hash changes trigger re-ingest, but the window of exposure between the leak and the rescan is the operator's risk).

Stream C found no existing secret-scanning gate anywhere in Fortemi — neither for Managed uploads (which is a separate gap worth addressing in a follow-up) nor for the new Referenced ingest path. So WS-3 (the `ScanWalker` workstream) introduces the gate for the first time.

Three combinations of detection were considered:

- **A: Path-based denylist only** — fast, simple, catches obvious cases (`.env`, `*.pem`, `id_rsa*`, `.ssh/`, etc.) at near-zero cost. Misses secrets that appear in unexpected files (e.g., AWS access key hardcoded in a Python script).
- **B: Pre-ingest content-based scanning** — apply gitleaks-style regex set (PEM private-key headers, AWS access key pattern, GitHub PAT pattern, JWT prefix pattern) to each file before chunking. Catches secrets regardless of filename.
- **C: A + B + post-ingest re-scan on key rotation** — full pipeline plus periodic re-validation.

The synthesis (§3 Decision 5 resolution) selected B + path-denylist combined, no continuous re-scan. The combination catches the most cases at the lowest cost; continuous re-scan is unnecessary because file modification already changes content_hash and triggers re-ingest, which re-runs the secret check.

The remaining question (§6 Q-3) is whether to expose `scan_config.skip_secret_scan: bool` for users who accept the risk in exchange for marginal speedup on very large repos. The synthesis recommends "no opt-out in v1" — the cost of the check is minimal (sub-millisecond per file for the patterns in Decision 7) and the risk of opt-out becoming the default for users who don't read warnings outweighs the marginal speedup.

## Decision

**Secret detection is mandatory at ingest for all Referenced archives in v1. No opt-out is exposed.** The detection mechanism is a combined path-denylist + content-regex check applied by `ScanWalker` (WS-3) before any file is hashed, chunked, embedded, or stored.

Concretely:

1. **Path-denylist (hard-stop, applied first):** Skip and quarantine any file whose path matches the denylist patterns enumerated in synthesis §3 Decision 7:
   - `.env*`, `.envrc`
   - `*.pem`, `*.key`, `*.p12`, `*.pfx`, `*.jks`
   - `id_rsa*`, `id_ed25519*`, `id_ecdsa*`, `*_rsa`, `*_dsa`, `*_ecdsa`
   - `.ssh/`, `.gnupg/`, `.aws/credentials`, `.aws/config`
   - `.kube/config`, `.docker/config.json`
   - `secrets.*`, `*.secret`, `credentials.json`
2. **Content-regex (hard-stop, applied second, for files passing path-denylist):** Read the first ~64 KB of the file and apply the regex set:
   - `-----BEGIN .* PRIVATE KEY-----` (catches PEM-formatted RSA, EC, DSA, OpenSSH, PGP private keys)
   - AWS access key pattern: `(AKIA|ASIA)[0-9A-Z]{16}`
   - GitHub PAT pattern: `ghp_[a-zA-Z0-9]{36}`, `github_pat_[a-zA-Z0-9_]{82}`
   - JWT pattern: `eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}` (with confidence threshold per gitleaks heuristics)
3. **Quarantine logging:** Every skipped file produces a `QuarantineEvent` written to the per-archive `archive_quarantine_log` table. The log records the absolute path, the reason (`path-denylist` or `content-secret-detected`), and the matched pattern name (e.g., `pem-private-key`, `aws-access-key`). **File contents are never logged** per `.claude/rules/token-security.md`.
4. **API exposure:** `GET /api/v1/archives/{name}/quarantined-files` (WS-7) returns the paginated quarantine log so operators can audit what was skipped.
5. **No opt-out config in v1.** The `scan_config` JSONB column accepts `additional_ignores` (extends the default ignore list) and `disable_default_ignores: bool` (replaces the *general* ignore list per Decision 7), but it does **not** accept `skip_secret_scan`. The secret-detection regex set and path-denylist are non-overridable.
6. **Defaults are conservative and unconfigurable in v1.** The specific patterns above are baked in. A future ADR may introduce operator-configurable secret-detection rules; v1 ships with the fixed set.

Edge cases:
- **False positives are accepted.** If a fixture file in a test repo contains a known-fake `-----BEGIN PRIVATE KEY-----` block for testing purposes, it will be quarantined. The operator can move it outside the source path or accept the quarantine.
- **Binary files >64 KB are content-scanned for the first 64 KB only.** Secrets embedded deeper than 64 KB into a binary are not caught; this is a deliberate cost/value trade-off (secrets in binary files >64 KB are rare; full-file scans would dominate the ingest cost).
- **Symlinked secret targets are caught the same way.** The `ignore` crate by default does not follow symlinks pointing outside the root (synthesis §5 R-7); for in-tree symlinks, the resolved target is what gets path-matched and content-scanned.

## Consequences

### Positive

- **Hard floor against the most common leakage scenarios.** PEM private keys, AWS access keys, GitHub PATs, JWTs — the patterns that dominate real incident reports are blocked at ingest before any content lands in the index.
- **No accidental opt-out via configuration.** Users who don't read warnings can't disable secret detection by setting a flag they didn't understand.
- **Quarantine log is auditable.** Operators can review what got skipped and confirm the detection is working as intended (or surface false positives). Path-only logging means the audit log itself is safe to share.
- **Low operational cost.** Sub-millisecond per file for the regex set; path-denylist is O(1) string match. Compared to BLAKE3 streaming and tree-sitter chunking, secret detection is in the noise.
- **Reinforces the trust model.** Users handing Fortemi access to their source directories expect Fortemi to be a responsible steward. Mandatory secret detection is the visible manifestation of that stewardship.
- **No invalidation problem for stale embeddings.** Because the gate fires before chunking, no chunks of secret content ever enter pgvector. There is no "redact the embeddings" remediation to attempt (a hard problem with vector data — the original content can sometimes be reconstructed from embeddings).

### Negative

- **False positives are operator burden.** A repo with fixture secrets (test fixtures, examples, security-research samples) will produce quarantine entries that the operator must triage. The audit endpoint helps but is not zero-cost.
- **64 KB content-scan limit may miss deep secrets.** A secret embedded at byte offset 100k of a 1 MB binary is not caught. This is a deliberate trade-off; full-file scanning would multiply ingest cost.
- **Regex set is necessarily incomplete.** Custom secret formats (proprietary tokens, less-common cloud-vendor keys) are not in the v1 pattern set. The synthesis (§3 Decision 7) notes the v1 defaults catch the 95% case; the long tail is acknowledged as accepted residual risk.
- **No "I really do want to index my Terraform state file with vault paths in it" escape valve.** Some legitimate workflows (security researchers indexing example malicious payloads, devops indexing template files with placeholder secrets) will be blocked. v1 stance: those workflows should not use Referenced archives over those directories. If demand emerges, a per-rule opt-out (e.g., `disable_pattern: aws-access-key`) is a future ADR.

### Neutral

- The pattern set will likely need updating as new secret formats emerge (e.g., new GitHub PAT formats, new cloud-vendor key prefixes). This becomes a maintenance burden, but the same maintenance applies to any secret-scanning tool. A periodic review (annually, or on a new-format incident) is the steady-state expectation.
- The mandatory-on stance can be revisited in v2 if operator demand for opt-out proves strong. v1 deliberately starts strict; loosening later is easier than tightening retroactively.

## Alternatives Considered

### Alternative A: Path-denylist only

Fast, simple, catches obvious cases. **Rejected** because:
- Misses secrets in non-obvious filenames (AWS keys in Python configs, JWTs in test scripts).
- The marginal cost of adding content-regex is small; the marginal value is large.

### Alternative C: A + B + post-ingest re-scan on rotation

Full pipeline plus periodic re-validation. **Rejected for v1** because:
- File modification already changes content_hash and triggers re-ingest, which re-runs the gate. Continuous re-scan is therefore redundant for the normal modification flow.
- The case it would catch is "secret in unchanged file, but the gate's regex set was updated and now would catch it." This is rare enough to be acceptable residual risk; the operator-triggered `POST /rescan?full=true` (synthesis §3 Decision 5) covers the explicit-update case.

### Alternative D (synthesis §6 Q-3 operator alternative): Allow opt-out via `scan_config.skip_secret_scan: bool`

Expose a config flag for users who accept the risk. **Rejected for v1** because:
- The cost saved is small (sub-millisecond per file).
- The risk of users enabling the opt-out without understanding it is high (synthesis §3 Decision 5 explicit reasoning).
- If demand emerges, a per-rule opt-out (rather than blanket skip) would be the safer v2 evolution.

## References

- Synthesis §3 Decision 5 — mandatory secret detection; combined path + content gate
- Synthesis §3 Decision 7 — concrete default path-denylist and content-regex patterns
- Synthesis §2.2 Disagreement 3 — Stream A's defense-in-depth framing vs Stream B's incident reports vs Stream C's no-existing-gate finding
- Synthesis §5 R-1 — secret leakage as top risk
- Synthesis §6 Q-3 — operator approval gate question (recommendation: A = no opt-out)
- `.claude/rules/token-security.md` — quarantine log path-only logging, never log file contents
- WS-3 (synthesis §4) — `ScanWalker` workstream where this gate is implemented
- WS-7 (synthesis §4) — `GET /quarantined-files` audit endpoint
- WS-9 (synthesis §4) — red-team test that drops PEM private keys into fixtures and verifies quarantine
- ADR-FORTEMI-101 — `ReferencedBackend` (the read path under which secret content would otherwise flow)
- `.aiwg/working/issue-planner-storage/architecture/software-architecture-doc.md` §4.3 — WS-3 component design with gate diagram
