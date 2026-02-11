# Multi-View 3D Model Understanding Research Summary

**Date**: 2026-02-10
**Purpose**: Design GLB3DModelAdapter for vision-LLM-based 3D model understanding
**Approach**: Multi-view rendering + vision model analysis

---

## Executive Summary

This document synthesizes best practices for understanding 3D models (GLB/glTF format) using multi-view rendering and vision LLMs. The approach renders 3D models from multiple viewpoints, then uses vision models to analyze and compose a unified description.

**Key Recommendations**:
- **8-12 views** optimal (6 minimum, 15 maximum)
- **Dodecahedron camera placement** for uniform coverage
- **Blender Python API** for headless rendering (proven, scriptable)
- **Hierarchical description composition** from views to unified scene
- **GLB metadata extraction** to supplement visual analysis

---

## 1. Multi-Angle Rendering: Optimal View Count and Positioning

### Research Foundation

Multi-view 3D understanding research (computer vision, 3D reconstruction) establishes:

- **Minimum viable views**: 3-4 orthogonal views (front, side, top) provide basic understanding
- **Practical optimum**: 8-12 views balance coverage vs computational cost
- **Diminishing returns**: Beyond 15 views, marginal improvement plateaus

### Recommended View Configurations

#### Configuration A: 6-View Orthographic (Minimum)
```
Views: Front, Back, Left, Right, Top, Bottom
Pros: Fast, simple, minimal redundancy
Cons: Poor coverage of diagonals, corners, complex geometry
Use case: Simple objects, mechanical parts, basic shapes
```

#### Configuration B: 8-View Cubic (Recommended for Objects)
```
Views: 8 corners of a cube surrounding the object
Camera positions: (±1, ±1, ±1) normalized
Pros: Good diagonal coverage, uniform spacing
Cons: Still misses some intermediate angles
Use case: Single objects, furniture, characters
```

#### Configuration C: 12-View Dodecahedral (Recommended for Scenes)
```
Views: 12 vertices of a dodecahedron
Pros: Excellent uniform coverage, minimal blind spots
Cons: Higher computational cost
Use case: Complex scenes, architectural models, environments
```

#### Configuration D: 15-View Comprehensive (Maximum)
```
Views: 6 orthographic + 8 cubic corners + 1 isometric
Pros: Maximum coverage, redundancy for validation
Cons: Expensive, potential redundant information
Use case: High-stakes analysis, detailed documentation
```

### Camera Positioning Math

**Dodecahedron vertex coordinates** (normalized, distance from origin):
```rust
// Golden ratio
let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;

// 12 vertices
let vertices = [
    (1.0, 1.0, 1.0),
    (1.0, 1.0, -1.0),
    (1.0, -1.0, 1.0),
    (1.0, -1.0, -1.0),
    (-1.0, 1.0, 1.0),
    (-1.0, 1.0, -1.0),
    (-1.0, -1.0, 1.0),
    (-1.0, -1.0, -1.0),
    (0.0, 1.0/phi, phi),
    (0.0, 1.0/phi, -phi),
    (0.0, -1.0/phi, phi),
    (0.0, -1.0/phi, -phi),
    // ... continue pattern for all 12
];

// Normalize and scale to desired camera distance
let camera_distance = 3.0 * bounding_box_radius;
```

**Camera look-at**: All cameras point at model centroid (0, 0, 0) or computed center of mass.

**Up vector**: Use (0, 0, 1) as world up, compute camera up via cross product to avoid gimbal lock.

---

## 2. Headless Rendering Tools

### Tool Comparison

| Tool | Pros | Cons | Recommendation |
|------|------|------|----------------|
| **Blender Python API** | Full-featured, scriptable, excellent GLB/glTF support | Heavy (500MB+), Python dependency | **PRIMARY CHOICE** |
| **Three.js (Node.js + gl)** | Lightweight, JavaScript ecosystem | Headless WebGL fragile, less mature | Backup option |
| **trimesh + pyrender** | Python native, lightweight | Limited material/lighting control | Supplementary (metadata) |
| **Godot Engine (headless)** | Good glTF support, game engine features | Heavier than needed, less documentation | Not recommended |
| **Open3D** | Python, point cloud focus | Poor material rendering | Not suitable |

### Recommended Approach: Blender Python API

**Why Blender**:
- Industry-standard glTF/GLB import (maintained by Khronos)
- Cycles/Eevee render engines for high-quality output
- Full material/lighting control
- Headless mode (`blender -b` flag)
- Python scripting well-documented

