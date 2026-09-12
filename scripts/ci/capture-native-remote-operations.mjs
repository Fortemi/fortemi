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
  'usage: capture-native-remote-operations.mjs <released-api> <core-tarball> <new-output-directory> <npm-cache>');
const unit = process.env.FORTEMI_LOCAL_TEST_UNIT;
assert.match(unit ?? '', /^fortemi-local-test-1000-[a-f0-9-]+\.service$/);
assert.ok(readFileSync('/proc/self/cgroup', 'utf8').split('\n').some(line => line.endsWith('/' + unit)));
assert.match(process.env.PGHOST ?? '', /^\/tmp\/fortemi-ephemeral-pg-/);
const bounds = spawnSync('systemctl', ['show', unit, '-p', 'PrivateNetwork', '-p', 'PrivateTmp', '-p', 'PrivateDevices', '-p', 'MemoryMax', '-p', 'MemorySwapMax', '-p', 'CPUQuotaPerSecUSec'], { encoding: 'utf8', timeout: 10000 });
assert.equal(bounds.status, 0);
for (const property of ['PrivateNetwork=yes', 'PrivateTmp=yes', 'PrivateDevices=yes', 'MemoryMax=8589934592', 'MemorySwapMax=0', 'CPUQuotaPerSecUSec=2s']) assert.ok(bounds.stdout.split('\n').includes(property));
const historicalBytes = readFileSync(new URL('../../contracts/openapi/fixtures/remote-operations.json', import.meta.url));
assert.equal(createHash('sha256').update(historicalBytes).digest('hex'), 'c086a4ec2f02fb3e1c12b25c93427dbe720a4527b6eadef6cd3244b568397d52');
const root = resolve(outputArgument), binary = resolve(binaryArgument), tarball = resolve(tarballArgument), cache = resolve(cacheArgument);
const hash = value => createHash('sha256').update(value).digest('hex');
const artifact = { version: '2026.9.9', commit: 'e91c595a896275f835cb7ed1aef173cb26056206', name: 'matric-api-x86_64-unknown-linux-gnu', sha256: '19b98d48d7c92e8e9d3d7d514fd60a8817f144926c1a4270adf6e0a052c39bc9', kind: 'published-native-linux-amd64' };
assert.equal(hash(readFileSync(binary)), artifact.sha256);
assert.equal(hash(readFileSync(tarball)), '4a126d59bc18af4fb981d5bdbf19a761402e246b7d85cf3e6445de4eacf92897');
mkdirSync(root, { mode: 0o700 });
const save = (name, value) => writeFileSync(root + '/' + name + '.json', JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const scratch = mkdtempSync(join(tmpdir(), 'lane-b-live-operations-'));
const report = { schemaVersion: 'fortemi.native-remote-operations.v1', status: 'RUNNING', checks: [], calls: [], artifact, packageVersion: '2026.9.4', packageCommit: '9f74c0bab0cce5ef8e2433e8944a4efda5575eb4', packageSha256: hash(readFileSync(tarball)), scope: 'Published native server and clean-installed published Core over real private-loopback HTTP. Required local API identity in personal AllowAllPolicy mode, not note-scope denial or hosted OIDC/multi-tenant qualification. Unavailable inference: explicit semantic/hybrid fallback only, no positive vector retrieval. Suite NO-GO.' };
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
  assert.ok(report.calls.length < 300, 'bounded HTTP request budget');
  const response = await fetch(base + path, { ...options, signal: AbortSignal.timeout(5000) });
  if (record) {
    const bytes = Buffer.from(await response.clone().arrayBuffer()); assert.ok(bytes.length <= 1048576);
    let body; try { body = JSON.parse(bytes); } catch { body = bytes.toString(); }
    report.calls.push({ check: currentCheck, rawBody: bytes.toString('utf8'), method: options.method ?? 'GET', path, requestBody: options.body ? JSON.parse(options.body) : null, status: response.status, contentType: response.headers.get('content-type'), retryAfter: response.headers.get('retry-after'), responseSha256: hash(bytes), body });
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
    RATE_LIMIT_ENABLED: 'true', RATE_LIMIT_REQUESTS: '500', RATE_LIMIT_PERIOD_SECS: '60',
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
  report.producerFixture = {
    commit: '9588dea2d20fe16086da26f078723f74c7c51327',
    sha256: 'c086a4ec2f02fb3e1c12b25c93427dbe720a4527b6eadef6cd3244b568397d52',
    historical: true,
  };
  await check('clean authenticated destination and capabilities', async () => {
    assert.equal((await reader.listNotes()).total, 0);
    assert.equal(writer.capabilities.write, true);
    assert.equal(writer.capabilities.merge, false);
    assert.equal(reader.capabilities.semantic, 'server');
  });
  const ids = [];
  const tag = 'native-package-' + randomUUID();
  await check('create both synthetic notes through published adapter', async () => {
    for (const selection of ['selected', 'excluded']) {
      const result = await writer.manageNote({ action: 'create', title: 'Synthetic ' + selection,
        content: 'PACKAGE REMOTE NEEDLE', tags: [tag, selection], source: 'native-package-test' });
      assert.match(result.note_id, /^[a-f0-9-]{36}$/);
      ids.push(result.note_id);
      assert.equal((await reader.getNoteFull(result.note_id)).content, 'PACKAGE REMOTE NEEDLE');
    }
    assert.equal(new Set(ids).size, 2);
    assert.equal(Number(sql('SELECT count(*) FROM note;')), 2);
  });
  const [first, second] = ids;
  await check('authenticated FTS q AND-tags and actual EnhancedSearchHit projection', async () => {
    const offset = report.calls.length;
    const result = await reader.search('NEEDLE', { mode: 'fts', tags: [tag, 'selected'], limit: 10 });
    assert.deepEqual(result.hits.map(hit => hit.note.id), [first]);
    assert.equal(result.total, 1);
    assert.equal(result.totalKind, 'returned-hits');
    assert.equal(result.requestedMode, 'fts'); assert.equal(result.effectiveMode, 'fts');
    assert.equal(result.degraded, false);
    const calls = report.calls.slice(offset);
    const searchCall = calls.find(call => new URL(call.path, base).pathname === '/api/v1/search');
    const params = new URL(searchCall.path, base).searchParams;
    assert.equal(params.get('q'), 'NEEDLE'); assert.equal(params.has('query'), false);
    assert.deepEqual(params.getAll('tags'), [[tag, 'selected'].join(',')]);
    assert.equal(params.get('limit'), '10');
    const raw = searchCall.body.results[0], hit = result.hits[0];
    assert.equal(hit.note.id, raw.note_id); assert.equal(hit.rank, raw.score);
    assert.equal(hit.snippet, raw.snippet); assert.equal(hit.note.title, raw.title);
    assert.deepEqual(hit.note.tags, raw.tags);
    for (const value of [hit.note.createdAt, hit.note.updatedAt]) assert.match(value, /(?:Z|\+00:00)$/);
    assert.ok(calls.some(call => call.path === '/api/v1/notes/' + first));
    report.searchProjection = { result, raw };
  });
  await check('AND tags exclude nonmatching notes', async () => {
    assert.equal((await reader.search('NEEDLE', { tags: ['selected', 'excluded'], limit: 10 })).hits.length, 0);
  });
  await check('empty FTS results preserve returned-hit total', async () => {
    const result = await reader.search('NO_MATCH_PACKAGE_TERM', { limit: 10 });
    assert.equal(result.total, 0); assert.deepEqual(result.hits, []); assert.equal(result.degraded, false);
  });
  await check('bounded FTS limit and rank order', async () => {
    const offset = report.calls.length;
    const result = await reader.search('NEEDLE', { limit: 1 });
    assert.equal(result.hits.length, 1); assert.equal(result.total, 1);
    const raw = report.calls.slice(offset).find(call => new URL(call.path, base).pathname === '/api/v1/search').body.results;
    assert.deepEqual(result.hits.map(hit => hit.note.id), raw.map(hit => hit.note_id));
  });
  await check('semantic report explicitly retains unavailable-inference FTS fallback', async () => {
    const result = await reader.semanticWithReport('NEEDLE', 10);
    assert.equal(result.requestedMode, 'semantic'); assert.equal(result.effectiveMode, 'fts');
    assert.equal(result.degraded, true); assert.ok(result.degradation);
    assert.deepEqual(result.hits.map(hit => hit.note.id).sort(), ids.toSorted());
    report.semanticDegradation = result.degradation;
  });
  await check('array-only semantic rejects fallback', async () => {
    await assert.rejects(reader.semantic('NEEDLE', 10), error => error instanceof core.RemoteBackendError && error.kind === 'degraded-search');
  });
  await check('hybrid explicitly reports FTS degradation', async () => {
    const result = await reader.search('NEEDLE', { mode: 'hybrid', limit: 10 });
    assert.equal(result.requestedMode, 'hybrid'); assert.equal(result.effectiveMode, 'fts');
    assert.equal(result.degraded, true); assert.ok(result.degradation);
  });
  await check('invalid and unsupported intents reject before HTTP', async () => {
    const before = report.calls.length;
    for (const options of [{offset: 1}, {source: ['unsupported']}]) {
      await assert.rejects(reader.search('NEEDLE', options), error => error.kind === 'unsupported-operation');
    }
    for (const options of [{limit: 0}, {limit: 101}, {mode: 'bogus'}]) {
      await assert.rejects(reader.search('NEEDLE', options), error => error.kind === 'invalid-request');
    }
    for (const input of [
      {action: 'update', note_id: first, title: 'unsupported'},
      {action: 'merge', note_id: first}, {action: 'create', content: 'x', unknown: true},
    ]) await assert.rejects(writer.manageNote(input), error => ['unsupported-operation', 'invalid-request'].includes(error.kind));
    assert.equal(report.calls.length, before);
  });
  await check('actual producer rejects legacy missing-q request', async () => {
    const response = await request('/api/v1/search?query=NEEDLE', {headers: {Authorization: 'Bearer ' + keys.reader}});
    assert.equal(response.status, 400);
  });
  for (const [name, key] of [['missing', undefined], ['invalid', 'mm_key_invalid_fixture']]) {
    await check(name + ' identity cannot search or dispatch advertised mutations', async () => {
      const backend = remote(key);
      const denied = error => error instanceof core.RemoteBackendError && error.kind === 'http' && error.status === 401;
      await assert.rejects(backend.search('NEEDLE'), denied);
      await assert.rejects(backend.semanticWithReport('NEEDLE', 10), denied);
      for (const input of [
        {action: 'create', content: 'MUST NOT BE STORED'},
        {action: 'update', note_id: first, content: 'MUST NOT BE STORED'},
        ...['star','unstar','archive','unarchive','delete','restore'].map(action => ({action, note_id: first})),
      ]) await assert.rejects(backend.manageNote(input), denied);
      assert.equal(Number(sql('SELECT count(*) FROM note;')), 2);
      assert.equal((await reader.getNoteFull(first)).content, 'PACKAGE REMOTE NEEDLE');
    });
  }
  for (const [action, field, expected] of [
    ['star', 'starred', true], ['unstar', 'starred', false],
    ['archive', 'archived', true], ['unarchive', 'archived', false],
  ]) await check('authenticated ' + action + ' persists expected state', async () => {
    const result = await writer.manageNote({action, note_id: first});
    assert.equal(result.note[field], expected);
    assert.equal((await reader.getNote(first))[field], expected);
  });
  await check('authenticated content update persists through composed read', async () => {
    const result = await writer.manageNote({action: 'update', note_id: first, content: 'PACKAGE UPDATED'});
    assert.equal(result.note.content, 'PACKAGE UPDATED');
    assert.equal((await reader.getNoteFull(first)).content, 'PACKAGE UPDATED');
  });
  await check('authenticated tag update persists and changes search membership', async () => {
    const result = await writer.manageNote({action: 'update', note_id: first, tags: [tag, 'updated']});
    assert.deepEqual(result.note.tags.toSorted(), [tag, 'updated'].toSorted());
    assert.deepEqual((await reader.getNote(first)).tags.toSorted(), [tag, 'updated'].toSorted());
    assert.deepEqual((await reader.search('UPDATED', {tags: ['updated'], limit: 10})).hits.map(hit => hit.note.id), [first]);
    assert.equal((await reader.search('UPDATED', {tags: ['selected'], limit: 10})).total, 0);
  });
  await check('authenticated delete yields authoritative absence', async () => {
    await writer.manageNote({action: 'delete', note_id: first});
    assert.equal(await reader.getNote(first), null);
    assert.equal((await reader.listNotes()).total, 1);
  });
  await check('authenticated restore preserves content and identity', async () => {
    const result = await writer.manageNote({action: 'restore', note_id: first});
    assert.equal(result.note_id, first);
    assert.equal((await reader.getNoteFull(first)).content, 'PACKAGE UPDATED');
    assert.equal((await reader.listNotes()).total, 2);
  });
  await check('mutation404 is typed failure not false success', async () => {
    await assert.rejects(writer.manageNote({action: 'update', note_id: randomUUID(), content: 'MISSING'}),
      error => error instanceof core.RemoteBackendError && error.kind === 'http' && error.status === 404);
  });
  await check('synthetic lifecycle cleanup verifies both tombstones and no visible notes', async () => {
    for (const id of ids) await writer.manageNote({action: 'delete', note_id: id});
    assert.equal((await reader.listNotes()).total, 0);
    for (const id of ids) assert.equal(await reader.getNote(id), null);
    report.syntheticCleanup = {visibleNotes: 0, physicalNotes: Number(sql('SELECT count(*) FROM note;')),
      deletedNotes: Number(sql('SELECT count(*) FROM note WHERE deleted_at IS NOT NULL;')), ids};
    assert.equal(report.syntheticCleanup.physicalNotes, 2);
    assert.equal(report.syntheticCleanup.deletedNotes, 2);
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
