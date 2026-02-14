# Phase 2G: 3D Model Processing — Results

**Date**: 2026-02-13
**Version**: v2026.2.8
**Result**: 10 tests — 9 PASS, 1 FAIL (90%)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| MDL-001 | Check 3D Model Backend Availability | PASS | extraction.3d_model.enabled: true, renderer: three.js, vision: qwen3-vl:8b |
| MDL-002 | Guidance Tool Without Note ID | PASS | Returns 5-step workflow guide, supported_formats includes model/gltf-binary |
| MDL-003 | Guidance Tool With Note ID | PASS | Returns 4-step workflow guide referencing note, supported_formats includes model/obj |
| MDL-004 | Create Note for 3D Model | PASS | note_id: 019c5a3f-3385-7632-ad2d-201f6c31f35a |
| MDL-005 | Upload GLB via Curl Command | PASS | attachment_id: 019c5a3f-cee3-7650-b27d-b7f48232598f, extraction_strategy: glb3_d_model |
| MDL-006 | Extraction Job Created | PASS | job_id: 019c5a3f-cee6-7440-9386-4a17f2faa59f, status: failed (renderer unreachable) |
| MDL-007 | Extraction Job Completes with Multi-View | FAIL | Three.js renderer at localhost:8080 unreachable — **#355 filed** |
| MDL-008 | 3D Model Content Searchable | PASS | Note appears as top result for "UAT 3D Model Duck" (score: 0.5) |
| MDL-009 | Guidance for OBJ Format | PASS | supported_formats includes "model/obj" |
| MDL-010 | Guidance for STL Format | PASS | supported_formats includes "model/stl" |

## 3D Model Extraction Pipeline Details

### Backend Configuration
- **Renderer**: Three.js (expected at localhost:8080 or RENDERER_URL)
- **Vision Model**: qwen3-vl:8b
- **Extraction Strategy**: glb3_d_model

### Upload Response
- **attachment_id**: 019c5a3f-cee3-7650-b27d-b7f48232598f
- **blob_id**: 019c5a3f-cedf-7302-86d5-8d1a1a7f8a09
- **extraction_strategy**: glb3_d_model

### Extraction Job Result
```json
{
  "job_id": "019c5a3f-cee6-7440-9386-4a17f2faa59f",
  "status": "failed",
  "error": "Extraction failed: Internal error: Failed to call renderer: error sending request for url (http://localhost:8080/render): error trying to connect: tcp connect error: Connection refused (os error 111)"
}
```

### Infrastructure Issue
The Three.js renderer container/service is not running or not accessible from the API server's network. The MCP tool layer and extraction job creation work correctly, but the actual rendering cannot be performed.

**Root Cause**: Three.js renderer service at `http://localhost:8080` not deployed/started on production server.

**MCP Layer**: Working correctly ✓
**Job Creation**: Working correctly ✓
**Renderer**: Not reachable ✗

### Search Verification
Query: "UAT 3D Model Duck" returned note 019c5a3f-3385-7632-ad2d-201f6c31f35a as top result (based on note title/body, not extraction metadata).

## Stored IDs
- model_test_note_id: 019c5a3f-3385-7632-ad2d-201f6c31f35a
- model_attachment_id: 019c5a3f-cee3-7650-b27d-b7f48232598f
- model_job_id: 019c5a3f-cee6-7440-9386-4a17f2faa59f

## Supported Formats (from guidance tool)
- model/gltf-binary (GLB)
- model/gltf+json (GLTF)
- model/obj (OBJ)
- model/fbx (FBX)
- model/stl (STL)
- model/ply (PLY)
- model/step (STEP)
- model/iges (IGES)
- model/vnd.usdz+zip (USDZ)

## Issues Filed
- **#355**: Three.js renderer service (localhost:8080) not reachable from API server — extraction jobs fail with "Connection refused"
