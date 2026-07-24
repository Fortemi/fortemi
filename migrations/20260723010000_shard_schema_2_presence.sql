-- Schema 2.0 direct-key-presence support.
--
-- Most authority fields have an unambiguous relational representation:
-- schema-required nullable fields serialize as null, optional non-nullable
-- fields serialize as absent, and empty/value states retain their type and
-- value. These are the only two record fields where both absent and null are
-- valid distinct states, so their own-property bit must live beside the value.

ALTER TABLE note
    ADD COLUMN shard_deleted_at_present BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE embedding
    ADD COLUMN shard_contract_fingerprint_present BOOLEAN NOT NULL DEFAULT TRUE;

COMMENT ON COLUMN note.shard_deleted_at_present IS
    'Schema 2.0 own-property bit for notes.deleted_at (absent versus explicit null/value).';

COMMENT ON COLUMN embedding.shard_contract_fingerprint_present IS
    'Schema 2.0 own-property bit for embeddings.contract_fingerprint (absent versus explicit null/value).';
