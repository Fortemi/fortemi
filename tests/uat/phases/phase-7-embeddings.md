# UAT Phase 7: Embeddings

**Purpose**: Verify embedding sets and embedding configuration
**Duration**: ~5 minutes
**Prerequisites**: Phase 1 seed data exists
**Tools Tested**: `list_embedding_sets`, `get_embedding_set`, `create_embedding_set`, `add_set_members`, `list_set_members`, `remove_set_member`, `search_notes`, `refresh_embedding_set`, `update_embedding_set`, `delete_embedding_set`, `reembed_all`, `list_embedding_configs`, `get_default_embedding_config`, `get_embedding_config`, `create_embedding_config`, `update_embedding_config`, `delete_embedding_config`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. The MCP tool name and exact parameters are specified for each test.

---

## Embedding Sets

### EMB-001: List Embedding Sets

**MCP Tool**: `list_embedding_sets`

```javascript
list_embedding_sets()
```

**Pass Criteria**: Returns array with default set having `slug: "default"`

---

### EMB-002: Get Default Set

**MCP Tool**: `get_embedding_set`

```javascript
get_embedding_set({ slug: "default" })
```

**Pass Criteria**: Returns full set details including `embedding_config_id`

---

### EMB-003: Create Embedding Set

**MCP Tool**: `create_embedding_set`

```javascript
create_embedding_set({
  slug: "uat-test-set",
  name: "UAT Test Set",
  description: "Embedding set for UAT testing"
})
```

**Pass Criteria**: Returns created set with ID

---

### EMB-004: Add Members to Set

**MCP Tool**: `add_set_members`

```javascript
add_set_members({
  slug: "uat-test-set",
  note_ids: ["<ml_note_1_id>", "<ml_note_2_id>"]
})
```

**Pass Criteria**: Success response

---

### EMB-005: List Set Members

**MCP Tool**: `list_set_members`

```javascript
list_set_members({ slug: "uat-test-set" })
```

**Pass Criteria**: Returns the added note IDs

---

### EMB-006: Remove Set Member

**MCP Tool**: `remove_set_member`

```javascript
remove_set_member({
  slug: "uat-test-set",
  note_id: "<ml_note_1_id>"
})
```

**Pass Criteria**: Note removed from set

---

### EMB-007: Search Within Set

**MCP Tool**: `search_notes`

```javascript
search_notes({
  query: "neural",
  mode: "hybrid",
  set: "uat-test-set",
  limit: 10
})
```

**Pass Criteria**: Results only from notes in the embedding set

---

### EMB-008: Refresh Embedding Set

**MCP Tool**: `refresh_embedding_set`

```javascript
refresh_embedding_set({ slug: "uat-test-set" })
```

**Pass Criteria**: Returns job ID for re-embedding

---

## Embedding Configs

### EMB-009: List Embedding Configs

**MCP Tool**: `list_embedding_configs`

```javascript
list_embedding_configs()
```

**Pass Criteria**: Returns array of available configs

---

### EMB-010: Get Default Embedding Config

**MCP Tool**: `get_default_embedding_config`

```javascript
get_default_embedding_config()
```

**Pass Criteria**: Returns config with model name, dimensions, etc.

---

### EMB-011: Index Status

**MCP Tool**: `list_embedding_sets`

```javascript
list_embedding_sets()
```

**Pass Criteria**: Each set has valid `index_status` enum value

**Valid Values**: `pending`, `indexing`, `ready`, `stale`, `error`

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| EMB-001 | List Embedding Sets | `list_embedding_sets` | |
| EMB-002 | Get Default Set | `get_embedding_set` | |
| EMB-003 | Create Embedding Set | `create_embedding_set` | |
| EMB-004 | Add Members | `add_set_members` | |
| EMB-005 | List Members | `list_set_members` | |
| EMB-006 | Remove Member | `remove_set_member` | |
| EMB-007 | Search Within Set | `search_notes` | |
| EMB-008 | Refresh Set | `refresh_embedding_set` | |
| EMB-009 | List Configs | `list_embedding_configs` | |
| EMB-010 | Get Default Config | `get_default_embedding_config` | |
| EMB-011 | Index Status | `list_embedding_sets` | |

---

### EMB-012: Update Embedding Set

**MCP Tool**: `update_embedding_set`

```javascript
update_embedding_set({
  slug: "uat-test-set",
  name: "UAT Test Set Updated",
  description: "Updated description",
  keywords: ["uat", "testing", "updated"]
})
```

**Pass Criteria**: Set metadata updated successfully

---

### EMB-013: Delete Embedding Set

**MCP Tool**: `delete_embedding_set`

```javascript
delete_embedding_set({ slug: "uat-test-set" })
```

**Pass Criteria**: Set deleted (cannot delete "default")

**Verify**: `list_embedding_sets()` no longer includes "uat-test-set"

---

### EMB-014: Re-embed All Notes

**MCP Tool**: `reembed_all`

```javascript
reembed_all({
  force: false  // Only notes without embeddings
})
```

**Pass Criteria**: Batch re-embedding job queued

---

