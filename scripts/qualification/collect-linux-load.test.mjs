import test from 'node:test';
import assert from 'node:assert/strict';
import os from 'node:os';
import { collectLinuxLoad, parseRss, parseCpuUsage, cpuPercentBetween } from './collect-linux-load.mjs';
test('parses rollup RSS and cgroup usage with exact units', () => {
  assert.equal(parseRss('Rss:                2048 kB\nPss: 10 kB\n'), 2097152);
  assert.equal(parseCpuUsage('usage_usec 123\nuser_usec 100\n'), 123);
});
for (const text of ['', 'Rss: 1 MB', 'Rss: -1 kB', 'Rss: 1 kB\nRss: 2 kB', 'Rss: 9007199254740991 kB']) {
  test(`rejects malformed RSS ${JSON.stringify(text)}`, () => assert.throws(() => parseRss(text)));
}
test('rejects missing, duplicated and unsafe CPU counters', () => {
  for (const text of ['', 'usage_usec 1\nusage_usec 2', 'usage_usec 9007199254740992']) assert.throws(() => parseCpuUsage(text));
});
const before = { usageUsec: 100, observedNs: '1000000000', device: '1', inode: '2' };
const after = { ...before, usageUsec: 1000100, observedNs: '2000000000' };
test('normalizes CPU by allocation without clamping over-allocation', () => {
  assert.equal(cpuPercentBetween(before, after, 2), 50);
  assert.equal(cpuPercentBetween(before, after, 0.5), 200);
});
test('rejects CPU scope changes, counter reset and invalid clocks/allocation', () => {
  for (const a of [{ ...after, inode: '3' }, { ...after, usageUsec: 99 }, { ...after, observedNs: before.observedNs },
    { ...after, observedNs: '-1' }]) assert.throws(() => cpuPercentBetween(before, a, 1));
  for (const n of [0, -1, NaN, Infinity]) assert.throws(() => cpuPercentBetween(before, after, n));
});
test('reads real local kernel RSS, CPU and disk without starting workload', { skip: process.platform !== 'linux' }, () => {
  const r = collectLinuxLoad({ pids: [process.pid], cgroupDirectory: '/sys/fs/cgroup', filesystemPath: os.tmpdir() });
  assert.equal(r.admitted, false); assert.ok(r.rssBytes > 0); assert.ok(r.cpu.usageUsec >= 0);
  assert.ok(r.filesystem.freeBytes >= 0); assert.ok(BigInt(r.endNs) >= BigInt(r.startNs));
  assert.ok(r.processes[0].startTicks);
});
test('rejects duplicate scope, unavailable process and non-cgroup filesystem', () => {
  const scope = { pids: [process.pid], cgroupDirectory: '/sys/fs/cgroup', filesystemPath: os.tmpdir() };
  assert.throws(() => collectLinuxLoad({ ...scope, pids: [process.pid, process.pid] }));
  assert.throws(() => collectLinuxLoad({ ...scope, pids: [Number.MAX_SAFE_INTEGER] }));
  assert.throws(() => collectLinuxLoad({ ...scope, cgroupDirectory: os.tmpdir() }));
});
