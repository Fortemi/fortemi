# ADR-035: Structured and Symbolic Media Format Support

**Status:** Implemented
**Date:** 2026-02-02
**Deciders:** Architecture team
**Related:** ADR-031 (Intelligent Attachment Processing), ADR-034 (3D File Analysis), Epic #430

## Context

Beyond traditional binary media formats (JPEG, MP3, MP4), many file types contain structured or symbolic data that can be analyzed, rendered, and searched semantically:

- **Vector graphics** (SVG): XML-based, scalable, searchable text content
- **Music notation** (MIDI, MusicXML): Symbolic musical data, not audio waveforms
- **Diagrams** (Mermaid, PlantUML, Graphviz): Text-based visual representations
- **Scientific formats** (LaTeX, ChemDraw, SMILES): Domain-specific notation
- **CAD drawings** (DXF, DWG): 2D technical drawings
- **Geospatial** (GeoJSON, KML, GPX): Location data with semantic meaning

These formats share common characteristics:
1. **Text-based or structured**: Content is parseable, not just pixels/samples
2. **Semantic richness**: Structure carries meaning beyond visual appearance
3. **Transformable**: Can be rendered to images, converted between formats
4. **Searchable**: Internal text/structure can be indexed

## Decision

Implement **specialized extraction strategies** for structured media formats, treating them as first-class content types with format-aware analysis.

### 1. Supported Formats by Category

#### Vector Graphics

| Format | Extension | MIME | Strategy | Analysis |
|--------|-----------|------|----------|----------|
| SVG | .svg | image/svg+xml | `vector_svg` | Text extraction, element analysis, rasterization |
| EPS | .eps | application/postscript | `vector_eps` | Ghostscript conversion to SVG/PNG |
| PDF (vector) | .pdf | application/pdf | `pdf_vector` | Detect vector content, extract paths |
| AI | .ai | application/illustrator | `vector_ai` | Convert via Inkscape |

#### Music & Audio Notation

| Format | Extension | MIME | Strategy | Analysis |
|--------|-----------|------|----------|----------|
| MIDI | .mid, .midi | audio/midi | `music_midi` | Note extraction, tempo, instruments, structure |
| MusicXML | .musicxml, .mxl | application/vnd.recordare.musicxml | `music_notation` | Full score analysis |
| ABC | .abc | text/vnd.abc | `music_notation` | Folk music notation |
| LilyPond | .ly | text/x-lilypond | `music_notation` | Engraving notation |
| Guitar Pro | .gp, .gp5 | application/x-guitarpro | `music_tabs` | Tablature extraction |

#### Tracker Modules (Demoscene/Chiptune)

Classic tracker formats from the Amiga/DOS era - these contain both sample data AND pattern sequences, making them uniquely analyzable.

| Format | Extension | MIME | Strategy | Analysis |
|--------|-----------|------|----------|----------|
| MOD | .mod | audio/x-mod | `music_tracker` | ProTracker/NoiseTracker - 4 channels, Amiga classic |
| S3M | .s3m | audio/x-s3m | `music_tracker` | Scream Tracker 3 - 16+ channels, adlib support |
| XM | .xm | audio/x-xm | `music_tracker` | FastTracker 2 - envelopes, extended samples |
| IT | .it | audio/x-it | `music_tracker` | Impulse Tracker - NNA, filters, most advanced |
| MTM | .mtm | audio/x-mtm | `music_tracker` | MultiTracker Module |
| 669 | .669 | audio/x-669 | `music_tracker` | Composer 669/UNIS 669 |
| MED | .med, .mmd0-3 | audio/x-med | `music_tracker` | OctaMED - Amiga 8 channels |
| OKT | .okt | audio/x-okt | `music_tracker` | Oktalyzer - Amiga 8 channels |
| STM | .stm | audio/x-stm | `music_tracker` | Scream Tracker 2 |
| ULT | .ult | audio/x-ult | `music_tracker` | Ultra Tracker |
| FAR | .far | audio/x-far | `music_tracker` | Farandole Composer |
| AHX | .ahx | audio/x-ahx | `music_tracker` | Abyss' Highest eXperience (chiptune) |
| HVL | .hvl | audio/x-hvl | `music_tracker` | HivelyTracker (AHX successor) |

#### Diagrams & Graphs

| Format | Extension | MIME | Strategy | Analysis |
|--------|-----------|------|----------|----------|
| Mermaid | .mmd | text/x-mermaid | `diagram_text` | Parse diagram type, nodes, edges |
| PlantUML | .puml | text/x-plantuml | `diagram_text` | UML structure extraction |
| Graphviz | .dot, .gv | text/vnd.graphviz | `diagram_text` | Graph structure, node labels |
| Draw.io | .drawio | application/x-drawio | `diagram_xml` | XML extraction, element labels |
| Excalidraw | .excalidraw | application/json | `diagram_json` | Element text, structure |