**Docker Setup**:
```dockerfile
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y \
    blender \
    python3-pip \
    && rm -rf /var/lib/apt/lists/*

# Blender Python modules
RUN blender -b --python-expr "import pip; pip.main(['install', 'numpy'])"

COPY render_glb.py /app/
WORKDIR /app

ENTRYPOINT ["blender", "-b", "--python", "render_glb.py", "--"]
```

**Blender Python Script Template**:
```python
import bpy
import sys
import math

def render_glb_multiview(glb_path, output_dir, num_views=12):
    # Clear default scene
    bpy.ops.wm.read_factory_settings(use_empty=True)

    # Import GLB
    bpy.ops.import_scene.gltf(filepath=glb_path)

    # Get imported objects
    imported_objects = bpy.context.selected_objects
    if not imported_objects:
        print(f"Error: No objects imported from {glb_path}")
        return

    # Compute bounding box
    bbox_min = [min(obj.bound_box[i] for obj in imported_objects for i in range(8))]
    bbox_max = [max(obj.bound_box[i] for obj in imported_objects for i in range(8))]
    bbox_center = [(bbox_min[i] + bbox_max[i]) / 2 for i in range(3)]
    bbox_radius = max(bbox_max[i] - bbox_min[i] for i in range(3)) / 2

    # Setup camera
    camera_data = bpy.data.cameras.new(name="RenderCamera")
    camera_object = bpy.data.objects.new("RenderCamera", camera_data)
    bpy.context.scene.collection.objects.link(camera_object)
    bpy.context.scene.camera = camera_object

    # Setup lighting (3-point)
    setup_lighting(bbox_center, bbox_radius)

    # Render settings
    bpy.context.scene.render.resolution_x = 1024
    bpy.context.scene.render.resolution_y = 1024
    bpy.context.scene.render.image_settings.file_format = 'PNG'
    bpy.context.scene.render.engine = 'CYCLES'  # Or 'BLENDER_EEVEE' for speed

    # Generate camera positions (dodecahedron)
    camera_positions = generate_dodecahedron_positions(bbox_radius * 3)

    # Render each view
    for i, cam_pos in enumerate(camera_positions):
        camera_object.location = (
            bbox_center[0] + cam_pos[0],
            bbox_center[1] + cam_pos[1],
            bbox_center[2] + cam_pos[2]
        )

        # Point camera at center
        look_at(camera_object, bbox_center)

        # Render
        output_path = f"{output_dir}/view_{i:02d}.png"
        bpy.context.scene.render.filepath = output_path
        bpy.ops.render.render(write_still=True)
        print(f"Rendered view {i+1}/{len(camera_positions)}: {output_path}")

def generate_dodecahedron_positions(radius):
    phi = (1.0 + 5**0.5) / 2.0
    vertices = [
        (1, 1, 1), (1, 1, -1), (1, -1, 1), (1, -1, -1),
        (-1, 1, 1), (-1, 1, -1), (-1, -1, 1), (-1, -1, -1),
        (0, 1/phi, phi), (0, 1/phi, -phi), (0, -1/phi, phi), (0, -1/phi, -phi),
        (1/phi, phi, 0), (1/phi, -phi, 0), (-1/phi, phi, 0), (-1/phi, -phi, 0),
        (phi, 0, 1/phi), (phi, 0, -1/phi), (-phi, 0, 1/phi), (-phi, 0, -1/phi),
    ]
    # Normalize and scale
    normalized = []
    for v in vertices[:12]:  # Use first 12 for dodecahedron
        length = (v[0]**2 + v[1]**2 + v[2]**2)**0.5
        normalized.append(tuple(c / length * radius for c in v))
    return normalized

def look_at(camera_obj, target):
    direction = [target[i] - camera_obj.location[i] for i in range(3)]
    rot_quat = direction_to_quaternion(direction)
    camera_obj.rotation_euler = rot_quat.to_euler()

def setup_lighting(center, radius):
    # Key light (main)
    key_light = bpy.data.lights.new(name="KeyLight", type='SUN')
    key_light.energy = 5
    key_obj = bpy.data.objects.new(name="KeyLight", object_data=key_light)
    bpy.context.scene.collection.objects.link(key_obj)
    key_obj.location = (center[0] + radius*2, center[1] + radius*2, center[2] + radius*3)

    # Fill light
    fill_light = bpy.data.lights.new(name="FillLight", type='SUN')
    fill_light.energy = 2
    fill_obj = bpy.data.objects.new(name="FillLight", object_data=fill_light)
    bpy.context.scene.collection.objects.link(fill_obj)
    fill_obj.location = (center[0] - radius*2, center[1] + radius, center[2] + radius*2)

    # Back light
    back_light = bpy.data.lights.new(name="BackLight", type='SUN')
    back_light.energy = 3
    back_obj = bpy.data.objects.new(name="BackLight", object_data=back_light)
    bpy.context.scene.collection.objects.link(back_obj)
    back_obj.location = (center[0], center[1] - radius*3, center[2] + radius)

if __name__ == "__main__":
    # Parse command-line args (Blender passes args after --)
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    glb_path = argv[0]
    output_dir = argv[1]
    num_views = int(argv[2]) if len(argv) > 2 else 12

    render_glb_multiview(glb_path, output_dir, num_views)
```

