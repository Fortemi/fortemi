import fs from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';

/** Content-addressed publication into an existing operator-selected directory. */
export function createLoadEvidenceWriter(root, limits) {
  if (process.platform !== 'linux' || !['maxFileBytes', 'maxTotalBytes', 'maxFiles'].every(k => Number.isSafeInteger(limits?.[k]) && limits[k] > 0)
    || limits.maxFileBytes > limits.maxTotalBytes || limits.maxTotalBytes > 1024 ** 3 || limits.maxFiles > 10000) throw new Error('bounded evidence writer configuration required');
  const budget = { ...limits };
  const directory = fs.openSync(root, fs.constants.O_RDONLY | fs.constants.O_DIRECTORY | fs.constants.O_NOFOLLOW);
  const anchor = `/proc/self/fd/${directory}`;
  let closed = false, attemptedFiles = 0, attemptedBytes = 0;
  function sameFile(file, bytes) {
    const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
    try {
      const before = fs.fstatSync(fd);
      if (!before.isFile() || before.size !== bytes.length) throw new Error('conflicting evidence artifact');
      const actual = Buffer.alloc(bytes.length); let offset = 0;
      while (offset < actual.length) {
        const n = fs.readSync(fd, actual, offset, actual.length - offset, offset);
        if (!n) break; offset += n;
      }
      if (offset !== actual.length || fs.readSync(fd, Buffer.alloc(1), 0, 1, offset) !== 0
        || !actual.equals(bytes) || fs.fstatSync(fd).size !== before.size) throw new Error('conflicting evidence bytes');
    } finally { fs.closeSync(fd); }
  }
  return {
    write(input) {
      if (closed) throw new Error('evidence writer closed');
      if (!Buffer.isBuffer(input) || input.length > budget.maxFileBytes) throw new Error('bounded artifact bytes required');
      if (attemptedFiles + 1 > budget.maxFiles || attemptedBytes + input.length > budget.maxTotalBytes) throw new Error('evidence write budget exceeded');
      // Charge attempts, including retries and failures: repeated publication
      // cannot evade the caller's per-run I/O budget.
      attemptedFiles++; attemptedBytes += input.length;
      const bytes = Buffer.from(input);
      const hex = createHash('sha256').update(bytes).digest('hex');
      const name = `sha256-${hex}`, destination = `${anchor}/${name}`;
      const temporary = `${anchor}/.load-evidence-${randomUUID()}.tmp`;
      let fd, created = false;
      try {
        fd = fs.openSync(temporary, fs.constants.O_WRONLY | fs.constants.O_CREAT | fs.constants.O_EXCL | fs.constants.O_NOFOLLOW, 0o600);
        created = true; let offset = 0;
        while (offset < bytes.length) {
          const n = fs.writeSync(fd, bytes, offset, bytes.length - offset);
          if (!n) throw new Error('incomplete evidence write'); offset += n;
        }
        fs.fchmodSync(fd, 0o400); fs.fsyncSync(fd); fs.closeSync(fd); fd = undefined;
        // Hard-link publication is atomic and cannot replace a pre-existing name.
        try { fs.linkSync(temporary, destination); }
        catch (error) { if (error.code !== 'EEXIST') throw error; sameFile(destination, bytes); }
        fs.unlinkSync(temporary); created = false; fs.fsyncSync(directory);
        return { path: name, digest: `sha256:${hex}`, bytes: bytes.length };
      } finally {
        if (fd !== undefined) fs.closeSync(fd);
        if (created) fs.unlinkSync(temporary);
      }
    },
    usage() { return { attemptedFiles, attemptedBytes }; },
    close() { if (!closed) { closed = true; fs.closeSync(directory); } },
  };
}
