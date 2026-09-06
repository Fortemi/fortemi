import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
const execute = promisify(execFile);
const connectionKeys = new Set(['PGHOST', 'PGPORT', 'PGDATABASE', 'PGUSER', 'PGPASSWORD', 'PGSSLMODE', 'PGSSLROOTCERT', 'PGSSLCERT', 'PGSSLKEY']);
const counters = ['databaseOid', 'databaseBytes', 'deadlocks', 'connections', 'walGeneratedBytes',
  'walRetainedBytes', 'waitingLocks', 'missingLockWaitStarts'];
export function parsePostgresLoad(text) {
  if (typeof text !== 'string' || Buffer.byteLength(text) > 65536) throw new Error('invalid PostgreSQL snapshot size');
  let value;
  try { value = JSON.parse(text); } catch { throw new Error('invalid PostgreSQL snapshot JSON'); }
  if (!value || typeof value !== 'object' || Array.isArray(value) || !/^18\d{4}$/.test(value.serverVersion)
    || value.trackCounts !== true || value.trackActivities !== true || value.canReadAllStats !== true) throw new Error('PostgreSQL 18 complete monitoring scope required');
  for (const field of ['serverStartedAt', 'databaseStatsReset', 'walStatsReset']) {
    if ((field !== 'serverStartedAt' && value[field] === null)) continue;
    if (typeof value[field] !== 'string' || !Number.isFinite(Date.parse(value[field]))) throw new Error('invalid database observation identity');
  }
  for (const field of counters) {
    if (typeof value[field] !== 'string' || !/^\d{1,30}$/.test(value[field]) || BigInt(value[field]) > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error(`invalid database counter: ${field}`);
  }
  if (typeof value.longestLockWaitSeconds !== 'string' || !/^\d+(?:\.\d+)?$/.test(value.longestLockWaitSeconds)
    || !Number.isFinite(Number(value.longestLockWaitSeconds)) || value.missingLockWaitStarts !== '0') throw new Error('complete lock wait observations required');
  return value;
}
/** Explicit connection configuration only; never discovers a deployed database. */
export async function collectPostgresLoad(connection) {
  if (!connection || typeof connection !== 'object' || Array.isArray(connection)
    || Object.entries(connection).some(([k, v]) => !connectionKeys.has(k) || typeof v !== 'string' || v.includes('\0'))
    || ['PGHOST', 'PGPORT', 'PGDATABASE', 'PGUSER'].some(k => !connection[k])
    || !/^\d{1,5}$/.test(connection.PGPORT) || Number(connection.PGPORT) < 1 || Number(connection.PGPORT) > 65535) throw new Error('explicit PostgreSQL connection scope required');
  const startNs = process.hrtime.bigint().toString();
  try {
    const { stdout } = await execute('/usr/bin/psql', ['--no-psqlrc', '--no-password', '--quiet', '--tuples-only', '--no-align',
      '--set=ON_ERROR_STOP=1', '--file', fileURLToPath(new URL('./load-postgres-snapshot.sql', import.meta.url))], {
      timeout: 5000, killSignal: 'SIGKILL', maxBuffer: 65536,
      env: { PATH: '/usr/bin:/bin', LANG: 'C.UTF-8', ...connection, PGCONNECT_TIMEOUT: '3',
        PGOPTIONS: '-c default_transaction_read_only=on -c statement_timeout=2000 -c lock_timeout=1000' },
    });
    return { admitted: false, executionAuthorized: false, startNs, endNs: process.hrtime.bigint().toString(),
      databaseScope: 'connected-database', walScope: 'whole-cluster', snapshot: parsePostgresLoad(stdout) };
  } catch { throw new Error('POSTGRES_OBSERVATION_UNAVAILABLE'); }
}

/** Derive run-baseline growth only when database/server/reset identities agree. */
export function postgresLoadDelta(baseline, current) {
  const a = parsePostgresLoad(JSON.stringify(baseline)), b = parsePostgresLoad(JSON.stringify(current));
  for (const field of ['serverVersion', 'databaseOid', 'serverStartedAt', 'databaseStatsReset', 'walStatsReset']) {
    if (a[field] !== b[field]) throw new Error('POSTGRES_BASELINE_IDENTITY_CHANGED');
  }
  const delta = metric => Number(BigInt(b[metric]) - BigInt(a[metric]));
  for (const field of ['walGeneratedBytes', 'deadlocks']) if (delta(field) < 0) throw new Error('POSTGRES_COUNTER_RESET');
  return { storageGrowthBytes: Math.max(0, delta('databaseBytes')), walGrowthBytes: Math.max(0, delta('walRetainedBytes')),
    walGeneratedBytes: delta('walGeneratedBytes'), deadlocks: delta('deadlocks'), lockWaitSeconds: Number(b.longestLockWaitSeconds) };
}