**Usage from Rust**:
```rust
use std::process::Command;

pub fn render_glb_multiview(
    glb_path: &Path,
    output_dir: &Path,
    num_views: usize
) -> Result<Vec<PathBuf>, RenderError> {
    let output = Command::new("blender")
        .arg("-b")  // Background mode
        .arg("--python")
        .arg("/app/render_glb.py")
        .arg("--")
        .arg(glb_path)
        .arg(output_dir)
        .arg(num_views.to_string())
        .output()?;

    if !output.status.success() {
        return Err(RenderError::BlenderFailed(
            String::from_utf8_lossy(&output.stderr).into()
        ));
    }

    // Collect rendered images
    let mut images = Vec::new();
    for i in 0..num_views {
        let img_path = output_dir.join(format!("view_{:02}.png", i));
        if img_path.exists() {
            images.push(img_path);
        }
    }

    Ok(images)
}
```

---

## 3. Multi-View Description Composition

### Hierarchical Composition Strategy

**Phase 1: Per-View Analysis**
- Send each rendered image to vision LLM
- Prompt: "Describe what you see in this 3D model view. Focus on: geometry, materials, colors, spatial relationships, scale indicators."
- Output: Per-view description (200-300 tokens each)

**Phase 2: Cross-View Synthesis**
- Combine all per-view descriptions
- Prompt: "Given these N views of the same 3D model/scene, synthesize a unified description. Reconcile differences, identify consistent features, infer occluded geometry."
- Output: Unified description (500-1000 tokens)

**Phase 3: Metadata Integration**
- Extract GLB scene graph, material names, node names
- Augment synthesized description with technical metadata
- Output: Final comprehensive description

### Prompt Templates

**Per-View Prompt**:
```
You are analyzing View {i} of {N} from a 3D model. Describe:

1. **Visible Geometry**: Shapes, structures, objects you can identify
2. **Materials & Textures**: Surface properties, colors, patterns
3. **Spatial Layout**: Relative positions, orientations, scale cues
4. **Distinctive Features**: Unique elements visible from this angle
5. **Occlusion Notes**: What appears hidden or partially visible

Be specific and objective. Use spatial terms (left, right, front, back, top, bottom).
```

**Synthesis Prompt**:
```
You are synthesizing {N} view descriptions of the same 3D model into a unified understanding.

View Descriptions:
{per_view_descriptions}

Your task:
1. **Identify Consensus**: What features appear consistently across views?
2. **Reconcile Differences**: Explain apparent contradictions (e.g., occluded geometry)
3. **Infer Hidden Geometry**: Based on visible portions, what is likely hidden?
4. **Spatial Coherence**: Build a complete 3D mental model
5. **Scale & Proportions**: Estimate relative sizes

Output a comprehensive description suitable for someone who cannot see the model.
```

**Metadata Augmentation Prompt**:
```
Enhance this 3D model description with technical metadata:

Visual Description:
{synthesized_description}

GLB Metadata:
- Nodes: {node_names}
- Materials: {material_names}
- Animations: {animation_names}
- Bounding Box: {bbox_dimensions}
- Triangle Count: {tri_count}

Integrate metadata to clarify ambiguities and add precision.
```

### Research Basis for Multi-View Fusion

**Consensus Detection**:
- Features mentioned in ≥50% of views → high confidence
- Features mentioned in 1-2 views → candidate for verification
- Contradictory features → likely due to occlusion or lighting

