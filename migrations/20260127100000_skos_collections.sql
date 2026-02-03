-- SKOS Collections and Ordered Collections (W3C SKOS Reference, REF-033)
--
-- Collections group concepts without imposing hierarchy, complementing
-- ConceptSchemes which provide vocabulary namespaces. Ordered collections
-- preserve sequence for workflows and learning paths.

-- Collections table
CREATE TABLE IF NOT EXISTS skos_collection (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    uri TEXT UNIQUE,
    pref_label TEXT NOT NULL,
    definition TEXT,
    is_ordered BOOLEAN NOT NULL DEFAULT FALSE,
    scheme_id UUID REFERENCES skos_concept_scheme(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Collection membership (concepts in a collection)
CREATE TABLE IF NOT EXISTS skos_collection_member (
    collection_id UUID NOT NULL REFERENCES skos_collection(id) ON DELETE CASCADE,
    concept_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,
    position INTEGER,  -- For ordered collections; NULL for unordered
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (collection_id, concept_id)
);

-- Index for efficient ordered retrieval
CREATE INDEX IF NOT EXISTS idx_skos_collection_member_position
    ON skos_collection_member(collection_id, position)
    WHERE position IS NOT NULL;

-- Index for scheme-based collection listing
CREATE INDEX IF NOT EXISTS idx_skos_collection_scheme
    ON skos_collection(scheme_id)
    WHERE scheme_id IS NOT NULL;

-- Index for collection label search
CREATE INDEX IF NOT EXISTS idx_skos_collection_label
    ON skos_collection(pref_label);

-- Auto-update updated_at on collection changes
CREATE OR REPLACE FUNCTION update_skos_collection_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_skos_collection_updated
    BEFORE UPDATE ON skos_collection
    FOR EACH ROW
    EXECUTE FUNCTION update_skos_collection_timestamp();
