# UAT Phase 2G: 3D Model Processing

**Purpose**: Verify 3D model processing guidance tool and attachment pipeline extraction for 3D model files
**Duration**: ~10 minutes
**Prerequisites**: Phase 0 preflight passed, Phase 2b attachment uploads working
**Critical**: No (requires optional Blender headless + vision backend)
**Tools Tested**: `process_3d_model`, `get_system_info`, `create_note`, `upload_attachment`, `get_attachment`, `list_jobs`

> **Attachment Pipeline**: 3D model files are processed through the standard attachment pipeline — NOT via base64 ad-hoc API. The `process_3d_model` MCP tool is a **guidance tool** that returns workflow instructions for agents. Actual processing happens when a 3D model file is uploaded as an attachment and the background job worker renders and describes it.

> **Backend Requirements**: Full extraction requires:
> - **Blender** headless in PATH (for multi-view rendering)
> - **OLLAMA_VISION_MODEL** set (for view description — required for meaningful extraction)
>
> If Blender is not available, MDL-002 detects this and attachment pipeline tests (MDL-004+) are marked SKIPPED. Guidance tool tests (MDL-001, MDL-003) always execute.

> **Test Data**: This phase uses 3D model files. Provide at least one GLB file (<5MB) for attachment pipeline testing. Khronos glTF sample models work well (e.g., Box.glb, Duck.glb).

---

## Tests

### MDL-001: Check 3D Model Extraction Backend Availability

**MCP Tool**: `get_system_info`
**Parameters**: `{}`

**Pass Criteria**:
- Response includes `extraction.3d_model` object
- Record `extraction.3d_model.enabled` value
- Record `extraction.3d_model.blender_available` (boolean)
- If `blender_available === true` AND vision model is available: continue to MDL-004
- If `blender_available === false`: mark MDL-004 through MDL-010 as SKIPPED (guidance tests MDL-002, MDL-003 still execute)

**Notes**: Gate test for attachment pipeline tests. Guidance tool tests always run regardless.

---

### MDL-002: Guidance Tool — No Note ID

**MCP Tool**: `process_3d_model`
**Parameters**:
```javascript
process_3d_model({
  filename: "sculpture.glb"
})
```

**Pass Criteria**:
- Response contains `workflow` field with value `"attachment_pipeline"`
- Response contains `message` (non-empty string mentioning "attachment pipeline")
- Response contains `steps` (array with 5 entries — includes note creation step)
- Response contains `supported_formats` (array including `"model/gltf-binary"`)
- Response contains `requires` object with `blender` and `vision_model` keys
- Response contains `extraction_features` object with `multi_view_rendering` key
- Step 1 mentions `create_note`
- Step 2 mentions `upload_attachment`

**Notes**: Verifies the guidance tool returns proper workflow instructions when no note ID is provided.

---

### MDL-003: Guidance Tool — With Note ID

**MCP Tool**: `process_3d_model`
**Parameters**:
```javascript
process_3d_model({
  note_id: "00000000-0000-0000-0000-000000000000",
  filename: "architectural-model.obj"
})
```

**Pass Criteria**:
- Response contains `workflow` field with value `"attachment_pipeline"`
- Response contains `steps` (array with 4 entries — no note creation step)
- Step 1 mentions `upload_attachment` with the provided note_id
- Response contains `supported_formats` (array including `"model/obj"`)

**Notes**: Verifies the guidance tool adapts instructions when an existing note ID is provided.

---

### MDL-004: Create Note for 3D Model Upload

**MCP Tool**: `create_note`
**Parameters**:
```javascript
create_note({
  title: "UAT 3D Model: Test Object",
  body: "3D model uploaded for extraction pipeline testing"
})
```

**Pass Criteria**:
- Response contains `id` (UUID string)
- Response contains `title` matching input
- Save `note_id` for MDL-005

**Notes**: Creates the parent note for 3D model attachment. Required by attachment pipeline.

---

### MDL-005: Upload 3D Model Attachment

