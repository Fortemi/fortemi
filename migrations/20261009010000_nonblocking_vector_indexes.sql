-- no-transaction
-- Adds the build_set_index job type on its own: a new enum value must be
-- committed before use, and the follow-up migration 20261009010100 uses it.
-- Nonblocking vector index ownership (#1175/#1181).
--
-- HNSW indexes are now keyed by stable vector shape rather than by embedding
-- config/set membership. Runtime callers enqueue BuildSetIndex jobs; the worker
-- runs CREATE INDEX CONCURRENTLY on a plain connection. HNSW m/ef_construction
-- are taken from the first non-deferred config using a shape; later configs with
-- the same (dimension, vector_type) reuse that shape index.

ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'build_set_index';
