-- Fortemi #1174 R1: embedding space contract identity.
--
-- Store the verbatim embedding-space contract JSON on embedding_config, along
-- with its canonical SHA-256 space_id. Embedding sets already inherit model,
-- dimension, vector_type, provider and storage behavior through
-- embedding_set.embedding_config_id, so config-level storage keeps one
-- authority for the space contract and avoids per-set drift. Set read APIs
-- surface the inherited values by joining their config.
--
-- No uniqueness rule is added: distinct configs may intentionally share the
-- same embedding space while varying chunking, HNSW tuning, document
-- composition, availability metadata, or rollout status.

ALTER TABLE public.embedding_config
    ADD COLUMN IF NOT EXISTS space_contract JSONB,
    ADD COLUMN IF NOT EXISTS space_id TEXT;

DO $embedding_space_contract_checks$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.embedding_config'::regclass
          AND conname = 'embedding_config_space_id_sha256'
    ) THEN
        ALTER TABLE public.embedding_config
            ADD CONSTRAINT embedding_config_space_id_sha256
            CHECK (
                space_id IS NULL
                OR space_id ~ '^[0-9a-f]{64}$'
            );
    END IF;
END
$embedding_space_contract_checks$;

CREATE INDEX IF NOT EXISTS idx_embedding_config_space_id
    ON public.embedding_config (space_id)
    WHERE space_id IS NOT NULL;

COMMENT ON COLUMN public.embedding_config.space_contract IS
    'Embedding-space contract JSON used to derive space_id; NULL for legacy or unconstrained configs';

COMMENT ON COLUMN public.embedding_config.space_id IS
    'SHA-256 lowercase hex of the canonical embedding-space contract JSON; NULL when no contract is declared';