#### Scientific & Technical

| Format | Extension | MIME | Strategy | Analysis |
|--------|-----------|------|----------|----------|
| LaTeX | .tex | text/x-latex | `scientific_latex` | Document structure, equations |
| SMILES | .smi | chemical/x-daylight-smiles | `chemistry_smiles` | Molecule structure |
| MOL | .mol | chemical/x-mdl-molfile | `chemistry_mol` | Molecular data |
| CIF | .cif | chemical/x-cif | `chemistry_crystal` | Crystallographic data |

#### Geospatial

| Format | Extension | MIME | Strategy | Analysis |
|--------|-----------|------|----------|----------|
| GeoJSON | .geojson | application/geo+json | `geospatial_json` | Features, coordinates, properties |
| KML/KMZ | .kml, .kmz | application/vnd.google-earth.kml+xml | `geospatial_kml` | Placemarks, paths, polygons |
| GPX | .gpx | application/gpx+xml | `geospatial_gpx` | Tracks, waypoints, routes |
| Shapefile | .shp | application/x-esri-shapefile | `geospatial_shape` | GIS features |

#### CAD 2D

| Format | Extension | MIME | Strategy | Analysis |
|--------|-----------|------|----------|----------|
| DXF | .dxf | image/vnd.dxf | `cad_2d` | Layers, entities, dimensions |
| DWG | .dwg | image/vnd.dwg | `cad_2d` | AutoCAD native (via ODA) |

### 2. SVG Processing

SVG is particularly valuable because it's XML-based with searchable text.

```python
#!/usr/bin/env python3
"""SVG processing for matric-memory."""

import sys
import json
from pathlib import Path
from xml.etree import ElementTree as ET
import cairosvg  # For PNG rendering

def process_svg(input_path: str, output_dir: str) -> dict:
    """Extract metadata and render SVG."""
    path = Path(input_path)
    out = Path(output_dir)

    # Parse SVG
    tree = ET.parse(input_path)
    root = tree.getroot()

    # Namespace handling
    ns = {'svg': 'http://www.w3.org/2000/svg'}

    metadata = {
        'format': 'svg',
        'width': root.get('width'),
        'height': root.get('height'),
        'viewBox': root.get('viewBox'),
    }

    # Extract all text content
    texts = []
    for text_elem in root.iter('{http://www.w3.org/2000/svg}text'):
        text_content = ''.join(text_elem.itertext()).strip()
        if text_content:
            texts.append(text_content)

    for tspan in root.iter('{http://www.w3.org/2000/svg}tspan'):
        text_content = ''.join(tspan.itertext()).strip()
        if text_content and text_content not in texts:
            texts.append(text_content)

    metadata['text_content'] = texts
    metadata['text_combined'] = ' '.join(texts)

    # Count elements
    elements = {
        'paths': len(list(root.iter('{http://www.w3.org/2000/svg}path'))),
        'rects': len(list(root.iter('{http://www.w3.org/2000/svg}rect'))),
        'circles': len(list(root.iter('{http://www.w3.org/2000/svg}circle'))),
        'texts': len(texts),
        'groups': len(list(root.iter('{http://www.w3.org/2000/svg}g'))),
        'images': len(list(root.iter('{http://www.w3.org/2000/svg}image'))),
    }
    metadata['elements'] = elements
    metadata['total_elements'] = sum(elements.values())

    # Extract title and description
    title = root.find('{http://www.w3.org/2000/svg}title')
    desc = root.find('{http://www.w3.org/2000/svg}desc')
    metadata['title'] = title.text if title is not None else None
    metadata['description'] = desc.text if desc is not None else None

    # Render to PNG for thumbnail
    thumbnail_path = out / f"{path.stem}_thumb.png"
    try:
        cairosvg.svg2png(
            url=str(path),
            write_to=str(thumbnail_path),
            output_width=512,
            output_height=512
        )
        metadata['thumbnail_path'] = str(thumbnail_path)
    except Exception as e:
        metadata['thumbnail_error'] = str(e)

    return metadata

if __name__ == '__main__':
    result = process_svg(sys.argv[1], sys.argv[2])
    print(json.dumps(result, indent=2))
```

### 3. MIDI Processing

MIDI contains symbolic music data that can be analyzed structurally.

