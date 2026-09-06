BEGIN READ ONLY;
SET LOCAL statement_timeout = '2s';
SET LOCAL lock_timeout = '1s';
SET LOCAL stats_fetch_consistency = 'snapshot';
WITH db AS (
  SELECT * FROM pg_stat_database WHERE datid = (SELECT oid FROM pg_database WHERE datname = current_database())
), waits AS (
  SELECT count(*) AS waiting, count(*) FILTER (WHERE l.waitstart IS NULL) AS missing_starts,
    max(EXTRACT(EPOCH FROM clock_timestamp() - l.waitstart)) AS longest_seconds
  FROM pg_locks l JOIN pg_stat_activity a ON a.pid = l.pid
  WHERE a.datid = (SELECT datid FROM db) AND NOT l.granted
), wal_files AS (
  SELECT sum(size) AS retained_bytes FROM pg_ls_waldir()
)
SELECT json_build_object(
  'serverVersion', current_setting('server_version_num'),
  'databaseOid', db.datid::text,
  'serverStartedAt', pg_postmaster_start_time(),
  'databaseStatsReset', db.stats_reset,
  'walStatsReset', w.stats_reset,
  'trackCounts', current_setting('track_counts') = 'on',
  'trackActivities', current_setting('track_activities') = 'on',
  'canReadAllStats', pg_has_role(current_user, 'pg_read_all_stats', 'USAGE'),
  'databaseBytes', pg_database_size(current_database())::text,
  'deadlocks', db.deadlocks::text,
  'connections', db.numbackends::text,
  'walGeneratedBytes', w.wal_bytes::text,
  'walRetainedBytes', wal_files.retained_bytes::text,
  'waitingLocks', waits.waiting::text,
  'missingLockWaitStarts', waits.missing_starts::text,
  'longestLockWaitSeconds', CASE WHEN waits.waiting = 0 THEN '0' ELSE waits.longest_seconds::text END
) FROM db CROSS JOIN pg_stat_wal w CROSS JOIN waits CROSS JOIN wal_files;
COMMIT;
