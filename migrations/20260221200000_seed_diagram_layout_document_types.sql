-- Seed: Diagramming and layout document types (#516)
-- Adds formats not covered by existing seed migrations:
--   - Draw.io, D2, Excalidraw (diagram)
--   - Visio, OmniGraffle (binary diagram)
--   - Typst (layout/typesetting)
--
-- Already seeded elsewhere:
--   - Mermaid, PlantUML, Graphviz (seed_media_document_types)
--   - SVG (seed_media_document_types)
--   - LaTeX, AsciiDoc, reStructuredText (seed_technical_document_types)

-- ============================================================================
-- Text-based Diagramming (extractable source)
-- ============================================================================

INSERT INTO document_type (name, display_name, category, description, extraction_strategy, requires_attachment, file_extensions, mime_types, chunking_strategy, is_system)
VALUES
    ('d2-diagram', 'D2 Diagram', 'markup', 'D2 declarative diagram definition', 'text_native', false,
     ARRAY['d2'], ARRAY['text/x-d2'],
     'semantic', TRUE),
    ('excalidraw-diagram', 'Excalidraw Diagram', 'markup', 'Excalidraw whiteboard diagram (JSON)', 'structured_extract', true,
     ARRAY['excalidraw'], ARRAY['application/x-excalidraw+json'],
     'whole', TRUE),
    ('drawio-diagram', 'Draw.io Diagram', 'markup', 'Draw.io/diagrams.net diagram (XML)', 'structured_extract', true,
     ARRAY['drawio'], ARRAY['application/x-drawio', 'application/x-drawio+xml'],
     'whole', TRUE)
ON CONFLICT (name) DO UPDATE SET
    file_extensions = EXCLUDED.file_extensions,
    mime_types = EXCLUDED.mime_types;

-- ============================================================================
-- Binary Diagramming (metadata extraction only)
-- ============================================================================

INSERT INTO document_type (name, display_name, category, description, extraction_strategy, requires_attachment, file_extensions, mime_types, chunking_strategy, is_system)
VALUES
    ('visio-diagram', 'Visio Diagram', 'markup', 'Microsoft Visio diagramming file', 'office_convert', true,
     ARRAY['vsdx', 'vsd'], ARRAY['application/vnd.ms-visio.drawing', 'application/vnd.visio'],
     'whole', TRUE),
    ('omnigraffle-diagram', 'OmniGraffle Diagram', 'markup', 'OmniGraffle diagramming file (macOS)', 'structured_extract', true,
     ARRAY['graffle'], ARRAY['application/x-omnigraffle'],
     'whole', TRUE)
ON CONFLICT (name) DO UPDATE SET
    file_extensions = EXCLUDED.file_extensions,
    mime_types = EXCLUDED.mime_types;

-- ============================================================================
-- Layout / Typesetting (Typst only — LaTeX, AsciiDoc, RST already seeded)
-- ============================================================================

INSERT INTO document_type (name, display_name, category, description, extraction_strategy, requires_attachment, file_extensions, mime_types, chunking_strategy, is_system)
VALUES
    ('typst', 'Typst Document', 'docs', 'Typst typesetting document', 'text_native', false,
     ARRAY['typ'], ARRAY['text/x-typst'],
     'per_section', TRUE)
ON CONFLICT (name) DO UPDATE SET
    file_extensions = EXCLUDED.file_extensions,
    mime_types = EXCLUDED.mime_types;
