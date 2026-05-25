# Tenant Isolation Regression Suite — TI-EXTSTORAGE-*

**Issue**: fortemi/fortemi#736
**Phase**: Phase 3 — SDLC Corpus Generation
**Date**: 2026-05-21
**Sole input**: `@.aiwg/working/issue-planner-storage/synthesis.md`

Per Fortemi's existing TI-1..TI-10 pattern (the multi-tenant boundary suite from the fortemi-auth memory and ADR-090 schema isolation), this document specifies the corresponding TI-EXTSTORAGE-1 through TI-EXTSTORAGE-10 scenarios for Referenced storage. These are the explicit attacker scenarios that the WS-9 security suite must implement and that must all pass on every PR touching this epic.

The suite follows synthesis §5 (R-1, R-2, R-7 risks) and operationalizes synthesis §3 Decisions 2, 3, 5, 7, 8 against adversarial conditions.

---

## Suite Conventions

- Every test creates two tenants, A and B, with isolated PG schemas (`tenant_a`, `tenant_b`).
- Tenant A and Tenant B authenticate with distinct API keys (or OAuth clients in multi-tenant deployments per CLAUDE.md FORTEMI_MULTI_TENANT).
- Source paths use unique tmpdirs per test to prevent cross-test pollution.
- Test framework: `#[tokio::test]` with manual pool (per CLAUDE.md PostgreSQL Migration Compatibility — these tests cannot run in transactions because they exercise schema-level isolation across multiple schemas).
- Each TI test is independent. No shared state. No `#[ignore]`.
- The assertion command shown in each scenario is the load-bearing check — if that command's output changes, the test fails and the suite blocks the PR.

---

## TI-EXTSTORAGE-1: Cross-Tenant Archive Listing

**Threat**: Tenant B discovers the existence of Tenant A's Referenced archives via the list-archives API or metadata leak.

**Setup**:
- Tenant A creates Referenced archive `tenant-a-code` with `source_path=/tmp/A/code`
- Tenant B authenticates with its own credentials and queries `GET /api/v1/archives`

**Action**:
```bash
curl -H "Authorization: Bearer ${TENANT_B_TOKEN}" \
  -H "X-Fortemi-Memory: default" \
  http://localhost:3000/api/v1/archives | jq .
```

**Expected**: Response does NOT contain `tenant-a-code`. Response contains only archives Tenant B owns or has been granted access to.

**Verification**: `jq '.[] | select(.name == "tenant-a-code")' < response.json` returns empty.

**References**: Synthesis §3 Decision 1 (archive-level mode); §2.1 finding 8 (per-archive PG schema isolation must be preserved); ADR-090.

---

## TI-EXTSTORAGE-2: Path-Traversal via API Read

**Threat**: Tenant B knows Tenant A's archive name (or guesses it) and attempts to read files from Tenant A's referenced root via path-traversal in a download/stream endpoint.

**Setup**:
- Tenant A creates Referenced archive `tenant-a-code` with `source_path=/srv/fortemi/A/code`
- File `/srv/fortemi/A/code/secret.txt` exists with content "TENANT-A-CONFIDENTIAL"
- Tenant B authenticates and crafts a download URL with `../../etc` or absolute-path bypass against a different archive Tenant B owns

**Action**:
```bash
# Tenant B owns archive "tenant-b-data". Try path-traversal in the file-download endpoint:
curl -H "Authorization: Bearer ${TENANT_B_TOKEN}" \
  "http://localhost:3000/api/v1/archives/tenant-b-data/files/../../srv/fortemi/A/code/secret.txt"
```

**Expected**: HTTP 404 (canonicalization rejects the path) or HTTP 403 (access denied). Response body MUST NOT contain "TENANT-A-CONFIDENTIAL".

**Verification**:
```bash
test "$HTTP_STATUS" = "404" -o "$HTTP_STATUS" = "403"
! grep -q "TENANT-A-CONFIDENTIAL" "$RESPONSE_BODY"
```

