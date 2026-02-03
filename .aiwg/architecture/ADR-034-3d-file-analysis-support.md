# ADR-034: 3D File Analysis and AI Understanding Support

**Status:** Implemented
**Date:** 2026-02-02
**Deciders:** Architecture team
**Related:** ADR-031 (Intelligent Attachment Processing), ADR-033 (File Storage Architecture), Epic #430

## Context

3D files represent a significant content category for knowledge management systems. Users working with CAD, 3D design, game development, AR/VR, and digital manufacturing need to store, search, and understand 3D assets alongside traditional documents.

Current state:
- No support for 3D file formats
- No thumbnail generation for 3D models
- No geometric metadata extraction
- No semantic search for 3D content

Key challenges:
1. **Format diversity**: 50+ 3D formats with varying capabilities
2. **AI understanding**: Native 3D AI models are still research-stage
3. **Processing complexity**: 3D files require specialized libraries
4. **Search semantics**: "Find gears" should return 3D gear models

## Decision

Implement **phased 3D file support** with geometric analysis (Phase 1) and AI-enhanced understanding (Phase 2).

### 1. Supported Formats

**Phase 1 (Priority):**
| Format | Extension | Use Case | Library |
|--------|-----------|----------|---------|
| glTF/GLB | .gltf, .glb | Web, AR/VR, games | trimesh, gltf-rs |
| OBJ | .obj | Traditional interchange | trimesh |
| STL | .stl | 3D printing | trimesh |
| PLY | .ply | Point clouds, scanning | Open3D |

**Phase 2 (Extended):**
| Format | Extension | Use Case | Library |
|--------|-----------|----------|---------|
| FBX | .fbx | Animation, games | Assimp |
| STEP | .step, .stp | CAD/engineering | pythonOCC |
| 3MF | .3mf | 3D printing (advanced) | trimesh |
| USD | .usd, .usdc | Film/games production | OpenUSD |

**Phase 3 (Point Clouds):**
| Format | Extension | Use Case | Library |
|--------|-----------|----------|---------|
| LAS/LAZ | .las, .laz | LiDAR, geospatial | Open3D, laspy |
| E57 | .e57 | AEC scanning | pye57 |
| PCD | .pcd | Point Cloud Library format | Open3D |

### 2. Processing Pipeline

```
Upload 3D File
      │
      v
┌─────────────────┐
│  Format         │ ─── Magic byte detection
│  Detection      │     MIME: model/gltf-binary, etc.
└────────┬────────┘
         │
         v
┌─────────────────┐
│  Load Mesh/     │ ─── trimesh.load() or Open3D
│  Point Cloud    │
└────────┬────────┘
         │
         v
┌─────────────────┐     ┌──────────────────┐
│  Geometric      │────>│  Metadata JSON   │
│  Analysis       │     │  vertices, faces │
└────────┬────────┘     │  bounds, volume  │
         │              └──────────────────┘
         v
┌─────────────────┐     ┌──────────────────┐
│  Thumbnail      │────>│  PNG 512×512     │
│  Generation     │     │  Multi-view opt. │
└────────┬────────┘     └──────────────────┘
         │
         v (Phase 2)
┌─────────────────┐     ┌──────────────────┐
│  AI Analysis    │────>│  LLaVA Vision    │
│  (Multi-view)   │     │  Description     │
└────────┬────────┘     └──────────────────┘
         │
         v
┌─────────────────┐
│  Store & Index  │ ─── Metadata, thumbnail, description
└─────────────────┘
```

### 3. Schema Design

