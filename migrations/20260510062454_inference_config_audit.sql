-- Inference config audit log (Issue #656)
--
-- Records every operator-driven mutation to the inference configuration
-- (POST or DELETE on /api/v1/inference/config). Lets operators answer
-- "did anyone touch the config?" during incident triage and provides an
-- attribution trail in shared deployments.
--
-- Lives in the public schema (not per-memory) because inference config
-- itself is global. Per-archive overrides (when #655 lands) will use a
-- separate audit table or reuse this with a schema_name column.
--
-- API keys in before_json/after_json are redacted to first 8 chars + "..."
-- by the writer in matric-api/src/handlers/inference_config.rs — same
-- function used by GET /api/v1/inference/config.

CREATE TABLE IF NOT EXISTS inference_config_audit (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- OAuth subject claim when REQUIRE_AUTH=true; "anonymous" otherwise.
    -- Free-text so future auth schemes (API keys with named owners,
    -- service accounts) can populate without schema changes.
    changed_by TEXT NOT NULL DEFAULT 'anonymous',
    -- Action type: "set" (POST), "reset" (DELETE),
    -- "atomic_swap_committed" (POST?atomic=true success),
    -- "atomic_swap_rolled_back" (POST?atomic=true failure),
    -- "dry_run" (POST?dry_run=true — no live state change but still logged).
    action TEXT NOT NULL,
    -- Effective config before the action. Null for fresh installs where no
    -- override existed yet. JSONB for indexable lookups during forensics.
    before_json JSONB,
    -- Effective config after the action. Null for failed atomic swaps and
    -- DELETE events where "after" is implicit (env/defaults).
    after_json JSONB,
    -- Source IP from the request. Null when the handler can't determine it
    -- (unix socket connections, proxy chains without X-Forwarded-For).
    source_ip TEXT
);

-- Most common query: recent N entries, newest first.
CREATE INDEX idx_inference_config_audit_changed_at
    ON inference_config_audit (changed_at DESC);

-- Per-actor forensics: "what did this user touch?"
CREATE INDEX idx_inference_config_audit_changed_by
    ON inference_config_audit (changed_by, changed_at DESC);

-- Action-type filtering: "show me only the resets" / "show me failures".
CREATE INDEX idx_inference_config_audit_action
    ON inference_config_audit (action, changed_at DESC);
