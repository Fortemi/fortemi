-- ============================================================================
-- Fix version restore trigger to handle duplicate version numbers (#228)
-- When restoring a version, the trigger fires and may try to INSERT a
-- history entry with a version_number that already exists. Add ON CONFLICT
-- to handle this gracefully.
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

    -- Insert the old version into history (ON CONFLICT handles restore scenarios
    -- where the version_number already exists in history)
    INSERT INTO note_original_history (note_id, version_number, content, hash, created_by)
    VALUES (OLD.note_id, OLD.version_number, snapshot_content, OLD.hash, 'user')
    ON CONFLICT (note_id, version_number) DO UPDATE
    SET content = EXCLUDED.content, hash = EXCLUDED.hash, created_at_utc = NOW();

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
