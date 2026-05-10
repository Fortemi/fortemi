-- Per-archive inference provider override (Issue #655)
--
-- Stores schema-scoped overrides on top of the global inference config.
-- Resolution precedence: archive_override > db_override > env > default.
--
-- Lives in the public schema (NOT per-memory) because this is a routing
-- table — one row per archive that has an override. Archives without an
-- entry simply fall back to the global config from the existing
-- user_config.inference_override row.
--
-- The blob shape mirrors user_config.inference_override:
--   {
--     "default_backend": "openrouter",
--     "embedding_backend": "ollama",
--     "ollama":     { "base_url": "...", "generation_model": "..." },
--     "openai":     { "api_key": "...", ... },
--     "openrouter": { "api_key": "...", ... },
--     "llamacpp":   { "base_url": "...", ... }
--   }
-- so the existing build_effective_config helper can layer it.
--
-- API keys are persisted as supplied by the operator (raw, in JSONB).
-- The runtime redacts them on read via the same redact_api_key helper
-- the global config uses; reads through GET /api/v1/inference/config
-- with X-Fortemi-Memory return redacted values.

CREATE TABLE IF NOT EXISTS archive_inference_override (
    schema_name TEXT PRIMARY KEY,
    -- The override blob — same shape as user_config.inference_override.
    override JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for "show me everything overridden" admin views.
CREATE INDEX idx_archive_inference_override_updated_at
    ON archive_inference_override (updated_at DESC);
