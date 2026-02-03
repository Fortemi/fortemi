-- ============================================================================
-- matric-memory Database Schema - Initial Schema
-- ============================================================================
-- This schema provides the foundational database structure for matric-memory,
-- supporting vector-enhanced PostgreSQL storage, hybrid search, and NLP pipelines.
--
-- Version: 0.1.0
-- Generated: 2026-01-02
-- ============================================================================

-- ============================================================================
-- EXTENSIONS
-- ============================================================================

-- Enable pgvector for embeddings (CRITICAL: Must exist before vector columns)
CREATE EXTENSION IF NOT EXISTS vector;

-- Enable UUID generation
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ============================================================================
-- UUIDv7 GENERATION FUNCTION (RFC 9562)
-- ============================================================================
-- Generates time-ordered UUIDs with millisecond-precision timestamps.
-- Structure: 48-bit Unix timestamp (ms) | 4-bit version (7) | 12-bit rand_a |
--            2-bit variant (10) | 62-bit rand_b
--
-- Benefits:
-- - Naturally time-ordered for efficient B-tree indexing
-- - Timestamp extractable for temporal queries without additional columns
-- - Compatible with standard UUID storage and comparison

CREATE OR REPLACE FUNCTION gen_uuid_v7() RETURNS uuid AS $$
DECLARE
    unix_ts_ms bigint;
    rand_a int;
    rand_b bigint;
    uuid_bytes bytea;