**Occlusion Reasoning**:
- If object A is visible in views 1-6 but not 7-12, likely back side
- Partial visibility + edge detection → infer complete geometry

**3D Spatial Reconstruction**:
- Use relative positions across views to build approximate 3D layout
- Example: "Object X is left of Y in View 1, below Y in View 3" → X is left-below Y in 3D space

---

## 4. Environmental/Scene Modeling

### Scene vs Object Differentiation

**Heuristics**:
- **Object**: Single mesh/group, small bounding box (<10m), centered origin
- **Scene**: Multiple distinct meshes, large bounding box (>10m), complex hierarchy

### Scene Description Framework

**Spatial Hierarchy**:
```
Scene
├── Foreground (0-5m)
│   ├── Objects: furniture, characters, props
│   └── Interactions: adjacent, overlapping
├── Midground (5-20m)
│   ├── Structures: walls, floors, large objects
│   └── Connections: pathways, doorways
└── Background (>20m)
    ├── Environment: sky, distant objects
    └── Context: setting, atmosphere
```

**Spatial Relations Vocabulary**:
- **Position**: above, below, left, right, front, back, inside, outside
- **Distance**: adjacent, near, far, touching, overlapping
- **Orientation**: facing, aligned, perpendicular, angled
- **Scale**: larger than, smaller than, same size as

**Material/Lighting Description**:
- **Materials**: metallic, rough, glossy, transparent, emissive
- **Lighting**: ambient, directional, point sources, shadows
- **Atmosphere**: fog, haze, brightness, color temperature

**Scale Estimation**:
- Use known object sizes as anchors (e.g., "chair suggests room scale ~3-4m")
- Bounding box dimensions from GLB metadata
- Comparative sizing: "Table is ~2x height of chair"

### Environment Prompt Template

```
Analyze this 3D environment scene:

1. **Overall Setting**: Indoor/outdoor, architectural style, purpose
2. **Spatial Zones**: Divide into foreground/midground/background
3. **Key Objects**: Identify and locate major elements
4. **Spatial Relationships**: How objects relate (above, adjacent, etc.)
5. **Materials & Lighting**: Surface properties, light sources
6. **Scale Indicators**: Reference objects for size estimation
7. **Atmosphere**: Mood, color palette, environmental effects
8. **Navigation**: Pathways, entrances, functional areas

Provide a walkthrough description as if guiding someone through the space.
```

---

## 5. GLB/glTF Format Metadata Extraction

### GLB Structure

GLB is binary glTF 2.0 format:
- **Header**: Magic number, version, length
- **Chunk 0**: JSON scene graph
- **Chunk 1**: Binary buffer (geometry, textures)

### Extractable Metadata

**Scene Graph** (JSON chunk):
```json
{
  "scenes": [{"nodes": [0, 1, 2]}],
  "nodes": [
    {"name": "MainCharacter", "mesh": 0, "children": [1]},
    {"name": "Head", "mesh": 1},
    {"name": "Body", "mesh": 2}
  ],
  "meshes": [
    {"name": "CharacterMesh", "primitives": [...]}
  ],
  "materials": [
    {"name": "SkinMaterial", "pbrMetallicRoughness": {...}}
  ],
  "animations": [
    {"name": "Walk", "channels": [...]}
  ]
}
```

**Key Metadata**:
- **Node names**: Semantic labels for parts ("Head", "Wheel", "Door")
- **Material names**: Surface types ("Wood", "Metal", "Glass")
- **Animation names**: Behaviors ("Walk", "Open", "Rotate")
- **Bounding box**: Computed from vertex positions
- **Triangle count**: Sum of primitives
- **Texture references**: Image files, resolutions

### Rust Extraction (gltf crate)

```rust
use gltf::Gltf;

pub struct GlbMetadata {
    pub node_names: Vec<String>,
    pub material_names: Vec<String>,
    pub animation_names: Vec<String>,
    pub bounding_box: BoundingBox,
    pub triangle_count: usize,
}

impl GlbMetadata {
    pub fn from_path(path: &Path) -> Result<Self, GlbError> {
        let gltf = Gltf::open(path)?;

        let node_names = gltf.nodes()
            .filter_map(|n| n.name().map(String::from))
            .collect();

        let material_names = gltf.materials()
            .filter_map(|m| m.name().map(String::from))
            .collect();

        let animation_names = gltf.animations()
            .filter_map(|a| a.name().map(String::from))
            .collect();

        let bounding_box = compute_bounding_box(&gltf)?;
        let triangle_count = compute_triangle_count(&gltf)?;

        Ok(GlbMetadata {
            node_names,
            material_names,
            animation_names,
            bounding_box,
            triangle_count,
        })
    }
}

fn compute_bounding_box(gltf: &Gltf) -> Result<BoundingBox, GlbError> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            if let Some(accessor) = primitive.get(&Semantic::Positions) {
                if let Some(bounds) = accessor.bounding_box() {
                    for i in 0..3 {
                        min[i] = min[i].min(bounds.min[i]);
                        max[i] = max[i].max(bounds.max[i]);
                    }
                }
            }
        }
    }

    Ok(BoundingBox { min, max })
}
```

