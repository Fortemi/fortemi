-- PostgreSQL 18 native UUIDv7 migration (#397)
--
-- PostgreSQL 18 provides built-in uuidv7() and uuid_extract_timestamp().
-- This migration:
-- 1. Switches all gen_random_uuid() (v4) defaults to uuidv7()
-- 2. Switches all custom gen_uuid_v7() defaults to native uuidv7()
-- 3. Drops the custom gen_uuid_v7() and extract_uuid_v7_timestamp() functions
-- 4. Drops the uuid-ossp extension (no longer needed)

-- ============================================================================
-- Tables currently using gen_random_uuid() (UUIDv4 → UUIDv7)
-- ============================================================================

ALTER TABLE archive_registry ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE embedding_config ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE embedding_set ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE note_original ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE note_original_history ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE note_template ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE pke_keysets ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_audit_log ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_collection ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_concept ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_concept_label ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_concept_merge ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_concept_note ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_concept_scheme ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_mapping_relation_edge ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE skos_semantic_relation_edge ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE webhook ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE webhook_delivery ALTER COLUMN id SET DEFAULT uuidv7();

-- ============================================================================
-- Tables currently using custom gen_uuid_v7() (custom → native)
-- ============================================================================

ALTER TABLE activity_log ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE attachment ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE attachment_blob ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE attachment_embedding ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE document_type ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE embedding ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE file_upload_audit ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE fine_tuning_dataset ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE fine_tuning_sample ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE job_history ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE job_queue ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE model_3d_metadata ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE named_location ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE note_entity ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE note_share_grant ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE note_token_embeddings ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE prov_agent_device ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE prov_location ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE provenance ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE provenance_activity ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE provenance_edge ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE structured_media_metadata ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE user_metadata_label ALTER COLUMN id SET DEFAULT uuidv7();

-- ============================================================================
-- Drop custom UUID functions (replaced by pg18 built-ins)
-- ============================================================================

-- Native uuidv7() replaces our custom PL/pgSQL implementation
DROP FUNCTION IF EXISTS gen_uuid_v7();

-- Native uuid_extract_timestamp() replaces our custom implementation
DROP FUNCTION IF EXISTS extract_uuid_v7_timestamp(uuid);

-- uuid-ossp extension is no longer needed (we use built-in uuidv7())
DROP EXTENSION IF EXISTS "uuid-ossp";
