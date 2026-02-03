-- ============================================================================
-- DUAL-TRACK NOTE VERSIONING (#104)
-- Independent history for user content and AI revisions
-- ============================================================================

-- ============================================================================
-- 1. Add version tracking to note_original
-- ============================================================================

ALTER TABLE note_original ADD COLUMN IF NOT EXISTS version_number INTEGER NOT NULL DEFAULT 1;

-- ============================================================================
-- 2. Create note_original_history table for user content versions
-- ============================================================================

CREATE TABLE IF NOT EXISTS note_original_history (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
  version_number INTEGER NOT NULL,
  content TEXT NOT NULL,  -- Includes YAML frontmatter with tag/metadata snapshot
  hash TEXT NOT NULL,
  created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  created_by TEXT NOT NULL DEFAULT 'user',  -- 'user' | 'restore' | 'import'

  UNIQUE (note_id, version_number)
);

-- Index for efficient version lookups
CREATE INDEX IF NOT EXISTS idx_original_history_note
  ON note_original_history(note_id, version_number DESC);

CREATE INDEX IF NOT EXISTS idx_original_history_created
  ON note_original_history(created_at_utc DESC);

-- ============================================================================
-- 3. Add versioning configuration
-- ============================================================================

INSERT INTO user_config (key, value) VALUES
  ('versioning_enabled', 'true'::jsonb),
  ('versioning_max_history', '50'::jsonb)
ON CONFLICT (key) DO NOTHING;

-- ============================================================================
-- 4. Create versioning trigger function
-- ============================================================================

CREATE OR REPLACE FUNCTION snapshot_original_on_update()
RETURNS TRIGGER AS $$
DECLARE
  versioning_enabled BOOLEAN;
  max_history INTEGER;
  current_tags JSONB;
  snapshot_content TEXT;
BEGIN
  -- Check if versioning is enabled
  SELECT COALESCE((value::text)::boolean, true) INTO versioning_enabled
  FROM user_config WHERE key = 'versioning_enabled';

  -- Only snapshot if content actually changed
  IF versioning_enabled IS NOT FALSE AND OLD.content IS DISTINCT FROM NEW.content THEN
    -- Get current tags for the note
    SELECT COALESCE(jsonb_agg(tag_name), '[]'::jsonb) INTO current_tags
    FROM note_tag WHERE note_id = OLD.note_id;

    -- Build snapshot content with YAML frontmatter
    snapshot_content := format(
      E'---\nsnapshot_tags: %s\nsnapshot_at: "%s"\n---\n%s',
      current_tags::text,
      NOW()::text,
      OLD.content
    );

    -- Insert the old version into history
    INSERT INTO note_original_history (note_id, version_number, content, hash, created_by)
    VALUES (OLD.note_id, OLD.version_number, snapshot_content, OLD.hash, 'user');

    -- Increment version number
    NEW.version_number := OLD.version_number + 1;

    -- Cleanup old versions if we exceed max_history
    SELECT COALESCE((value::text)::integer, 50) INTO max_history
    FROM user_config WHERE key = 'versioning_max_history';

    DELETE FROM note_original_history
    WHERE note_id = OLD.note_id
      AND version_number <= (
        SELECT version_number FROM note_original_history
        WHERE note_id = OLD.note_id
        ORDER BY version_number DESC
        OFFSET max_history
        LIMIT 1
      );
  END IF;

  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 5. Create/replace versioning trigger
-- ============================================================================

-- Drop old trigger if exists (we're replacing update_original_edited)
DROP TRIGGER IF EXISTS snapshot_original_before_update ON note_original;

-- Create new versioning trigger
CREATE TRIGGER snapshot_original_before_update
BEFORE UPDATE OF content ON note_original
FOR EACH ROW
EXECUTE FUNCTION snapshot_original_on_update();

-- ============================================================================
-- 6. Comments
-- ============================================================================

COMMENT ON TABLE note_original_history IS 'Version history for user-authored note content';
COMMENT ON COLUMN note_original_history.content IS 'Original content with YAML frontmatter containing snapshot_tags and snapshot_at';
COMMENT ON COLUMN note_original_history.created_by IS 'Source of version: user (edit), restore (version restore), import (archive import)';
COMMENT ON COLUMN note_original.version_number IS 'Current version number for user content track';
COMMENT ON FUNCTION snapshot_original_on_update IS 'Automatically snapshots old content before updates, with tag metadata embedded';
