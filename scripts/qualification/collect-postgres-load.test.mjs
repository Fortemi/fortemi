import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { collectPostgresLoad, parsePostgresLoad } from './collect-postgres-load.mjs';
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