```sql
-- Extend extraction_strategy enum
ALTER TABLE document_type
ADD CONSTRAINT chk_extraction_strategy CHECK (
    extraction_strategy IN (
        'text_native', 'pdf_text', 'pdf_scanned', 'vision',
        'audio_transcribe', 'video_multimodal', 'code_ast',
        'office_convert', 'structured_extract',
        -- New 3D strategies
        'model_3d_mesh',      -- Mesh formats (glTF, STL, OBJ)
        'model_3d_cad',       -- CAD formats (STEP, IGES)
        'model_3d_pointcloud' -- Point cloud formats (LAS, PLY)
    )
);

-- 3D model metadata (extends file_attachment)
CREATE TABLE model_3d_metadata (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID NOT NULL REFERENCES file_attachment(id) ON DELETE CASCADE,

    -- Format info
    format TEXT NOT NULL,  -- 'gltf', 'stl', 'obj', etc.
    format_version TEXT,   -- e.g., 'glTF 2.0'

    -- Geometry metrics
    vertex_count INTEGER,
    face_count INTEGER,
    edge_count INTEGER,

    -- Bounding box (AABB)
    bounds_min DOUBLE PRECISION[3],  -- [x, y, z]
    bounds_max DOUBLE PRECISION[3],
    center DOUBLE PRECISION[3],

    -- Physical properties (if watertight)
    volume DOUBLE PRECISION,         -- cubic units
    surface_area DOUBLE PRECISION,   -- square units
    is_watertight BOOLEAN,
    is_manifold BOOLEAN,

    -- Mesh quality
    euler_number INTEGER,
    is_winding_consistent BOOLEAN,
    has_degenerate_faces BOOLEAN,

    -- Scene structure (glTF/USD)
    node_count INTEGER,
    mesh_count INTEGER,
    material_count INTEGER,
    texture_count INTEGER,
    animation_count INTEGER,
    has_rigging BOOLEAN,

    -- Point cloud specific
    point_count BIGINT,
    has_colors BOOLEAN,
    has_normals BOOLEAN,
    point_density DOUBLE PRECISION,  -- points per cubic unit

    -- Units and scale
    unit_scale TEXT,  -- 'meters', 'millimeters', 'inches'
    up_axis TEXT,     -- 'Y', 'Z'

    -- Thumbnail reference
    thumbnail_attachment_id UUID REFERENCES file_attachment(id),
    multi_view_attachment_ids UUID[],  -- For AI analysis

    -- AI-generated description (Phase 2)
    ai_description TEXT,
    ai_model TEXT,
    ai_generated_at TIMESTAMPTZ,

    -- Raw format-specific metadata
    format_metadata JSONB DEFAULT '{}',

    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_3d_attachment ON model_3d_metadata(attachment_id);
CREATE INDEX idx_3d_format ON model_3d_metadata(format);
CREATE INDEX idx_3d_vertex_count ON model_3d_metadata(vertex_count);
CREATE INDEX idx_3d_is_watertight ON model_3d_metadata(is_watertight);
```

### 4. Python Processing Script

```python
#!/usr/bin/env python3
"""3D file processing for matric-memory."""

import sys
import json
from pathlib import Path
import trimesh
import numpy as np

def process_3d_file(input_path: str, output_dir: str) -> dict:
    """Process a 3D file and extract metadata + thumbnail."""
    path = Path(input_path)
    out = Path(output_dir)

    # Load with trimesh (handles glTF, STL, OBJ, PLY, etc.)
    try:
        scene_or_mesh = trimesh.load(input_path)
    except Exception as e:
        return {"error": str(e), "format": path.suffix[1:]}

    # Handle both Scene and Mesh objects
    if isinstance(scene_or_mesh, trimesh.Scene):
        scene = scene_or_mesh
        # Get combined geometry
        mesh = scene.dump(concatenate=True) if scene.geometry else None
        is_scene = True
    else:
        mesh = scene_or_mesh
        scene = mesh.scene()
        is_scene = False

    metadata = {
        "format": path.suffix[1:].lower(),
        "is_scene": is_scene,
    }

    # Scene-level metadata (glTF, USD)
    if is_scene:
        metadata.update({
            "node_count": len(scene.graph.nodes),
            "mesh_count": len(scene.geometry) if scene.geometry else 0,
        })

    # Mesh geometry metadata
    if mesh is not None and hasattr(mesh, 'vertices'):
        metadata.update({
            "vertex_count": len(mesh.vertices),
            "face_count": len(mesh.faces) if hasattr(mesh, 'faces') else 0,
            "bounds_min": mesh.bounds[0].tolist(),
            "bounds_max": mesh.bounds[1].tolist(),
            "center": mesh.centroid.tolist(),
            "surface_area": float(mesh.area),
            "is_watertight": bool(mesh.is_watertight),
            "euler_number": int(mesh.euler_number) if hasattr(mesh, 'euler_number') else None,
            "is_winding_consistent": bool(mesh.is_winding_consistent) if hasattr(mesh, 'is_winding_consistent') else None,
        })

        # Volume only for watertight meshes
        if mesh.is_watertight:
            metadata["volume"] = float(mesh.volume)

    # Point cloud metadata
    if hasattr(mesh, 'vertices') and not hasattr(mesh, 'faces'):
        metadata.update({
            "point_count": len(mesh.vertices),
            "has_colors": hasattr(mesh, 'colors') and mesh.colors is not None,
            "has_normals": hasattr(mesh, 'vertex_normals') and mesh.vertex_normals is not None,
        })

    # Generate thumbnail
    thumbnail_path = out / f"{path.stem}_thumb.png"
    try:
        png_data = scene.save_image(resolution=[512, 512])
        if png_data is not None:
            with open(thumbnail_path, 'wb') as f:
                f.write(png_data)
            metadata["thumbnail_path"] = str(thumbnail_path)
    except Exception as e:
        metadata["thumbnail_error"] = str(e)

    # Multi-view rendering for AI analysis (4 angles)
    views = []
    for i, angle in enumerate([0, 90, 180, 270]):
        try:
            # Rotate camera around Y axis
            rotation = trimesh.transformations.rotation_matrix(
                np.radians(angle), [0, 1, 0], scene.centroid
            )
            scene.camera_transform = rotation
            view_path = out / f"{path.stem}_view_{i}.png"
            png_data = scene.save_image(resolution=[512, 512])
            if png_data:
                with open(view_path, 'wb') as f:
                    f.write(png_data)
                views.append(str(view_path))
        except:
            pass

    if views:
        metadata["multi_view_paths"] = views

    return metadata

if __name__ == '__main__':
    if len(sys.argv) < 3:
        print(json.dumps({"error": "Usage: process_3d.py <input> <output_dir>"}))
        sys.exit(1)

    result = process_3d_file(sys.argv[1], sys.argv[2])
    print(json.dumps(result, indent=2))
```

