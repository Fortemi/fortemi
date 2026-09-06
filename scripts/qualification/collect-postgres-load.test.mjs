import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { collectPostgresLoad, parsePostgresLoad, collectPostgresQueueLoad, parsePostgresQueueLoad } from './collect-postgres-load.mjs';
const sample = () => ({ serverVersion: '180003', databaseOid: '5', serverStartedAt: '2026-09-06T00:00:00Z',
  databaseStatsReset: null, walStatsReset: '2026-09-06T00:00:00Z', trackCounts: true, trackActivities: true,
  canReadAllStats: true, databaseBytes: '100', deadlocks: '0', connections: '2', walGeneratedBytes: '1000',
  walRetainedBytes: '16777216', waitingLocks: '0', missingLockWaitStarts: '0', longestLockWaitSeconds: '0' });
test('parses counters without conflating WAL and database scope', () => assert.equal(parsePostgresLoad(JSON.stringify(sample())).walGeneratedBytes, '1000'));
for (const [name, mutate] of [
  ['disabled statistics', s => s.trackCounts = false], ['restricted monitoring', s => s.canReadAllStats = false],
  ['missing wait timestamp', s => s.missingLockWaitStarts = '1'], ['unsafe integer', s => s.databaseBytes = '9007199254740992'],
  ['wrong major version', s => s.serverVersion = '160000'], ['negative wait', s => s.longestLockWaitSeconds = '-1'],
  ['missing start identity', s => delete s.serverStartedAt], ['null WAL size', s => s.walRetainedBytes = null],
]) test(`rejects ${name}`, () => { const s = sample(); mutate(s); assert.throws(() => parsePostgresLoad(JSON.stringify(s))); });
test('rejects implicit connection and injected psql options', async () => {
  await assert.rejects(collectPostgresLoad({}));
  await assert.rejects(collectPostgresLoad({ PGHOST: 'localhost', PGPORT: '5432', PGDATABASE: 'x', PGUSER: 'x', PGOPTIONS: '-c bad=1' }));
});
const bin = '/usr/lib/postgresql/18/bin';
test('collects using a monitoring role in a disposable PostgreSQL 18 cluster', {
  skip: !fs.existsSync(`${bin}/initdb`) || process.getuid?.() === 0,
}, async t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dq-load-pg-')), data = path.join(root, 'data');
  let started = false;
  t.after(() => {
    if (started) execFileSync(`${bin}/pg_ctl`, ['-D', data, '-m', 'immediate', '-w', 'stop'], { stdio: 'ignore', timeout: 10000 });
    fs.rmSync(root, { recursive: true, force: true });
  });
  execFileSync(`${bin}/initdb`, ['-D', data, '--no-locale', '--encoding=UTF8', '--auth=trust', '--no-sync'], { stdio: 'ignore', timeout: 10000 });
  // Private socket directory, no TCP listener; never contacts a configured service.
  fs.appendFileSync(path.join(data, 'postgresql.conf'), `\nlisten_addresses = ''\nunix_socket_directories = '${root}'\nshared_buffers = '8MB'\nmax_connections = 10\n`);
  execFileSync(`${bin}/pg_ctl`, ['-D', data, '-l', path.join(root, 'server.log'), '-w', 'start'], { stdio: 'ignore', timeout: 10000 }); started = true;
  execFileSync('/usr/bin/psql', ['-X', '-h', root, '-d', 'postgres', '-c', 'CREATE ROLE load_observer LOGIN; GRANT pg_monitor TO load_observer;'],
    { stdio: 'ignore', timeout: 5000, env: { PATH: '/usr/bin:/bin' } });
  const r = await collectPostgresLoad({ PGHOST: root, PGPORT: '5432', PGDATABASE: 'postgres', PGUSER: 'load_observer' });
  assert.equal(r.walScope, 'whole-cluster'); assert.equal(r.admitted, false);
  assert.ok(BigInt(r.snapshot.databaseBytes) > 0n); assert.ok(BigInt(r.snapshot.walRetainedBytes) > 0n);
  const connection = { PGHOST: root, PGPORT: '5432', PGDATABASE: 'postgres', PGUSER: 'load_observer' };
  const sql = text => execFileSync('/usr/bin/psql', ['-X', '-h', root, '-d', 'postgres', '-v', 'ON_ERROR_STOP=1', '-c', text],
    { stdio: 'ignore', timeout: 5000, env: { PATH: '/usr/bin:/bin' } });
  // Minimal physical table fixture; this does not exercise production migrations.
  sql('CREATE TABLE public.job_queue (status text, created_at timestamptz, next_attempt_at timestamptz, job_type text)');
  await assert.rejects(collectPostgresQueueLoad(connection), /POSTGRES_OBSERVATION_UNAVAILABLE/);
  sql('GRANT SELECT ON public.job_queue TO load_observer');
  const empty = await collectPostgresQueueLoad(connection);
  assert.equal(empty.snapshot.active, '0'); assert.equal(empty.snapshot.oldestActiveSeconds, '0');
  sql(`INSERT INTO public.job_queue VALUES
    ('pending', NOW() - INTERVAL '60 seconds', NOW() + INTERVAL '1 hour', 'unsupported'),
    ('running', NOW() - INTERVAL '30 seconds', NULL, 'known'),
    ('completed', NOW() - INTERVAL '1 day', NULL, 'known')`);
  const queued = await collectPostgresQueueLoad(connection);
  assert.equal(queued.snapshot.active, '2');
  assert.ok(Number(queued.snapshot.oldestActiveSeconds) >= 60);
  assert.ok(Number(queued.snapshot.oldestActiveSeconds) < 90);
  assert.equal(queued.queueScope, 'public.job_queue:all-pending-and-running');
  sql("INSERT INTO public.job_queue VALUES ('pending', NOW() + INTERVAL '1 hour', NULL, 'known')");
  await assert.rejects(collectPostgresQueueLoad(connection), /POSTGRES_OBSERVATION_UNAVAILABLE/);
  sql('DELETE FROM public.job_queue WHERE created_at > NOW(); ALTER TABLE public.job_queue ENABLE ROW LEVEL SECURITY');
  await assert.rejects(collectPostgresQueueLoad(connection), /POSTGRES_OBSERVATION_UNAVAILABLE/);
});

