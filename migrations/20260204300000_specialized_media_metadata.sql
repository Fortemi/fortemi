-- Specialized media format metadata tables
-- Issues: #438 (3D files), #439 (Structured media)

-- ============================================================================
-- 3D Model Metadata (GLB, STL, OBJ, PLY)
-- ============================================================================

CREATE TABLE IF NOT EXISTS model_3d_metadata (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID NOT NULL REFERENCES attachment(id) ON DELETE CASCADE,

    -- Format info
    format TEXT NOT NULL,  -- 'glb', 'stl', 'obj', 'ply', 'fbx'
    format_version TEXT,

    -- Geometry stats
    vertex_count INTEGER,
    face_count INTEGER,
    edge_count INTEGER,

    -- Bounding box
    bounds_min_x DOUBLE PRECISION,
    bounds_min_y DOUBLE PRECISION,
    bounds_min_z DOUBLE PRECISION,
    bounds_max_x DOUBLE PRECISION,
    bounds_max_y DOUBLE PRECISION,
    bounds_max_z DOUBLE PRECISION,

    -- Computed properties
    volume DOUBLE PRECISION,
    surface_area DOUBLE PRECISION,
    is_watertight BOOLEAN,
    is_manifold BOOLEAN,

    -- Materials and textures
    material_count INTEGER DEFAULT 0,
    texture_count INTEGER DEFAULT 0,
    has_vertex_colors BOOLEAN DEFAULT FALSE,
    has_uv_mapping BOOLEAN DEFAULT FALSE,

    -- Preview
    thumbnail_attachment_id UUID REFERENCES attachment(id),

    -- AI description
    ai_description TEXT,
    ai_model TEXT,
    ai_processed_at TIMESTAMPTZ,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT model_3d_attachment_unique UNIQUE (attachment_id)
);

CREATE INDEX idx_model_3d_format ON model_3d_metadata(format);
CREATE INDEX idx_model_3d_watertight ON model_3d_metadata(is_watertight) WHERE is_watertight = true;
CREATE INDEX idx_model_3d_fts ON model_3d_metadata USING GIN (to_tsvector('english', COALESCE(ai_description, '')));

-- ============================================================================
-- Structured Media Metadata (SVG, MIDI, Mermaid, Tracker modules)
-- ============================================================================

CREATE TABLE IF NOT EXISTS structured_media_metadata (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    attachment_id UUID NOT NULL REFERENCES attachment(id) ON DELETE CASCADE,

    -- Format category
    format TEXT NOT NULL,  -- 'svg', 'midi', 'mermaid', 'plantuml', 'mod', 's3m', 'xm', 'it'
    format_category TEXT NOT NULL,  -- 'vector', 'music', 'diagram', 'tracker'

    -- SVG specific
    svg_width REAL,
    svg_height REAL,
    svg_element_count INTEGER,
    svg_text_content TEXT,  -- All text extracted from SVG

    -- MIDI specific
    midi_duration_seconds REAL,
    midi_tempo_bpm INTEGER,
    midi_time_signature TEXT,  -- e.g., "4/4"
    midi_track_count INTEGER,
    midi_channel_count INTEGER,
    midi_note_count INTEGER,
    midi_instrument_names TEXT[],  -- Array of instrument names
    midi_pitch_range_low INTEGER,  -- MIDI note number
    midi_pitch_range_high INTEGER,

    -- Tracker module specific (MOD/S3M/XM/IT)
    tracker_pattern_count INTEGER,
    tracker_order_count INTEGER,
    tracker_channel_count INTEGER,
    tracker_sample_count INTEGER,
    tracker_sample_names TEXT[],
    tracker_instrument_names TEXT[],
    tracker_title TEXT,
    tracker_message TEXT,  -- Composer message
    tracker_software TEXT,  -- Tracker software used
    demoscene_era TEXT,  -- 'amiga', 'pc_dos', 'modern'

    -- Diagram specific (Mermaid, PlantUML)
    diagram_type TEXT,  -- 'flowchart', 'sequence', 'class', 'er', 'gantt'
    diagram_node_count INTEGER,
    diagram_edge_count INTEGER,
    diagram_labels TEXT[],  -- Extracted node/edge labels

    -- Preview/render
    thumbnail_attachment_id UUID REFERENCES attachment(id),
    audio_preview_attachment_id UUID REFERENCES attachment(id),  -- For MIDI/tracker

    -- Combined text for FTS
    text_combined TEXT,  -- All searchable text

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT structured_media_attachment_unique UNIQUE (attachment_id)
);