### EMB-015: Re-embed Specific Set

**MCP Tool**: `reembed_all`

```javascript
reembed_all({
  embedding_set_slug: "default",
  force: true  // Re-embed all notes in set
})
```

**Pass Criteria**: Set-specific re-embedding job queued

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| EMB-001 | List Embedding Sets | `list_embedding_sets` | |
| EMB-002 | Get Default Set | `get_embedding_set` | |
| EMB-003 | Create Embedding Set | `create_embedding_set` | |
| EMB-004 | Add Members | `add_set_members` | |
| EMB-005 | List Members | `list_set_members` | |
| EMB-006 | Remove Member | `remove_set_member` | |
| EMB-007 | Search Within Set | `search_notes` | |
| EMB-008 | Refresh Set | `refresh_embedding_set` | |
| EMB-009 | List Configs | `list_embedding_configs` | |
| EMB-010 | Get Default Config | `get_default_embedding_config` | |
| EMB-011 | Index Status | `list_embedding_sets` | |
| EMB-012 | Update Embedding Set | `update_embedding_set` | |
| EMB-013 | Delete Embedding Set | `delete_embedding_set` | |
| EMB-014 | Re-embed All Notes | `reembed_all` | |
| EMB-015 | Re-embed Specific Set | `reembed_all` | |

---

## Embedding Config Management

### EMB-016: Get Embedding Config by ID

**MCP Tool**: `get_embedding_config`

```javascript
get_embedding_config({ id: "<config_id>" })
```

**Expected Response**:
```json
{
  "id": "<uuid>",
  "name": "Default Config",
  "model_name": "nomic-embed-text",
  "dimensions": 768,
  "max_tokens": 8192,
  "is_default": true,
  "created_at": "<timestamp>"
}
```

**Pass Criteria**: Returns full config details

---

### EMB-017: Create Embedding Config

**MCP Tool**: `create_embedding_config`

```javascript
create_embedding_config({
  name: "UAT Test Config",
  model_name: "nomic-embed-text",
  dimensions: 768,
  max_tokens: 8192
})
```

**Expected Response**:
```json
{
  "id": "<uuid>",
  "name": "UAT Test Config",
  "is_default": false
}
```

**Pass Criteria**: New config created (not set as default)

**Store**: `test_config_id`

---

### EMB-018: Update Embedding Config

**MCP Tool**: `update_embedding_config`

```javascript
update_embedding_config({
  id: test_config_id,
  name: "UAT Test Config Updated",
  max_tokens: 4096
})
```

**Pass Criteria**: Config updated successfully

---

### EMB-019: Delete Non-Default Config

**MCP Tool**: `delete_embedding_config`

```javascript
// Cannot delete default config
delete_embedding_config({ id: test_config_id })
```

**Pass Criteria**: Test config deleted

**Verify**: `list_embedding_configs()` no longer includes test config

---

### EMB-020: Cannot Delete Default Config

**Isolation**: Required — negative test expects error response

**MCP Tool**: `get_default_embedding_config`, `delete_embedding_config`

```javascript
const defaultConfig = get_default_embedding_config()
delete_embedding_config({ id: defaultConfig.id })
```

**Pass Criteria**: Error - cannot delete default config

---

## Final Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| EMB-001 | List Embedding Sets | `list_embedding_sets` | |
| EMB-002 | Get Default Set | `get_embedding_set` | |
| EMB-003 | Create Embedding Set | `create_embedding_set` | |
| EMB-004 | Add Members | `add_set_members` | |
| EMB-005 | List Members | `list_set_members` | |
| EMB-006 | Remove Member | `remove_set_member` | |
| EMB-007 | Search Within Set | `search_notes` | |
| EMB-008 | Refresh Set | `refresh_embedding_set` | |
| EMB-009 | List Configs | `list_embedding_configs` | |
| EMB-010 | Get Default Config | `get_default_embedding_config` | |
| EMB-011 | Index Status | `list_embedding_sets` | |
| EMB-012 | Update Embedding Set | `update_embedding_set` | |
| EMB-013 | Delete Embedding Set | `delete_embedding_set` | |
| EMB-014 | Re-embed All Notes | `reembed_all` | |
| EMB-015 | Re-embed Specific Set | `reembed_all` | |
| EMB-016 | Get Config by ID | `get_embedding_config` | |
| EMB-017 | Create Config | `create_embedding_config` | |
| EMB-018 | Update Config | `update_embedding_config` | |
| EMB-019 | Delete Non-Default Config | `delete_embedding_config` | |
| EMB-020 | Cannot Delete Default | `get_default_embedding_config`, `delete_embedding_config` | |

**MCP Tools Covered**: `list_embedding_sets`, `get_embedding_set`, `create_embedding_set`, `list_set_members`, `add_set_members`, `remove_set_member`, `refresh_embedding_set`, `update_embedding_set`, `delete_embedding_set`, `reembed_all`, `list_embedding_configs`, `get_default_embedding_config`, `get_embedding_config`, `create_embedding_config`, `update_embedding_config`

**Phase Result**: [ ] PASS / [ ] FAIL

**Notes**:
