import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { once } from 'node:events';

export const controlsUrl = new URL('../../contracts/openapi/fixtures/remote-negative-controls.json', import.meta.url);
export const nativeUrl = new URL('../../contracts/openapi/fixtures/native-remote-auth.json', import.meta.url);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const expectedKinds = { body: 'invalid-response', reset: 'transport', abort: 'aborted', truncate: 'invalid-response', proxy404: 'http', mismatched404: 'http', problem: 'http' };

export function validateControls(controls, nativeBytes) {
  assert.equal(controls.schemaVersion, 'fortemi.remote-negative-controls.v1');
  assert.equal(controls.classification, 'producer-owned-controlled-fault-injection');
  assert.equal(controls.basis.fixture, 'native-remote-auth.json');
  assert.equal(controls.basis.sha256, '96a4955315108cd83473d66c8b9a7626e0b97ab9784e761375707c1da3f5efb3');
  assert.equal(hash(nativeBytes), controls.basis.sha256);
  assert.deepEqual(controls.claims, { liveFortemiServer: false, hostedNoteDenial: false, inference: false, suiteParity: false });
  assert.equal(controls.controls.length, 21);
  assert.equal(new Set(controls.controls.map(c => c.id)).size, 21);
  const modes = new Set();
  for (const control of controls.controls) {
    assert.match(control.id, /^[a-z0-9-]{1,64}$/);
    assert.ok(['note', 'links', 'concepts', 'provenance'].includes(control.stage));
    assert.ok(Object.hasOwn(expectedKinds, control.mode));
    assert.equal(control.expectedKind, expectedKinds[control.mode]);
    const keys = ['id', 'stage', 'mode', 'expectedKind'];
    if (control.mode === 'body') {
      keys.push('rawBody');
      assert.equal(typeof control.rawBody, 'string');
      assert.ok(Buffer.byteLength(control.rawBody) <= 16384);
    }
    if (control.mode === 'problem') {
      keys.push('nativeStatus');
      assert.ok([401, 403, 404, 429, 500].includes(control.nativeStatus));
      assert.equal(control.stage, 'links');
    }
    assert.deepEqual(Object.keys(control).sort(), keys.sort());
    modes.add(control.mode);
  }
  assert.deepEqual([...modes].sort(), Object.keys(expectedKinds).sort());
  return controls;
}

export function loadControls() {
  const bytes = readFileSync(controlsUrl), nativeBytes = readFileSync(nativeUrl);
  return { controls: validateControls(JSON.parse(bytes), nativeBytes), native: JSON.parse(nativeBytes), sha256: hash(bytes) };
}

