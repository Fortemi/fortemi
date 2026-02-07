-- PKE public key registry (Issue #113)
-- Stores public keys indexed by PKE address for secure note sharing

CREATE TABLE pke_public_keys (
  address TEXT PRIMARY KEY,
  public_key BYTEA NOT NULL,
  label TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pke_public_keys_label ON pke_public_keys(label);

COMMENT ON TABLE pke_public_keys IS 'PKE public key registry for secure note sharing (Issue #113)';
COMMENT ON COLUMN pke_public_keys.address IS 'PKE address (unique identifier for the key)';
COMMENT ON COLUMN pke_public_keys.public_key IS 'Raw public key bytes';
COMMENT ON COLUMN pke_public_keys.label IS 'Human-readable label for the key';
