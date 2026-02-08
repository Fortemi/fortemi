-- Fix #238: SKOS concept deleted during collection ops
--
-- The skos_collection_member table had ON DELETE CASCADE on concept_id FK,
-- which caused concepts to be deleted when removed from a collection.
-- The cascade should only go FROM collection → members (parent → child),
-- not from membership → concept (child → parent).
--
-- Change concept_id FK from ON DELETE CASCADE to ON DELETE RESTRICT.

ALTER TABLE skos_collection_member
  DROP CONSTRAINT skos_collection_member_concept_id_fkey;

ALTER TABLE skos_collection_member
  ADD CONSTRAINT skos_collection_member_concept_id_fkey
  FOREIGN KEY (concept_id) REFERENCES skos_concept(id) ON DELETE RESTRICT;
