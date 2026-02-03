# UAT Phase 0: Pre-flight Checks

**Purpose**: Verify system readiness before testing
**Duration**: ~2 minutes
**Prerequisites**: MCP connection active

---

## Test Cases

### PF-001: System Health Check

**Tool**: `memory_info()`

**Expected Response**:
```json
{
  "summary": { ... },
  "storage": { ... }
}
```

**Pass Criteria**: Response contains `summary` and `storage` objects

**Failure Actions**:
- Check API is running: `curl http://localhost:3000/health`
- Verify database connection
- Check Ollama availability

---

### PF-002: Backup System Status

**Tool**: `backup_status()`

**Expected Response**:
```json
{
  "status": "ok",
  ...
}
```

**Pass Criteria**: Response contains `status` field

**Failure Actions**:
- Check backup directory permissions
- Verify disk space available

---

### PF-003: Embedding Pipeline Status

**Tool**: `list_embedding_sets()`

**Expected Response**:
```json
{
  "sets": [
    { "slug": "default", ... }
  ]
}
```

**Pass Criteria**: Response contains set with `slug: "default"`

**Failure Actions**:
- Run database migrations
- Check embedding configuration

---

## Phase Summary

| Test ID | Name | Status |
|---------|------|--------|
| PF-001 | System Health Check | |
| PF-002 | Backup System Status | |
| PF-003 | Embedding Pipeline Status | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
