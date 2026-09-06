#!/usr/bin/env node
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { compileLoadConfig, loadConfigTemplate } from './load-config.mjs';
import { canonicalJson } from './canonical-json.mjs';

export function compileLoadPlanCli(args) {
  let configPath, profile, output, template = false;
  const overrides = [], seen = new Set();
  for (let i = 0; i < args.length; i++) {
    const option = args[i];
    if (option === '--template') { if (template) throw Error('duplicate --template'); template = true; continue; }
    if (!['--config', '--profile', '--output', '--set'].includes(option) || i + 1 === args.length) throw Error('expected --config FILE --profile NAME [--set path=JSON] [--output FILE], or --template');
    if (option !== '--set' && seen.has(option)) throw Error(`duplicate ${option}`);
    seen.add(option); const value = args[++i];
    if (option === '--config') configPath = value;
    else if (option === '--profile') profile = value;
    else if (option === '--output') output = value;
    else overrides.push(value);
  }
  if (template && (configPath || profile || overrides.length)) throw Error('--template cannot select a run');
  let result;
  if (template) result = loadConfigTemplate();
  else {
    if (!configPath || !profile) throw Error('--config and --profile required');
    const fd = fs.openSync(configPath, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
    let bytes;
    try {
      const stat = fs.fstatSync(fd);
      if (!stat.isFile() || stat.size > 1024 * 1024) throw Error('config must be a regular file <=1 MiB');
      bytes = Buffer.alloc(1024 * 1024 + 1); let used = 0;
      while (used < bytes.length) { const n = fs.readSync(fd, bytes, used, bytes.length - used, null); if (!n) break; used += n; }
      if (used > 1024 * 1024) throw Error('config exceeds 1 MiB');
      bytes = bytes.subarray(0, used);
    } finally { fs.closeSync(fd); }
    result = compileLoadConfig(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)), profile, overrides);
  }
  const text = template ? `${JSON.stringify(result, null, 2)}\n` : canonicalJson(result);
  if (output) fs.writeFileSync(output, text, { flag: 'wx', mode: 0o600 });
  return { result, text, output };
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    const { result, text, output } = compileLoadPlanCli(process.argv.slice(2));
    process.stdout.write(output ? `${JSON.stringify({ output, ready: result.ready ?? false, digest: result.digest ?? null, missing: result.missing ?? [], executionAuthorized: false }, null, 2)}\n` : text);
  } catch (error) { process.stderr.write(`${error.message}\n`); process.exitCode = 1; }
}