```python
#!/usr/bin/env python3
"""MIDI processing for matric-memory."""

import sys
import json
from pathlib import Path
import mido  # pip install mido

def process_midi(input_path: str, output_dir: str) -> dict:
    """Extract MIDI metadata and structure."""
    path = Path(input_path)

    midi = mido.MidiFile(input_path)

    metadata = {
        'format': 'midi',
        'type': midi.type,  # 0, 1, or 2
        'ticks_per_beat': midi.ticks_per_beat,
        'length_seconds': midi.length,
        'track_count': len(midi.tracks),
    }

    # Analyze tracks
    tracks = []
    all_instruments = set()
    all_notes = []
    tempo = 500000  # Default: 120 BPM

    for i, track in enumerate(midi.tracks):
        track_info = {
            'index': i,
            'name': track.name,
            'message_count': len(track),
        }

        notes_in_track = 0
        instruments = set()

        for msg in track:
            if msg.type == 'set_tempo':
                tempo = msg.tempo
            elif msg.type == 'program_change':
                instruments.add(msg.program)
                all_instruments.add(msg.program)
            elif msg.type == 'note_on' and msg.velocity > 0:
                notes_in_track += 1
                all_notes.append(msg.note)

        track_info['note_count'] = notes_in_track
        track_info['instruments'] = list(instruments)
        tracks.append(track_info)

    metadata['tracks'] = tracks
    metadata['tempo_bpm'] = round(60000000 / tempo, 2)
    metadata['total_notes'] = len(all_notes)

    # Analyze pitch range
    if all_notes:
        metadata['pitch_range'] = {
            'lowest': min(all_notes),
            'highest': max(all_notes),
            'lowest_name': note_name(min(all_notes)),
            'highest_name': note_name(max(all_notes)),
        }

    # Map instruments to names
    metadata['instruments'] = [
        {'program': p, 'name': GM_INSTRUMENTS.get(p, 'Unknown')}
        for p in sorted(all_instruments)
    ]

    # Estimate time signature from message patterns (simplified)
    metadata['estimated_key'] = estimate_key(all_notes) if all_notes else None

    return metadata

def note_name(midi_note: int) -> str:
    """Convert MIDI note number to name (e.g., 60 -> C4)."""
    notes = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']
    octave = (midi_note // 12) - 1
    note = notes[midi_note % 12]
    return f"{note}{octave}"

def estimate_key(notes: list) -> str:
    """Simple key estimation from note distribution."""
    # Count pitch classes
    pitch_classes = [0] * 12
    for n in notes:
        pitch_classes[n % 12] += 1

    # Find most common (likely tonic)
    tonic = pitch_classes.index(max(pitch_classes))
    notes_names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B']
    return f"{notes_names[tonic]} (estimated)"

# General MIDI instrument names (partial)
GM_INSTRUMENTS = {
    0: 'Acoustic Grand Piano', 1: 'Bright Acoustic Piano',
    24: 'Acoustic Guitar (nylon)', 25: 'Acoustic Guitar (steel)',
    32: 'Acoustic Bass', 33: 'Electric Bass (finger)',
    40: 'Violin', 41: 'Viola', 42: 'Cello',
    56: 'Trumpet', 57: 'Trombone', 65: 'Alto Sax',
    # ... full list in implementation
}

if __name__ == '__main__':
    result = process_midi(sys.argv[1], sys.argv[2])
    print(json.dumps(result, indent=2))
```

### 4. Tracker Module Processing (MOD/S3M/XM/IT)

Tracker modules are uniquely rich - they contain both the composition (patterns, sequences) AND the instrument samples, making deep analysis possible.

