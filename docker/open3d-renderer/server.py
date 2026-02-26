"""
Open3D 3D Model Renderer — GPU-accelerated headless rendering via EGL

Push-only design: Receives model data via multipart POST, renders multi-view images,
and returns all rendered PNG images directly in a multipart response.
No download endpoints — all data returned in single request/response cycle.

Requires:
  - NVIDIA GPU + NVIDIA Container Toolkit for EGL rendering
  - Open3D 0.19.0+
  - For CPU fallback: set OPEN3D_CPU_RENDERING=true
"""
import io
import logging
import math
import os
import sys
import tempfile
import time

import numpy as np
from flask import Flask, Response, jsonify, request
from PIL import Image

import open3d as o3d
import open3d.visualization.rendering as rendering

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    stream=sys.stdout,
)
log = logging.getLogger("renderer")

app = Flask(__name__)

SUPPORTED_FORMATS = ["glb", "gltf", "obj", "stl", "ply", "off"]
VERSION = "2.0.0"


def _detect_renderer_type():
    """Probe Open3D to determine if EGL (GPU) or software rendering is active."""
    try:
        r = rendering.OffscreenRenderer(64, 64)
        r.render_to_image()
        del r
        if os.environ.get("OPEN3D_CPU_RENDERING", "").lower() in ("true", "1"):
            return "software"
        return "open3d-egl"
    except Exception as exc:
        log.warning("Renderer probe failed: %s", exc)
        return "unavailable"


RENDERER_TYPE = None  # set on startup


def load_geometry(filepath):
    """Load a 3D model. Tries TriangleMeshModel first (preserves PBR materials)."""
    try:
        model = o3d.io.read_triangle_model(filepath)
        if model and len(model.meshes) > 0:
            return ("model", model)
    except Exception:
        pass

    mesh = o3d.io.read_triangle_mesh(filepath, enable_post_processing=True)
    if mesh.is_empty():
        raise ValueError(f"Open3D could not load model from {filepath}")
    if not mesh.has_vertex_normals():
        mesh.compute_vertex_normals()
    if not mesh.has_vertex_colors():
        mesh.paint_uniform_color([0.7, 0.7, 0.7])
    return ("mesh", mesh)


def _create_grid_plane(center, extent):
    """Create a ground-plane grid (like 3D modelling apps) for spatial reference.

    Returns an Open3D LineSet with light silver lines spanning 2x the model
    footprint, centered below the object bounding box.
    """
    grid_half = max(extent[0], extent[2]) * 1.5  # 1.5× footprint on each side
    divisions = 10
    step = (grid_half * 2) / divisions
    y = center[1] - extent[1] / 2  # bottom of bounding box

    points = []
    lines = []
    idx = 0

    # Lines parallel to X axis
    for i in range(divisions + 1):
        z = center[2] - grid_half + i * step
        points.append([center[0] - grid_half, y, z])
        points.append([center[0] + grid_half, y, z])
        lines.append([idx, idx + 1])
        idx += 2

    # Lines parallel to Z axis
    for i in range(divisions + 1):
        x = center[0] - grid_half + i * step
        points.append([x, y, center[2] - grid_half])
        points.append([x, y, center[2] + grid_half])
        lines.append([idx, idx + 1])
        idx += 2

    grid = o3d.geometry.LineSet()
    grid.points = o3d.utility.Vector3dVector(points)
    grid.lines = o3d.utility.Vector2iVector(lines)
    # Muted grid lines — visible on the dark background but not distracting
    grid.colors = o3d.utility.Vector3dVector([[0.50, 0.50, 0.50]] * len(lines))
    return grid


