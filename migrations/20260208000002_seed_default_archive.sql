-- Seed the default "public" archive representing the public PostgreSQL schema.
-- This ensures list_archives() returns at least one entry on fresh deployments.
-- Fixes issue #158: No default archive representing public schema.

INSERT INTO archive_registry (name, schema_name, description, is_default, created_at)
VALUES ('public', 'public', 'Default archive (public schema)', true, NOW())
ON CONFLICT DO NOTHING;