**References**: Synthesis §3 Decision 7 / Q-5 (path canonicalization + allowlist at create time); §5 R-2; WS-7 gating.

---

## TI-EXTSTORAGE-3: Scan Job Schema-Context Isolation

**Threat**: A scan job for Tenant A's archive executes SQL under Tenant B's schema (e.g., `search_path` confusion in the job worker), leaking chunks/blobs/embeddings into Tenant B's tables.

**Setup**:
- Tenant A creates Referenced archive `tenant-a-code` (which provisions schema `tenant_a` and ensures `archive_context.storage_mode='referenced'`).
- Tenant B has an existing archive `tenant-b-data` in schema `tenant_b`.
- Tenant A triggers `POST /api/v1/archives/tenant-a-code/rescan`.

**Action**: Allow the scan to complete. Inspect schema-level table contents.

**Expected**:
- After scan: `tenant_a.blobs` contains entries for Tenant A's source files.
- After scan: `tenant_b.blobs` count is unchanged from pre-scan baseline.
- After scan: `tenant_a.attachments` storage_backend values are all 'referenced' (with the derived ones being 'filesystem').
- After scan: `tenant_b.attachments` has zero rows where storage_backend='referenced' AND created_at > scan_start_timestamp.

**Verification**:
```sql
-- Baseline before scan
SELECT count(*) FROM tenant_b.blobs;  -- N

-- After scan
SELECT count(*) FROM tenant_b.blobs;  -- must still be N
SELECT count(*) FROM tenant_a.blobs WHERE storage_backend = 'referenced';  -- must be > 0
SELECT count(*) FROM tenant_b.attachments
  WHERE storage_backend = 'referenced'
    AND created_at > '${SCAN_START}';  -- must be 0
```

**References**: Synthesis §2.1 finding 8 (per-archive schema isolation); §2.3 constraint 1 (trait surface fixed); WS-4 scan handler must respect `SET LOCAL search_path` discipline.

---

## TI-EXTSTORAGE-4: Symlink Escape Skipped and Logged

**Threat**: Tenant A's source directory contains a malicious symlink pointing to `/etc/passwd` (or `/etc/shadow`, or another tenant's source root). The scan follows the symlink and indexes the target file.

**Setup**:
- Tenant A creates `/tmp/tenant-a-source/` with a real file `/tmp/tenant-a-source/legit.txt` and a symlink `/tmp/tenant-a-source/escape -> /etc/passwd`.
- Tenant A creates Referenced archive `tenant-a-code` with `source_path=/tmp/tenant-a-source`.
- Tenant A triggers scan.

**Action**: Wait for `scan_status='idle'`, then inspect `tenant_a.blobs` for any entries derived from `/etc/passwd` content.

**Expected**:
- `tenant_a.blobs` contains the BLAKE3 hash of `legit.txt` only.
- `tenant_a.blobs` does NOT contain a BLAKE3 hash matching `/etc/passwd` content.
- Quarantine log contains an entry for `escape` with reason `symlink_out_of_root` (or equivalent).

**Verification**:
```sql
-- /etc/passwd known hash (computed once and pinned in the test)
SELECT count(*) FROM tenant_a.blobs WHERE content_hash = '${ETC_PASSWD_BLAKE3}';  -- must be 0

SELECT count(*) FROM tenant_a.quarantine_log
  WHERE path = 'escape' AND reason = 'symlink_out_of_root';  -- must be 1
```

**References**: Synthesis §3 Decision 7 (default `ignore` crate symlink-loop protection, never-follow-symlinks-out-of-root); §5 R-7; WS-3 gate.

---

## TI-EXTSTORAGE-5: Mount Disappearance Returns Degraded Status

**Threat**: Mid-scan, Tenant A's NFS mount disappears (mount unmounts, network drops). The scan job hard-fails with a 500, the archive enters an inconsistent state, or worse — partial inserts leave stale chunks.

**Setup**:
- Tenant A creates Referenced archive against a mount point Tenant A controls.
- Test simulates mount disappearance mid-scan (use a tmpfs that the test unmounts, or a directory the test renames atomically).
- Trigger scan.