const queueSample = () => ({ scope: 'public.job_queue', databaseOid: '5', relationOid: '16400', rowSecurity: false,
  observedAt: '2026-09-06T01:00:00Z', active: '2', missingCreatedAt: '0', futureCreatedAt: '0', oldestActiveSeconds: '60.25' });
test('queue age preserves exact scope and raw decimal seconds', () => {
  assert.equal(parsePostgresQueueLoad(JSON.stringify(queueSample())).oldestActiveSeconds, '60.25');
});
for (const [name, mutate] of [
  ['filtered rows', s => s.rowSecurity = true], ['missing timestamps', s => s.missingCreatedAt = '1'],
  ['future timestamps', s => s.futureCreatedAt = '1'], ['unknown scope', s => s.scope = 'tenant.job_queue'],
  ['unsafe count', s => s.active = '9007199254740992'], ['empty queue with nonzero age', s => s.active = '0'],
  ['negative age', s => s.oldestActiveSeconds = '-1'], ['missing relation', s => delete s.relationOid],
]) test(`queue observation rejects ${name}`, () => {
  const s = queueSample(); mutate(s); assert.throws(() => parsePostgresQueueLoad(JSON.stringify(s)));
});
test('baseline derivation rejects restarts, scope changes and reset counters', async () => {
  const { postgresLoadDelta } = await import('./collect-postgres-load.mjs');
  const a = sample(), b = { ...a, databaseBytes: '120', walGeneratedBytes: '1100' };
  assert.equal(postgresLoadDelta(a, b).storageGrowthBytes, 20);
  assert.equal(postgresLoadDelta(a, b).walGeneratedBytes, 100);
  for (const changed of [{ ...b, databaseOid: '6' }, { ...b, walStatsReset: null },
    { ...b, serverStartedAt: '2026-09-06T00:01:00Z' }, { ...b, walGeneratedBytes: '999' }]) {
    assert.throws(() => postgresLoadDelta(a, changed));
  }
});
