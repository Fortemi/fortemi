#!/usr/bin/env python3
"""
3D file processing script using trimesh.
Extracts geometric metadata and generates thumbnails.
"""

import json
import sys
import argparse
from pathlib import Path

try:
    import trimesh
    import numpy as np
except ImportError:
    print(json.dumps({"error": "trimesh not installed. Run: pip install trimesh"}))
    sys.exit(1)


def process_3d_file(filepath: str, output_thumbnail: str = None) -> dict:
    """Process a 3D file and extract metadata."""
    try:
        mesh = trimesh.load(filepath)
    except Exception as e:
        return {"error": f"Failed to load file: {e}"}

    # Handle scene vs mesh
    if isinstance(mesh, trimesh.Scene):
        if len(mesh.geometry) == 0:
            return {"error": "Empty scene"}
        # Merge all geometries
        mesh = trimesh.util.concatenate(list(mesh.geometry.values()))

    if not isinstance(mesh, trimesh.Trimesh):
        return {"error": f"Unsupported geometry type: {type(mesh).__name__}"}

    # Extract metadata
    bounds = mesh.bounds
    metadata = {
        "format": Path(filepath).suffix.lstrip('.').lower(),
        "vertex_count": len(mesh.vertices),
        "face_count": len(mesh.faces),
        "edge_count": len(mesh.edges_unique),
        "bounds_min": bounds[0].tolist(),
        "bounds_max": bounds[1].tolist(),
        "is_watertight": mesh.is_watertight,
        "is_manifold": mesh.is_volume if hasattr(mesh, 'is_volume') else None,
    }

    # Compute volume and surface area if watertight
    if mesh.is_watertight:
        metadata["volume"] = float(mesh.volume)
        metadata["surface_area"] = float(mesh.area)
    else:
        metadata["surface_area"] = float(mesh.area)

    # Check for materials/textures
    if hasattr(mesh, 'visual'):
        if hasattr(mesh.visual, 'material'):
            metadata["has_materials"] = True
        if hasattr(mesh.visual, 'uv'):
            metadata["has_uv_mapping"] = mesh.visual.uv is not None
        if hasattr(mesh.visual, 'vertex_colors'):
            metadata["has_vertex_colors"] = mesh.visual.vertex_colors is not None

    # Generate thumbnail if requested
    if output_thumbnail:
        try:
            # Use trimesh's built-in scene rendering
            scene = trimesh.Scene(mesh)
            png = scene.save_image(resolution=[512, 512])
            if png is not None:
                with open(output_thumbnail, 'wb') as f:
                    f.write(png)
                metadata["thumbnail_generated"] = True
        except Exception as e:
            metadata["thumbnail_error"] = str(e)

    return metadata


def main():
    parser = argparse.ArgumentParser(description='Process 3D file and extract metadata')
    parser.add_argument('filepath', help='Path to 3D file')
    parser.add_argument('--thumbnail', help='Output path for thumbnail PNG')
    parser.add_argument('--json', action='store_true', help='Output as JSON')

    args = parser.parse_args()

    result = process_3d_file(args.filepath, args.thumbnail)

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        for key, value in result.items():
            print(f"{key}: {value}")


if __name__ == '__main__':
    main()