### 5. Rust Integration

```rust
// crates/matric-jobs/src/handlers/process_3d.rs

use std::process::Command;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Model3dMetadata {
    pub format: String,
    pub vertex_count: Option<u32>,
    pub face_count: Option<u32>,
    pub bounds_min: Option<[f64; 3]>,
    pub bounds_max: Option<[f64; 3]>,
    pub center: Option<[f64; 3]>,
    pub volume: Option<f64>,
    pub surface_area: Option<f64>,
    pub is_watertight: Option<bool>,
    pub thumbnail_path: Option<String>,
    pub multi_view_paths: Option<Vec<String>>,
    pub error: Option<String>,
}

pub async fn process_3d_attachment(
    attachment_id: Uuid,
    file_path: &Path,
    output_dir: &Path,
) -> Result<Model3dMetadata, Error> {
    let output = Command::new("python3")
        .arg("/app/scripts/process_3d.py")
        .arg(file_path)
        .arg(output_dir)
        .output()
        .await?;

    if !output.status.success() {
        return Err(Error::ProcessingFailed(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }

    let metadata: Model3dMetadata = serde_json::from_slice(&output.stdout)?;
    Ok(metadata)
}
```

### 6. AI Description Generation (Phase 2)

```rust
// Integration with matric-inference for LLaVA

pub async fn generate_3d_description(
    inference: &InferenceClient,
    multi_view_paths: &[PathBuf],
) -> Result<String, Error> {
    // Load multi-view images
    let images: Vec<Vec<u8>> = multi_view_paths
        .iter()
        .map(|p| std::fs::read(p))
        .collect::<Result<_, _>>()?;

    // Prompt for 3D understanding
    let prompt = r#"
These are 4 views of the same 3D object from different angles (0°, 90°, 180°, 270°).

Analyze this 3D object and provide:
1. What type of object is this?
2. What is it likely used for?
3. Key geometric features (symmetry, holes, protrusions)
4. Estimated material or construction
5. Any notable design characteristics

Be specific and technical where appropriate.
"#;

    let response = inference
        .generate_from_images(&images, prompt)
        .await?;

    Ok(response)
}
```

### 7. Document Type Integration

**New 3D-specific doctypes:**

