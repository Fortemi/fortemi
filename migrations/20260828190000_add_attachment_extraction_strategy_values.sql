-- Keep the PostgreSQL extraction_strategy enum aligned with the canonical
-- Rust ExtractionStrategy variants used by attachment strategy assignment.
-- IF NOT EXISTS preserves operators' v2026.7.19 field workaround.
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'archive';
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'spreadsheet';
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'email';
