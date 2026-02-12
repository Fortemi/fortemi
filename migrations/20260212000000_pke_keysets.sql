-- PKE keysets table for REST API (Issues #328, #332)
-- Stores named keysets with encrypted private keys for PKE operations

CREATE TABLE pke_keysets (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name TEXT NOT NULL UNIQUE,
  public_key BYTEA NOT NULL,
  encrypted_private_key BYTEA NOT NULL,
  address TEXT NOT NULL,
  label TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Track which keyset is active (only one can be active at a time)
CREATE TABLE pke_active_keyset (
  id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1), -- Singleton row
  keyset_id UUID REFERENCES pke_keysets(id) ON DELETE SET NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert singleton row
INSERT INTO pke_active_keyset (id, keyset_id) VALUES (1, NULL);

CREATE INDEX idx_pke_keysets_name ON pke_keysets(name);
CREATE INDEX idx_pke_keysets_address ON pke_keysets(address);

COMMENT ON TABLE pke_keysets IS 'Named PKE keysets for REST API encryption operations (Issues #328, #332)';
COMMENT ON COLUMN pke_keysets.name IS 'Unique keyset name (alphanumeric, hyphens, underscores)';
COMMENT ON COLUMN pke_keysets.public_key IS 'Raw public key bytes (32 bytes for X25519)';
COMMENT ON COLUMN pke_keysets.encrypted_private_key IS 'Passphrase-encrypted private key';
COMMENT ON COLUMN pke_keysets.address IS 'PKE address (mm:...) derived from public key';
COMMENT ON TABLE pke_active_keyset IS 'Singleton table tracking which keyset is currently active';