```python
#!/usr/bin/env python3
"""Tracker module processing for matric-memory.

Supports MOD, S3M, XM, IT, and related demoscene formats.
Uses libopenmpt via python-openmpt for comprehensive parsing.
"""

import sys
import json
from pathlib import Path
import struct

# Try python-openmpt first (best support), fall back to basic parsing
try:
    import openmpt
    HAS_OPENMPT = True
except ImportError:
    HAS_OPENMPT = False

def process_tracker(input_path: str, output_dir: str) -> dict:
    """Extract tracker module metadata and render preview."""
    path = Path(input_path)
    out = Path(output_dir)
    ext = path.suffix.lower()

    metadata = {
        'format': ext[1:],  # 'mod', 's3m', 'xm', 'it'
        'format_category': 'tracker',
        'demoscene_era': get_era(ext),
    }

    if HAS_OPENMPT:
        metadata.update(process_with_openmpt(input_path))
    else:
        metadata.update(process_basic(input_path, ext))

    # Render audio preview (first 30 seconds)
    if HAS_OPENMPT:
        try:
            preview_path = out / f"{path.stem}_preview.wav"
            render_preview(input_path, str(preview_path), duration_sec=30)
            metadata['audio_preview_path'] = str(preview_path)
        except Exception as e:
            metadata['preview_error'] = str(e)

    return metadata

def process_with_openmpt(input_path: str) -> dict:
    """Full analysis using libopenmpt."""
    with open(input_path, 'rb') as f:
        mod = openmpt.module(f.read())

    return {
        'title': mod.get_metadata('title') or None,
        'artist': mod.get_metadata('artist') or None,
        'tracker': mod.get_metadata('tracker') or None,
        'message': mod.get_metadata('message') or None,

        # Structure
        'duration_seconds': mod.duration_seconds,
        'channel_count': mod.get_num_channels(),
        'pattern_count': mod.get_num_patterns(),
        'order_count': mod.get_num_orders(),
        'instrument_count': mod.get_num_instruments(),
        'sample_count': mod.get_num_samples(),

        # Musical analysis
        'tempo_bpm': estimate_tracker_tempo(mod),
        'speed': mod.get_current_speed(),

        # Instrument/sample names
        'instrument_names': [
            mod.get_instrument_name(i)
            for i in range(mod.get_num_instruments())
            if mod.get_instrument_name(i)
        ],
        'sample_names': [
            mod.get_sample_name(i)
            for i in range(mod.get_num_samples())
            if mod.get_sample_name(i)
        ],

        # Subsongs (for multi-song modules)
        'subsong_count': mod.get_num_subsongs(),
    }

def process_basic(input_path: str, ext: str) -> dict:
    """Basic parsing without libopenmpt - header only."""
    with open(input_path, 'rb') as f:
        data = f.read()

    if ext == '.mod':
        return parse_mod_header(data)
    elif ext == '.s3m':
        return parse_s3m_header(data)
    elif ext == '.xm':
        return parse_xm_header(data)
    elif ext == '.it':
        return parse_it_header(data)
    else:
        return {'parse_method': 'unknown'}

def parse_mod_header(data: bytes) -> dict:
    """Parse ProTracker MOD header."""
    # Title at offset 0, 20 bytes
    title = data[0:20].decode('latin-1', errors='replace').rstrip('\x00').strip()

    # Detect format from magic at offset 1080
    magic = data[1080:1084].decode('latin-1', errors='replace')
    channel_count = {
        'M.K.': 4, 'M!K!': 4, 'FLT4': 4, 'FLT8': 8,
        '4CHN': 4, '6CHN': 6, '8CHN': 8,
    }.get(magic, 4)

    # Sample count (31 for most MODs)
    sample_count = 31 if len(data) > 1084 else 15

    # Sample names at offset 20, 30 bytes each
    sample_names = []
    for i in range(sample_count):
        offset = 20 + (i * 30)
        name = data[offset:offset+22].decode('latin-1', errors='replace').rstrip('\x00').strip()
        if name:
            sample_names.append(name)

    return {
        'title': title or None,
        'channel_count': channel_count,
        'sample_count': sample_count,
        'sample_names': sample_names,
        'format_variant': magic,
        'parse_method': 'basic_header',
    }

def parse_s3m_header(data: bytes) -> dict:
    """Parse Scream Tracker 3 header."""
    title = data[0:28].decode('latin-1', errors='replace').rstrip('\x00').strip()
    order_count = struct.unpack('<H', data[32:34])[0]
    instrument_count = struct.unpack('<H', data[34:36])[0]
    pattern_count = struct.unpack('<H', data[36:38])[0]
    flags = struct.unpack('<H', data[38:40])[0]
    tracker_version = struct.unpack('<H', data[40:42])[0]

    # Global volume, initial speed, initial tempo
    global_volume = data[48]
    initial_speed = data[49]
    initial_tempo = data[50]

    return {
        'title': title or None,
        'order_count': order_count,
        'instrument_count': instrument_count,
        'pattern_count': pattern_count,
        'global_volume': global_volume,
        'speed': initial_speed,
        'tempo_bpm': initial_tempo,
        'tracker_version': f"{tracker_version >> 8}.{tracker_version & 0xFF:02d}",
        'parse_method': 'basic_header',
    }

def parse_xm_header(data: bytes) -> dict:
    """Parse FastTracker 2 XM header."""
    # Magic: "Extended Module: "
    if data[0:17] != b'Extended Module: ':
        return {'error': 'Not a valid XM file'}

    title = data[17:37].decode('latin-1', errors='replace').rstrip('\x00').strip()
    tracker_name = data[38:58].decode('latin-1', errors='replace').rstrip('\x00').strip()

    # Header size at offset 60 (4 bytes)
    header_size = struct.unpack('<I', data[60:64])[0]
    song_length = struct.unpack('<H', data[64:66])[0]
    restart_pos = struct.unpack('<H', data[66:68])[0]
    channel_count = struct.unpack('<H', data[68:70])[0]
    pattern_count = struct.unpack('<H', data[70:72])[0]
    instrument_count = struct.unpack('<H', data[72:74])[0]
    flags = struct.unpack('<H', data[74:76])[0]
    default_tempo = struct.unpack('<H', data[76:78])[0]
    default_bpm = struct.unpack('<H', data[78:80])[0]

    return {
        'title': title or None,
        'tracker': tracker_name,
        'channel_count': channel_count,
        'pattern_count': pattern_count,
        'instrument_count': instrument_count,
        'order_count': song_length,
        'speed': default_tempo,
        'tempo_bpm': default_bpm,
        'linear_frequency': bool(flags & 1),
        'parse_method': 'basic_header',
    }

def parse_it_header(data: bytes) -> dict:
    """Parse Impulse Tracker header."""
    # Magic: "IMPM"
    if data[0:4] != b'IMPM':
        return {'error': 'Not a valid IT file'}

    title = data[4:30].decode('latin-1', errors='replace').rstrip('\x00').strip()
    order_count = struct.unpack('<H', data[32:34])[0]
    instrument_count = struct.unpack('<H', data[34:36])[0]
    sample_count = struct.unpack('<H', data[36:38])[0]
    pattern_count = struct.unpack('<H', data[38:40])[0]

    # Tracker version
    cwt_v = struct.unpack('<H', data[40:42])[0]
    cmwt_v = struct.unpack('<H', data[42:44])[0]

    # Flags and special
    flags = struct.unpack('<H', data[44:46])[0]

    # Initial tempo/speed
    global_volume = data[48]
    mix_volume = data[49]
    initial_speed = data[50]
    initial_tempo = data[51]

    return {
        'title': title or None,
        'order_count': order_count,
        'instrument_count': instrument_count,
        'sample_count': sample_count,
        'pattern_count': pattern_count,
        'global_volume': global_volume,
        'speed': initial_speed,
        'tempo_bpm': initial_tempo,
        'stereo': bool(flags & 1),
        'use_instruments': bool(flags & 4),
        'linear_slides': bool(flags & 8),
        'tracker_version': f"{cwt_v >> 8}.{cwt_v & 0xFF:02x}",
        'parse_method': 'basic_header',
    }

def get_era(ext: str) -> str:
    """Classify tracker format by demoscene era."""
    eras = {
        '.mod': 'Amiga (1987-1995)',
        '.s3m': 'DOS (1994-1998)',
        '.xm': 'DOS/Win (1995-2000)',
        '.it': 'DOS/Win (1996-2002)',
        '.mtm': 'DOS (1992-1995)',
        '.669': 'DOS (1992-1994)',
        '.med': 'Amiga (1989-1996)',
        '.ahx': 'Amiga Chiptune (1998+)',
        '.hvl': 'Cross-platform Chiptune (2007+)',
    }
    return eras.get(ext, 'Unknown')

def estimate_tracker_tempo(mod) -> float:
    """Estimate BPM from tracker speed/tempo values."""
    # Tracker tempo formula: BPM = tempo * 2 / 5 * speed
    # This is approximate - actual timing is complex
    return mod.get_current_tempo()

def render_preview(input_path: str, output_path: str, duration_sec: int = 30):
    """Render audio preview using libopenmpt."""
    import wave
    import array

    with open(input_path, 'rb') as f:
        mod = openmpt.module(f.read())

    # Render at 44100 Hz stereo
    sample_rate = 44100
    samples_needed = sample_rate * duration_sec * 2  # stereo

    audio_data = array.array('h')
    while len(audio_data) < samples_needed:
        frames = mod.read_interleaved_stereo(sample_rate, 1024)
        if not frames:
            break
        audio_data.extend(frames)

    # Write WAV
    with wave.open(output_path, 'wb') as wav:
        wav.setnchannels(2)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(audio_data.tobytes())

# Tracker software identification
TRACKER_SIGNATURES = {
    'M.K.': 'ProTracker',
    'M!K!': 'ProTracker (>64 patterns)',
    'FLT4': 'StarTrekker 4ch',
    'FLT8': 'StarTrekker 8ch',
    'SCRM': 'Scream Tracker 3',
    'IMPM': 'Impulse Tracker',
}

if __name__ == '__main__':
    result = process_tracker(sys.argv[1], sys.argv[2])
    print(json.dumps(result, indent=2))
```

