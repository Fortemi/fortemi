import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { exerciseControl, loadControls } from './remote-negative-controls.mjs';

const [tarballArgument, outputArgument, cacheArgument, ...extra] = process.argv.slice(2);
assert.ok(tarballArgument && outputArgument && cacheArgument && extra.length === 0,
  'usage: verify-remote-negative-package.mjs <published-core-tarball> <new-output-directory> <npm-cache>');
const unit = process.env.FORTEMI_LOCAL_TEST_UNIT;
assert.match(unit ?? '', /^fortemi-local-test-1000-[a-f0-9-]+\.service$/);
assert.ok(readFileSync('/proc/self/cgroup', 'utf8').split('\n').some(line => line.endsWith('/' + unit)));
const bounds = spawnSync('systemctl', ['show', unit, '-p', 'PrivateNetwork', '-p', 'PrivateTmp', '-p', 'PrivateDevices', '-p', 'MemoryMax', '-p', 'MemorySwapMax', '-p', 'CPUQuotaPerSecUSec'], { encoding: 'utf8', timeout: 10000 });
assert.equal(bounds.status, 0);
for (const property of ['PrivateNetwork=yes', 'PrivateTmp=yes', 'PrivateDevices=yes', 'MemoryMax=8589934592', 'MemorySwapMax=0', 'CPUQuotaPerSecUSec=2s']) assert.ok(bounds.stdout.split('\n').includes(property));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const tarball = resolve(tarballArgument), root = resolve(outputArgument), cache = resolve(cacheArgument);
const packageSha256 = hash(readFileSync(tarball));
assert.equal(packageSha256, '4a126d59bc18af4fb981d5bdbf19a761402e246b7d85cf3e6445de4eacf92897');
const { controls, native, sha256 } = loadControls();
mkdirSync(root, { mode: 0o700 });
const save = (name, data) => writeFileSync(join(root, name + '.json'), JSON.stringify(data, null, 2) + '\n', { flag: 'wx' });
const scratch = mkdtempSync(join(tmpdir(), 'remote-negative-package-'));
const report = { schemaVersion: 'fortemi.remote-negative-package.v1', status: 'RUNNING', unit, packageVersion: '2026.9.4',
  packageCommit: '9f74c0bab0cce5ef8e2433e8944a4efda5575eb4', packageSha256, fixtureSha256: sha256,
  basisSha256: controls.basis.sha256, scriptSha256: hash(readFileSync(import.meta.filename)),
  helperSha256: hash(readFileSync(new URL('./remote-negative-controls.mjs', import.meta.url))),
  classification: controls.classification, claims: controls.claims, boundary: controls.boundary, checks: [] };
try {
  writeFileSync(scratch + '/package.json', JSON.stringify({ private: true, type: 'module' }));
  writeFileSync(scratch + '/user.npmrc', ''); writeFileSync(scratch + '/global.npmrc', '');
  const install = spawnSync('npm', ['install', '--offline', '--ignore-scripts', '--no-audit', '--no-fund', tarball], {
    cwd: scratch, encoding: 'utf8', timeout: 90000, maxBuffer: 1048576,
    env: { ...process.env, npm_config_cache: cache, npm_config_userconfig: scratch + '/user.npmrc', npm_config_globalconfig: scratch + '/global.npmrc' },
  });
  save('install', { status: install.status, stdout: install.stdout, stderr: install.stderr });
  assert.equal(install.status, 0);
  const core = await import(pathToFileURL(scratch + '/node_modules/@fortemi/core/dist/index.js'));
  assert.equal(core.VERSION, report.packageVersion);
  for (const control of controls.controls) {
    for (const method of control.stage === 'note' ? ['getNote', 'getNoteFull'] : ['getNoteFull']) {
      report.checks.push(await exerciseControl(core, control, native, method));
    }
  }
  assert.equal(report.checks.length, 31);
  assert.ok(report.checks.every(check => check.listenerClosed && check.injected === 1));
  report.status = 'PASS';
} catch (error) {
  report.status = 'FAIL';
  report.failure = String(error).slice(0, 2000);
  throw error;
} finally {
  rmSync(scratch, { recursive: true });
  report.scratchRemoved = !existsSync(scratch);
  save('results', report);
}
console.log(JSON.stringify({ status: report.status, controls: controls.controls.length, checks: report.checks.length,
  requests: report.checks.reduce((n, c) => n + c.requests.length, 0), packageSha256, fixtureSha256: sha256, scratchRemoved: report.scratchRemoved }));
