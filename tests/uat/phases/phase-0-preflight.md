# UAT Phase 0: Pre-flight Checks

**Purpose**: Verify system readiness before testing
**Duration**: ~2 minutes
**Prerequisites**: MCP connection active
**Tools Tested**: `memory_info`, `backup_status`, `list_embedding_sets`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Test Cases

### PF-001: System Health Check

**MCP Tool**: `memory_info`

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
- Verify MCP server is running and reachable
- Verify database connection
- Check Ollama availability

---

### PF-002: Backup System Status

**MCP Tool**: `backup_status`

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

**MCP Tool**: `list_embedding_sets`

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

### PF-004: Test Data Availability

**MCP Tool**: N/A (filesystem check)

**Description**: Verify that UAT test data has been generated and required files exist before proceeding with testing.

**Steps**:
```bash
# Verify test data directory exists and contains expected files
ls tests/uat/data/images/jpeg-with-exif.jpg
ls tests/uat/data/documents/code-python.py
ls tests/uat/data/multilingual/english.txt
ls tests/uat/data/provenance/paris-eiffel-tower.jpg
ls tests/uat/data/edge-cases/empty.txt
ls tests/uat/data/audio/english-speech-5s.mp3

# Count total files (expect 50+)
find tests/uat/data/ -type f -not -path '*/scripts/*' | wc -l
```

**Pass Criteria**:
- All 6 key files exist
- Total data file count >= 44
- If files missing, run: `cd tests/uat/data/scripts && ./generate-test-data.sh`

**Failure Actions**:
- Run the generation script: `cd tests/uat/data/scripts && ./generate-test-data.sh`
- Verify Python dependencies: `pip install Pillow piexif faker`
- Check that ImageMagick and exiftool are installed

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| PF-001 | System Health Check | `memory_info` | |
| PF-002 | Backup System Status | `backup_status` | |
| PF-003 | Embedding Pipeline Status | `list_embedding_sets` | |
| PF-004 | Test Data Availability | N/A (filesystem) | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