### 5. Diagram Processing (Mermaid/PlantUML)

```python
#!/usr/bin/env python3
"""Diagram text format processing."""

import sys
import json
import re
from pathlib import Path
import subprocess

def process_mermaid(input_path: str, output_dir: str) -> dict:
    """Process Mermaid diagram."""
    path = Path(input_path)
    out = Path(output_dir)

    content = path.read_text()

    metadata = {
        'format': 'mermaid',
        'raw_content': content,
    }

    # Detect diagram type
    first_line = content.strip().split('\n')[0].lower()
    if 'graph' in first_line or 'flowchart' in first_line:
        metadata['diagram_type'] = 'flowchart'
    elif 'sequencediagram' in first_line or 'sequence' in first_line:
        metadata['diagram_type'] = 'sequence'
    elif 'classDiagram' in first_line or 'class' in first_line:
        metadata['diagram_type'] = 'class'
    elif 'erDiagram' in first_line or 'er' in first_line:
        metadata['diagram_type'] = 'er'
    elif 'gantt' in first_line:
        metadata['diagram_type'] = 'gantt'
    elif 'pie' in first_line:
        metadata['diagram_type'] = 'pie'
    elif 'stateDiagram' in first_line:
        metadata['diagram_type'] = 'state'
    else:
        metadata['diagram_type'] = 'unknown'

    # Extract node labels and text
    # Pattern for node definitions: A[Label] or A(Label) or A{Label}
    node_pattern = r'(\w+)\s*[\[\(\{]([^\]\)\}]+)[\]\)\}]'
    nodes = re.findall(node_pattern, content)
    metadata['nodes'] = [{'id': n[0], 'label': n[1]} for n in nodes]

    # Extract all text in brackets/parentheses
    all_text = re.findall(r'[\[\(\{]([^\]\)\}]+)[\]\)\}]', content)
    metadata['text_content'] = list(set(all_text))
    metadata['text_combined'] = ' '.join(set(all_text))

    # Render to PNG using mermaid-cli
    thumbnail_path = out / f"{path.stem}_thumb.png"
    try:
        subprocess.run([
            'mmdc',  # mermaid-cli
            '-i', str(path),
            '-o', str(thumbnail_path),
            '-w', '512',
            '-H', '512'
        ], check=True, capture_output=True)
        metadata['thumbnail_path'] = str(thumbnail_path)
    except Exception as e:
        metadata['thumbnail_error'] = str(e)

    return metadata
```