**MCP Tool**: `upload_attachment`
**Parameters**:
```javascript
upload_attachment({
  note_id: "<note_id from MDL-004>",
  filename: "test-model.glb",
  content_type: "model/gltf-binary"
})
```

Then execute the returned curl command with an actual GLB file.

**Pass Criteria**:
- `upload_attachment` returns a curl command template
- Executing the curl command returns 200/201 with attachment metadata
- Response contains `id` (attachment UUID)
- Response contains `extraction_strategy` — should be `"glb_3d_model"` or equivalent
- Save `attachment_id` for MDL-006

**Notes**: Binary upload via curl (approved exception per MCP-First policy). The upload triggers background extraction.

---

### MDL-006: Check Extraction Job Created

**MCP Tool**: `list_jobs`
**Parameters**:
```javascript
list_jobs({
  limit: 5,
  status: "pending"
})
```

**Pass Criteria**:
- Response contains at least one job related to the uploaded attachment
- OR: If extraction is fast, job may already be `completed` — check with `status: "completed"` too

**Notes**: Verifies the attachment upload triggered a background extraction job.

---

### MDL-007: Wait and Check Extraction Results

**MCP Tool**: `get_attachment`
**Parameters**:
```javascript
get_attachment({
  id: "<attachment_id from MDL-005>"
})
```

**Pass Criteria**:
- Response contains attachment metadata
- `extraction_strategy` is `"glb_3d_model"` or similar
- If extraction completed: `extraction_metadata` contains extracted content
- If extraction pending: retry after 15 seconds (max 3 retries — Blender rendering takes longer)
- Extraction metadata (when present) should contain:
  - `ai_description` (composite description synthesized from multiple views)
  - `metadata` with `num_views`, `model` (vision model name), `filename`

**Notes**: Multi-view rendering with Blender can take 60-120 seconds depending on model complexity and number of views. Each view is rendered then described by the vision model.

---

### MDL-008: 3D Model Content Searchable After Extraction

**MCP Tool**: `search_notes`
**Parameters**:
```javascript
search_notes({
  query: "UAT 3D Model Test Object",
  limit: 5
})
```

**Pass Criteria**:
- Results include the note created in MDL-004
- Note appears in search results (title match at minimum)

**Notes**: After extraction completes, 3D model descriptions should be indexed and searchable.

---

### MDL-009: Guidance for OBJ Format

**MCP Tool**: `process_3d_model`
**Parameters**:
```javascript
process_3d_model({
  filename: "scene.obj"
})
```

**Pass Criteria**:
- Response contains workflow instructions
- `supported_formats` includes `"model/obj"`
- Steps reference the provided filename

**Notes**: Verifies guidance covers OBJ format (common non-GLB 3D format).

---

### MDL-010: Guidance for STL Format

**MCP Tool**: `process_3d_model`
**Parameters**:
```javascript
process_3d_model({
  filename: "part.stl"
})
```

**Pass Criteria**:
- Response contains workflow instructions
- `supported_formats` includes `"model/stl"`

**Notes**: Verifies guidance covers STL format (common in 3D printing).

---

## Phase Summary

| Test ID | Test Name | Status | Notes |
|---------|-----------|--------|-------|
| MDL-001 | Check 3D Model Extraction Backend | | Gate test for pipeline tests |
| MDL-002 | Guidance Tool — No Note ID | | Always executes |
| MDL-003 | Guidance Tool — With Note ID | | Always executes |
| MDL-004 | Create Note for 3D Model Upload | | Requires Blender + vision |
| MDL-005 | Upload 3D Model Attachment | | Requires Blender + test GLB |
| MDL-006 | Check Extraction Job Created | | Requires Blender |
| MDL-007 | Wait and Check Extraction Results | | Requires Blender + vision |
| MDL-008 | 3D Model Content Searchable | | After extraction |
| MDL-009 | Guidance for OBJ Format | | Always executes |
| MDL-010 | Guidance for STL Format | | Always executes |

**Total Tests**: 10
**Always-Execute Tests**: 5 (MDL-001, MDL-002, MDL-003, MDL-009, MDL-010)
**Conditional Tests**: 5 (MDL-004 through MDL-008, require Blender + vision)