**Action**: During scan, atomically rename or unmount the source path.

**Expected**:
- Scan job completes with `scan_status='error'` (not silently 'idle').
- Error reason is logged with cause `source_path_unreachable` (or equivalent).
- No partial blob inserts (transaction rollback for the unreachable directory).
- Subsequent `GET /api/v1/search` against the archive returns cached results from any blobs that DID complete pre-disappearance, with a warning flag indicating staleness (per §3 Decision 8 lenient mode).
- Subsequent `POST /api/v1/archives/{name}/rescan` returns HTTP 503 (per §3 Decision 8 write-side strict).

**Verification**:
```bash
# Read still works (lenient)
test "$(curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/api/v1/search?...)" = "200"

# Write/rescan refused
test "$(curl -s -X POST -o /dev/null -w '%{http_code}' \
  http://localhost:3000/api/v1/archives/tenant-a-code/rescan)" = "503"

# Scan status reflects error
test "$(curl -s http://localhost:3000/api/v1/archives/tenant-a-code/scan-status | jq -r .status)" = "error"
```

**References**: Synthesis §3 Decision 8 (failure modes — lenient read, strict write); §5 R-5; WS-7.

---

## TI-EXTSTORAGE-6: Secret Quarantine — Tenant-Scoped Logging

**Threat**: Tenant A's source directory contains an `id_rsa` file. The secret-detection layer correctly skips it, but the quarantine log entry leaks to Tenant B (cross-tenant log visibility), or the file's content_hash is computed and recorded in a way that lets Tenant B discover the secret existed.

**Setup**:
- Tenant A creates `/tmp/tenant-a-source/id_rsa` containing fake-but-PEM-formatted RSA private key.
- Tenant A creates Referenced archive and triggers scan.
- Tenant B authenticates and queries `GET /api/v1/archives/tenant-a-code/quarantined-files`.

**Action**:
```bash
curl -H "Authorization: Bearer ${TENANT_B_TOKEN}" \
  http://localhost:3000/api/v1/archives/tenant-a-code/quarantined-files
```