CREATE INDEX idx_structured_media_format ON structured_media_metadata(format);
CREATE INDEX idx_structured_media_category ON structured_media_metadata(format_category);
CREATE INDEX idx_structured_media_fts ON structured_media_metadata
    USING GIN (to_tsvector('english', COALESCE(text_combined, '')));

-- For tracker module sample name search
CREATE INDEX idx_structured_media_samples ON structured_media_metadata
    USING GIN (tracker_sample_names) WHERE tracker_sample_names IS NOT NULL;

-- ============================================================================
-- Document Types for Specialized Formats
-- ============================================================================

-- 3D Model types
INSERT INTO document_type (name, display_name, category, description, extraction_strategy, requires_attachment, file_extensions, mime_types)
VALUES
    ('model-3d-generic', '3D Model', 'media', 'Generic 3D model file', 'vision', true,
     ARRAY['glb', 'gltf', 'obj', 'stl', 'ply', 'fbx', '3ds'],
     ARRAY['model/gltf-binary', 'model/gltf+json', 'model/obj', 'model/stl']),
    ('model-3d-printable', '3D Printable Model', 'media', '3D model optimized for printing (watertight)', 'vision', true,
     ARRAY['stl', 'obj', '3mf'],
     ARRAY['model/stl', 'model/3mf']),
    ('model-3d-cad', 'CAD Model', 'media', 'CAD/engineering model', 'vision', true,
     ARRAY['step', 'stp', 'iges', 'igs'],
     ARRAY['model/step', 'model/iges'])
ON CONFLICT (name) DO UPDATE SET
    file_extensions = EXCLUDED.file_extensions,
    mime_types = EXCLUDED.mime_types;

-- Structured media types
INSERT INTO document_type (name, display_name, category, description, extraction_strategy, requires_attachment, file_extensions, mime_types)
VALUES
    ('svg-graphic', 'SVG Graphic', 'media', 'Scalable Vector Graphics image', 'structured_extract', true,
     ARRAY['svg'], ARRAY['image/svg+xml']),
    ('midi-music', 'MIDI Music', 'media', 'MIDI music file', 'structured_extract', true,
     ARRAY['mid', 'midi'], ARRAY['audio/midi', 'audio/x-midi']),
    ('tracker-module', 'Tracker Module', 'media', 'Demoscene tracker music module (MOD/S3M/XM/IT)', 'audio_transcribe', true,
     ARRAY['mod', 's3m', 'xm', 'it', 'mtm', '669', 'med', 'okt'],
     ARRAY['audio/mod', 'audio/x-mod', 'audio/s3m', 'audio/xm', 'audio/it']),
    ('mermaid-diagram', 'Mermaid Diagram', 'markup', 'Mermaid diagram definition', 'text_native', false,
     ARRAY['mmd', 'mermaid'], ARRAY['text/x-mermaid']),
    ('plantuml-diagram', 'PlantUML Diagram', 'markup', 'PlantUML diagram definition', 'text_native', false,
     ARRAY['puml', 'plantuml', 'pu'], ARRAY['text/x-plantuml']),
    ('graphviz-diagram', 'Graphviz Diagram', 'markup', 'Graphviz DOT diagram', 'text_native', false,
     ARRAY['dot', 'gv'], ARRAY['text/vnd.graphviz'])
ON CONFLICT (name) DO UPDATE SET
    file_extensions = EXCLUDED.file_extensions,
    mime_types = EXCLUDED.mime_types;

COMMENT ON TABLE model_3d_metadata IS '3D model geometric and AI-generated metadata';
COMMENT ON TABLE structured_media_metadata IS 'Metadata for SVG, MIDI, diagrams, and tracker modules';
