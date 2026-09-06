BEGIN READ ONLY;
SET LOCAL statement_timeout = '2s';
SET LOCAL lock_timeout = '1s';
-- Refuse silent RLS filtering; this is a complete-table observation, not a
-- tenant isolation probe. This setting never grants bypass privileges.
SET LOCAL row_security = off;
WITH active AS (
  SELECT created_at FROM public.job_queue WHERE status::text IN ('pending', 'running')
), measured AS (
  SELECT count(*) AS active,
    count(*) FILTER (WHERE created_at IS NULL) AS missing_created_at,
    count(*) FILTER (WHERE created_at > CURRENT_TIMESTAMP) AS future_created_at,
    CASE WHEN count(*) = 0 THEN 0
      ELSE EXTRACT(EPOCH FROM CURRENT_TIMESTAMP - min(created_at)) END AS oldest_seconds
  FROM active
)
SELECT json_build_object(
  'scope', 'public.job_queue',
  'databaseOid', (SELECT oid::text FROM pg_database WHERE datname = current_database()),
  'relationOid', 'public.job_queue'::regclass::oid::text,
  'rowSecurity', row_security_active('public.job_queue'::regclass),
  'observedAt', CURRENT_TIMESTAMP,
  'active', active::text,
  'missingCreatedAt', missing_created_at::text,
  'futureCreatedAt', future_created_at::text,
  'oldestActiveSeconds', oldest_seconds::text
) FROM measured;
COMMIT;
