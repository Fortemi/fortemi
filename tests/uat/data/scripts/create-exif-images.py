#!/usr/bin/env python3
"""
Generate test images with EXIF metadata for UAT testing.

Creates images with GPS coordinates, timestamps, and camera information
to test EXIF extraction and W3C PROV provenance tracking.
"""

import os
import sys
from datetime import datetime
from pathlib import Path
from typing import Optional, Tuple

try:
    from PIL import Image, ImageDraw, ImageFont
    import piexif
except ImportError as e:
    print(f"Error: Missing required package: {e}")
    print("Install with: pip3 install Pillow piexif")
    sys.exit(1)


class ExifImageGenerator:
    """Generate test images with EXIF metadata."""

    def __init__(self, output_dir: Path):
        self.output_dir = output_dir
        self.output_dir.mkdir(parents=True, exist_ok=True)

    def create_image(
        self,
        filename: str,
        size: Tuple[int, int] = (1920, 1080),
        text: str = "Test Image",
        color: str = "#4A90E2"
    ) -> Path:
        """Create a simple colored image with text."""
        img = Image.new('RGB', size, color)
        draw = ImageDraw.Draw(img)

        # Try to use a nice font, fallback to default
        try:
            font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 60)
        except OSError:
            try:
                font = ImageFont.truetype("/System/Library/Fonts/Helvetica.ttc", 60)
            except OSError:
                font = ImageFont.load_default()

        # Calculate text position (centered)
        bbox = draw.textbbox((0, 0), text, font=font)
        text_width = bbox[2] - bbox[0]
        text_height = bbox[3] - bbox[1]
        position = ((size[0] - text_width) // 2, (size[1] - text_height) // 2)

        # Draw text
        draw.text(position, text, fill="white", font=font)

        # Save
        output_path = self.output_dir / filename
        img.save(output_path, "JPEG", quality=85)
        return output_path

    def add_exif(
        self,
        image_path: Path,
        gps: Optional[Tuple[float, float, float]] = None,  # lat, lon, alt
        datetime_str: Optional[str] = None,
        make: Optional[str] = None,
        model: Optional[str] = None,
        software: Optional[str] = None
    ):
        """Add EXIF metadata to an image."""
        exif_dict = {"0th": {}, "Exif": {}, "GPS": {}, "1st": {}, "thumbnail": None}

        # Camera info
        if make:
            exif_dict["0th"][piexif.ImageIFD.Make] = make.encode('utf-8')
        if model:
            exif_dict["0th"][piexif.ImageIFD.Model] = model.encode('utf-8')
        if software:
            exif_dict["0th"][piexif.ImageIFD.Software] = software.encode('utf-8')

        # Datetime
        if datetime_str:
            # EXIF datetime format: "YYYY:MM:DD HH:MM:SS"
            dt = datetime.fromisoformat(datetime_str.replace('Z', '+00:00'))
            exif_datetime = dt.strftime("%Y:%m:%d %H:%M:%S")
            exif_dict["Exif"][piexif.ExifIFD.DateTimeOriginal] = exif_datetime.encode('utf-8')
            exif_dict["Exif"][piexif.ExifIFD.DateTimeDigitized] = exif_datetime.encode('utf-8')
            exif_dict["0th"][piexif.ImageIFD.DateTime] = exif_datetime.encode('utf-8')

        # GPS
        if gps:
            lat, lon, alt = gps

            # Convert decimal degrees to degrees, minutes, seconds
            def decimal_to_dms(decimal: float) -> Tuple[Tuple[int, int], Tuple[int, int], Tuple[int, int]]:
                """Convert decimal degrees to (degrees, minutes, seconds) as rationals."""
                abs_decimal = abs(decimal)
                degrees = int(abs_decimal)
                minutes_decimal = (abs_decimal - degrees) * 60
                minutes = int(minutes_decimal)
                seconds_decimal = (minutes_decimal - minutes) * 60
                seconds = int(seconds_decimal * 1000)  # Store with 3 decimal places

                return (
                    (degrees, 1),
                    (minutes, 1),
                    (seconds, 1000)
                )

            lat_dms = decimal_to_dms(lat)
            lon_dms = decimal_to_dms(lon)

            exif_dict["GPS"][piexif.GPSIFD.GPSLatitude] = lat_dms
            exif_dict["GPS"][piexif.GPSIFD.GPSLatitudeRef] = b'N' if lat >= 0 else b'S'
            exif_dict["GPS"][piexif.GPSIFD.GPSLongitude] = lon_dms
            exif_dict["GPS"][piexif.GPSIFD.GPSLongitudeRef] = b'E' if lon >= 0 else b'W'

            if alt is not None:
                # Altitude as rational
                alt_int = int(abs(alt) * 100)  # 2 decimal places
                exif_dict["GPS"][piexif.GPSIFD.GPSAltitude] = (alt_int, 100)
                exif_dict["GPS"][piexif.GPSIFD.GPSAltitudeRef] = 0 if alt >= 0 else 1

        # Write EXIF
        exif_bytes = piexif.dump(exif_dict)
        piexif.insert(exif_bytes, str(image_path))

    def strip_exif(self, image_path: Path):
        """Remove all EXIF metadata from an image."""
        img = Image.open(image_path)
        # Remove EXIF by saving without it
        data = list(img.getdata())
        image_no_exif = Image.new(img.mode, img.size)
        image_no_exif.putdata(data)
        image_no_exif.save(image_path, "JPEG", quality=85)


def main():
    script_dir = Path(__file__).parent
    data_dir = script_dir.parent
    images_dir = data_dir / "images"
    provenance_dir = data_dir / "provenance"

    gen = ExifImageGenerator(images_dir)
    prov_gen = ExifImageGenerator(provenance_dir)

    print("Generating test images with EXIF metadata...")

    # 1. JPEG with full EXIF metadata
    print("  Creating jpeg-with-exif.jpg...")
    img_path = gen.create_image(
        "jpeg-with-exif.jpg",
        size=(4032, 3024),
        text="Paris 2024",
        color="#E74C3C"
    )
    gen.add_exif(
        img_path,
        gps=(48.8584, 2.2945, 35.0),  # Eiffel Tower
        datetime_str="2024-06-15T14:30:00Z",
        make="Apple",
        model="iPhone 15 Pro",
        software="iOS 17.5"
    )

    # 2. JPEG without metadata
    print("  Creating jpeg-no-metadata.jpg...")
    img_path = gen.create_image(
        "jpeg-no-metadata.jpg",
        size=(1920, 1080),
        text="No Metadata",
        color="#95A5A6"
    )
    gen.strip_exif(img_path)

    # 3. PNG with transparency (no EXIF support)
    print("  Creating png-transparent.png...")
    png_path = images_dir / "png-transparent.png"
    img = Image.new('RGBA', (512, 512), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    draw.ellipse([128, 128, 384, 384], fill=(74, 144, 226, 255))
    img.save(png_path, "PNG")

    # 4. WebP modern format
    print("  Creating webp-modern.webp...")
    webp_path = images_dir / "webp-modern.webp"
    img = Image.new('RGB', (1920, 1080), "#3498DB")
    draw = ImageDraw.Draw(img)
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 80)
    except OSError:
        font = ImageFont.load_default()
    draw.text((600, 480), "WebP Format", fill="white", font=font)
    img.save(webp_path, "WEBP", quality=85)

    # 5. Faces group photo placeholder (would need real photo or AI generation)
    print("  Creating faces-group-photo.jpg (placeholder)...")
    img_path = gen.create_image(
        "faces-group-photo.jpg",
        size=(2048, 1536),
        text="Group Photo\n(Use real photo or download)",
        color="#E67E22"
    )

    # 6. Object scene placeholder
    print("  Creating object-scene.jpg (placeholder)...")
    img_path = gen.create_image(
        "object-scene.jpg",
        size=(1920, 1080),
        text="Workspace Scene\n(Use real photo or download)",
        color="#16A085"
    )

    # 7. Unicode filename with emoji
    print("  Creating emoji-unicode-名前.jpg...")
    img_path = gen.create_image(
        "emoji-unicode-名前.jpg",
        size=(1024, 768),
        text="Unicode 🎨 名前",
        color="#9B59B6"
    )

    # Provenance images
    print("")
    print("Generating provenance test images...")

    # Paris - Eiffel Tower
    print("  Creating paris-eiffel-tower.jpg...")
    img_path = prov_gen.create_image(
        "paris-eiffel-tower.jpg",
        size=(3840, 2160),
        text="Paris 🗼",
        color="#E74C3C"
    )
    prov_gen.add_exif(
        img_path,
        gps=(48.8584, 2.2945, 35.0),
        datetime_str="2024-07-14T12:00:00Z",
        make="Canon",
        model="EOS R5"
    )

    # New York - Statue of Liberty
    print("  Creating newyork-statue-liberty.jpg...")
    img_path = prov_gen.create_image(
        "newyork-statue-liberty.jpg",
        size=(4096, 2732),
        text="New York 🗽",
        color="#3498DB"
    )
    prov_gen.add_exif(
        img_path,
        gps=(40.6892, -74.0445, 10.0),
        datetime_str="2024-07-04T16:30:00Z",
        make="Nikon",
        model="Z9"
    )

    # Tokyo - Shibuya
    print("  Creating tokyo-shibuya.jpg...")
    img_path = prov_gen.create_image(
        "tokyo-shibuya.jpg",
        size=(4320, 2880),
        text="Tokyo 🏙️",
        color="#E67E22"
    )
    prov_gen.add_exif(
        img_path,
        gps=(35.6595, 139.7004, 30.0),
        datetime_str="2024-03-21T09:00:00Z",
        make="Sony",
        model="α7R V"
    )

    # Historical date
    print("  Creating dated-2020-01-01.jpg...")
    img_path = prov_gen.create_image(
        "dated-2020-01-01.jpg",
        size=(3024, 4032),
        text="2020-01-01",
        color="#1ABC9C"
    )
    prov_gen.add_exif(
        img_path,
        datetime_str="2020-01-01T00:00:00Z",
        make="Apple",
        model="iPhone 11"
    )

    # Future date
    print("  Creating dated-2025-12-31.jpg...")
    img_path = prov_gen.create_image(
        "dated-2025-12-31.jpg",
        size=(4080, 3072),
        text="2025-12-31",
        color="#9B59B6"
    )
    prov_gen.add_exif(
        img_path,
        datetime_str="2025-12-31T23:59:59Z",
        make="Google",
        model="Pixel 9 Pro"
    )

    # Duplicate content test files
    print("  Creating duplicate-content-1.txt...")
    duplicate_content = """This is duplicate content for testing content-based deduplication.

The hash of this content should match duplicate-content-2.txt exactly.

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""
    (provenance_dir / "duplicate-content-1.txt").write_text(duplicate_content)

    print("  Creating duplicate-content-2.txt...")
    (provenance_dir / "duplicate-content-2.txt").write_text(duplicate_content)

    print("")
    print("✓ Image generation complete!")
    print(f"  Images: {len(list(images_dir.glob('*')))}")
    print(f"  Provenance: {len(list(provenance_dir.glob('*')))}")


if __name__ == "__main__":
    main()