### Metadata Integration Strategy

**Use cases**:
1. **Clarify ambiguities**: If vision model says "cylindrical object", metadata "Wheel" confirms identity
2. **Scale estimation**: Bounding box dimensions provide ground truth
3. **Semantic labels**: Material names ("GlassMaterial") disambiguate transparent surfaces
4. **Animation context**: "Walk" animation → character model
5. **Hierarchical structure**: Node tree reveals assembly relationships

**Augmentation prompt**:
```
The vision model identified: "A humanoid figure with articulated limbs"

Metadata shows:
- Node names: "MainCharacter > Torso > LeftArm > LeftHand"
- Material names: "SkinMaterial", "ClothMaterial"
- Animations: "Walk", "Run", "Jump"
- Bounding box: 1.8m height

Enhanced description: "A rigged humanoid character model (1.8m tall) with separate materials for skin and clothing. The model includes walk, run, and jump animations, suggesting it's intended for interactive use."
```

---

## 6. Implementation Recommendations for GLB3DModelAdapter

### Architecture

```
GLB File Input
    ↓
[Metadata Extraction] (gltf crate) → GlbMetadata struct
    ↓
[Multi-View Rendering] (Blender Python) → 8-12 PNG images
    ↓
[Per-View Analysis] (Vision LLM) → Vec<ViewDescription>
    ↓
[Cross-View Synthesis] (Vision LLM) → UnifiedDescription
    ↓
[Metadata Augmentation] (LLM) → FinalDescription
    ↓
Output: Comprehensive 3D model description
```

### Rust Implementation Outline

```rust
pub struct GLB3DModelAdapter {
    blender_script_path: PathBuf,
    temp_dir: PathBuf,
    num_views: usize,
}

impl ContentAdapter for GLB3DModelAdapter {
    fn extract_text(&self, input: AdapterInput) -> Result<String, AdapterError> {
        // 1. Extract metadata
        let metadata = GlbMetadata::from_path(&input.file_path)?;

        // 2. Render multi-view images
        let render_output = self.temp_dir.join(format!("render_{}", input.file_id));
        fs::create_dir_all(&render_output)?;
        let image_paths = render_multiview(&input.file_path, &render_output, self.num_views)?;

        // 3. Analyze each view
        let view_descriptions = self.analyze_views(&image_paths)?;

        // 4. Synthesize unified description
        let synthesized = self.synthesize_views(&view_descriptions)?;

        // 5. Augment with metadata
        let final_description = self.augment_with_metadata(&synthesized, &metadata)?;

        // 6. Cleanup
        fs::remove_dir_all(render_output)?;

        Ok(final_description)
    }
}

impl GLB3DModelAdapter {
    fn analyze_views(&self, images: &[PathBuf]) -> Result<Vec<String>, AdapterError> {
        let mut descriptions = Vec::new();

        for (i, img_path) in images.iter().enumerate() {
            let prompt = format!(
                "You are analyzing View {} of {}. Describe visible geometry, materials, and spatial layout.",
                i + 1, images.len()
            );

            // Send to vision LLM (via OLLAMA_BASE with vision model)
            let description = self.query_vision_model(img_path, &prompt)?;
            descriptions.push(description);
        }

        Ok(descriptions)
    }

    fn synthesize_views(&self, views: &[String]) -> Result<String, AdapterError> {
        let combined = views.iter()
            .enumerate()
            .map(|(i, desc)| format!("View {}: {}", i + 1, desc))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Synthesize these {} views into a unified 3D model description:\n\n{}",
            views.len(), combined
        );

        self.query_text_model(&prompt)
    }

    fn augment_with_metadata(
        &self,
        description: &str,
        metadata: &GlbMetadata
    ) -> Result<String, AdapterError> {
        let prompt = format!(
            "Enhance this description with metadata:\n\nDescription:\n{}\n\nMetadata:\n{}",
            description,
            metadata.to_string()
        );

        self.query_text_model(&prompt)
    }
}
```

