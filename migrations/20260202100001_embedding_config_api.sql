-- ============================================================================
-- Dynamic Embedding Config API Migration (#392)
-- ============================================================================
-- Adds provider support to embedding_config table for multi-provider
-- embedding generation (Ollama, OpenAI, Voyage, Cohere, Custom).
--
-- Related: ADR-026 (Dynamic Embedding Config API)
-- ============================================================================

-- ============================================================================
-- PART 1: PROVIDER ENUM
-- ============================================================================

-- Provider enum for embedding generation
CREATE TYPE embedding_provider AS ENUM (
    'ollama',     -- Local Ollama instance (default)
    'openai',     -- OpenAI API
    'voyage',     -- Voyage AI
    'cohere',     -- Cohere API
    'custom'      -- Custom HTTP endpoint
);

-- ============================================================================
-- PART 2: ADD PROVIDER COLUMNS
-- ============================================================================

-- Add provider support to embedding_config
ALTER TABLE embedding_config
    ADD COLUMN IF NOT EXISTS provider embedding_provider DEFAULT 'ollama',
    ADD COLUMN IF NOT EXISTS provider_config JSONB DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS content_types TEXT[] DEFAULT '{}';

-- ============================================================================
-- PART 3: INDICES
-- ============================================================================

-- Index for content type queries
CREATE INDEX IF NOT EXISTS idx_embedding_config_content_types
    ON embedding_config USING GIN(content_types);

-- Index for provider queries
CREATE INDEX IF NOT EXISTS idx_embedding_config_provider
    ON embedding_config(provider);

-- ============================================================================
-- PART 4: UPDATE EXISTING CONFIGS
-- ============================================================================

-- Set provider to 'ollama' for all existing configs (default)
UPDATE embedding_config
SET provider = 'ollama'
WHERE provider IS NULL;

-- ============================================================================
-- PART 5: COMMENTS
-- ============================================================================

COMMENT ON TYPE embedding_provider IS 'Embedding generation provider: ollama (local), openai, voyage, cohere, or custom HTTP endpoint';
COMMENT ON COLUMN embedding_config.provider IS 'Embedding generation provider (ollama, openai, voyage, cohere, custom)';
COMMENT ON COLUMN embedding_config.provider_config IS 'Provider-specific configuration (API key env var, base URL, etc.)';
COMMENT ON COLUMN embedding_config.content_types IS 'Content types this config is optimized for (e.g., code, text, multilingual)';