### 5. Schema Extension

```sql
-- Structured media metadata (for SVG, MIDI, diagrams, etc.)
CREATE TABLE structured_media_metadata (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID NOT NULL REFERENCES file_attachment(id) ON DELETE CASCADE,

    -- Format identification
    format TEXT NOT NULL,  -- 'svg', 'midi', 'mermaid', etc.
    format_category TEXT NOT NULL,  -- 'vector', 'music', 'diagram', etc.

    -- Dimensions (for visual formats)
    width TEXT,
    height TEXT,
    viewbox TEXT,

    -- Extracted text (searchable)
    text_content TEXT[],  -- Array of text elements
    text_combined TEXT,   -- Combined for FTS

    -- Structure metrics
    element_count INTEGER,
    element_breakdown JSONB,  -- {"paths": 10, "texts": 5, ...}

    -- Format-specific metadata
    format_metadata JSONB DEFAULT '{}',

    -- For music formats (MIDI)
    tempo_bpm REAL,
    duration_seconds REAL,
    track_count INTEGER,
    instrument_names TEXT[],
    pitch_range_low TEXT,
    pitch_range_high TEXT,
    estimated_key TEXT,

    -- For tracker modules (MOD/S3M/XM/IT)
    channel_count INTEGER,
    pattern_count INTEGER,
    order_count INTEGER,
    sample_count INTEGER,
    sample_names TEXT[],
    tracker_software TEXT,       -- 'ProTracker', 'Scream Tracker 3', etc.
    demoscene_era TEXT,          -- 'Amiga (1987-1995)', 'DOS (1994-1998)'
    global_volume INTEGER,
    has_audio_preview BOOLEAN DEFAULT FALSE,

    -- For diagram formats
    diagram_type TEXT,  -- 'flowchart', 'sequence', 'er', etc.
    node_count INTEGER,
    edge_count INTEGER,

    -- Thumbnail
    thumbnail_attachment_id UUID REFERENCES file_attachment(id),

    -- AI description
    ai_description TEXT,
    ai_model TEXT,
    ai_generated_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_structured_media_attachment ON structured_media_metadata(attachment_id);
CREATE INDEX idx_structured_media_format ON structured_media_metadata(format);
CREATE INDEX idx_structured_media_category ON structured_media_metadata(format_category);
CREATE INDEX idx_structured_media_text ON structured_media_metadata
    USING GIN (to_tsvector('english', text_combined));
```