def render_views(model_data, filename, num_views=6, width=512, height=512):
    """Render multi-view PNG images of a 3D model."""
    ext = os.path.splitext(filename)[1].lower() or ".glb"

    with tempfile.NamedTemporaryFile(suffix=ext, delete=False) as f:
        f.write(model_data)
        tmp_path = f.name

    try:
        kind, geometry = load_geometry(tmp_path)

        renderer = rendering.OffscreenRenderer(width, height)
        scene = renderer.scene

        # Medium-dark background for clear contrast with both light and dark
        # objects.  The previous near-white [0.94] was invisible for PBR
        # metallic surfaces whose reflections matched the background.
        scene.set_background([0.35, 0.37, 0.40, 1.0])
        scene.set_lighting(
            rendering.Open3DScene.LightingProfile.MED_SHADOWS,
            [0.577, -0.577, -0.577],
        )

        # Boost IBL (indirect/environment light) so metallic PBR surfaces
        # have something to reflect — without this, metals appear flat gray.
        try:
            scene.scene.set_indirect_light_intensity(45000)
        except Exception:
            pass  # older Open3D versions may not support this

        if kind == "model":
            scene.add_model("object", geometry)
        else:
            mat = rendering.MaterialRecord()
            mat.shader = "defaultLit"
            mat.base_color = [0.8, 0.8, 0.8, 1.0]
            scene.add_geometry("object", geometry, mat)

        bbox = scene.bounding_box
        center = bbox.get_center()
        extent = bbox.get_extent()
        max_dim = max(extent)

        # Safeguard: if bounding box is degenerate (zero/tiny), use a
        # fallback distance so the camera isn't placed inside the model.
        if max_dim < 1e-6:
            log.warning(
                "Degenerate bbox (max_dim=%.2e) for %s — using fallback",
                max_dim, filename,
            )
            max_dim = 1.0

        distance = max_dim * 2.5

        log.info(
            "Camera: center=[%.4f,%.4f,%.4f] extent=[%.4f,%.4f,%.4f] "
            "max_dim=%.4f dist=%.4f",
            center[0], center[1], center[2],
            extent[0], extent[1], extent[2],
            max_dim, distance,
        )

        # Add ground-plane grid for spatial reference
        grid = _create_grid_plane(center, extent)
        grid_mat = rendering.MaterialRecord()
        grid_mat.shader = "unlitLine"
        grid_mat.line_width = 1.0
        scene.add_geometry("grid", grid, grid_mat)

        views = []
        for i in range(num_views):
            angle_rad = (2 * math.pi * i) / num_views
            elevation_rad = math.pi / 6 if i % 2 == 0 else math.pi / 3
            angle_deg = (360.0 / num_views) * i
            elevation_label = "low_30deg" if i % 2 == 0 else "high_60deg"

            eye = [
                center[0] + distance * math.cos(angle_rad) * math.cos(elevation_rad),
                center[1] + distance * math.sin(elevation_rad),
                center[2] + distance * math.sin(angle_rad) * math.cos(elevation_rad),
            ]

            renderer.setup_camera(50.0, center, eye, [0, 1, 0])
            img = renderer.render_to_image()

            # Convert Open3D image → numpy → PIL → PNG bytes
            img_np = np.asarray(img)
            pil_img = Image.fromarray(img_np)
            buf = io.BytesIO()
            pil_img.save(buf, format="PNG")
            png_bytes = buf.getvalue()

            views.append(
                {
                    "index": i,
                    "angle_degrees": angle_deg,
                    "elevation": elevation_label,
                    "data": png_bytes,
                }
            )

        del renderer
        return views

    finally:
        os.unlink(tmp_path)


@app.route("/health")
def health():
    return jsonify(
        {
            "status": "healthy" if RENDERER_TYPE != "unavailable" else "degraded",
            "renderer": RENDERER_TYPE,
            "formats": SUPPORTED_FORMATS,
            "version": VERSION,
            "open3d_version": o3d.__version__,
        }
    )


@app.route("/render", methods=["POST"])
def render():
    start = time.time()

    if "model" not in request.files:
        return jsonify({"error": "model file is required"}), 400

    model_file = request.files["model"]
    model_data = model_file.read()
    if not model_data:
        return jsonify({"error": "model file is empty"}), 400

    filename = request.form.get("filename", model_file.filename or "model.glb")
    num_views = min(int(request.form.get("num_views", "6")), 15)

    log.info("Rendering %s with %d views", filename, num_views)

    try:
        views = render_views(model_data, filename, num_views)
    except Exception as exc:
        log.error("Render error: %s", exc, exc_info=True)
        return jsonify({"error": "Rendering failed", "message": str(exc)}), 500

    # Build multipart/mixed response (same format as legacy Three.js renderer)
    boundary = "----Open3dRenderBoundary"
    parts = []

    for view in views:
        header = (
            f"--{boundary}\r\n"
            f"Content-Type: image/png\r\n"
            f'Content-Disposition: attachment; '
            f'filename="view_{view["index"]:03d}.png"; '
            f'index="{view["index"]}"; '
            f'angle_degrees="{view["angle_degrees"]}"; '
            f'elevation="{view["elevation"]}"\r\n'
            f'Content-Length: {len(view["data"])}\r\n'
            f"\r\n"
        )
        parts.append(header.encode() + view["data"] + b"\r\n")

    parts.append(f"--{boundary}--\r\n".encode())

    body = b"".join(parts)
    duration_ms = int((time.time() - start) * 1000)

    log.info("Rendered %d views in %dms", len(views), duration_ms)

    return Response(
        body,
        mimetype=f"multipart/mixed; boundary={boundary}",
        headers={
            "X-Render-Views": str(len(views)),
            "X-Render-Success": "true",
            "X-Render-Duration-Ms": str(duration_ms),
        },
    )


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "8080"))
    RENDERER_TYPE = _detect_renderer_type()
    log.info("Open3D renderer starting on port %d", port)
    log.info("Open3D version: %s", o3d.__version__)
    log.info("Renderer backend: %s", RENDERER_TYPE)
    app.run(host="0.0.0.0", port=port, threaded=False)