```yaml
# model-3d-generic
name: "3D Model"
slug: "model-3d-generic"
extraction_strategy: "model_3d_mesh"
mime_patterns:
  - "model/*"
  - "application/octet-stream"  # with extension check
file_extensions:
  - ".glb"
  - ".gltf"
  - ".obj"
  - ".stl"
  - ".ply"
  - ".fbx"
agentic_config:
  generation_prompt: |
    Create a comprehensive description of this 3D model.

    Geometric properties:
    - Vertices: {{vertex_count}}
    - Faces: {{face_count}}
    - Volume: {{volume}}
    - Surface area: {{surface_area}}
    - Watertight: {{is_watertight}}

    AI Analysis:
    {{ai_description}}

    Generate a note that describes what this model represents,
    its likely use cases, and any notable characteristics.

# model-3d-cad
name: "CAD Model"
slug: "model-3d-cad"
extraction_strategy: "model_3d_cad"
mime_patterns:
  - "application/step"
  - "model/step"
file_extensions:
  - ".step"
  - ".stp"
  - ".iges"
  - ".igs"
agentic_config:
  generation_prompt: |
    This is a CAD/engineering model.
    Extract part numbers, tolerances, and manufacturing notes if present.

# model-3d-printable
name: "3D Print Model"
slug: "model-3d-printable"
extraction_strategy: "model_3d_mesh"
file_extensions:
  - ".stl"
  - ".3mf"
agentic_config:
  generation_prompt: |
    Analyze this 3D printable model.

    Print analysis:
    - Watertight: {{is_watertight}} (required for printing)
    - Volume: {{volume}} (affects material usage)
    - Surface area: {{surface_area}}

    Assess printability and suggest optimal orientation.
```

### 8. Search Capabilities

**Query patterns:**

```sql
-- Find 3D models by geometric properties
SELECT n.*, m.*
FROM note n
JOIN file_attachment fa ON fa.note_id = n.id
JOIN model_3d_metadata m ON m.attachment_id = fa.id
WHERE m.is_watertight = TRUE
  AND m.vertex_count < 100000
  AND m.volume BETWEEN 10 AND 100;

-- Full-text search on AI descriptions
SELECT n.*, m.*
FROM note n
JOIN file_attachment fa ON fa.note_id = n.id
JOIN model_3d_metadata m ON m.attachment_id = fa.id
WHERE to_tsvector('english', m.ai_description) @@ websearch_to_tsquery('gear mechanical');

-- Search with attachment type filter
?attachment:3d gear mechanism
?attachment:stl printable
```

**API endpoint:**

```
GET /api/search/3d
  ?format=stl,gltf
  &watertight=true
  &min_vertices=1000
  &max_vertices=100000
  &q=mechanical+gear

GET /api/notes/{id}/3d-viewer
  → Returns embedded Three.js viewer HTML
```

### 9. MCP Tools

```javascript
// Search 3D models
search_3d_models({
  query: 'mechanical gear',
  format: ['stl', 'gltf'],
  watertight: true,
  max_vertices: 100000
})

// Get 3D metadata
get_3d_metadata(attachment_id)
// Returns: vertices, faces, bounds, volume, thumbnail_url, description

// Generate description (if not already done)
describe_3d_model(attachment_id)
// Triggers LLaVA analysis, returns description
```

## Consequences

### Positive

- (+) **3D asset management**: Store, search, and organize 3D files
- (+) **Semantic search**: "Find gears" returns actual gear models
- (+) **Printability analysis**: Identify watertight models for 3D printing
- (+) **Cross-format search**: Search by properties regardless of format
- (+) **AI understanding**: Visual AI describes 3D objects
- (+) **Thumbnail previews**: Visual browsing without specialized software

### Negative

- (-) **Python dependency**: Requires trimesh, Open3D for processing
- (-) **Processing time**: 3D analysis takes 2-10 seconds per file
- (-) **Storage overhead**: Thumbnails + multi-view images (~2MB per model)
- (-) **AI accuracy**: Multi-view approach less accurate than native 3D models

### Mitigations

- Python already in Docker bundle for other processing
- Background jobs handle processing asynchronously
- Thumbnail compression reduces storage
- Multi-view + good prompts achieve acceptable accuracy

## Implementation

### Phase 1: Core Support (Weeks 1-2)
- Python processing script with trimesh
- Schema migration for model_3d_metadata
- Thumbnail generation
- Basic geometric metadata extraction
- API endpoints for upload/metadata

### Phase 2: AI Enhancement (Weeks 3-4)
- Multi-view rendering
- LLaVA integration via matric-inference
- AI description generation
- Semantic search on descriptions

### Phase 3: Advanced Features (Future)
- Interactive 3D viewer (Three.js)
- Point cloud support (Open3D)
- CAD format support (pythonOCC)
- Shape similarity search (OpenShape)

## References

- trimesh documentation: https://trimesh.org
- Open3D documentation: https://www.open3d.org
- glTF specification: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
- LLaVA model: https://github.com/haotian-liu/LLaVA
- OpenShape (future): https://github.com/Colin97/OpenShape_code
