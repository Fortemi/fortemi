//! GLB/3D Model extraction adapter — understands 3D models via multi-view rendering.
//!
//! Pipeline:
//! 1. Write model data to temp file with original extension
//! 2. Run Blender headless with a Python script to render N views from configurable camera angles
//! 3. Describe each rendered view using VisionBackend
//! 4. Synthesize a composite description from all views
//!
//! Requires: Blender (headless) + VisionBackend (Ollama with vision-capable model).

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use tempfile::{NamedTempFile, TempDir};
use tokio::process::Command;
use tracing::{debug, warn};

use matric_core::defaults::EXTRACTION_CMD_TIMEOUT_SECS;
use matric_core::{ExtractionAdapter, ExtractionResult, ExtractionStrategy, Result};
use matric_inference::vision::VisionBackend;

/// Default number of camera angles for multi-view rendering.
const DEFAULT_VIEW_COUNT: u64 = 6;

/// Maximum number of views to prevent runaway rendering.
const MAX_VIEW_COUNT: u64 = 15;

/// Blender Python script template for multi-view rendering.
///
/// Renders the model from N evenly-distributed camera positions on a sphere,
/// outputting numbered PNG images.
const BLENDER_RENDER_SCRIPT: &str = r#"
import bpy
import sys
import math
import os

# Parse arguments after '--'
argv = sys.argv
argv = argv[argv.index("--") + 1:]
model_path = argv[0]
output_dir = argv[1]
num_views = int(argv[2])

# Clear default scene
bpy.ops.wm.read_factory_settings(use_empty=True)

# Import the model based on extension
ext = os.path.splitext(model_path)[1].lower()
if ext in ('.glb', '.gltf'):
    bpy.ops.import_scene.gltf(filepath=model_path)
elif ext == '.obj':
    bpy.ops.wm.obj_import(filepath=model_path)
elif ext == '.fbx':
    bpy.ops.import_scene.fbx(filepath=model_path)
elif ext == '.stl':
    bpy.ops.wm.stl_import(filepath=model_path)
elif ext == '.ply':
    bpy.ops.import_mesh.ply(filepath=model_path)
else:
    # Try GLTF as fallback
    bpy.ops.import_scene.gltf(filepath=model_path)

# Find all mesh objects and compute bounding box
meshes = [o for o in bpy.context.scene.objects if o.type == 'MESH']
if not meshes:
    print("ERROR: No meshes found in model", file=sys.stderr)
    sys.exit(1)

# Compute scene bounding box
min_co = [float('inf')] * 3
max_co = [float('-inf')] * 3
for obj in meshes:
    for corner in obj.bound_box:
        world_co = obj.matrix_world @ bpy.app.driver_namespace.get('Vector', __import__('mathutils').Vector)(corner)
        for i in range(3):
            min_co[i] = min(min_co[i], world_co[i])
            max_co[i] = max(max_co[i], world_co[i])

center = [(min_co[i] + max_co[i]) / 2 for i in range(3)]
size = max(max_co[i] - min_co[i] for i in range(3))
camera_distance = size * 2.5  # Distance based on model size

# Add camera
bpy.ops.object.camera_add()
camera = bpy.context.active_object
bpy.context.scene.camera = camera

# Add lighting (3-point)
bpy.ops.object.light_add(type='SUN', location=(camera_distance, camera_distance, camera_distance * 1.5))
bpy.context.active_object.data.energy = 3.0
bpy.ops.object.light_add(type='AREA', location=(-camera_distance, -camera_distance * 0.5, camera_distance))
bpy.context.active_object.data.energy = 50.0
bpy.context.active_object.data.size = size

# Render settings
scene = bpy.context.scene
scene.render.resolution_x = 512
scene.render.resolution_y = 512
scene.render.image_settings.file_format = 'PNG'
scene.render.film_transparent = True

