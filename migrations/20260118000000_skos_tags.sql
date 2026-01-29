-- ============================================================================
-- matric-memory Database Schema - W3C SKOS-Compliant Hierarchical Tag System
-- ============================================================================
-- This migration implements a full W3C SKOS-compliant knowledge organization
-- system with PMEST facets, validation rules, and anti-pattern detection.
--
-- Version: 0.2.0
-- Generated: 2026-01-18
-- Standards: W3C SKOS (Simple Knowledge Organization System)
-- Reference: https://www.w3.org/TR/skos-reference/
-- ============================================================================

-- ============================================================================
-- CONFIGURATION CONSTANTS (enforced via constraints and triggers)
-- ============================================================================
-- MAX_DEPTH: 5 levels
-- MAX_BREADTH: 10 children per node
-- MAX_POLYHIERARCHY: 3 parents
-- LITERARY_WARRANT: 3+ notes before promotion

-- ============================================================================
-- ENUMS FOR SKOS TYPES
-- ============================================================================

-- SKOS semantic relation types
CREATE TYPE skos_semantic_relation AS ENUM (
    'broader',           -- skos:broader (hierarchical)
    'narrower',          -- skos:narrower (inverse of broader)
    'related'            -- skos:related (associative, non-hierarchical)
);

-- SKOS mapping relation types for cross-vocabulary links
CREATE TYPE skos_mapping_relation AS ENUM (
    'exact_match',       -- skos:exactMatch (equivalence)
    'close_match',       -- skos:closeMatch (near equivalence)
    'broad_match',       -- skos:broadMatch (broader in external vocab)
    'narrow_match',      -- skos:narrowMatch (narrower in external vocab)
    'related_match'      -- skos:relatedMatch (related in external vocab)
);

-- SKOS label types
CREATE TYPE skos_label_type AS ENUM (
    'pref_label',        -- skos:prefLabel (preferred, max 1 per lang)
    'alt_label',         -- skos:altLabel (alternative)
    'hidden_label'       -- skos:hiddenLabel (for search, not display)
);

-- SKOS documentation types
CREATE TYPE skos_note_type AS ENUM (
    'definition',        -- skos:definition
    'scope_note',        -- skos:scopeNote
    'example',           -- skos:example
    'history_note',      -- skos:historyNote
    'editorial_note',    -- skos:editorialNote
    'change_note',       -- skos:changeNote
    'note'               -- skos:note (general)
);

