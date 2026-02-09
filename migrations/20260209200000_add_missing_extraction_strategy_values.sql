-- Add extraction_strategy enum values used by Rust code but missing from DB.
-- The original enum used 'pandoc' and 'code_analysis'; Rust uses 'office_convert' and 'code_ast'.
-- Add both so the enum accepts values from either naming convention.
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'office_convert';
ALTER TYPE extraction_strategy ADD VALUE IF NOT EXISTS 'code_ast';
