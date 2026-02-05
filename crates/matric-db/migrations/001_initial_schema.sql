-- Initial schema for matric-memory
-- Requires pgvector extension

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Job types enum
CREATE TYPE job_type AS ENUM (
    'ai_revision',
    'embedding',
    'linking',
    'context_update',
    'title_generation'
);

-- Job status enum
CREATE TYPE job_status AS ENUM (
    'pending',
    'running',
    'completed',
    'failed',
    'cancelled'
);

-- Collections table
CREATE TABLE IF NOT EXISTS collection (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Notes table
CREATE TABLE IF NOT EXISTS note (
    id UUID PRIMARY KEY,
    format TEXT NOT NULL DEFAULT 'markdown',
    source TEXT NOT NULL DEFAULT 'user',
    collection_id UUID REFERENCES collection(id) ON DELETE SET NULL,
    starred BOOLEAN NOT NULL DEFAULT FALSE,
    archived BOOLEAN NOT NULL DEFAULT FALSE,
    title TEXT,
    metadata JSONB,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ,
    access_count INTEGER NOT NULL DEFAULT 0,
    deleted_at TIMESTAMPTZ
);

-- Note content (original user input)
CREATE TABLE IF NOT EXISTS note_original (
    id UUID PRIMARY KEY,
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    hash TEXT NOT NULL,
    user_created_at TIMESTAMPTZ,
    user_last_edited_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Note revisions (AI-enhanced versions)
CREATE TABLE IF NOT EXISTS note_revision (
    id UUID PRIMARY KEY,
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    rationale TEXT,
    model TEXT,
    ai_metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- View for current revised content
CREATE OR REPLACE VIEW note_revised_current AS
SELECT DISTINCT ON (note_id)
    note_id,
    id as last_revision_id,
    content,
    ai_metadata,
    to_tsvector('english', content) as tsv
FROM note_revision
ORDER BY note_id, created_at DESC;

-- Tags table
CREATE TABLE IF NOT EXISTS tag (
    name TEXT PRIMARY KEY,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Note-tag associations
CREATE TABLE IF NOT EXISTS note_tag (
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    tag_name TEXT NOT NULL REFERENCES tag(name) ON DELETE CASCADE,
    source TEXT NOT NULL DEFAULT 'user',
    PRIMARY KEY (note_id, tag_name)
);

-- Links between notes
CREATE TABLE IF NOT EXISTS link (
    id UUID PRIMARY KEY,
    from_note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    to_note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    to_url TEXT,
    kind TEXT NOT NULL DEFAULT 'related',
    score REAL NOT NULL DEFAULT 0.0,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB
);

-- Embeddings table with vector column
CREATE TABLE IF NOT EXISTS embedding (
    id UUID PRIMARY KEY,
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL DEFAULT 0,
    text TEXT NOT NULL,
    vector vector(768),
    model TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Job queue table
CREATE TABLE IF NOT EXISTS job_queue (
    id UUID PRIMARY KEY,
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    job_type job_type NOT NULL,
    status job_status NOT NULL DEFAULT 'pending',
    priority INTEGER NOT NULL DEFAULT 0,
    payload JSONB,
    result JSONB,
    error_message TEXT,
    progress_percent INTEGER NOT NULL DEFAULT 0,
    progress_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    estimated_duration_ms INTEGER,
    actual_duration_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

-- Job history for statistics
CREATE TABLE IF NOT EXISTS job_history (
    id UUID PRIMARY KEY,
    job_type job_type NOT NULL,
    duration_ms INTEGER NOT NULL,
    payload_size INTEGER,
    success BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Function to estimate job duration based on history
CREATE OR REPLACE FUNCTION estimate_job_duration(p_job_type job_type, p_payload_size INTEGER)
RETURNS INTEGER AS $$
DECLARE
    avg_duration INTEGER;
BEGIN
    SELECT COALESCE(AVG(duration_ms)::INTEGER, 5000)
    INTO avg_duration
    FROM job_history
    WHERE job_type = p_job_type
      AND success = TRUE
      AND created_at > NOW() - INTERVAL '30 days';

    RETURN avg_duration;
END;
$$ LANGUAGE plpgsql;

-- Activity log table
CREATE TABLE IF NOT EXISTS activity_log (
    id UUID PRIMARY KEY,
    at_utc TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    note_id UUID REFERENCES note(id) ON DELETE CASCADE,
    meta JSONB NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_activity_log_note ON activity_log(note_id);
CREATE INDEX IF NOT EXISTS idx_activity_log_at ON activity_log(at_utc DESC);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_note_collection ON note(collection_id);
CREATE INDEX IF NOT EXISTS idx_note_starred ON note(starred) WHERE starred = TRUE;
CREATE INDEX IF NOT EXISTS idx_note_archived ON note(archived) WHERE archived = TRUE;
CREATE INDEX IF NOT EXISTS idx_note_deleted ON note(deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_note_created ON note(created_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_note_updated ON note(updated_at_utc DESC);
CREATE INDEX IF NOT EXISTS idx_note_accessed ON note(last_accessed_at DESC);

CREATE INDEX IF NOT EXISTS idx_note_original_note ON note_original(note_id);
CREATE INDEX IF NOT EXISTS idx_note_revision_note ON note_revision(note_id);
CREATE INDEX IF NOT EXISTS idx_note_revision_created ON note_revision(note_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_note_tag_note ON note_tag(note_id);
CREATE INDEX IF NOT EXISTS idx_note_tag_tag ON note_tag(tag_name);

CREATE INDEX IF NOT EXISTS idx_link_from ON link(from_note_id);
CREATE INDEX IF NOT EXISTS idx_link_to ON link(to_note_id);

CREATE INDEX IF NOT EXISTS idx_embedding_note ON embedding(note_id);

-- Vector similarity index (HNSW for fast approximate search)
CREATE INDEX IF NOT EXISTS idx_embedding_vector ON embedding USING hnsw (vector vector_cosine_ops);

-- Full-text search index on revised content
CREATE INDEX IF NOT EXISTS idx_revision_fts ON note_revision USING GIN (to_tsvector('english', content));

CREATE INDEX IF NOT EXISTS idx_job_queue_pending ON job_queue(priority DESC, created_at) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_job_queue_note ON job_queue(note_id);
CREATE INDEX IF NOT EXISTS idx_job_queue_type ON job_queue(job_type);
