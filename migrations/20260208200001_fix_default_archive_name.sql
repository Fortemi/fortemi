-- Fix #208: Rename default archive from 'default' to 'public' so API routes
-- using the schema_name ("public") work correctly for clone/delete operations.
-- The archive name should match what users expect from the schema_name.
UPDATE archive_registry
SET name = 'public'
WHERE schema_name = 'public' AND name = 'default';
