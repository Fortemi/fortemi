import fs from 'node:fs';
import path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';

/** Supervise a caller-authorized, digest-pinned Node entrypoint in its own process group. */
export async function superviseLoadProcess({ entrypoint, entryDigest, input, durationMs, graceMs,
  maxOutputBytes, heapMiB, cwd, environment = {}, signal }) {
  if (process.platform !== 'linux' || !path.isAbsolute(entrypoint || '') || !path.isAbsolute(cwd || '')
    || !Number.isSafeInteger(durationMs) || durationMs < 1 || durationMs > 43200000
    || !Number.isSafeInteger(graceMs) || graceMs < 1 || graceMs > 5000
    || !Number.isSafeInteger(maxOutputBytes) || maxOutputBytes < 1 || maxOutputBytes > 64 * 1024 * 1024
    || !Number.isSafeInteger(heapMiB) || heapMiB < 16 || heapMiB > 1024
    || !environment || typeof environment !== 'object' || Array.isArray(environment)
    || Object.entries(environment).some(([k, v]) => !/^[A-Z_][A-Z0-9_]*$/.test(k) || typeof v !== 'string' || v.includes('\0')
      || ['NODE_OPTIONS', 'NODE_PATH', 'LD_PRELOAD', 'LD_LIBRARY_PATH'].includes(k))) throw new Error('bounded supervisor configuration required');
  const fd = fs.openSync(entrypoint, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  let bytes;
  try {
    const stat = fs.fstatSync(fd);
    if (!stat.isFile() || stat.size > 4 * 1024 * 1024) throw new Error('bounded regular entrypoint required');
    const buffer = Buffer.alloc(4 * 1024 * 1024 + 1); let used = 0;
    while (used < buffer.length) {
      const n = fs.readSync(fd, buffer, used, buffer.length - used, null);
      if (!n) break; used += n;
    }
    if (used > 4 * 1024 * 1024) throw new Error('entrypoint grew beyond bound');
    bytes = buffer.subarray(0, used);
  } finally { fs.closeSync(fd); }
  if (`sha256:${createHash('sha256').update(bytes).digest('hex')}` !== entryDigest) throw new Error('entrypoint digest mismatch');
  const stdin = Buffer.from(JSON.stringify(input));
  if (stdin.length > 1024 * 1024) throw new Error('supervisor input budget exceeded');
  if (signal?.aborted) return { reason: 'external-abort', spawned: false, cleanupVerified: false };
  // A private snapshot preserves relative imports while preventing replacement
  // of the original path between digest verification and execution.
  const snapshot = path.join(path.dirname(entrypoint), `.load-entry-${randomUUID()}.mjs`);
  fs.writeFileSync(snapshot, bytes, { flag: 'wx', mode: 0o400 });
  try { return await new Promise(resolve => {
    const child = spawn(process.execPath, ['--unhandled-rejections=strict', `--max-old-space-size=${heapMiB}`, snapshot],
      { cwd, detached: true, stdio: ['pipe', 'pipe', 'pipe'], env: { PATH: '/usr/bin:/bin', LANG: 'C.UTF-8', ...environment } });
    let reason = null, total = 0, exitCode = null, exitSignal = null, settled = false;
    let hardTimer, finishTimer; const chunks = [];
    const exists = () => {
      if (!child.pid) return false;
      try { process.kill(-child.pid, 0); return true; } catch (e) { return e.code !== 'ESRCH'; }
    };
    const send = sig => { if (child.pid) { try { process.kill(-child.pid, sig); } catch (e) { if (e.code !== 'ESRCH') reason ||= 'signal-failed'; } } };
    const finish = () => {
      if (settled) return; settled = true;
      clearTimeout(deadline); clearTimeout(hardTimer); clearTimeout(finishTimer);
      signal?.removeEventListener('abort', abort);
      child.stdout.destroy(); child.stderr.destroy(); child.stdin.destroy();
      resolve({ spawned: Boolean(child.pid), reason, exitCode, exitSignal,
        processGroupGone: !exists(), outputBytes: total, captured: Buffer.concat(chunks),
        cleanupVerified: false, admitted: false });
    };
    const stop = why => {
      reason ||= why;
      if (hardTimer || settled) return;
      send('SIGTERM');
      hardTimer = setTimeout(() => {
        if (exists()) send('SIGKILL');
        // Give the OS a bounded interval to reap; never label residual processes clean.
        finishTimer = setTimeout(finish, graceMs);
      }, graceMs);
    };
    const abort = () => stop('external-abort');
    const deadline = setTimeout(() => stop('deadline'), durationMs);
    signal?.addEventListener('abort', abort, { once: true });
    if (signal?.aborted) abort();
    for (const stream of [child.stdout, child.stderr]) stream.on('data', data => {
      const remaining = Math.max(0, maxOutputBytes - total);
      if (remaining) chunks.push(data.subarray(0, remaining));
      total += data.length;
      if (total > maxOutputBytes) stop('output-limit');
    });
    child.on('error', () => { reason ||= 'spawn-error'; finish(); });
    child.on('exit', (code, sig) => {
      exitCode = code; exitSignal = sig;
      if (exists()) stop('residual-process-group');
    });
    child.on('close', () => {
      if (exists()) stop('residual-process-group'); else finish();
    });
    child.stdin.on('error', () => stop('input-delivery-failed'));
    child.stdin.end(stdin);
  }); } finally { fs.unlinkSync(snapshot); }
}
