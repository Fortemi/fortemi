-- Add cost_tier column for tiered atomic job architecture.
-- NULL = agnostic (backward compat for existing/in-flight jobs).
-- 0 = CPU/NER (GLiNER), 1 = Fast GPU (8B), 2 = Standard GPU (20B).
ALTER TABLE job_queue ADD COLUMN cost_tier SMALLINT;

-- Partial index for efficient tier-based claim queries.
CREATE INDEX idx_job_queue_cost_tier ON job_queue (cost_tier) WHERE status = 'pending';
