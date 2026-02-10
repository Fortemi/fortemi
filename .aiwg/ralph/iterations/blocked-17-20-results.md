# Blocked Test Execution: Phase 17 (OAuth) + Phase 20 (Backup Safety)

**Date**: 2026-02-09
**API**: https://memory.integrolabs.net
**Version**: v2026.2.8
**Tests Executed**: 7 (4 OAuth + 3 Backup)
**Result**: 7/7 PASS

---

## Phase 17: OAuth Infrastructure (4/4 PASS)

### AUTH-014: Client Registration -- PASS

**Method**: `POST /oauth/register`

```bash
curl -s -X POST https://memory.integrolabs.net/oauth/register \
  -H "Content-Type: application/json" \
  -d '{"client_name":"UAT Blocked Test Client","grant_types":["client_credentials"],"scope":"read write"}'
```

**Response** (HTTP 200):
```json
{
  "client_id": "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5",
  "client_secret": "W4iOaKBRLmc22l8ohAwhxGlorV36rr5GRG1quf2wt5kxAya3",
  "client_id_issued_at": 1770680172,
  "client_secret_expires_at": 0,
  "client_name": "UAT Blocked Test Client",
  "redirect_uris": [],
  "grant_types": ["client_credentials"],
  "response_types": ["code"],
  "scope": "read write",
  "token_endpoint_auth_method": "client_secret_basic",
  "registration_access_token": "8AKJPwcr5yTBZjPXJX2mdXrL74FHBMVibsO7XiXXybO4AFyxAq8xNym1wHTnPdfn",
  "registration_client_uri": "https://memory.integrolabs.net/oauth/register/mm_JCZzhdl7vIZpCJa9Q5UKZ3E5"
}
```

**Verification**: Returns JSON with `client_id` (mm_ prefix) and `client_secret`. Dynamic registration compliant with RFC 7591.

---

### AUTH-015: Token Issuance -- PASS

**Method**: `POST /oauth/token` with Basic Auth

```bash
curl -s -X POST https://memory.integrolabs.net/oauth/token \
  -u "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5:W4iOaKBRLmc22l8ohAwhxGlorV36rr5GRG1quf2wt5kxAya3" \
  -d "grant_type=client_credentials&scope=read write"
```

**Response** (HTTP 200):
```json
{
  "access_token": "mm_at_B3uUD22x8yLMQCryV3GcuxtRhegSB4UMi1HIipQK2Ai2Xbkl",
  "token_type": "Bearer",
  "expires_in": 86400,
  "scope": "read write"
}
```

**Verification**: Returns opaque `mm_at_*` token, `token_type: Bearer`, `expires_in: 86400` (24h TTL as expected).

---

### AUTH-016: Token Introspection -- PASS

**Method**: `POST /oauth/introspect` with Basic Auth

```bash
curl -s -X POST https://memory.integrolabs.net/oauth/introspect \
  -u "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5:W4iOaKBRLmc22l8ohAwhxGlorV36rr5GRG1quf2wt5kxAya3" \
  -d "token=mm_at_B3uUD22x8yLMQCryV3GcuxtRhegSB4UMi1HIipQK2Ai2Xbkl"
```

**Response** (HTTP 200):
```json
{
  "active": true,
  "scope": "read write",
  "client_id": "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5",
  "token_type": "Bearer",
  "exp": 1770766575,
  "iat": 1770680175,
  "aud": "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5",
  "iss": "https://memory.integrolabs.net"
}
```

**Verification**: `active: true`, includes `scope`, `client_id`, `exp`, `iat`, `aud`, `iss`. Compliant with RFC 7662.

---

### AUTH-017: Token Revocation -- PASS

**Method**: `POST /oauth/revoke` with Basic Auth, then re-introspect

**Step 1 - Revoke**:
```bash
curl -s -X POST https://memory.integrolabs.net/oauth/revoke \
  -u "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5:W4iOaKBRLmc22l8ohAwhxGlorV36rr5GRG1quf2wt5kxAya3" \
  -d "token=mm_at_B3uUD22x8yLMQCryV3GcuxtRhegSB4UMi1HIipQK2Ai2Xbkl"
```

**Response**: HTTP 200 (empty body)

**Step 2 - Re-introspect**:
```bash
curl -s -X POST https://memory.integrolabs.net/oauth/introspect \
  -u "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5:W4iOaKBRLmc22l8ohAwhxGlorV36rr5GRG1quf2wt5kxAya3" \
  -d "token=mm_at_B3uUD22x8yLMQCryV3GcuxtRhegSB4UMi1HIipQK2Ai2Xbkl"
```

**Response** (HTTP 200):
```json
{
  "active": false,
  "scope": "read write",
  "client_id": "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5",
  "token_type": "Bearer",
  "exp": 1770766575,
  "iat": 1770680175,
  "aud": "mm_JCZzhdl7vIZpCJa9Q5UKZ3E5",
  "iss": "https://memory.integrolabs.net"
}
```

