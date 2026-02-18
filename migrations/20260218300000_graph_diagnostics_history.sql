-- Graph diagnostics snapshot history (#484).
-- Stores before/after diagnostic snapshots for validating embedding pipeline changes.

CREATE TABLE IF NOT EXISTS graph_diagnostics_history (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    label TEXT NOT NULL,
    metrics JSONB NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_graph_diag_history_label ON graph_diagnostics_history(label);
CREATE INDEX idx_graph_diag_history_captured ON graph_diagnostics_history(captured_at DESC);