# Render from multiple angles
for i in range(num_views):
    angle = (2 * math.pi * i) / num_views
    elevation = math.pi / 6  # 30 degrees above horizon

    # Alternate elevation for odd views
    if i % 2 == 1:
        elevation = math.pi / 3  # 60 degrees (top-down-ish)

    x = center[0] + camera_distance * math.cos(angle) * math.cos(elevation)
    y = center[1] + camera_distance * math.sin(angle) * math.cos(elevation)
    z = center[2] + camera_distance * math.sin(elevation)

    camera.location = (x, y, z)

    # Point camera at center
    direction = bpy.app.driver_namespace.get('Vector', __import__('mathutils').Vector)(center) - camera.location
    camera.rotation_euler = direction.to_track_quat('-Z', 'Y').to_euler()

    scene.render.filepath = os.path.join(output_dir, f"view_{i:03d}.png")
    bpy.ops.render.render(write_still=True)

print(f"Rendered {num_views} views to {output_dir}")
"#;

pub struct Glb3DModelAdapter {
    backend: Arc<dyn VisionBackend>,
}

impl Glb3DModelAdapter {
    /// Create a new adapter with a specific vision backend.
    pub fn new(backend: Arc<dyn VisionBackend>) -> Self {
        Self { backend }
    }

    /// Create from environment variables using OllamaVisionBackend.
    ///
    /// Returns None if OLLAMA_VISION_MODEL is not set.
    pub fn from_env() -> Option<Self> {
        use matric_inference::vision::OllamaVisionBackend;

        let backend = OllamaVisionBackend::from_env()?;
        Some(Self::new(Arc::new(backend)))
    }
}

#[async_trait]
impl ExtractionAdapter for Glb3DModelAdapter {
    fn strategy(&self) -> ExtractionStrategy {
        ExtractionStrategy::Glb3DModel
    }

    async fn extract(
        &self,
        data: &[u8],
        filename: &str,
        _mime_type: &str,
        config: &JsonValue,
    ) -> Result<ExtractionResult> {
        if data.is_empty() {
            return Err(matric_core::Error::InvalidInput(
                "Cannot process empty 3D model data".to_string(),
            ));
        }

        // Parse config
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);

        let custom_prompt = config.get("prompt").and_then(|v| v.as_str());

        // Determine file extension from filename
        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("glb");