// The server is a fault injector, never a replacement implementation of Fortemi.
export async function exerciseControl(core, control, native, readMethod) {
  const captures = native.calls.filter(c => c.check === 'composed existing note remains accessible');
  const detail = captures.find(c => !/\/(links|concepts|provenance)$/.test(c.path));
  const notePath = detail.path, id = notePath.split('/').at(-1);
  const target = control.stage === 'note' ? notePath : `${notePath}/${control.stage}`;
  const controller = new AbortController();
  const requests = [], responses = [], timers = new Set(), pending = new Set();
  let armed = false, injected = 0, handlerError, deadlineFired = false, intentionalAbort = false;
  const later = fn => { const timer = setTimeout(() => { timers.delete(timer); fn(); }, 50); timers.add(timer); };
  const problem = control.mode === 'problem' ? native.calls.find(c => c.status === control.nativeStatus) : undefined;
  const send = (res, path, rawBody, status, contentType, retryAfter = null) => {
    responses.push({ path, status, responseSha256: hash(rawBody), bytes: Buffer.byteLength(rawBody), injected: armed && path === target });
    res.writeHead(status, { 'Content-Type': contentType, ...(retryAfter ? { 'Retry-After': retryAfter } : {}) });
    res.end(rawBody);
  };
  const server = createServer((req, res) => {
    try {
      assert.equal(req.method, 'GET');
      const capture = captures.find(c => c.path === req.url);
      assert.ok(capture, 'unexpected route');
      requests.push({ method: req.method, path: req.url });
      assert.ok(requests.length <= 8, 'request budget exceeded');
      if (!armed || req.url !== target) return send(res, req.url, capture.rawBody, capture.status, capture.contentType, capture.retryAfter);
      injected++;
      if (control.mode === 'reset') return req.socket.destroy();
      if (control.mode === 'abort') return later(() => { intentionalAbort = true; controller.abort(); });
      if (control.mode === 'truncate') {
        res.writeHead(200, { 'Content-Type': 'application/json', 'Content-Length': '100' });
        res.flushHeaders(); res.write('{');
        return later(() => res.destroy());
      }
      if (control.mode === 'body') return send(res, req.url, control.rawBody, 200, 'application/json');
      if (control.mode === 'proxy404') return send(res, req.url, 'SYNTHETIC-PRIVATE-PROXY', 404, 'text/plain');
      if (control.mode === 'mismatched404') return send(res, req.url, JSON.stringify({ status: 500, type: 'https://fortemi.com/problems/not-found', detail: 'SYNTHETIC-PRIVATE-DETAIL' }), 404, 'application/problem+json');
      assert.ok(problem);
      return send(res, req.url, problem.rawBody, problem.status, problem.contentType, problem.retryAfter);
    } catch (error) { handlerError = error; res.destroy(); }
  });
  server.requestTimeout = 5000;
  server.headersTimeout = 5000;
  const deadline = setTimeout(() => { deadlineFired = true; controller.abort(); }, 5000);
  let port, outcome;
  try {
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    port = server.address().port;
    const base = `http://127.0.0.1:${port}`;
    const remote = core.createRemoteBackend({ baseUrl: base, fetchImpl: (url, init) => {
      assert.equal(new URL(url).origin, base);
      const request = fetch(url, { ...init, signal: controller.signal });
      pending.add(request); request.then(() => pending.delete(request), () => pending.delete(request));
      return request;
    } });
    // Relationship failure must not make a separately readable note appear absent.
    if (control.stage !== 'note') assert.equal((await remote.getNote(id)).id, id);
    armed = true;
    let error;
    try { await remote[readMethod](id); } catch (caught) { error = caught; }
    assert.ok(error instanceof core.RemoteBackendError, `${control.id}/${readMethod} must reject with typed error`);
    assert.equal(error.kind, control.expectedKind, `${control.id}/${readMethod}`);
    assert.equal(injected, 1, 'one injected fault, no automatic retry');
    if (problem) {
      assert.equal(error.status, problem.status);
      assert.equal(error.problemCode, problem.body.type.split('/').at(-1));
      assert.equal(error.requestId, problem.body.request_id);
      assert.equal(error.retryAfterSeconds, problem.status === 429 ? 60 : undefined);
      assert.ok(!JSON.stringify(error).includes(problem.body.detail));
    }
    if (['proxy404', 'mismatched404'].includes(control.mode)) {
      assert.equal(error.status, 404); assert.equal(error.problemCode, undefined);
    }
    for (const output of [String(error), JSON.stringify(error)]) {
      assert.ok(!output.includes('SYNTHETIC-PRIVATE'));
      assert.ok(!output.includes(base));
    }
    assert.equal(handlerError, undefined);
    assert.equal(deadlineFired, false, 'safety deadline is not an intentional abort control');
    assert.equal(intentionalAbort, control.mode === 'abort');
    outcome = { id: control.id, readMethod, expectedKind: control.expectedKind, actualKind: error.kind, status: error.status,
      problemCode: error.problemCode, retryAfterSeconds: error.retryAfterSeconds, requests, responses, injected,
      noteReadableBeforeEnrichmentFault: control.stage !== 'note', transport: 'real-private-loopback-http', classification: 'controlled-fault-injection' };
  } finally {
    clearTimeout(deadline);
    for (const timer of timers) clearTimeout(timer);
    controller.abort();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await Promise.allSettled([...pending]);
    assert.equal(server.listening, false);
    if (outcome) outcome.listenerClosed = true;
  }
  return outcome;
}