BEGIN
    -- Get current Unix timestamp in milliseconds
    unix_ts_ms := (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint;

    -- Generate random bits
    rand_a := (random() * 4095)::int;  -- 12 bits
    rand_b := (random() * 4611686018427387903)::bigint;  -- 62 bits

    -- Build 16-byte UUID (each set_byte creates a single byte, then concatenate all 16)
    -- Bytes 0-5: Unix timestamp (ms), big-endian
    uuid_bytes :=
        set_byte(E'\\x00'::bytea, 0, ((unix_ts_ms >> 40) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((unix_ts_ms >> 32) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((unix_ts_ms >> 24) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((unix_ts_ms >> 16) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((unix_ts_ms >> 8) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, (unix_ts_ms & 255)::int) ||
        -- Byte 6: version (0111) + top 4 bits of rand_a
        set_byte(E'\\x00'::bytea, 0, 112 | ((rand_a >> 8) & 15)) ||
        -- Byte 7: bottom 8 bits of rand_a
        set_byte(E'\\x00'::bytea, 0, rand_a & 255) ||
        -- Byte 8: variant (10) + top 6 bits of rand_b
        set_byte(E'\\x00'::bytea, 0, 128 | ((rand_b >> 56) & 63)::int) ||
        -- Bytes 9-15: remaining 56 bits of rand_b
        set_byte(E'\\x00'::bytea, 0, ((rand_b >> 48) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((rand_b >> 40) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((rand_b >> 32) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((rand_b >> 24) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((rand_b >> 16) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, ((rand_b >> 8) & 255)::int) ||
        set_byte(E'\\x00'::bytea, 0, (rand_b & 255)::int);

    RETURN encode(uuid_bytes, 'hex')::uuid;
END;
$$ LANGUAGE plpgsql VOLATILE;

-- Function to extract timestamp from UUIDv7 (useful for queries)
CREATE OR REPLACE FUNCTION extract_uuid_v7_timestamp(uuid_val uuid) RETURNS timestamptz AS $$
DECLARE
    uuid_bytes bytea;
    unix_ts_ms bigint;
BEGIN
    uuid_bytes := decode(replace(uuid_val::text, '-', ''), 'hex');

    -- Extract 48-bit timestamp from first 6 bytes
    unix_ts_ms :=
        (get_byte(uuid_bytes, 0)::bigint << 40) |
        (get_byte(uuid_bytes, 1)::bigint << 32) |
        (get_byte(uuid_bytes, 2)::bigint << 24) |
        (get_byte(uuid_bytes, 3)::bigint << 16) |
        (get_byte(uuid_bytes, 4)::bigint << 8) |
        get_byte(uuid_bytes, 5)::bigint;

    RETURN to_timestamp(unix_ts_ms / 1000.0);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- ============================================================================
-- ENUMS
-- ============================================================================

-- Job status and type enums
CREATE TYPE job_status AS ENUM ('pending', 'running', 'completed', 'failed', 'cancelled');
CREATE TYPE job_type AS ENUM ('ai_revision', 'embedding', 'linking', 'context_update', 'title_generation');

-- Visibility levels for notes (security filter)
CREATE TYPE note_visibility AS ENUM ('private', 'shared', 'internal', 'public');

-- ============================================================================
-- CORE TABLES
-- ============================================================================

-- Main note table
CREATE TABLE note (
  id UUID PRIMARY KEY,
  collection_id UUID,
  format TEXT NOT NULL,
  source TEXT NOT NULL,
  created_at_utc TIMESTAMPTZ NOT NULL,
  updated_at_utc TIMESTAMPTZ NOT NULL,
  starred BOOLEAN DEFAULT FALSE,
  archived BOOLEAN DEFAULT FALSE,
  last_accessed_at TIMESTAMPTZ,
  access_count INTEGER DEFAULT 0,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  title TEXT,
  deleted_at TIMESTAMPTZ DEFAULT NULL,
  -- Security fields (Phase 4: Unified Strict Filter)
  owner_id UUID,                                -- Owner user ID
  tenant_id UUID,                               -- Tenant ID for multi-tenant isolation
  visibility note_visibility DEFAULT 'private'  -- Visibility level
);

-- Share grants for fine-grained note sharing
CREATE TABLE note_share_grant (
  id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
  note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
  grantee_id UUID NOT NULL,                    -- User or group receiving access
  permission TEXT NOT NULL DEFAULT 'read',      -- 'read', 'write', 'admin'
  granted_by UUID,                              -- User who granted access
  granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ,                       -- Optional expiration
  revoked_at TIMESTAMPTZ,                       -- Soft revocation timestamp
  UNIQUE(note_id, grantee_id)
);

-- Original note content (immutable)
CREATE TABLE note_original (
  note_id UUID PRIMARY KEY REFERENCES note(id) ON DELETE CASCADE,
  content TEXT NOT NULL,
  hash TEXT NOT NULL,
  user_created_at TIMESTAMPTZ DEFAULT NOW(),
  user_last_edited_at TIMESTAMPTZ DEFAULT NOW()
);

-- Current revised note content
CREATE TABLE note_revised_current (
  note_id UUID PRIMARY KEY REFERENCES note(id) ON DELETE CASCADE,
  content TEXT NOT NULL,
  last_revision_id UUID,
  ai_metadata JSONB DEFAULT '{}'::jsonb,
  tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED
);

-- Note revision history
CREATE TABLE note_revision (
  id UUID PRIMARY KEY,
  note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
  parent_revision_id UUID REFERENCES note_revision(id),
  revision_number INTEGER NOT NULL,
  content TEXT NOT NULL,
  type TEXT NOT NULL DEFAULT 'ai_enhancement',
  summary TEXT,
  rationale TEXT,
  created_at_utc TIMESTAMPTZ NOT NULL,
  ai_generated_at TIMESTAMPTZ DEFAULT NOW(),
  user_last_edited_at TIMESTAMPTZ,
  is_user_edited BOOLEAN NOT NULL DEFAULT FALSE,
  generation_count INTEGER NOT NULL DEFAULT 1,
  model TEXT
);

-- Collections for organizing notes
CREATE TABLE collection (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  description TEXT,
  created_at_utc TIMESTAMPTZ NOT NULL
);

-- Tags table
CREATE TABLE tag (
  name TEXT PRIMARY KEY,
  created_at_utc TIMESTAMPTZ NOT NULL
);

-- Note-tag relationships
CREATE TABLE note_tag (
  note_id UUID REFERENCES note(id) ON DELETE CASCADE,
  tag_name TEXT REFERENCES tag(name) ON DELETE CASCADE,
  source TEXT DEFAULT 'manual',
  PRIMARY KEY (note_id, tag_name)
);

-- Links between notes
CREATE TABLE link (
  id UUID PRIMARY KEY,
  from_note_id UUID REFERENCES note(id) ON DELETE CASCADE,
  to_note_id UUID,
  to_url TEXT,
  kind TEXT NOT NULL,
  score REAL NOT NULL,
  created_at_utc TIMESTAMPTZ NOT NULL,
  metadata JSONB DEFAULT '{}'::jsonb,
  FOREIGN KEY (to_note_id) REFERENCES note(id) ON DELETE CASCADE,
  CHECK ((to_note_id IS NOT NULL AND to_url IS NULL) OR
         (to_note_id IS NULL AND to_url IS NOT NULL))
);

-- User-defined metadata labels
CREATE TABLE user_metadata_label (
  id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
  note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
  label TEXT NOT NULL,
  color TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- User configuration/preferences table
CREATE TABLE user_config (
  key TEXT PRIMARY KEY,
  value JSONB NOT NULL,
  updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Activity log for audit trail
CREATE TABLE activity_log (
  id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
  at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  note_id UUID REFERENCES note(id) ON DELETE SET NULL,
  meta JSONB DEFAULT '{}'::jsonb
);

-- Embeddings table for vector search
CREATE TABLE embedding (
  id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
  note_id UUID REFERENCES note(id) ON DELETE CASCADE,
  chunk_index INTEGER NOT NULL,
  text TEXT NOT NULL,
  vector vector(768),
  model TEXT NOT NULL,
  created_at TIMESTAMPTZ DEFAULT NOW(),
  UNIQUE(note_id, chunk_index)
);

-- Provenance edges for tracking content relationships
CREATE TABLE provenance_edge (
  id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
  revision_id UUID REFERENCES note_revision(id) ON DELETE CASCADE,
  source_note_id UUID REFERENCES note(id) ON DELETE SET NULL,
  source_url TEXT,
  relation TEXT NOT NULL,
  created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- JOB QUEUE SYSTEM
-- ============================================================================

-- Job queue for ML pipeline operations
CREATE TABLE job_queue (
  id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
  note_id UUID REFERENCES note(id) ON DELETE CASCADE,
  job_type job_type NOT NULL,
  status job_status NOT NULL DEFAULT 'pending',
  priority INTEGER NOT NULL DEFAULT 5,
  payload JSONB,
  result JSONB,
  error_message TEXT,
  estimated_duration_ms INTEGER,
  actual_duration_ms INTEGER,
  progress_percent INTEGER NOT NULL DEFAULT 0,
  progress_message TEXT,
  logs TEXT[],
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  started_at TIMESTAMPTZ,
  completed_at TIMESTAMPTZ,
  retry_count INTEGER NOT NULL DEFAULT 0,
  max_retries INTEGER NOT NULL DEFAULT 3
);

-- Job history for calculating estimates
CREATE TABLE job_history (
  id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
  job_type job_type NOT NULL,
  duration_ms INTEGER NOT NULL,
  payload_size INTEGER,
  success BOOLEAN NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  logs TEXT[]
);

-- ============================================================================
-- INDICES
-- ============================================================================

-- Core note indices
CREATE INDEX idx_note_created_at ON note(created_at_utc DESC);
CREATE INDEX idx_note_updated_at ON note(updated_at_utc DESC);
CREATE INDEX idx_note_collection ON note(collection_id);
CREATE INDEX idx_note_starred ON note(starred) WHERE starred = TRUE;
CREATE INDEX idx_note_archived ON note(archived) WHERE archived = TRUE;
CREATE INDEX idx_note_last_accessed ON note(last_accessed_at DESC NULLS LAST);
CREATE INDEX idx_note_access_count ON note(access_count DESC);
CREATE INDEX idx_note_title ON note(title);
CREATE INDEX idx_note_deleted_at ON note(deleted_at) WHERE deleted_at IS NULL;

-- Security field indices
CREATE INDEX idx_note_owner ON note(owner_id) WHERE owner_id IS NOT NULL;
CREATE INDEX idx_note_tenant ON note(tenant_id) WHERE tenant_id IS NOT NULL;
CREATE INDEX idx_note_visibility ON note(visibility);
CREATE INDEX idx_note_security_compound ON note(tenant_id, owner_id, visibility);

-- Share grant indices
CREATE INDEX idx_share_grant_note ON note_share_grant(note_id);
CREATE INDEX idx_share_grant_grantee ON note_share_grant(grantee_id);
-- Active grants: not explicitly revoked (expiration checked at query time)
CREATE INDEX idx_share_grant_active ON note_share_grant(grantee_id, note_id)
  WHERE revoked_at IS NULL;

-- Note original indices
CREATE INDEX idx_note_original_user_edited ON note_original(user_last_edited_at DESC);

-- Link indices
CREATE INDEX idx_link_from_note ON link(from_note_id);
CREATE INDEX idx_link_to_note ON link(to_note_id);
CREATE INDEX idx_link_kind ON link(kind);
CREATE INDEX idx_link_metadata ON link USING GIN (metadata);

-- Revision indices
CREATE INDEX idx_revision_note ON note_revision(note_id, revision_number DESC);
CREATE INDEX idx_note_revision_ai_generated ON note_revision(ai_generated_at DESC);
CREATE INDEX idx_note_revision_user_edited ON note_revision(user_last_edited_at DESC) WHERE user_last_edited_at IS NOT NULL;

-- Tag indices
CREATE INDEX idx_note_tag_note ON note_tag(note_id);
CREATE INDEX idx_note_tag_tag ON note_tag(tag_name);

-- User metadata label indices
CREATE INDEX idx_user_metadata_label_note_id ON user_metadata_label(note_id);
CREATE INDEX idx_user_metadata_label_label ON user_metadata_label(label);
CREATE INDEX idx_user_metadata_label_tsv ON user_metadata_label
  USING gin (to_tsvector('english', label));
CREATE UNIQUE INDEX idx_user_metadata_label_unique ON user_metadata_label(note_id, label);

-- Job queue indices
CREATE INDEX idx_job_queue_status ON job_queue(status);
CREATE INDEX idx_job_queue_priority ON job_queue(priority DESC, created_at ASC) WHERE status = 'pending';
CREATE INDEX idx_job_queue_note_id ON job_queue(note_id);
CREATE INDEX idx_job_queue_created_at ON job_queue(created_at);
CREATE INDEX idx_job_queue_completed_at ON job_queue(completed_at DESC)
  WHERE status IN ('completed', 'failed');
CREATE INDEX idx_job_history_type_created ON job_history(job_type, created_at DESC);

-- JSONB metadata indices
CREATE INDEX idx_note_metadata ON note USING gin (metadata);
CREATE INDEX idx_note_metadata_tags ON note USING gin ((metadata->'tags'));
CREATE INDEX idx_note_metadata_tags_exists ON note ((metadata ? 'tags'));

-- Full-text search indices
CREATE INDEX idx_note_original_fts ON note_original
  USING gin (to_tsvector('english', content));
CREATE INDEX idx_note_revised_tsv ON note_revised_current
  USING gin (tsv);

-- Activity log indices
CREATE INDEX idx_activity_log_note ON activity_log(note_id);
CREATE INDEX idx_activity_log_at ON activity_log(at_utc DESC);
CREATE INDEX idx_activity_log_actor ON activity_log(actor);

-- Embedding indices
CREATE INDEX idx_embedding_note ON embedding(note_id);
CREATE INDEX idx_embedding_vector ON embedding USING ivfflat (vector vector_cosine_ops);

-- Provenance indices
CREATE INDEX idx_provenance_revision ON provenance_edge(revision_id);
CREATE INDEX idx_provenance_source_note ON provenance_edge(source_note_id);

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Function to calculate estimated job duration based on history
CREATE OR REPLACE FUNCTION estimate_job_duration(p_job_type job_type, p_payload_size INTEGER DEFAULT NULL)
RETURNS INTEGER AS $$
DECLARE
    avg_duration INTEGER;
BEGIN
    SELECT AVG(duration_ms)::INTEGER INTO avg_duration
    FROM (
        SELECT duration_ms
        FROM job_history
        WHERE job_type = p_job_type
        AND success = true
        ORDER BY created_at DESC
        LIMIT 10
    ) recent_jobs;

    IF avg_duration IS NULL THEN
        CASE p_job_type
            WHEN 'ai_revision' THEN avg_duration := 15000;
            WHEN 'embedding' THEN avg_duration := 5000;
            WHEN 'linking' THEN avg_duration := 3000;
            WHEN 'context_update' THEN avg_duration := 10000;
            WHEN 'title_generation' THEN avg_duration := 5000;
            ELSE avg_duration := 8000;
        END CASE;
    END IF;

    RETURN avg_duration;
END;
$$ LANGUAGE plpgsql;

-- Function to update access tracking
CREATE OR REPLACE FUNCTION update_note_access(note_id_param UUID)
RETURNS VOID AS $$
BEGIN
    UPDATE note
    SET
        last_accessed_at = NOW(),
        access_count = access_count + 1
    WHERE id = note_id_param;
END;
$$ LANGUAGE plpgsql;

-- Function to update user_last_edited_at on note_original changes
CREATE OR REPLACE FUNCTION update_original_edited_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.user_last_edited_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Function to track user edits on revisions
CREATE OR REPLACE FUNCTION track_revision_edit()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.content IS DISTINCT FROM NEW.content THEN
        NEW.user_last_edited_at = NOW();
        NEW.is_user_edited = TRUE;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Function to cleanup old jobs (keep last 100)
CREATE OR REPLACE FUNCTION cleanup_old_jobs()
RETURNS void AS $$
BEGIN
    DELETE FROM job_queue
    WHERE id NOT IN (
        SELECT id FROM job_queue
        ORDER BY
            CASE
                WHEN status IN ('pending', 'running') THEN 0
                ELSE 1
            END,
            completed_at DESC NULLS LAST
        LIMIT 100
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- VIEWS
-- ============================================================================

-- View for active and pending jobs with estimates
CREATE VIEW job_queue_status AS
SELECT
    jq.*,
    n.metadata->>'title' as note_title,
    CASE
        WHEN jq.status = 'running' THEN
            GREATEST(0, 100 - jq.progress_percent) * jq.estimated_duration_ms / 100
        WHEN jq.status = 'pending' THEN
            jq.estimated_duration_ms
        ELSE 0
    END as remaining_ms,
    CASE
        WHEN jq.status = 'pending' THEN
            (SELECT COALESCE(SUM(estimated_duration_ms), 0)
             FROM job_queue jq2
             WHERE jq2.status = 'pending'
             AND (jq2.priority > jq.priority
                  OR (jq2.priority = jq.priority AND jq2.created_at < jq.created_at)))
        ELSE 0
    END as queue_wait_ms
FROM job_queue jq
LEFT JOIN note n ON jq.note_id = n.id
WHERE jq.status IN ('pending', 'running')
ORDER BY
    CASE WHEN jq.status = 'running' THEN 0 ELSE 1 END,
    jq.priority DESC,
    jq.created_at ASC;

-- View for archived notes
CREATE VIEW archived_notes_view AS
SELECT
    n.id,
    n.collection_id,
    n.format,
    n.source,
    n.created_at_utc,
    n.updated_at_utc,
    n.starred,
    n.archived,
    no.content as original_content,
    nr.content as revised_content
FROM note n
LEFT JOIN note_original no ON n.id = no.note_id
LEFT JOIN note_revised_current nr ON n.id = nr.note_id
WHERE n.archived = TRUE;

-- ============================================================================
-- TRIGGERS
-- ============================================================================

-- Trigger to update user_last_edited_at on note_original changes
CREATE TRIGGER update_original_edited
BEFORE UPDATE OF content ON note_original
FOR EACH ROW
WHEN (OLD.content IS DISTINCT FROM NEW.content)
EXECUTE FUNCTION update_original_edited_timestamp();

-- Trigger to track user edits on revisions
CREATE TRIGGER track_revision_user_edit
BEFORE UPDATE OF content ON note_revision
FOR EACH ROW
EXECUTE FUNCTION track_revision_edit();

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE note IS 'Main note table storing metadata and relationships';
COMMENT ON TABLE note_original IS 'Immutable original content of notes';
COMMENT ON TABLE note_revised_current IS 'Current AI-enhanced revision of notes';
COMMENT ON TABLE note_revision IS 'Complete revision history with rationales';
COMMENT ON TABLE job_queue IS 'Queue for ML pipeline operations with single-GPU constraint';
COMMENT ON TABLE user_metadata_label IS 'User-defined labels for custom note organization';
COMMENT ON TABLE embedding IS 'Vector embeddings for semantic search (768-dim, pgvector)';
