import fs from 'node:fs';
import path from 'node:path';

const safeNumber = n => {
  if (n < 0n || n > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('counter exceeds safe numeric range');
  return Number(n);
};
function readBounded(file) {
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  try {
    if (!fs.fstatSync(fd).isFile()) throw new Error('regular kernel interface required');
    const buffer = Buffer.alloc(65537); let used = 0;
    while (used < buffer.length) {
      const n = fs.readSync(fd, buffer, used, buffer.length - used, null);
      if (!n) break; used += n;
    }
    if (used > 65536) throw new Error('kernel interface size limit exceeded');
    return buffer.subarray(0, used).toString('utf8');
  } finally { fs.closeSync(fd); }
}
export function parseRss(text) {
  const rows = [...text.matchAll(/^Rss:\s+(\d+) kB\s*$/gm)];
  if (rows.length !== 1) throw new Error('smaps rollup RSS missing or duplicated');
  return safeNumber(BigInt(rows[0][1]) * 1024n);
}
export function parseCpuUsage(text) {
  const rows = [...text.matchAll(/^usage_usec (\d+)\s*$/gm)];
  if (rows.length !== 1) throw new Error('cgroup CPU usage missing or duplicated');
  return safeNumber(BigInt(rows[0][1]));
}
function identity(pid) {
  const text = readBounded(`/proc/${pid}/stat`);
  // comm can contain spaces and parentheses; fields after its final ')' start at field 3.
  const end = text.lastIndexOf(')');
  const fields = text.slice(end + 2).trim().split(/\s+/);
  if (end < 0 || !/^\d+$/.test(fields[19] || '')) throw new Error('process start identity unavailable');
  return fields[19];
}

/** Read-only snapshot of explicitly selected processes, cgroup and filesystem. */
export function collectLinuxLoad({ pids, cgroupDirectory, filesystemPath }) {
  if (process.platform !== 'linux') throw new Error('Linux collector required');
  if (!Array.isArray(pids) || !pids.length || pids.length > 32 || new Set(pids).size !== pids.length
    || pids.some(p => !Number.isSafeInteger(p) || p < 1)
    || typeof cgroupDirectory !== 'string' || !path.isAbsolute(cgroupDirectory)
    || typeof filesystemPath !== 'string' || !path.isAbsolute(filesystemPath)) throw new Error('explicit bounded collector scope required');
  const startNs = process.hrtime.bigint();
  const processes = pids.map(pid => {
    const startTicks = identity(pid);
    const rssBytes = parseRss(readBounded(`/proc/${pid}/smaps_rollup`));
    const observedNs = process.hrtime.bigint().toString();
    if (identity(pid) !== startTicks) throw new Error('process identity changed during collection');
    return { pid, startTicks, rssBytes, observedNs };
  });
  const cgroup = fs.openSync(cgroupDirectory, fs.constants.O_RDONLY | fs.constants.O_DIRECTORY | fs.constants.O_NOFOLLOW);
  let cpu;
  try {
    const anchor = `/proc/self/fd/${cgroup}`;
    if (fs.statfsSync(anchor).type !== 0x63677270) throw new Error('cgroup v2 filesystem required');
    const st = fs.fstatSync(cgroup, { bigint: true });
    cpu = { usageUsec: parseCpuUsage(readBounded(`${anchor}/cpu.stat`)),
      observedNs: process.hrtime.bigint().toString(), device: st.dev.toString(), inode: st.ino.toString() };
  } finally { fs.closeSync(cgroup); }
  const disk = fs.statfsSync(filesystemPath, { bigint: true });
  const filesystem = { freeBytes: safeNumber(disk.bavail * disk.bsize), freeInodes: safeNumber(disk.ffree),
    type: disk.type.toString(), observedNs: process.hrtime.bigint().toString() };
  return { admitted: false, executionAuthorized: false, startNs: startNs.toString(),
    endNs: process.hrtime.bigint().toString(), processes,
    rssBytes: safeNumber(processes.reduce((sum, p) => sum + BigInt(p.rssBytes), 0n)), cpu, filesystem };
}

/** Allocation must come from the approved topology. Values are never clamped. */
export function cpuPercentBetween(before, after, allocatedCores) {
  if (typeof allocatedCores !== 'number' || !Number.isFinite(allocatedCores) || allocatedCores <= 0
    || !before || !after
    || ![before.device, before.inode, after.device, after.inode].every(n => typeof n === 'string' && /^\d{1,30}$/.test(n))
    || before.device !== after.device || before.inode !== after.inode
    || ![before.usageUsec, after.usageUsec].every(n => Number.isSafeInteger(n) && n >= 0)
    || ![before.observedNs, after.observedNs].every(n => typeof n === 'string' && /^\d{1,30}$/.test(n))) throw new Error('invalid CPU observation scope');
  const elapsedNs = BigInt(after.observedNs) - BigInt(before.observedNs);
  const usage = after.usageUsec - before.usageUsec;
  if (elapsedNs <= 0n || usage < 0) throw new Error('CPU clock or counter reset');
  const value = usage * 1000 / Number(elapsedNs) / allocatedCores * 100;
  if (!Number.isFinite(value)) throw new Error('nonfinite CPU percentage');
  return value;
}
