-- Note templates for reusable note structures

CREATE TABLE note_template (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            VARCHAR(255) NOT NULL UNIQUE,
    description     TEXT,
    content         TEXT NOT NULL,
    format          VARCHAR(50) NOT NULL DEFAULT 'markdown',
    default_tags    TEXT[],
    collection_id   UUID REFERENCES collection(id) ON DELETE SET NULL,
    created_at_utc  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at_utc  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for listing templates
CREATE INDEX idx_template_name ON note_template(name);
