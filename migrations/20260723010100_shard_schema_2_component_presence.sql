-- Preserve whether live-required/default rows were present in an imported
-- Knowledge Shard 2.0 component. The live persistence model may require or
-- seed rows that an exact shard snapshot legitimately omits.

ALTER TABLE note_original
    ADD COLUMN IF NOT EXISTS shard_export_present BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE note_revised_current
    ADD COLUMN IF NOT EXISTS shard_export_present BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE note_revision
    ADD COLUMN IF NOT EXISTS shard_export_present BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS shard_export_present BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE embedding_set
    ADD COLUMN IF NOT EXISTS shard_export_present BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE embedding_set_member
    ADD COLUMN IF NOT EXISTS shard_export_present BOOLEAN NOT NULL DEFAULT TRUE;

COMMENT ON COLUMN note_original.shard_export_present IS
    'True when this live-required row belongs to the exact imported schema-2 component state.';
COMMENT ON COLUMN note_revised_current.shard_export_present IS
    'True when this live-required row belongs to the exact imported schema-2 component state.';
COMMENT ON COLUMN note_revision.shard_export_present IS
    'True when this live-generated row belongs to the exact imported schema-2 component state.';
COMMENT ON COLUMN embedding_config.shard_export_present IS
    'True when this live/default row belongs to the exact imported schema-2 component state.';
COMMENT ON COLUMN embedding_set.shard_export_present IS
    'True when this live/default row belongs to the exact imported schema-2 component state.';
COMMENT ON COLUMN embedding_set_member.shard_export_present IS
    'True when this live/default row belongs to the exact imported schema-2 component state.';