### 6. Document Type Integration

```yaml
# svg-graphic
name: "SVG Vector Graphic"
slug: "svg-graphic"
extraction_strategy: "vector_svg"
mime_patterns:
  - "image/svg+xml"
file_extensions:
  - ".svg"
agentic_config:
  generation_prompt: |
    This is an SVG vector graphic.

    Title: {{title}}
    Description: {{description}}
    Text elements: {{text_combined}}

    Element breakdown: {{element_breakdown}}

    Describe what this graphic depicts based on its content and structure.

# midi-music
name: "MIDI Music File"
slug: "midi-music"
extraction_strategy: "music_midi"
mime_patterns:
  - "audio/midi"
  - "audio/x-midi"
file_extensions:
  - ".mid"
  - ".midi"
agentic_config:
  generation_prompt: |
    This is a MIDI music file.

    Duration: {{duration_seconds}} seconds
    Tempo: {{tempo_bpm}} BPM
    Tracks: {{track_count}}
    Instruments: {{instrument_names}}
    Pitch range: {{pitch_range_low}} to {{pitch_range_high}}
    Estimated key: {{estimated_key}}

    Describe this musical piece based on its structure and instrumentation.

# mermaid-diagram
name: "Mermaid Diagram"
slug: "mermaid-diagram"
extraction_strategy: "diagram_text"
mime_patterns:
  - "text/x-mermaid"
file_extensions:
  - ".mmd"
  - ".mermaid"
agentic_config:
  generation_prompt: |
    This is a Mermaid {{diagram_type}} diagram.

    Nodes: {{nodes}}
    Text content: {{text_combined}}

    Describe what this diagram represents and its key components.

# geojson-map
name: "GeoJSON Geographic Data"
slug: "geojson-map"
extraction_strategy: "geospatial_json"
mime_patterns:
  - "application/geo+json"
file_extensions:
  - ".geojson"
agentic_config:
  generation_prompt: |
    This is a GeoJSON file containing geographic data.

    Feature count: {{feature_count}}
    Geometry types: {{geometry_types}}
    Properties: {{property_names}}
    Bounding box: {{bbox}}

    Describe the geographic features in this file.

# tracker-module
name: "Tracker Module"
slug: "tracker-module"
extraction_strategy: "music_tracker"
mime_patterns:
  - "audio/x-mod"
  - "audio/x-s3m"
  - "audio/x-xm"
  - "audio/x-it"
  - "audio/x-med"
file_extensions:
  - ".mod"
  - ".s3m"
  - ".xm"
  - ".it"
  - ".mtm"
  - ".669"
  - ".med"
  - ".okt"
  - ".stm"
  - ".ult"
  - ".far"
  - ".ahx"
  - ".hvl"
agentic_config:
  generation_prompt: |
    This is a tracker module from the demoscene/chiptune era.

    Title: {{title}}
    Artist: {{artist}}
    Tracker: {{tracker_software}}
    Era: {{demoscene_era}}

    Technical details:
    - Duration: {{duration_seconds}} seconds
    - Channels: {{channel_count}}
    - Patterns: {{pattern_count}}
    - Samples: {{sample_count}}
    - Tempo: {{tempo_bpm}} BPM

    Sample names: {{sample_names}}
    Instrument names: {{instrument_names}}

    {{#if message}}
    Composer's message:
    {{message}}
    {{/if}}

    Describe this tracker module, its musical style, and historical context
    in the demoscene/chiptune community.
```

### 7. Search Capabilities

```sql
-- Search SVG by text content
SELECT n.*, sm.*
FROM note n
JOIN file_attachment fa ON fa.note_id = n.id
JOIN structured_media_metadata sm ON sm.attachment_id = fa.id
WHERE sm.format = 'svg'
  AND to_tsvector('english', sm.text_combined) @@ websearch_to_tsquery('logo brand');

-- Search MIDI by musical properties
SELECT n.*, sm.*
FROM note n
JOIN file_attachment fa ON fa.note_id = n.id
JOIN structured_media_metadata sm ON sm.attachment_id = fa.id
WHERE sm.format = 'midi'
  AND sm.tempo_bpm BETWEEN 100 AND 140
  AND 'Piano' = ANY(sm.instrument_names);

-- Search diagrams by type and content
SELECT n.*, sm.*
FROM note n
JOIN file_attachment fa ON fa.note_id = n.id
JOIN structured_media_metadata sm ON sm.attachment_id = fa.id
WHERE sm.format_category = 'diagram'
  AND sm.diagram_type = 'flowchart'
  AND sm.text_combined ILIKE '%authentication%';
```

