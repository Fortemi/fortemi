-- Provider-agnostic real-time call session persistence (issues #839/#845).

CREATE TABLE IF NOT EXISTS call_sessions (
    call_id UUID PRIMARY KEY DEFAULT uuidv7(),
    provider TEXT NOT NULL,
    provider_call_id TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    end_reason TEXT,
    asr_backend TEXT,
    remote_party TEXT,
    archive_id UUID REFERENCES archive_registry(id) ON DELETE SET NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (provider, provider_call_id)
);

CREATE INDEX IF NOT EXISTS idx_call_sessions_started_at_desc
    ON call_sessions (started_at DESC);

CREATE INDEX IF NOT EXISTS idx_call_sessions_provider
    ON call_sessions (provider);

CREATE TABLE IF NOT EXISTS transcript_segments (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    call_id UUID NOT NULL REFERENCES call_sessions(call_id) ON DELETE CASCADE,
    speaker_label TEXT,
    text TEXT NOT NULL,
    start_ts DOUBLE PRECISION,
    end_ts DOUBLE PRECISION,
    confidence REAL,
    sequence INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_transcript_segments_call_id_sequence
    ON transcript_segments (call_id, sequence);