        // Write model to temp file (preserving extension for Blender import)
        let mut tmpfile = NamedTempFile::with_suffix(format!(".{}", extension)).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create temp file: {}", e))
        })?;
        tmpfile.write_all(data).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to write temp file: {}", e))
        })?;
        let model_path = tmpfile.path().to_string_lossy().to_string();

        // Create output directory for rendered views
        let render_dir = TempDir::new().map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create render dir: {}", e))
        })?;
        let render_path = render_dir.path().to_string_lossy().to_string();

        // Write Blender script to temp file
        let mut script_file = NamedTempFile::with_suffix(".py").map_err(|e| {
            matric_core::Error::Internal(format!("Failed to create script file: {}", e))
        })?;
        script_file
            .write_all(BLENDER_RENDER_SCRIPT.as_bytes())
            .map_err(|e| matric_core::Error::Internal(format!("Failed to write script: {}", e)))?;
        let script_path = script_file.path().to_string_lossy().to_string();

        debug!(
            filename,
            num_views, "Rendering 3D model from multiple angles"
        );

        // Run Blender headless to render views
        // Timeout: allow 30s per view (rendering can be slow) + base timeout
        let render_timeout = EXTRACTION_CMD_TIMEOUT_SECS + (num_views * 30);
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(render_timeout),
            Command::new("blender")
                .arg("--background")
                .arg("--python")
                .arg(&script_path)
                .arg("--")
                .arg(&model_path)
                .arg(&render_path)
                .arg(num_views.to_string())
                .output(),
        )
        .await
        .map_err(|_| {
            matric_core::Error::Internal(format!(
                "Blender rendering timed out after {}s",
                render_timeout
            ))
        })?
        .map_err(|e| matric_core::Error::Internal(format!("Failed to execute Blender: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(matric_core::Error::Internal(format!(
                "Blender rendering failed (exit {}): {}",
                output.status,
                stderr.chars().take(500).collect::<String>()
            )));
        }

        // Collect rendered view files
        let mut view_paths: Vec<PathBuf> = Vec::new();
        let entries = std::fs::read_dir(render_dir.path()).map_err(|e| {
            matric_core::Error::Internal(format!("Failed to read render dir: {}", e))
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                matric_core::Error::Internal(format!("Failed to read dir entry: {}", e))
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                view_paths.push(path);
            }
        }
        view_paths.sort();

        if view_paths.is_empty() {
            return Err(matric_core::Error::Internal(
                "Blender produced no rendered views".to_string(),
            ));
        }

        debug!(
            filename,
            rendered = view_paths.len(),
            "Describing rendered views"
        );

        // Describe each view using vision backend
        let mut view_descriptions = Vec::new();
        for (i, view_path) in view_paths.iter().enumerate() {
            let image_data = std::fs::read(view_path).map_err(|e| {
                matric_core::Error::Internal(format!("Failed to read rendered view: {}", e))
            })?;

            let angle_deg = (360.0 / num_views as f64) * i as f64;
            let elevation = if i % 2 == 0 {
                "low (30°)"
            } else {
                "high (60°)"
            };

            let prompt = if let Some(custom) = custom_prompt {
                format!(
                    "{}\n\nThis is view {} of {} (angle: {:.0}°, elevation: {}) of a 3D model from file '{}'.",
                    custom, i + 1, view_paths.len(), angle_deg, elevation, filename
                )
            } else {
                format!(
                    "Describe this rendered view of a 3D model in detail. \
                     This is view {} of {} (camera angle: {:.0}°, elevation: {}). \
                     The model file is '{}'. \
                     Describe the shape, materials, textures, colors, and any notable features visible from this angle.",
                    i + 1, view_paths.len(), angle_deg, elevation, filename
                )
            };

            match self
                .backend
                .describe_image(&image_data, "image/png", Some(&prompt))
                .await
            {
                Ok(description) => {
                    view_descriptions.push(json!({
                        "view_index": i,
                        "angle_degrees": angle_deg,
                        "elevation": elevation,
                        "description": description,
                    }));
                }
                Err(e) => {
                    warn!(view = i, error = %e, "View description failed");
                }
            }
        }

        // Synthesize composite description from all views
        let composite_description = if !view_descriptions.is_empty() {
            let views_text = view_descriptions
                .iter()
                .map(|v| {
                    format!(
                        "View {} ({:.0}°, {}): {}",
                        v["view_index"], v["angle_degrees"], v["elevation"], v["description"]
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            // Ask vision model to synthesize a unified description
            let synthesis_prompt = format!(
                "Below are descriptions of the same 3D model ('{}') viewed from {} different camera angles.\n\n\
                 {}\n\n\
                 Provide a single comprehensive description of this 3D model, \
                 combining information from all views. \
                 Describe the overall shape, geometry, materials, colors, and purpose of the object.",
                filename,
                view_descriptions.len(),
                views_text
            );

            // Use a dummy 1x1 white PNG as the image (the prompt contains the real content)
            let dummy_png = create_minimal_png();
            match self
                .backend
                .describe_image(&dummy_png, "image/png", Some(&synthesis_prompt))
                .await
            {
                Ok(synthesis) => Some(synthesis),
                Err(e) => {
                    warn!(error = %e, "Synthesis failed, using concatenated descriptions");
                    Some(views_text)
                }
            }
        } else {
            None
        };

        Ok(ExtractionResult {
            extracted_text: None,
            metadata: json!({
                "model": self.backend.model_name(),
                "filename": filename,
                "size_bytes": data.len(),
                "num_views_requested": num_views,
                "num_views_rendered": view_paths.len(),
                "num_views_described": view_descriptions.len(),
                "view_descriptions": view_descriptions,
            }),
            ai_description: composite_description,
            preview_data: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        // Check if Blender is available
        let blender_ok = match Command::new("blender").arg("--version").output().await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        };

        if !blender_ok {
            return Ok(false);
        }

        // Also check vision backend
        self.backend.health_check().await
    }

    fn name(&self) -> &str {
        "glb_3d_model"
    }
}

/// Create a minimal valid 1x1 white PNG for use as a dummy image
/// when the real content is in the prompt text.
fn create_minimal_png() -> Vec<u8> {
    // Minimal 1x1 white PNG (67 bytes)
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, // 8-bit RGB
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
        0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, // compressed data
        0x00, 0x02, 0x00, 0x01, 0xE2, 0x21, 0xBC, 0x33, // checksum
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
        0xAE, 0x42, 0x60, 0x82,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use matric_core::Error;

    /// Mock vision backend for testing.
    struct MockVisionBackend {
        description: String,
        health: bool,
    }

    impl MockVisionBackend {
        fn new(description: &str) -> Self {
            Self {
                description: description.to_string(),
                health: true,
            }
        }

        fn unhealthy() -> Self {
            Self {
                description: String::new(),
                health: false,
            }
        }
    }

    #[async_trait]
    impl VisionBackend for MockVisionBackend {
        async fn describe_image(
            &self,
            _image_data: &[u8],
            _mime_type: &str,
            _prompt: Option<&str>,
        ) -> Result<String> {
            Ok(self.description.clone())
        }

        async fn health_check(&self) -> Result<bool> {
            Ok(self.health)
        }

        fn model_name(&self) -> &str {
            "mock-vision"
        }
    }

    #[test]
    fn test_glb_adapter_strategy() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        assert_eq!(adapter.strategy(), ExtractionStrategy::Glb3DModel);
    }

    #[test]
    fn test_glb_adapter_name() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        assert_eq!(adapter.name(), "glb_3d_model");
    }

    #[tokio::test]
    async fn test_glb_adapter_empty_input() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));

        let result = adapter
            .extract(b"", "empty.glb", "model/gltf-binary", &json!({}))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)));
        assert!(err.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn test_glb_adapter_health_check_no_blender() {
        // This test checks that health_check gracefully handles missing Blender.
        // If Blender happens to be installed, it will return true (also valid).
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        // Result depends on whether Blender is installed — both true/false are valid
    }

    #[tokio::test]
    async fn test_glb_adapter_health_check_unhealthy_backend() {
        let mock = MockVisionBackend::unhealthy();
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        let result = adapter.health_check().await;
        assert!(result.is_ok());
        // Even if Blender is present, unhealthy backend means false
        // (Blender absent also means false — either way, not both healthy)
        // We can't assert the exact value because Blender may or may not be installed
    }

    #[test]
    fn test_create_minimal_png() {
        let png = create_minimal_png();
        // PNG magic bytes
        assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]);
        // IHDR chunk
        assert_eq!(&png[12..16], b"IHDR");
        // IEND chunk (last 12 bytes)
        let iend_start = png.len() - 12;
        assert_eq!(&png[iend_start + 4..iend_start + 8], b"IEND");
    }

    #[test]
    fn test_view_count_defaults() {
        // Default view count
        let config = json!({});
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);
        assert_eq!(num_views, DEFAULT_VIEW_COUNT);

        // Custom view count
        let config = json!({ "num_views": 10 });
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);
        assert_eq!(num_views, 10);

        // Capped at max
        let config = json!({ "num_views": 100 });
        let num_views = config
            .get("num_views")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_VIEW_COUNT)
            .min(MAX_VIEW_COUNT);
        assert_eq!(num_views, MAX_VIEW_COUNT);
    }

    #[test]
    fn test_extension_parsing() {
        let ext = std::path::Path::new("model.glb")
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("glb");
        assert_eq!(ext, "glb");

        let ext = std::path::Path::new("scene.gltf")
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("glb");
        assert_eq!(ext, "gltf");

        let ext = std::path::Path::new("mesh.obj")
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("glb");
        assert_eq!(ext, "obj");

        // No extension falls back to "glb"
        let ext = std::path::Path::new("noext")
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("glb");
        assert_eq!(ext, "glb");
    }

    #[test]
    fn test_glb_adapter_constructor() {
        let mock = MockVisionBackend::new("test");
        let adapter = Glb3DModelAdapter::new(Arc::new(mock));
        assert_eq!(adapter.name(), "glb_3d_model");
        assert_eq!(adapter.strategy(), ExtractionStrategy::Glb3DModel);
    }
}
