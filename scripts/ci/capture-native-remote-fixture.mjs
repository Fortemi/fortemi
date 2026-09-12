import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, mkdtempSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
const [binaryArgument, tarballArgument, outputArgument, cacheArgument] = process.argv.slice(2);
assert.ok(binaryArgument && tarballArgument && outputArgument && cacheArgument,
  'usage: capture-native-remote-fixture.mjs <released-api> <core-tarball> <new-output-directory> <npm-cache>');
const unit = process.env.FORTEMI_LOCAL_TEST_UNIT;
assert.match(unit ?? '', /^fortemi-local-test-1000-[a-f0-9-]+\.service$/);
assert.ok(readFileSync('/proc/self/cgroup', 'utf8').split('\n').some(line => line.endsWith('/' + unit)));
assert.match(process.env.PGHOST ?? '', /^\/tmp\/fortemi-ephemeral-pg-/);
const bounds = spawnSync('systemctl', ['show', unit, '-p', 'PrivateNetwork', '-p', 'PrivateTmp', '-p', 'PrivateDevices', '-p', 'MemoryMax', '-p', 'MemorySwapMax', '-p', 'CPUQuotaPerSecUSec'], { encoding: 'utf8', timeout: 10000 });
assert.equal(bounds.status, 0);
for (const property of ['PrivateNetwork=yes', 'PrivateTmp=yes', 'PrivateDevices=yes', 'MemoryMax=8589934592', 'MemorySwapMax=0', 'CPUQuotaPerSecUSec=2s']) assert.ok(bounds.stdout.split('\n').includes(property));
const root = resolve(outputArgument), binary = resolve(binaryArgument), tarball = resolve(tarballArgument), cache = resolve(cacheArgument);
const hash = value => createHash('sha256').update(value).digest('hex');
const artifact = { version: '2026.9.9', commit: 'e91c595a896275f835cb7ed1aef173cb26056206', name: 'matric-api-x86_64-unknown-linux-gnu', sha256: '19b98d48d7c92e8e9d3d7d514fd60a8817f144926c1a4270adf6e0a052c39bc9', kind: 'published-native-linux-amd64' };
assert.equal(hash(readFileSync(binary)), artifact.sha256);
assert.equal(hash(readFileSync(tarball)), '4a126d59bc18af4fb981d5bdbf19a761402e246b7d85cf3e6445de4eacf92897');
mkdirSync(root, { mode: 0o700 });
const save = (name, value) => writeFileSync(root + '/' + name + '.json', JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const scratch = mkdtempSync(join(tmpdir(), 'lane-b-live-read-'));
const report = { schemaVersion: 'fortemi.native-remote-fixture.v1', status: 'RUNNING', checks: [], calls: [], artifact, packageVersion: '2026.9.4', packageCommit: '9f74c0bab0cce5ef8e2433e8944a4efda5575eb4', packageSha256: hash(readFileSync(tarball)), scope: 'Published native server and clean-installed published Core over real private-loopback HTTP. Required local API identity in personal AllowAllPolicy mode, not note-scope denial or hosted OIDC/multi-tenant qualification. No inference. Suite NO-GO.' };
report.probeSha256 = hash(readFileSync(import.meta.filename));
report.boundedUnit = unit;
const privateValues = [], apiOutput = [];
let api, exitPromise, outputBytes = 0;
function sql(query) {
  const r = spawnSync('/usr/lib/postgresql/18/bin/psql', ['-X', '-v', 'ON_ERROR_STOP=1', '-At'], { input: query, encoding: 'utf8', timeout: 10000, maxBuffer: 131072, env: { ...process.env, PGOPTIONS: '-c app.current_tenant=00000000-0000-0000-0000-000000000000' } });
  if (r.status !== 0) throw new Error('fixture SQL failed: ' + r.stderr.slice(-1000));
  return r.stdout.trim();
}
const base = 'http://127.0.0.1:39151';
let core;
let currentCheck = 'fixture setup';
const check = async (name, fn) => { currentCheck = name; await fn(); report.checks.push(name); currentCheck = 'fixture setup'; };
const request = async (path, options = {}, record = true) => {
  const response = await fetch(base + path, { ...options, signal: AbortSignal.timeout(5000) });
  if (record) {
    const bytes = Buffer.from(await response.clone().arrayBuffer()); assert.ok(bytes.length <= 1048576);
    let body; try { body = JSON.parse(bytes); } catch { body = bytes.toString(); }
    report.calls.push({ check: currentCheck, rawBody: bytes.toString('utf8'), method: options.method ?? 'GET', path, status: response.status, contentType: response.headers.get('content-type'), retryAfter: response.headers.get('retry-after'), responseSha256: hash(bytes), body });
  }
  return response;
};
try {
  writeFileSync(scratch + '/package.json', JSON.stringify({ private: true, type: 'module' }));
  writeFileSync(scratch + '/user.npmrc', ''); writeFileSync(scratch + '/global.npmrc', '');
  const installed = spawnSync('npm', ['install', '--offline', '--ignore-scripts', '--no-audit', '--no-fund', tarball], { cwd: scratch, encoding: 'utf8', timeout: 90000, maxBuffer: 1048576,
    env: { ...process.env, npm_config_cache: cache, npm_config_userconfig: scratch + '/user.npmrc', npm_config_globalconfig: scratch + '/global.npmrc' } });
  save('install', { status: installed.status, stdout: installed.stdout, stderr: installed.stderr });
  assert.equal(installed.status, 0);
  core = await import(pathToFileURL(scratch + '/node_modules/@fortemi/core/dist/index.js'));
  assert.equal(core.VERSION, '2026.9.4');
  sql('CREATE EXTENSION IF NOT EXISTS postgis;');
  api = spawn(binary, [], { cwd: scratch, stdio: ['ignore', 'pipe', 'pipe'], env: {
    ...process.env, HOME: scratch, XDG_CONFIG_HOME: scratch + '/config', HOST: '127.0.0.1', PORT: '39151',
    REQUIRE_AUTH: 'true', FORTEMI_MULTI_TENANT: 'false', ISSUER_URL: base, FORTEMI_ALLOW_LOCAL_ISSUER: 'true',
    MATRIC_ATTACHMENT_SCAN_MODE: 'disabled',
    WORKER_ENABLED: 'false', JOB_WORKER_ENABLED: 'false', OLLAMA_BASE_URL: 'http://127.0.0.1:1',
    REDIS_ENABLED: 'false', WHISPER_BASE_URL: '', OLLAMA_VISION_MODEL: '', DIARIZATION_BASE_URL: '',
    FILE_STORAGE_PATH: scratch + '/files', TUS_STAGING_DIR: scratch + '/tus',
    RATE_LIMIT_ENABLED: 'true', RATE_LIMIT_REQUESTS: '80', RATE_LIMIT_PERIOD_SECS: '60',
    MATRIC_SHUTDOWN_GRACE_SECS: '2', RUST_LOG: 'warn', TOKIO_WORKER_THREADS: '2',
  } });
  exitPromise = new Promise(resolve => { api.once('exit', (code, signal) => resolve({ code, signal })); api.once('error', error => resolve({ error: error.message })); });
  for (const stream of [api.stdout, api.stderr]) stream.on('data', chunk => { outputBytes += chunk.length; if (outputBytes <= 262144) apiOutput.push(chunk); else api.kill('SIGTERM'); });
  report.apiPid = api.pid;
  if (api.pid) assert.ok(readFileSync('/proc/' + api.pid + '/cgroup', 'utf8').includes('/' + unit));
  let health;
  for (let n = 0; n < 240; n++) {
    if (api.exitCode !== null || api.signalCode !== null) throw new Error('API exited before readiness');
    try { const response = await request('/health', {}, false); if (response.status === 200) { health = await response.json(); break; } } catch {}
    await delay(250);
  }
  assert.ok(health, 'API readiness timeout'); report.health = health;
  assert.equal(health.version, '2026.9.9'); assert.equal(health.capabilities.auth_required, true);
  report.executableSha256 = hash(readFileSync('/proc/' + api.pid + '/exe')); assert.equal(report.executableSha256, artifact.sha256);
  report.healthIdentityBoundary = 'Health git_sha is environment-derived and not used as independent source proof; executable digest and released provenance bind source.';
  report.initialPhysicalNotes = Number(sql('SELECT count(*) FROM note;')); assert.equal(report.initialPhysicalNotes, 0);
  report.migrations = Number(sql('SELECT count(*) FROM _sqlx_migrations WHERE success;'));
  const keys = {};
  for (const [name, scope] of [['reader', 'read'], ['writer', 'read write'], ['mcpOnly', 'mcp']]) {
    const value = 'mm_key_' + randomBytes(32).toString('hex'); privateValues.push(value, hash(value)); keys[name] = value;
    sql(`INSERT INTO api_key(id,key_hash,key_prefix,name,scope) VALUES ('${randomUUID()}','${hash(value)}','fixture-only','lane-b-${name}','${scope}');`);
  }
  const remote = key => core.createRemoteBackend({ baseUrl: base, fetchImpl: async (url, init = {}) => {
    assert.equal(new URL(url).origin, base);
    const headers = new Headers(init.headers); if (key) headers.set('Authorization', 'Bearer ' + key);
    return request(new URL(url).pathname + new URL(url).search, { ...init, headers });
  } });
  const reader = remote(keys.reader), writer = remote(keys.writer);
  await check('empty authenticated list', async () => assert.equal((await reader.listNotes()).total, 0));
  const ids = [];
  for (const name of ['first', 'second']) {
    const created = await writer.manageNote({ action: 'create', title: 'Synthetic ' + name, content: 'LANE B LIVE READ CONTENT', tags: ['lane-b-live-read'], source: 'bounded-native-fixture' });
    assert.match(created.note_id, /^[a-f0-9-]{36}$/); ids.push(created.note_id);
  }
  const [first, second] = ids;
  await check('nonempty authenticated list', async () => { const result = await reader.listNotes({ limit: 10 }); assert.equal(result.total, 2); assert.deepEqual(result.items.map(n => n.id).sort(), ids.toSorted()); });
  await check('note identity UTC tags', async () => { const note = await reader.getNote(first); assert.equal(note.id, first); assert.equal(note.title, 'Synthetic first'); for (const value of [note.createdAt, note.updatedAt]) { assert.ok(Number.isFinite(Date.parse(value))); assert.match(value, /(?:Z|\+00:00)$/); } assert.ok(note.tags.includes('lane-b-live-read')); });
  const link = await request('/api/v1/notes/' + first + '/links', { method: 'POST', headers: { Authorization: 'Bearer ' + keys.writer, 'Content-Type': 'application/json' }, body: JSON.stringify({ to_note_id: second, kind: 'explicit', score: 0.75 }) });
  assert.equal(link.status, 201);
  await check('directional relationship reads', async () => {
    const outgoing = await reader.linksOf(first), incoming = await reader.linksOf(second);
    assert.equal(outgoing[0].direction, 'outgoing'); assert.equal(incoming[0].direction, 'incoming');
    assert.equal(outgoing[0].fromNoteId, first); assert.equal(outgoing[0].toNoteId, second);
  });
  await check('composed existing note remains accessible', async () => { const full = await reader.getNoteFull(first); assert.equal(full.id, first); assert.equal(full.content, 'LANE B LIVE READ CONTENT'); assert.equal(full.links.length, 1); assert.equal(full.provenanceGraph.note_id, first); });
  const revision = randomUUID();
  sql(`BEGIN;
INSERT INTO note_revision(id,note_id,revision_number,content,created_at_utc) VALUES ('${revision}','${first}',1,'Synthetic provenance content','2026-09-12T00:00:00Z');
INSERT INTO provenance_activity(id,note_id,revision_id,activity_type,model_name,started_at,ended_at,metadata) VALUES ('${randomUUID()}','${first}','${revision}','ai_revision','synthetic-fixture-model','2026-09-12T00:00:00Z','2026-09-12T00:00:01Z','{"synthetic":true}');
INSERT INTO provenance_edge(id,revision_id,source_note_id,relation,created_at_utc) VALUES ('${randomUUID()}','${revision}','${second}','wasDerivedFrom','2026-09-12T00:00:01Z');
UPDATE note_revised_current SET last_revision_id='${revision}',content='Synthetic current revised content' WHERE note_id='${first}';
COMMIT;`);
  await check('seeded nonempty provenance and revised content', async () => {
    const graph = await reader.provenanceGraphOf(first); assert.equal(graph.all_activities.length, 1); assert.equal(graph.all_edges.length, 1);
    const full = await reader.getNoteFull(first); assert.equal(full.content, 'Synthetic current revised content'); assert.deepEqual(full.provenanceGraph, graph); assert.equal(full.concepts.length, 1);
  });
  await check('authenticated authoritative not-found', async () => { const absent = randomUUID(); assert.equal(await reader.getNote(absent), null); assert.equal(await reader.getNoteFull(absent), null); });
  for (const [name, key, status] of [['missing identity', undefined, 401], ['invalid identity', 'mm_key_invalid_fixture', 401]]) await check(name, async () => {
    for (const method of ['getNote', 'getNoteFull']) await assert.rejects(remote(key)[method](first), e => e instanceof core.RemoteBackendError && e.kind === 'http' && e.status === status);
  });
  await check('personal AllowAllPolicy permits authenticated MCP-scoped note read', async () => {
    for (const method of ['getNote', 'getNoteFull']) assert.equal((await remote(keys.mcpOnly)[method](first)).id, first);
  });
  await check('producer operator inventory denies non-admin identity', async () => {
    const denied = await request('/api/v1/operator/openapi.yaml', { headers: { Authorization: 'Bearer ' + keys.reader } });
    assert.equal(denied.status, 403);
    report.denialBoundary = 'Actual 403 is producer-only operator inventory, not Core getNote/getNoteFull denial. Personal mode uses AllowAllPolicy for note routes; hosted note denial and read-only mutation enforcement remain unqualified.';
  });
  await check('real producer500 from reversible private database fault', async () => {
    sql('ALTER TABLE note RENAME TO lane_b_temporarily_unavailable_note;');
    try {
      for (const method of ['getNote', 'getNoteFull']) await assert.rejects(reader[method](first), e => e.kind === 'http' && e.status === 500 && !String(e).includes('lane_b_temporarily'));
    } finally { sql('ALTER TABLE lane_b_temporarily_unavailable_note RENAME TO note;'); }
    assert.equal((await reader.getNote(first)).id, first);
    report.databaseFaultRestored = true;
  });
  await check('real rate limit429 reaches installed consumer', async () => {
    let limited = false;
    for (let n = 0; n < 100; n++) {
      try { await reader.getNote(first); } catch (e) { assert.equal(e.kind, 'http'); assert.equal(e.status, 429); limited = true; break; }
    }
    assert.equal(limited, true, 'rate limit not reached within bounded probe');
    await assert.rejects(reader.getNoteFull(first), e => e.kind === 'http' && e.status === 429);
  });
  report.status = 'PASS';
  report.syntheticPhysicalNotesBeforeClusterRemoval = Number(sql('SELECT count(*) FROM note;'));
} catch (error) { report.status = 'FAIL'; report.error = String(error.stack).slice(0, 2500); process.exitCode = 1; }
finally {
  if (api) {
    if (api.exitCode === null && api.signalCode === null) api.kill('SIGTERM');
    const grace = await Promise.race([exitPromise, delay(5000).then(() => null)]);
    if (!grace && api.exitCode === null && api.signalCode === null) api.kill('SIGKILL');
    report.apiExit = await exitPromise;
    report.apiPidAbsent = api.pid ? !existsSync('/proc/' + api.pid) : true;
  }
  rmSync(scratch, { recursive: true, force: true }); report.scratchRemoved = !existsSync(scratch);
  let logs = Buffer.concat(apiOutput).toString();
  for (const value of privateValues) { logs = logs.replaceAll(value, '[REDACTED]'); if (report.error) report.error = report.error.replaceAll(value, '[REDACTED]'); }
  writeFileSync(root + '/api.log', logs, { flag: 'wx' });
  let encoded = JSON.stringify(report, null, 2);
  for (const value of privateValues) assert.ok(!encoded.includes(value), 'Do not persist test authentication material');
  save('live', report);
}
console.log(JSON.stringify({ status: report.status, checks: report.checks.length, calls: report.calls.length, error: report.error }));