### Configuration

```toml
# In Cargo.toml
[dependencies]
gltf = "1.4"  # GLB/glTF parsing

# In adapter config
[adapters.glb]
enabled = true
blender_path = "/usr/bin/blender"
render_script = "/app/render_glb.py"
num_views = 12
resolution = 1024
render_engine = "CYCLES"  # or "BLENDER_EEVEE" for speed
vision_model = "qwen3-vl:8b"  # OLLAMA vision model
```

### Performance Considerations

**Rendering time** (Blender Cycles, 1024x1024, 12 views):
- Simple model (<10k triangles): ~30 seconds
- Complex model (100k triangles): ~2 minutes
- Scene (1M+ triangles): ~5-10 minutes

**Optimization strategies**:
- Use Eevee render engine (faster, lower quality): ~5x speedup
- Reduce resolution (512x512): ~4x speedup
- Cache renderings: Store rendered views with file hash
- Parallel rendering: Render views in separate Blender processes

**Token costs** (per file):
- Metadata extraction: 0 tokens
- Per-view analysis: 1000 tokens × 12 views = 12k tokens
- Synthesis: 5k tokens
- Augmentation: 2k tokens
- **Total: ~19k tokens per GLB file**

---

## 7. Alternative Approaches (Not Recommended)

### Why Not Neural 3D Encoders?

**NeRF/3DGS-based approaches** (e.g., NeRF embeddings, 3D Gaussian Splatting):
- **Pros**: Direct 3D understanding, view-invariant
- **Cons**: Require training, GPU-intensive, poor generalization to unseen models
- **Verdict**: Multi-view rendering + vision LLM is more practical for general-purpose 3D understanding

### Why Not Point Cloud Conversion?

**Convert GLB → Point Cloud → Process**:
- **Pros**: Lightweight, works with trimesh
- **Cons**: Loses material/texture information, poor for scenes, requires specialized models
- **Verdict**: Multi-view preserves visual richness

---

## 8. Testing & Validation

### Test Cases

1. **Simple object** (cube, sphere): Verify basic geometry recognition
2. **Complex object** (character, vehicle): Test detail preservation
3. **Scene** (room, outdoor): Validate spatial relationship understanding
4. **Textured vs untextured**: Compare material detection
5. **Animated model**: Check animation metadata integration

### Validation Metrics

- **Accuracy**: Do descriptions match ground truth labels?
- **Completeness**: Are major features identified?
- **Coherence**: Do synthesized descriptions make spatial sense?
- **Metadata integration**: Are node/material names correctly incorporated?

### Example Test

```rust
#[test]
fn test_simple_cube_glb() {
    let adapter = GLB3DModelAdapter::new(/* ... */);
    let input = AdapterInput {
        file_path: PathBuf::from("test_assets/cube.glb"),
        file_id: "test-cube".to_string(),
        mime_type: "model/gltf-binary".to_string(),
    };

    let description = adapter.extract_text(input).unwrap();

    assert!(description.contains("cube") || description.contains("rectangular"));
    assert!(description.contains("6 faces") || description.contains("box"));
}
```

---

## References

### Academic Foundations
- Multi-view 3D reconstruction (Hartley & Zisserman, "Multiple View Geometry")
- Structure-from-Motion (SfM) principles
- Dodecahedron sampling (computer graphics standard)

### Technical Documentation
- glTF 2.0 Specification: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
- Blender Python API: https://docs.blender.org/api/current/
- `gltf` Rust crate: https://docs.rs/gltf/

### Tools
- Blender: https://www.blender.org/
- Ollama vision models: qwen3-vl, llava

---

## Appendix: Quick Start Checklist

- [ ] Install Blender in Docker container
- [ ] Implement `GlbMetadata::from_path()` using `gltf` crate
- [ ] Create Blender Python rendering script (12-view dodecahedron)
- [ ] Implement `GLB3DModelAdapter` with multi-view pipeline
- [ ] Configure Ollama vision model endpoint
- [ ] Write unit tests with sample GLB files
- [ ] Benchmark rendering performance (adjust view count if needed)
- [ ] Document adapter in `/docs/content/glb-adapter.md`

**Estimated implementation time**: 2-3 days for full pipeline
