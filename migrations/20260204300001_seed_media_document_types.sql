-- Seed: Specialized media document types (3D, SVG, MIDI, diagrams)
-- Related migration: 20260204300000_specialized_media_metadata.sql

-- ============================================================================
-- 3D Model Types
-- ============================================================================

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

-- ============================================================================
-- Structured Media Types
-- ============================================================================

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
