-- Persist the canonical non-secret embedding-space identity (#995).
ALTER TABLE embedding
    ADD COLUMN IF NOT EXISTS contract_fingerprint CHAR(64);

ALTER TABLE embedding
    DROP CONSTRAINT IF EXISTS embedding_contract_fingerprint_sha256;

ALTER TABLE embedding
    ADD CONSTRAINT embedding_contract_fingerprint_sha256
    CHECK (
        contract_fingerprint IS NULL
        OR contract_fingerprint ~ '^[0-9a-f]{64}$'
    );

COMMENT ON COLUMN embedding.contract_fingerprint IS
    'SHA-256 of provider/model/dimension/normalization/embedding-set contract; NULL for legacy rows';