```sql
-- Search tracker modules by era and properties
SELECT n.*, sm.*
FROM note n
JOIN file_attachment fa ON fa.note_id = n.id
JOIN structured_media_metadata sm ON sm.attachment_id = fa.id
WHERE sm.format_category = 'tracker'
  AND sm.demoscene_era LIKE '%Amiga%'
  AND sm.channel_count = 4;

-- Search by sample/instrument names (common in tracker scene)
SELECT n.*, sm.*
FROM note n
JOIN file_attachment fa ON fa.note_id = n.id
JOIN structured_media_metadata sm ON sm.attachment_id = fa.id
WHERE sm.format IN ('mod', 's3m', 'xm', 'it')
  AND 'kick' = ANY(LOWER(sm.sample_names::text)::text[]);
```

**Search modifiers:**
- `attachment:svg company logo`
- `attachment:midi tempo:>120 piano`
- `attachment:mermaid type:sequence`
- `attachment:geojson california`
- `attachment:mod channels:4 amiga`
- `attachment:tracker era:amiga`
- `attachment:xm chiptune`

### 9. Dependencies

**Python packages:**
```bash
pip install \
    cairosvg \      # SVG rendering
    mido \          # MIDI parsing
    music21 \       # Advanced music analysis (optional)
    geojson \       # GeoJSON parsing
    lxml \          # Better XML parsing
    Pillow \        # Image handling
    openmpt         # Tracker module parsing (MOD/S3M/XM/IT)
```

**System dependencies:**
```dockerfile
RUN apt-get update && apt-get install -y \
    libcairo2-dev \        # For cairosvg
    libpango1.0-dev \      # For SVG text
    librsvg2-bin \         # SVG rendering fallback
    libopenmpt-dev \       # Tracker module support
    libopenmpt-modplug1 \  # MOD/S3M/XM/IT playback
    npm                    # For mermaid-cli

RUN npm install -g @mermaid-js/mermaid-cli
```

**Alternative tracker libraries (if openmpt unavailable):**
- `libxmp` - Cross-platform tracker player
- `mikmod` - Classic MOD player library
- Pure Python fallback for header-only parsing (included in processor)

## Consequences

### Positive

- (+) **Searchable vector graphics**: Find SVGs by their text content
- (+) **Musical analysis**: Search MIDI by tempo, instruments, key
- (+) **Diagram understanding**: Index flowcharts, sequence diagrams by labels
- (+) **Format-aware processing**: Each format gets optimal extraction
- (+) **Thumbnail generation**: Visual previews for all formats
- (+) **Text extraction**: All embedded text is searchable

### Negative

- (-) **Many dependencies**: Each format may need specific libraries
- (-) **Processing complexity**: Format-specific parsing logic
- (-) **Rendering challenges**: Some formats hard to render consistently

### Mitigations

- Graceful degradation: If specific library fails, fall back to basic metadata
- Docker bundle includes all dependencies
- Format detection prevents wrong processor from running

## Implementation

### Phase 1: SVG & Diagrams (Week 1)
- SVG text extraction and rendering
- Mermaid/PlantUML parsing and rendering
- Schema migration
- Basic search integration

### Phase 2: Music Formats (Week 2)
- MIDI parsing with mido
- MusicXML support (optional)
- Musical property search

### Phase 2.5: Tracker Modules (Week 2-3)
- MOD/S3M/XM/IT parsing with libopenmpt
- Sample/instrument name extraction
- Audio preview rendering (WAV/MP3)
- Demoscene era classification
- Tracker software identification

### Phase 3: Geospatial (Week 3)
- GeoJSON parsing
- KML/GPX support
- Integration with PostGIS from ADR-032

### Phase 4: Scientific (Future)
- LaTeX equation extraction
- Chemical structure formats
- Specialized rendering

## References

### General
- CairoSVG: https://cairosvg.org/
- Mido (MIDI): https://mido.readthedocs.io/
- Mermaid CLI: https://github.com/mermaid-js/mermaid-cli
- Music21: https://web.mit.edu/music21/
- GeoJSON spec: https://geojson.org/

### Tracker/Demoscene
- libopenmpt: https://lib.openmpt.org/libopenmpt/
- MOD format spec: https://wiki.openmpt.org/Manual:_Module_formats
- Demoscene Wiki: https://demozoo.org/
- Modland (archive): https://modland.com/
- The Mod Archive: https://modarchive.org/
- Demoscene documentary: https://www.youtube.com/watch?v=5MexnBunH_g
