-- TI-16: note.collection_id historically had no foreign key. Add both the
-- ordinary relationship and the tenant-qualified guard without making NULL
-- collection ids invalid.

CREATE UNIQUE INDEX IF NOT EXISTS uq_collection_tenant_id_id
    ON collection (tenant_id, id);

ALTER TABLE note
    ADD CONSTRAINT fk_note_collection
    FOREIGN KEY (collection_id)
    REFERENCES collection(id)
    ON DELETE SET NULL
    NOT VALID;

ALTER TABLE note VALIDATE CONSTRAINT fk_note_collection;

ALTER TABLE note
    ADD CONSTRAINT fk_note_tenant_collection
    FOREIGN KEY (tenant_id, collection_id)
    REFERENCES collection(tenant_id, id)
    NOT VALID;

ALTER TABLE note VALIDATE CONSTRAINT fk_note_tenant_collection;
