import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { compileLoadPlanCli } from './compile-load-plan.mjs';
import { parseCanonicalJson } from './canonical-json.mjs';

test('editable template and profile overrides produce canonical inspectable artifacts without replacement', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'load-config-'));
  try {
    const source = path.join(dir, 'config.json'), output = path.join(dir, 'plan.json');
    compileLoadPlanCli(['--template', '--output', source]);
    const r = compileLoadPlanCli(['--config', source, '--profile', 'calibration', '--set', 'runner.maxConcurrency=2', '--output', output]);
    const plan = parseCanonicalJson(fs.readFileSync(output));
    assert.equal(plan.config.runner.maxConcurrency, 2); assert.equal(plan.ready, false);
    assert.equal(plan.executionAuthorized, false); assert.equal(plan.digest, r.result.digest);
    assert.throws(() => compileLoadPlanCli(['--config', source, '--profile', 'calibration', '--output', output]), /EEXIST/);
    assert.throws(() => compileLoadPlanCli(['--config', source, '--profile', 'calibration', '--set', 'runner.maxConcurency=2']), /unknown/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
test('CLI rejects unknown flags and unsafe input kinds before compilation', () => {
  assert.throws(() => compileLoadPlanCli(['--execute']));
  assert.throws(() => compileLoadPlanCli(['--template', '--set', 'runner.pollMs=2']));
  assert.throws(() => compileLoadPlanCli(['--config', '/tmp', '--profile', 'calibration']), /regular file/);
  assert.throws(() => compileLoadPlanCli(['--profile', 'calibration', '--profile', 'qualification']));
});
