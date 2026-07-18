-- Persist portable graph and community artifacts for lossless shard transfer.

CREATE TABLE graph_source (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('link', 'similarity', 'search', 'manual', 'imported')),
    source_table TEXT
        CHECK (source_table IS NULL OR source_table IN ('link', 'embedding', 'manual')),
    embedding_set_id UUID,
    virtual_set_id TEXT,
    model TEXT,
    dimension INTEGER CHECK (dimension IS NULL OR dimension > 0),
    truncate_dimension INTEGER CHECK (
        truncate_dimension IS NULL
        OR (
            truncate_dimension > 0
            AND (dimension IS NULL OR truncate_dimension <= dimension)
        )
    ),
    metric TEXT
        CHECK (metric IS NULL OR metric IN ('cosine', 'inner_product', 'l2')),
    algorithm TEXT,
    parameters_json JSONB,
    input_hash TEXT NOT NULL,
    freshness_json JSONB NOT NULL
        CHECK (jsonb_typeof(freshness_json) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE graph_edge_artifact (
    graph_source_id TEXT NOT NULL REFERENCES graph_source(id) ON DELETE CASCADE,
    from_note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    to_note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    weight DOUBLE PRECISION NOT NULL CHECK (
        weight NOT IN (
            'NaN'::DOUBLE PRECISION,
            'Infinity'::DOUBLE PRECISION,
            '-Infinity'::DOUBLE PRECISION
        )
    ),
    kind TEXT NOT NULL CHECK (kind IN ('link', 'similarity', 'manual')),
    rank INTEGER CHECK (rank IS NULL OR rank >= 0),
    metadata_json JSONB,
    PRIMARY KEY (graph_source_id, from_note_id, to_note_id, kind),
    CHECK (from_note_id <> to_note_id)
);

CREATE TABLE community_set (
    id TEXT PRIMARY KEY,
    graph_source_id TEXT NOT NULL REFERENCES graph_source(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    source_type TEXT NOT NULL CHECK (
        source_type IN ('precomputed', 'dynamic-snapshot', 'user-authored', 'imported')
    ),
    algorithm TEXT,
    parameters_json JSONB,
    input_hash TEXT NOT NULL,
    freshness_json JSONB NOT NULL
        CHECK (jsonb_typeof(freshness_json) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE community (
    id TEXT NOT NULL,
    community_set_id TEXT NOT NULL REFERENCES community_set(id) ON DELETE CASCADE,
    label TEXT,
    rank INTEGER CHECK (rank IS NULL OR rank >= 0),
    size INTEGER CHECK (size IS NULL OR size >= 0),
    confidence DOUBLE PRECISION CHECK (
        confidence IS NULL OR (confidence >= 0 AND confidence <= 1)
    ),
    representative_note_ids UUID[],
    metadata_json JSONB,
    PRIMARY KEY (community_set_id, id)
);

CREATE TABLE community_assignment (
    community_set_id TEXT NOT NULL REFERENCES community_set(id) ON DELETE CASCADE,
    community_id TEXT NOT NULL,
    note_id UUID NOT NULL REFERENCES note(id) ON DELETE CASCADE,
    confidence DOUBLE PRECISION CHECK (
        confidence IS NULL OR (confidence >= 0 AND confidence <= 1)
    ),
    source_type TEXT NOT NULL CHECK (
        source_type IN ('precomputed', 'dynamic-snapshot', 'user-authored', 'imported')
    ),
    metadata_json JSONB,
    PRIMARY KEY (community_set_id, note_id),
    FOREIGN KEY (community_set_id, community_id)
        REFERENCES community(community_set_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_graph_edge_artifact_notes
    ON graph_edge_artifact(from_note_id, to_note_id);
CREATE INDEX idx_community_set_graph_source
    ON community_set(graph_source_id);
CREATE INDEX idx_community_assignment_community
    ON community_assignment(community_set_id, community_id);
CREATE INDEX idx_community_assignment_note
    ON community_assignment(note_id);