**Expected**: HTTP 403 or 404 (Tenant B is not authorized for Tenant A's archive metadata).

**Additional verification (Tenant A's view)**:
- Tenant A's query of `GET /api/v1/archives/tenant-a-code/quarantined-files` returns the `id_rsa` entry with reason `path_denylist_match: id_rsa_pattern`.
- Tenant A's quarantine log entry MUST NOT contain the file's content (the secret material itself).
- `tenant_a.blobs` MUST NOT contain a row for `id_rsa` — no hash, no embedding, no chunk.

**Verification**:
```bash
# Tenant B cannot see Tenant A's quarantine
test "$(curl -s -o /dev/null -w '%{http_code}' \
  -H "Authorization: Bearer ${TENANT_B_TOKEN}" \
  http://localhost:3000/api/v1/archives/tenant-a-code/quarantined-files)" = "403"

# Tenant A's quarantine entry does not contain the secret content
! grep -q "PRIVATE KEY" "$TENANT_A_QUARANTINE_RESPONSE"

# No blob inserted
psql -c "SELECT count(*) FROM tenant_a.blobs WHERE source_path LIKE '%id_rsa'"  # = 0
```

**References**: Synthesis §3 Decision 5 (pre-ingest content-based secret detection + quarantine logging); §5 R-1.

---

## TI-EXTSTORAGE-7: Path-Traversal Canonicalization at Archive Create

**Threat**: Tenant B crafts a `create_referenced_archive` request with `source_path` containing `../../etc/` to register an archive that, post-canonicalization, points at a system directory or another tenant's data.

**Setup**:
- Multi-tenant deployment with `FORTEMI_REFERENCED_STORAGE_ROOTS=/srv/fortemi/tenants` set (per synthesis Q-5).
- Tenant B authenticates and posts a creation request.

**Action**:
```bash
curl -X POST -H "Authorization: Bearer ${TENANT_B_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"name":"evil","source_path":"/srv/fortemi/tenants/B/../../A/code"}' \
  http://localhost:3000/api/v1/archives/referenced
```

**Expected**: HTTP 400. Response error indicates path is outside allowlist after canonicalization (the path resolves to `/srv/fortemi/A/code`, which IS under `/srv/fortemi/tenants` only by string-prefix, but canonicalization should resolve it correctly OR the allowlist should be tenant-scoped).

**Stronger requirement**: The allowlist check MUST canonicalize via `std::fs::canonicalize` (resolving symlinks, `..`, `.`) BEFORE comparing against `FORTEMI_REFERENCED_STORAGE_ROOTS` entries. String-prefix matching is forbidden.

**Verification**:
```bash
test "$HTTP_STATUS" = "400"
grep -q "outside allowed roots" "$RESPONSE_BODY"
# Confirm no archive was created
psql -c "SELECT count(*) FROM archive_registry WHERE name = 'evil'"  # = 0
```

**References**: Synthesis §3 Q-5 (path allowlist); §5 R-2; WS-7 create-time validation.

---

## TI-EXTSTORAGE-8: Cross-Tenant Dedup Independence

**Threat**: Tenant A and Tenant B both have a file with identical content (BLAKE3 hash collision is impossible cryptographically, but identical content is common — e.g., a published open-source file). The dedup logic in WS-4 incorrectly shares the chunk row across tenant schemas, allowing Tenant B to read content via Tenant A's chunks.

**Setup**:
- Tenant A's `/tmp/A/file.txt` and Tenant B's `/tmp/B/file.txt` both contain the same content.
- Both tenants create Referenced archives pointing at their respective sources.
- Both scans complete.

**Action**: Inspect `tenant_a.blobs` and `tenant_b.blobs` for the shared content hash.

**Expected**:
- Both schemas contain a row with the same `content_hash`.
- The rows are SEPARATE entities — each scoped to its own tenant schema.
- Tenant B querying search results sees only Tenant B's chunks (sourced from Tenant B's `tenant_b.chunks` table).
- No mechanism allows Tenant B to read `tenant_a.blobs` rows.

**Verification**:
```sql
-- Both tenants have an entry for the shared hash
SELECT count(*) FROM tenant_a.blobs WHERE content_hash = '${SHARED_HASH}';  -- = 1
SELECT count(*) FROM tenant_b.blobs WHERE content_hash = '${SHARED_HASH}';  -- = 1

-- Schema-level isolation: Tenant B's role/session cannot SELECT from tenant_a.blobs
SET SESSION AUTHORIZATION tenant_b_role;
SELECT count(*) FROM tenant_a.blobs;  -- must error: permission denied
```

**References**: Synthesis §2.1 finding 8 (per-archive schema isolation); §3 Decision 1 (archive-level mode); ADR-090.

---

## TI-EXTSTORAGE-9: Drop-Archive Source Preservation Invariant

**Threat**: `drop_archive_schema()` for a Referenced archive calls `backend.delete()` on the user's source files, destroying user data.

**Setup**:
- Tenant A creates Referenced archive `tenant-a-code` with `source_path=/tmp/tenant-a-source`.
- Tenant A's source directory contains 5 files.
- Tenant A triggers ingest (creating some derived artifacts in companion location).
- Tenant A deletes the archive via `DELETE /api/v1/archives/tenant-a-code`.

**Action**: After deletion, inspect `/tmp/tenant-a-source/` and `{FILE_STORAGE_PATH}/derived/{archive_id}/`.

**Expected**:
- `/tmp/tenant-a-source/` exists, contains the same 5 files, contents unchanged, mtimes unchanged.
- `{FILE_STORAGE_PATH}/derived/{archive_id}/` does NOT exist (companion artifacts cleaned up).
- PG schema `tenant_a` is dropped.
- `archive_registry` row for `tenant-a-code` is gone.

**Verification**:
```bash
# Source unchanged
test "$(ls /tmp/tenant-a-source/ | wc -l)" = "5"
test "$(sha256sum /tmp/tenant-a-source/*.txt | sort)" = "${ORIGINAL_HASHES}"

# Derived gone
test ! -d "${FILE_STORAGE_PATH}/derived/${ARCHIVE_ID}"

# Schema gone
psql -c "SELECT count(*) FROM information_schema.schemata WHERE schema_name = 'tenant_a'"  # = 0

# Registry gone
psql -c "SELECT count(*) FROM archive_registry WHERE name = 'tenant-a-code'"  # = 0
```

**References**: Synthesis §2.3 constraint 6 (`drop_archive_schema` safe for Referenced — orphan deletion gated on `storage_backend='filesystem'`); §3 Decision 3 (derived artifacts in companion location); §5 R-2 / R-7; WS-6.

---

## TI-EXTSTORAGE-10: MCP Cross-Session Protection

**Threat**: Tenant B's MCP session calls the `rescan_archive` tool with `archive_name='tenant-a-code'`, triggering a rescan against Tenant A's source dir. This would let Tenant B force expensive work against another tenant, or worse — discover Tenant A's archive contents via scan-status responses.

**Setup**:
- Tenant A has Referenced archive `tenant-a-code` in production.
- Tenant B's MCP server session is authenticated under Tenant B's OAuth client.
- Tenant B's MCP session calls the new `rescan_archive` tool (per synthesis §3 Decision 6 / WS-8).

**Action**:
```javascript
// In Tenant B's MCP session
mcpClient.callTool('rescan_archive', { archive_name: 'tenant-a-code' });
```

**Expected**:
- Tool call returns an error: `archive not found` or `access denied`.
- No scan job is enqueued under Tenant A's schema.
- No `archive_registry` row for `tenant-a-code` is reachable from Tenant B's authenticated context.

**Verification**:
```bash
# Job count before
JOB_COUNT_BEFORE=$(psql -c "SELECT count(*) FROM jobs WHERE archive_name = 'tenant-a-code' AND job_type = 'DirectoryScan'" | grep -oP '\d+')

# MCP call returns error
test "$MCP_RESULT_STATUS" = "error"
grep -qE "not found|access denied|unauthorized" "$MCP_RESULT_BODY"

# Job count after — unchanged
JOB_COUNT_AFTER=$(psql -c "SELECT count(*) FROM jobs WHERE archive_name = 'tenant-a-code' AND job_type = 'DirectoryScan'" | grep -oP '\d+')
test "$JOB_COUNT_AFTER" = "$JOB_COUNT_BEFORE"
```

**References**: Synthesis §3 Decision 6 (MCP surface extends existing — `rescan_archive` tool); §2.3 constraint 3 (MCP server is Node.js, MCP tool changes go in `mcp-server/`); WS-8 backward-compat + auth requirements.

---

## Suite Acceptance Criteria

All 10 TI-EXTSTORAGE-* tests must pass before WS-9 is considered complete. Per `.claude/rules/anti-laziness.md` Rule 1 and Rule 8: no test may be deleted, skipped (`#[ignore]`), or made conditional. If a test cannot pass, the underlying defect must be fixed.

If a TI-* test is determined to be testing an out-of-scope scenario (e.g., a synthesis decision changes during construction), the test is replaced with the correct equivalent — not removed. The suite count stays at 10.

## References

- @.aiwg/working/issue-planner-storage/synthesis.md — Decisions 1, 2, 3, 5, 7, 8; Risk register R-1, R-2, R-5, R-7; Workstreams 1-9
- @.aiwg/working/issue-planner-storage/testing/test-strategy.md — pyramid + CI integration
- @CLAUDE.md — Multi-Memory Architecture, Authentication, PostgreSQL Migration Compatibility
- @.claude/rules/anti-laziness.md — never skip/delete security tests
- ADR-090 — per-archive PostgreSQL schema isolation (referenced for the tenant-isolation pattern)
- ADR-094 — fail-closed auth (referenced for multi-tenant test setup)