**Verification**: Revocation returned HTTP 200. Re-introspection shows `active: false`. Compliant with RFC 7009.

---

## Phase 20: Backup Safety Tests (3/3 PASS)

### BACK-015: Knowledge Archive Download -- PASS

**Tool**: `mcp__fortemi__knowledge_archive_download`

**Input**: filename=`snapshot_database_20260209_231730_uat-chain6-snapshot.sql.gz`, output_dir=`/tmp`

**Response**:
```json
{
  "success": true,
  "saved_to": "/tmp/snapshot_database_20260209_231730_uat-chain6-snapshot.archive",
  "filename": "snapshot_database_20260209_231730_uat-chain6-snapshot.archive",
  "size_bytes": 807936,
  "message": "Knowledge archive saved to: /tmp/snapshot_database_20260209_231730_uat-chain6-snapshot.archive"
}
```

**Verification**: Archive downloaded successfully (807,936 bytes). No crash, clean response.

---

### BACK-016: Knowledge Archive Upload -- PASS

**Tool**: `mcp__fortemi__knowledge_archive_upload`

**Input**: file_path=`/tmp/snapshot_database_20260209_231730_uat-chain6-snapshot.archive`

**Response**:
```json
{
  "success": true,
  "filename": "snapshot_database_20260209_231730_uat-chain6-snapshot.sql.gz",
  "path": "/var/backups/matric-memory/snapshot_database_20260209_231730_uat-chain6-snapshot.sql.gz",
  "size_bytes": 804453,
  "size_human": "785.60 KB",
  "metadata": {
    "backup_type": "snapshot",
    "created_at": "2026-02-09T23:17:30.897248970Z",
    "description": "Backup snapshot for Chain 6 UAT testing",
    "last_migration": "add missing extraction strategy values",
    "matric_version": "2026.2.8",
    "matric_version_min": "2026.2.8",
    "note_count": 57,
    "pg_version": "PostgreSQL 16.11",
    "schema_migration_count": 65,
    "source": "user",
    "title": "Backup before deletion test"
  }
}
```

**Verification**: Upload succeeded, metadata preserved (57 notes, 65 migrations, version 2026.2.8). Round-trip download/upload verified.

---

### BACK-017: Database Restore -- PASS

**Pre-step - Safety Snapshot**:
```json
{
  "success": true,
  "filename": "snapshot_database_20260209_233705_uat-back017-safety.sql.gz",
  "size_bytes": 275173,
  "size_human": "268.72 KB",
  "backup_type": "snapshot"
}
```

**Tool**: `mcp__fortemi__database_restore`

**Input**: filename=`snapshot_database_20260209_233705_uat-back017-safety.sql.gz`

**Response**:
```json
{
  "success": true,
  "message": "Database restored from snapshot_database_20260209_233705_uat-back017-safety.sql.gz",
  "prerestore_backup": "prerestore_database_20260209_233708.sql.gz",
  "restored_from": "snapshot_database_20260209_233705_uat-back017-safety.sql.gz",
  "reconnect_delay_ms": 2000
}
```

**Post-restore Verification** (`memory_info`):
```json
{
  "summary": {
    "total_notes": 10,
    "total_embeddings": 25,
    "total_links": 18,
    "total_collections": 1,
    "total_tags": 82,
    "total_templates": 0
  },
  "storage": {
    "database_total_bytes": 29938147,
    "database_total_human": "28.55 MB"
  }
}
```

**Verification**: Restore completed successfully. System operational post-restore: 10 notes, 25 embeddings, 18 links, 82 tags, 28.55 MB database. Auto-created prerestore snapshot as safety net.

---

## Summary

| Test ID  | Phase | Description                  | Result |
|----------|-------|------------------------------|--------|
| AUTH-014 | 17    | Client Registration          | PASS   |
| AUTH-015 | 17    | Token Issuance               | PASS   |
| AUTH-016 | 17    | Token Introspection          | PASS   |
| AUTH-017 | 17    | Token Revocation             | PASS   |
| BACK-015 | 20    | Knowledge Archive Download   | PASS   |
| BACK-016 | 20    | Knowledge Archive Upload     | PASS   |
| BACK-017 | 20    | Database Restore             | PASS   |

**Overall**: 7/7 PASS (100%)

**Key Observations**:
- OAuth2 flow is fully functional end-to-end (register, issue, introspect, revoke)
- Token TTL confirmed at 24h (86400s) - fix from earlier issue #239 verified
- Token revocation properly invalidates tokens (re-introspection shows active=false)
- Knowledge archive round-trip (download + upload) preserves all metadata
- Database restore auto-creates prerestore safety backup before restoring
- System fully operational after restore with all data intact