-- PMEST Facet types (Ranganathan's classification)
CREATE TYPE pmest_facet AS ENUM (
    'personality',       -- What: the most specific subject
    'matter',            -- Material/substance
    'energy',            -- Process/activity
    'space',             -- Location/geography
    'time'               -- Temporal aspect
);

-- Tag status for workflow
CREATE TYPE tag_status AS ENUM (
    'candidate',         -- Proposed, not yet approved
    'approved',          -- Approved for use
    'deprecated',        -- Marked for removal
    'obsolete'           -- No longer valid, kept for history
);

-- Anti-pattern types for governance
CREATE TYPE tag_antipattern AS ENUM (
    'orphan',            -- No hierarchical connections
    'over_tagged',       -- Too many tags on single resource
    'under_used',        -- Tag rarely applied
    'too_broad',         -- Excessive narrower concepts
    'too_deep',          -- Exceeds depth limit
    'polyhierarchy_excess', -- Too many broader concepts
    'missing_labels',    -- Lacks required labels
    'circular_hierarchy' -- Circular broader/narrower chain
);

-- ============================================================================
-- CORE SKOS TABLES
-- ============================================================================

-- Concept Scheme: namespace/vocabulary container (skos:ConceptScheme)
CREATE TABLE skos_concept_scheme (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Identification
    uri TEXT UNIQUE,                          -- Canonical URI (e.g., "https://matric.io/schemes/topics")
    notation TEXT UNIQUE NOT NULL,            -- Short code (e.g., "topics", "domains")

    -- Metadata
    title TEXT NOT NULL,
    description TEXT,
    creator TEXT,
    publisher TEXT,
    rights TEXT,
    version TEXT DEFAULT '1.0.0',

    -- Status
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_system BOOLEAN NOT NULL DEFAULT FALSE, -- Protected system schemes

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    issued_at TIMESTAMPTZ,                    -- Official publication date
    modified_at TIMESTAMPTZ,                  -- Last content modification

    -- Embedding support
    embedding vector(768),
    embedding_model TEXT,
    embedded_at TIMESTAMPTZ
);

-- SKOS Concept: the core tag/concept entity
CREATE TABLE skos_concept (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Scheme membership (a concept can be in multiple schemes)
    -- Primary scheme is stored here; additional via junction table
    primary_scheme_id UUID NOT NULL REFERENCES skos_concept_scheme(id) ON DELETE RESTRICT,

    -- Identification
    uri TEXT UNIQUE,                          -- Canonical URI
    notation TEXT,                            -- Short code within scheme

    -- PMEST Facets
    facet_type pmest_facet,                   -- Primary facet classification
    facet_source TEXT,                        -- Domain/context for facet
    facet_domain TEXT,                        -- Subject domain
    facet_scope TEXT,                         -- Scope description

    -- Status and workflow
    status tag_status NOT NULL DEFAULT 'candidate',
    promoted_at TIMESTAMPTZ,                  -- When status changed to approved
    deprecated_at TIMESTAMPTZ,
    deprecation_reason TEXT,
    replaced_by_id UUID REFERENCES skos_concept(id), -- For deprecated concepts

    -- Literary warrant tracking
    note_count INTEGER NOT NULL DEFAULT 0,    -- Cached count of tagged notes
    first_used_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,

    -- Hierarchy metadata (denormalized for performance)
    depth INTEGER NOT NULL DEFAULT 0,         -- Distance from root (0 = top concept)
    broader_count INTEGER NOT NULL DEFAULT 0, -- Number of broader concepts
    narrower_count INTEGER NOT NULL DEFAULT 0,-- Number of narrower concepts
    related_count INTEGER NOT NULL DEFAULT 0, -- Number of related concepts

    -- Anti-pattern flags (computed by trigger/job)
    antipatterns tag_antipattern[] DEFAULT '{}',
    antipattern_checked_at TIMESTAMPTZ,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Embedding for semantic search
    embedding vector(768),
    embedding_model TEXT,
    embedded_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT valid_depth CHECK (depth >= 0 AND depth <= 5),
    CONSTRAINT valid_broader_count CHECK (broader_count >= 0 AND broader_count <= 3),
    CONSTRAINT valid_notation UNIQUE (primary_scheme_id, notation)
);

-- Concept labels (multilingual support, SKOS label properties)
CREATE TABLE skos_concept_label (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    concept_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,

    -- Label properties
    label_type skos_label_type NOT NULL DEFAULT 'pref_label',
    value TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en',      -- ISO 639-1 language code

    -- Search optimization
    tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', value)) STORED,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ensure no duplicate labels for the same concept
    CONSTRAINT unique_label UNIQUE (concept_id, label_type, language, value)
);

-- Add partial unique index for pref_label constraint (PostgreSQL workaround)
CREATE UNIQUE INDEX idx_unique_pref_label
    ON skos_concept_label(concept_id, language)
    WHERE label_type = 'pref_label';

-- Concept documentation (SKOS note properties)
CREATE TABLE skos_concept_note (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    concept_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,

    -- Note properties
    note_type skos_note_type NOT NULL DEFAULT 'note',
    value TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en',

    -- Attribution
    author TEXT,
    source TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- SKOS RELATIONSHIPS
-- ============================================================================

-- Semantic relations between concepts (broader/narrower/related)
CREATE TABLE skos_semantic_relation_edge (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Relation endpoints
    subject_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,
    object_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,
    relation_type skos_semantic_relation NOT NULL,

    -- Metadata
    inference_score REAL,                     -- AI confidence if auto-generated
    is_inferred BOOLEAN NOT NULL DEFAULT FALSE,
    is_validated BOOLEAN NOT NULL DEFAULT TRUE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT,

    -- Constraints
    CONSTRAINT no_self_relation CHECK (subject_id != object_id),
    CONSTRAINT unique_relation UNIQUE (subject_id, object_id, relation_type)
);

-- Mapping relations to external vocabularies
CREATE TABLE skos_mapping_relation_edge (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Internal concept
    concept_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,

    -- External target (URI or local reference)
    target_uri TEXT NOT NULL,                 -- External concept URI
    target_scheme_uri TEXT,                   -- External scheme URI
    target_label TEXT,                        -- Cached label for display

    -- Relation type
    relation_type skos_mapping_relation NOT NULL,

    -- Metadata
    confidence REAL,                          -- Match confidence (0-1)
    is_validated BOOLEAN NOT NULL DEFAULT FALSE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    validated_at TIMESTAMPTZ,
    validated_by TEXT,

    -- Constraints
    CONSTRAINT unique_mapping UNIQUE (concept_id, target_uri, relation_type)
);

-- Concept-to-scheme membership (for concepts in multiple schemes)
CREATE TABLE skos_concept_in_scheme (
    concept_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,
    scheme_id UUID NOT NULL REFERENCES skos_concept_scheme(id) ON DELETE CASCADE,

    -- Top concept flag (scheme entry points)
    is_top_concept BOOLEAN NOT NULL DEFAULT FALSE,

    -- Timestamps
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (concept_id, scheme_id)
);

-- ============================================================================
-- NOTE-TAG RELATIONSHIP (Enhanced)
-- ============================================================================

-- Enhanced note-concept tagging with provenance
CREATE TABLE note_skos_concept (
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    concept_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE CASCADE,

    -- Provenance
    source TEXT NOT NULL DEFAULT 'manual',    -- 'manual', 'ai', 'import', 'rule'
    confidence REAL,                          -- AI confidence if auto-tagged

    -- Position/relevance
    relevance_score REAL DEFAULT 1.0,         -- How relevant to the note (0-1)
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,-- Primary/main tag

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT,

    PRIMARY KEY (note_id, concept_id)
);

-- ============================================================================
-- GOVERNANCE AND AUDIT
-- ============================================================================

-- Tag governance audit log
CREATE TABLE skos_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Target
    entity_type TEXT NOT NULL,                -- 'concept', 'scheme', 'relation'
    entity_id UUID NOT NULL,

    -- Action
    action TEXT NOT NULL,                     -- 'create', 'update', 'delete', 'merge', 'split'
    changes JSONB,                            -- Delta/diff of changes

    -- Attribution
    actor TEXT NOT NULL,
    actor_type TEXT NOT NULL DEFAULT 'user',  -- 'user', 'system', 'ai'

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Concept merge history (for tracking merged concepts)
CREATE TABLE skos_concept_merge (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Source concepts (merged into target)
    source_ids UUID[] NOT NULL,
    target_id UUID NOT NULL REFERENCES skos_concept(id) ON DELETE SET NULL,

    -- Metadata
    reason TEXT,
    performed_by TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- INDICES FOR PERFORMANCE
-- ============================================================================

-- Concept scheme indices
CREATE INDEX idx_skos_scheme_notation ON skos_concept_scheme(notation);
CREATE INDEX idx_skos_scheme_active ON skos_concept_scheme(is_active) WHERE is_active = TRUE;

-- Concept indices
CREATE INDEX idx_skos_concept_scheme ON skos_concept(primary_scheme_id);
CREATE INDEX idx_skos_concept_status ON skos_concept(status);
CREATE INDEX idx_skos_concept_facet ON skos_concept(facet_type) WHERE facet_type IS NOT NULL;
CREATE INDEX idx_skos_concept_depth ON skos_concept(depth);
CREATE INDEX idx_skos_concept_note_count ON skos_concept(note_count DESC);
CREATE INDEX idx_skos_concept_notation ON skos_concept(notation) WHERE notation IS NOT NULL;
CREATE INDEX idx_skos_concept_antipatterns ON skos_concept USING GIN (antipatterns);
CREATE INDEX idx_skos_concept_embedding ON skos_concept USING ivfflat (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;
CREATE INDEX idx_skos_concept_updated ON skos_concept(updated_at DESC);

-- Label indices
CREATE INDEX idx_skos_label_concept ON skos_concept_label(concept_id);
CREATE INDEX idx_skos_label_type ON skos_concept_label(label_type);
CREATE INDEX idx_skos_label_language ON skos_concept_label(language);
CREATE INDEX idx_skos_label_value ON skos_concept_label(value);
CREATE INDEX idx_skos_label_tsv ON skos_concept_label USING GIN (tsv);

-- Note indices
CREATE INDEX idx_skos_note_concept ON skos_concept_note(concept_id);
CREATE INDEX idx_skos_note_type ON skos_concept_note(note_type);

-- Semantic relation indices
CREATE INDEX idx_skos_rel_subject ON skos_semantic_relation_edge(subject_id);
CREATE INDEX idx_skos_rel_object ON skos_semantic_relation_edge(object_id);
CREATE INDEX idx_skos_rel_type ON skos_semantic_relation_edge(relation_type);
CREATE INDEX idx_skos_rel_broader ON skos_semantic_relation_edge(subject_id)
    WHERE relation_type = 'broader';
CREATE INDEX idx_skos_rel_narrower ON skos_semantic_relation_edge(subject_id)
    WHERE relation_type = 'narrower';

-- Mapping relation indices
CREATE INDEX idx_skos_mapping_concept ON skos_mapping_relation_edge(concept_id);
CREATE INDEX idx_skos_mapping_target ON skos_mapping_relation_edge(target_uri);
CREATE INDEX idx_skos_mapping_type ON skos_mapping_relation_edge(relation_type);

-- Concept-in-scheme indices
CREATE INDEX idx_skos_cis_concept ON skos_concept_in_scheme(concept_id);
CREATE INDEX idx_skos_cis_scheme ON skos_concept_in_scheme(scheme_id);
CREATE INDEX idx_skos_cis_top ON skos_concept_in_scheme(scheme_id) WHERE is_top_concept = TRUE;

-- Note-concept indices
CREATE INDEX idx_note_skos_note ON note_skos_concept(note_id);
CREATE INDEX idx_note_skos_concept ON note_skos_concept(concept_id);
CREATE INDEX idx_note_skos_primary ON note_skos_concept(note_id) WHERE is_primary = TRUE;
CREATE INDEX idx_note_skos_source ON note_skos_concept(source);

-- Audit log indices
CREATE INDEX idx_skos_audit_entity ON skos_audit_log(entity_type, entity_id);
CREATE INDEX idx_skos_audit_time ON skos_audit_log(created_at DESC);
CREATE INDEX idx_skos_audit_actor ON skos_audit_log(actor);

-- ============================================================================
-- VALIDATION FUNCTIONS
-- ============================================================================

-- Function to calculate concept depth via broader relations
CREATE OR REPLACE FUNCTION skos_calculate_depth(concept_uuid UUID)
RETURNS INTEGER AS $$
DECLARE
    max_depth INTEGER := 0;
    current_depth INTEGER;
    broader_id UUID;
BEGIN
    -- Find all paths to root via broader relations
    FOR broader_id IN
        SELECT object_id FROM skos_semantic_relation_edge
        WHERE subject_id = concept_uuid AND relation_type = 'broader'
    LOOP
        SELECT skos_calculate_depth(broader_id) + 1 INTO current_depth;
        IF current_depth > max_depth THEN
            max_depth := current_depth;
        END IF;
    END LOOP;

    RETURN max_depth;
END;
$$ LANGUAGE plpgsql;

-- Function to check for circular hierarchies
CREATE OR REPLACE FUNCTION skos_has_circular_hierarchy(concept_uuid UUID)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        WITH RECURSIVE hierarchy AS (
            -- Start with direct broader concepts
            SELECT object_id, ARRAY[concept_uuid] AS path
            FROM skos_semantic_relation_edge
            WHERE subject_id = concept_uuid AND relation_type = 'broader'

            UNION ALL

            -- Traverse up the hierarchy
            SELECT e.object_id, h.path || e.subject_id
            FROM skos_semantic_relation_edge e
            JOIN hierarchy h ON e.subject_id = h.object_id
            WHERE e.relation_type = 'broader'
              AND NOT e.object_id = ANY(h.path)  -- Prevent infinite loop
              AND array_length(h.path, 1) < 10   -- Safety limit
        )
        SELECT 1 FROM hierarchy WHERE object_id = concept_uuid
    );
END;
$$ LANGUAGE plpgsql;

-- Function to validate broader relation constraints
CREATE OR REPLACE FUNCTION skos_validate_broader_relation()
RETURNS TRIGGER AS $$
DECLARE
    subject_depth INTEGER;
    broader_count INTEGER;
BEGIN
    IF NEW.relation_type != 'broader' THEN
        RETURN NEW;
    END IF;

    -- Check polyhierarchy limit (max 3 parents)
    SELECT COUNT(*) INTO broader_count
    FROM skos_semantic_relation_edge
    WHERE subject_id = NEW.subject_id AND relation_type = 'broader';

    IF broader_count >= 3 THEN
        RAISE EXCEPTION 'Polyhierarchy limit exceeded: concept already has 3 broader concepts';
    END IF;

    -- Check depth limit (would create depth > 5)
    SELECT COALESCE(depth, 0) + 1 INTO subject_depth
    FROM skos_concept
    WHERE id = NEW.object_id;

    IF subject_depth > 5 THEN
        RAISE EXCEPTION 'Depth limit exceeded: adding this relation would exceed maximum depth of 5';
    END IF;

    -- Check for circular hierarchy
    IF skos_has_circular_hierarchy(NEW.subject_id) THEN
        RAISE EXCEPTION 'Circular hierarchy detected';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Function to validate narrower relation (breadth limit)
CREATE OR REPLACE FUNCTION skos_validate_narrower_relation()
RETURNS TRIGGER AS $$
DECLARE
    narrower_count INTEGER;
BEGIN
    IF NEW.relation_type != 'narrower' THEN
        RETURN NEW;
    END IF;

    -- Check breadth limit (max 10 children)
    SELECT COUNT(*) INTO narrower_count
    FROM skos_semantic_relation_edge
    WHERE subject_id = NEW.subject_id AND relation_type = 'narrower';

    IF narrower_count >= 10 THEN
        RAISE EXCEPTION 'Breadth limit exceeded: concept already has 10 narrower concepts';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Function to update concept hierarchy metadata
CREATE OR REPLACE FUNCTION skos_update_hierarchy_metadata()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' OR TG_OP = 'UPDATE' THEN
        -- Update subject concept counts
        UPDATE skos_concept SET
            broader_count = (
                SELECT COUNT(*) FROM skos_semantic_relation_edge
                WHERE subject_id = NEW.subject_id AND relation_type = 'broader'
            ),
            narrower_count = (
                SELECT COUNT(*) FROM skos_semantic_relation_edge
                WHERE subject_id = NEW.subject_id AND relation_type = 'narrower'
            ),
            related_count = (
                SELECT COUNT(*) FROM skos_semantic_relation_edge
                WHERE subject_id = NEW.subject_id AND relation_type = 'related'
            ),
            depth = skos_calculate_depth(NEW.subject_id),
            updated_at = NOW()
        WHERE id = NEW.subject_id;

        -- Update object concept counts if different
        IF NEW.subject_id != NEW.object_id THEN
            UPDATE skos_concept SET
                broader_count = (
                    SELECT COUNT(*) FROM skos_semantic_relation_edge
                    WHERE subject_id = NEW.object_id AND relation_type = 'broader'
                ),
                narrower_count = (
                    SELECT COUNT(*) FROM skos_semantic_relation_edge
                    WHERE subject_id = NEW.object_id AND relation_type = 'narrower'
                ),
                related_count = (
                    SELECT COUNT(*) FROM skos_semantic_relation_edge
                    WHERE subject_id = NEW.object_id AND relation_type = 'related'
                ),
                depth = skos_calculate_depth(NEW.object_id),
                updated_at = NOW()
            WHERE id = NEW.object_id;
        END IF;
    END IF;

    IF TG_OP = 'DELETE' THEN
        -- Update both concepts on deletion
        UPDATE skos_concept SET
            broader_count = (
                SELECT COUNT(*) FROM skos_semantic_relation_edge
                WHERE subject_id = OLD.subject_id AND relation_type = 'broader'
            ),
            narrower_count = (
                SELECT COUNT(*) FROM skos_semantic_relation_edge
                WHERE subject_id = OLD.subject_id AND relation_type = 'narrower'
            ),
            related_count = (
                SELECT COUNT(*) FROM skos_semantic_relation_edge
                WHERE subject_id = OLD.subject_id AND relation_type = 'related'
            ),
            depth = skos_calculate_depth(OLD.subject_id),
            updated_at = NOW()
        WHERE id = OLD.subject_id;

        IF OLD.subject_id != OLD.object_id THEN
            UPDATE skos_concept SET
                broader_count = (
                    SELECT COUNT(*) FROM skos_semantic_relation_edge
                    WHERE subject_id = OLD.object_id AND relation_type = 'broader'
                ),
                narrower_count = (
                    SELECT COUNT(*) FROM skos_semantic_relation_edge
                    WHERE subject_id = OLD.object_id AND relation_type = 'narrower'
                ),
                related_count = (
                    SELECT COUNT(*) FROM skos_semantic_relation_edge
                    WHERE subject_id = OLD.object_id AND relation_type = 'related'
                ),
                depth = skos_calculate_depth(OLD.object_id),
                updated_at = NOW()
            WHERE id = OLD.object_id;
        END IF;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Function to update note count and literary warrant
CREATE OR REPLACE FUNCTION skos_update_note_count()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE skos_concept SET
            note_count = note_count + 1,
            first_used_at = COALESCE(first_used_at, NOW()),
            last_used_at = NOW(),
            -- Auto-promote if literary warrant met (3+ notes) and still candidate
            status = CASE
                WHEN status = 'candidate' AND note_count + 1 >= 3 THEN 'approved'
                ELSE status
            END,
            promoted_at = CASE
                WHEN status = 'candidate' AND note_count + 1 >= 3 THEN NOW()
                ELSE promoted_at
            END,
            updated_at = NOW()
        WHERE id = NEW.concept_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE skos_concept SET
            note_count = GREATEST(0, note_count - 1),
            updated_at = NOW()
        WHERE id = OLD.concept_id;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Function to create reciprocal relations
CREATE OR REPLACE FUNCTION skos_create_reciprocal_relation()
RETURNS TRIGGER AS $$
BEGIN
    -- Create inverse relation for broader/narrower
    IF NEW.relation_type = 'broader' THEN
        INSERT INTO skos_semantic_relation_edge (subject_id, object_id, relation_type, is_inferred, created_by)
        VALUES (NEW.object_id, NEW.subject_id, 'narrower', TRUE, NEW.created_by)
        ON CONFLICT (subject_id, object_id, relation_type) DO NOTHING;
    ELSIF NEW.relation_type = 'narrower' THEN
        INSERT INTO skos_semantic_relation_edge (subject_id, object_id, relation_type, is_inferred, created_by)
        VALUES (NEW.object_id, NEW.subject_id, 'broader', TRUE, NEW.created_by)
        ON CONFLICT (subject_id, object_id, relation_type) DO NOTHING;
    ELSIF NEW.relation_type = 'related' THEN
        -- Related is symmetric
        INSERT INTO skos_semantic_relation_edge (subject_id, object_id, relation_type, is_inferred, created_by)
        VALUES (NEW.object_id, NEW.subject_id, 'related', TRUE, NEW.created_by)
        ON CONFLICT (subject_id, object_id, relation_type) DO NOTHING;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Function to detect anti-patterns
CREATE OR REPLACE FUNCTION skos_detect_antipatterns(concept_uuid UUID)
RETURNS tag_antipattern[] AS $$
DECLARE
    patterns tag_antipattern[] := '{}';
    c RECORD;
BEGIN
    SELECT * INTO c FROM skos_concept WHERE id = concept_uuid;

    IF c IS NULL THEN
        RETURN patterns;
    END IF;

    -- Orphan: no broader or narrower relations
    IF c.broader_count = 0 AND c.narrower_count = 0 AND c.related_count = 0 THEN
        patterns := array_append(patterns, 'orphan'::tag_antipattern);
    END IF;

    -- Under-used: approved but rarely tagged
    IF c.status = 'approved' AND c.note_count < 3 THEN
        patterns := array_append(patterns, 'under_used'::tag_antipattern);
    END IF;

    -- Too broad: excessive children (>10, but we enforce this, so check threshold)
    IF c.narrower_count > 8 THEN
        patterns := array_append(patterns, 'too_broad'::tag_antipattern);
    END IF;

    -- Too deep: depth > 4 (approaching limit)
    IF c.depth > 4 THEN
        patterns := array_append(patterns, 'too_deep'::tag_antipattern);
    END IF;

    -- Polyhierarchy excess: >2 parents (approaching limit)
    IF c.broader_count > 2 THEN
        patterns := array_append(patterns, 'polyhierarchy_excess'::tag_antipattern);
    END IF;

    -- Missing labels: no pref_label
    IF NOT EXISTS (
        SELECT 1 FROM skos_concept_label
        WHERE concept_id = concept_uuid AND label_type = 'pref_label'
    ) THEN
        patterns := array_append(patterns, 'missing_labels'::tag_antipattern);
    END IF;

    -- Circular hierarchy
    IF skos_has_circular_hierarchy(concept_uuid) THEN
        patterns := array_append(patterns, 'circular_hierarchy'::tag_antipattern);
    END IF;

    RETURN patterns;
END;
$$ LANGUAGE plpgsql;

-- Function to update antipatterns for a concept
CREATE OR REPLACE FUNCTION skos_update_antipatterns()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE skos_concept SET
        antipatterns = skos_detect_antipatterns(NEW.id),
        antipattern_checked_at = NOW()
    WHERE id = NEW.id;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- TRIGGERS
-- ============================================================================

-- Validate broader relations
CREATE TRIGGER trg_skos_validate_broader
    BEFORE INSERT OR UPDATE ON skos_semantic_relation_edge
    FOR EACH ROW
    EXECUTE FUNCTION skos_validate_broader_relation();

-- Validate narrower relations
CREATE TRIGGER trg_skos_validate_narrower
    BEFORE INSERT OR UPDATE ON skos_semantic_relation_edge
    FOR EACH ROW
    EXECUTE FUNCTION skos_validate_narrower_relation();

-- Update hierarchy metadata after relation changes
CREATE TRIGGER trg_skos_update_hierarchy
    AFTER INSERT OR UPDATE OR DELETE ON skos_semantic_relation_edge
    FOR EACH ROW
    EXECUTE FUNCTION skos_update_hierarchy_metadata();

-- Create reciprocal relations
CREATE TRIGGER trg_skos_reciprocal
    AFTER INSERT ON skos_semantic_relation_edge
    FOR EACH ROW
    WHEN (NOT NEW.is_inferred)
    EXECUTE FUNCTION skos_create_reciprocal_relation();

-- Update note counts
CREATE TRIGGER trg_skos_note_count
    AFTER INSERT OR DELETE ON note_skos_concept
    FOR EACH ROW
    EXECUTE FUNCTION skos_update_note_count();

-- ============================================================================
-- VIEWS FOR COMMON QUERIES
-- ============================================================================

-- View: Concepts with their preferred labels
CREATE VIEW skos_concept_with_label AS
SELECT
    c.*,
    l.value AS pref_label,
    l.language AS label_language,
    s.notation AS scheme_notation,
    s.title AS scheme_title
FROM skos_concept c
LEFT JOIN skos_concept_label l ON c.id = l.concept_id
    AND l.label_type = 'pref_label'
    AND l.language = 'en'
LEFT JOIN skos_concept_scheme s ON c.primary_scheme_id = s.id;

-- View: Concept hierarchy with breadcrumb paths
CREATE VIEW skos_concept_hierarchy AS
WITH RECURSIVE hierarchy AS (
    -- Base: top concepts (no broader)
    SELECT
        c.id,
        c.notation,
        l.value AS label,
        0 AS level,
        ARRAY[c.id] AS path,
        ARRAY[l.value] AS label_path
    FROM skos_concept c
    LEFT JOIN skos_concept_label l ON c.id = l.concept_id
        AND l.label_type = 'pref_label' AND l.language = 'en'
    WHERE c.broader_count = 0

    UNION ALL

    -- Recursive: narrower concepts
    SELECT
        c.id,
        c.notation,
        l.value AS label,
        h.level + 1,
        h.path || c.id,
        h.label_path || l.value
    FROM skos_concept c
    JOIN skos_semantic_relation_edge e ON c.id = e.subject_id AND e.relation_type = 'broader'
    JOIN hierarchy h ON e.object_id = h.id
    LEFT JOIN skos_concept_label l ON c.id = l.concept_id
        AND l.label_type = 'pref_label' AND l.language = 'en'
    WHERE NOT c.id = ANY(h.path)
      AND h.level < 6
)
SELECT DISTINCT ON (id) * FROM hierarchy ORDER BY id, level;

-- View: Tag governance dashboard
CREATE VIEW skos_governance_dashboard AS
SELECT
    s.id AS scheme_id,
    s.notation AS scheme,
    s.title AS scheme_title,
    COUNT(DISTINCT c.id) AS total_concepts,
    COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'candidate') AS candidates,
    COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'approved') AS approved,
    COUNT(DISTINCT c.id) FILTER (WHERE c.status = 'deprecated') AS deprecated,
    COUNT(DISTINCT c.id) FILTER (WHERE 'orphan' = ANY(c.antipatterns)) AS orphans,
    COUNT(DISTINCT c.id) FILTER (WHERE 'under_used' = ANY(c.antipatterns)) AS under_used,
    COUNT(DISTINCT c.id) FILTER (WHERE c.embedding IS NULL) AS missing_embeddings,
    AVG(c.note_count)::NUMERIC(10,2) AS avg_note_count,
    MAX(c.depth) AS max_depth
FROM skos_concept_scheme s
LEFT JOIN skos_concept c ON c.primary_scheme_id = s.id
GROUP BY s.id, s.notation, s.title;

-- ============================================================================
-- DEFAULT DATA
-- ============================================================================

-- Create default concept scheme
INSERT INTO skos_concept_scheme (id, notation, uri, title, description, is_system)
VALUES (
    gen_uuid_v7(),
    'default',
    'https://matric.io/schemes/default',
    'Default Tags',
    'Default concept scheme for general-purpose tagging',
    TRUE
);

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE skos_concept_scheme IS 'W3C SKOS ConceptScheme - vocabulary/namespace containers';
COMMENT ON TABLE skos_concept IS 'W3C SKOS Concept - the core tag/concept entity with PMEST facets';
COMMENT ON TABLE skos_concept_label IS 'SKOS lexical labels (prefLabel, altLabel, hiddenLabel)';
COMMENT ON TABLE skos_concept_note IS 'SKOS documentation notes (definition, scopeNote, example, etc.)';
COMMENT ON TABLE skos_semantic_relation_edge IS 'SKOS semantic relations (broader, narrower, related)';
COMMENT ON TABLE skos_mapping_relation_edge IS 'SKOS mapping relations to external vocabularies';
COMMENT ON TABLE skos_concept_in_scheme IS 'Concept membership in multiple schemes';
COMMENT ON TABLE note_skos_concept IS 'Enhanced note-to-concept tagging with provenance';
COMMENT ON TABLE skos_audit_log IS 'Governance audit trail for taxonomy changes';
COMMENT ON TABLE skos_concept_merge IS 'History of merged concepts for provenance';

COMMENT ON COLUMN skos_concept.depth IS 'Hierarchy depth (0=root), max 5 levels enforced';
COMMENT ON COLUMN skos_concept.broader_count IS 'Polyhierarchy count, max 3 parents enforced';
COMMENT ON COLUMN skos_concept.narrower_count IS 'Children count, max 10 enforced (breadth limit)';
COMMENT ON COLUMN skos_concept.note_count IS 'Literary warrant: usage count for promotion';
COMMENT ON COLUMN skos_concept.antipatterns IS 'Detected governance issues';

-- ============================================================================
-- MIGRATION NOTES
-- ============================================================================
--
-- This migration creates a new, comprehensive SKOS tag system. The existing
-- `tag` and `note_tag` tables remain unchanged for backward compatibility.
--
-- To migrate existing tags to SKOS concepts:
-- 1. Create concepts from existing tags
-- 2. Map note_tag entries to note_skos_concept
-- 3. Deprecate old tag system (optional)
--
-- Example migration query (run manually after review):
--
-- INSERT INTO skos_concept (primary_scheme_id, notation, status)
-- SELECT (SELECT id FROM skos_concept_scheme WHERE is_system = TRUE AND notation = 'default'), name, 'approved'
-- FROM tag;
--
-- INSERT INTO skos_concept_label (concept_id, label_type, value)
-- SELECT c.id, 'pref_label', c.notation
-- FROM skos_concept c
-- WHERE NOT EXISTS (
--     SELECT 1 FROM skos_concept_label l
--     WHERE l.concept_id = c.id AND l.label_type = 'pref_label'
-- );
-- ============================================================================
