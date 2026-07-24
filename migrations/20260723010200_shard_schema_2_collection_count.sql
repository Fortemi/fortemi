-- A shard's declared collection note_count is snapshot state, not necessarily
-- the live derived count at import time. Preserve it for exact schema-2
-- re-export and invalidate it whenever live note membership changes.

ALTER TABLE collection
    ADD COLUMN IF NOT EXISTS shard_note_count INTEGER;

ALTER TABLE collection
    DROP CONSTRAINT IF EXISTS collection_shard_note_count_nonnegative;
ALTER TABLE collection
    ADD CONSTRAINT collection_shard_note_count_nonnegative
    CHECK (shard_note_count IS NULL OR shard_note_count >= 0);

CREATE OR REPLACE FUNCTION invalidate_shard_collection_note_count()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP <> 'INSERT' AND OLD.collection_id IS NOT NULL THEN
        UPDATE collection SET shard_note_count = NULL WHERE id = OLD.collection_id;
    END IF;
    IF TG_OP <> 'DELETE' AND NEW.collection_id IS NOT NULL THEN
        UPDATE collection SET shard_note_count = NULL WHERE id = NEW.collection_id;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS note_invalidate_shard_collection_note_count ON note;
CREATE TRIGGER note_invalidate_shard_collection_note_count
AFTER INSERT OR UPDATE OF collection_id OR DELETE ON note
FOR EACH ROW
EXECUTE FUNCTION invalidate_shard_collection_note_count();

COMMENT ON COLUMN collection.shard_note_count IS
    'Exact imported schema-2 snapshot count; NULL means derive from live note membership.';
