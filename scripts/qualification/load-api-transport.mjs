import { createHash, randomUUID } from 'node:crypto';
export class LoadTransportError extends Error {
  constructor(code, status = null) { super(code); this.name = 'LoadTransportError'; this.code = code; this.status = status; }
}
const bounded = n => Number.isSafeInteger(n) && n > 0 && n <= 64 * 1024 * 1024;

/** Bounded JSON/archive API transport matching the dataset controller's apiRequest signature. */
export function createLoadApiTransport({ origin, token, memory, allowedRequests, maxRequestBytes,
  maxResponseBytes, timeoutMs, observe, attemptBudget }) {
  const base = new URL(origin);
  if (!['http:', 'https:'].includes(base.protocol) || base.username || base.password || base.pathname !== '/'
    || base.search || base.hash || typeof token !== 'string' || !token.length || /[\r\n]/.test(token)
    || typeof memory !== 'string' || !memory.length || /[\r\n]/.test(memory)
    || !bounded(maxRequestBytes) || !bounded(maxResponseBytes)
    || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 300000
    || !Array.isArray(allowedRequests) || !allowedRequests.length || allowedRequests.length > 1000
    || typeof observe !== 'function' || (attemptBudget !== undefined && typeof attemptBudget?.reserve !== 'function')) throw new Error('explicit bounded API scope required');
  const allowed = new Map();
  function target(method, requestPath) {
    if (!['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].includes(method) || typeof requestPath !== 'string'
      || !requestPath.startsWith('/api/v1/') || requestPath.includes('\\')) throw new LoadTransportError('REQUEST_SCOPE_REJECTED');
    const url = new URL(requestPath, base);
    if (url.origin !== base.origin || url.hash || `${url.pathname}${url.search}` !== requestPath) throw new LoadTransportError('REQUEST_SCOPE_REJECTED');
    return url;
  }
  for (const r of allowedRequests) {
    target(r?.method, r?.path); const key = `${r.method} ${r.path}`;
    if (allowed.has(key)) throw new Error('duplicate allowed request');
    const responseKind = r.responseKind ?? 'json';
    if (!['json', 'gzip-archive'].includes(responseKind)) throw new Error('unsupported response kind');
    allowed.set(key, responseKind);
  }
  return async function apiRequest(method, requestPath, body = null, options = {}) {
    const url = target(method, requestPath);
    if (!allowed.has(`${method} ${requestPath}`)) throw new LoadTransportError('REQUEST_SCOPE_REJECTED');
    const responseKind = allowed.get(`${method} ${requestPath}`);
    let bytes;
    try { bytes = body === null ? undefined : Buffer.from(JSON.stringify(body), 'utf8'); }
    catch { throw new LoadTransportError('REQUEST_ENCODING_REJECTED'); }
    if ((bytes?.length || 0) > maxRequestBytes) throw new LoadTransportError('REQUEST_BYTES_EXCEEDED');
    if (method === 'GET' && bytes) throw new LoadTransportError('GET_BODY_REJECTED');
    const ids = { logicalOperationId: options.logicalOperationId ?? randomUUID(),
      requestId: options.requestId ?? randomUUID(), attemptId: options.attemptId ?? randomUUID() };
    if (!Object.values(ids).every(id => typeof id === 'string' && /^[A-Za-z0-9_.:-]{1,200}$/.test(id))) {
      throw new LoadTransportError('INVALID_ATTEMPT_ID');
    }
    if (options.signal?.aborted) throw new LoadTransportError('REQUEST_ABORTED');
    const reservation = attemptBudget?.reserve({ ...ids, reserveCostUsd: options.reserveCostUsd });
    const controller = new AbortController();
    const relay = () => controller.abort();
    if (options.signal?.aborted) relay();
    options.signal?.addEventListener('abort', relay, { once: true });
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    const startNs = process.hrtime.bigint().toString();
    let response, reader, observerAbort, result, failure, raw, received = 0;
    try {
      response = await fetch(url, { method, body: bytes, redirect: 'manual', signal: controller.signal,
        headers: { 'Content-Type': 'application/json', 'Accept-Encoding': 'identity', Authorization: `Bearer ${token}`, 'X-Fortemi-Memory': memory } });
      if (response.status >= 300 && response.status < 400) throw new LoadTransportError('REDIRECT_REJECTED', response.status);
      const contentLength = response.headers.get('content-length');
      if (contentLength !== null && (!/^\d+$/.test(contentLength) || BigInt(contentLength) > BigInt(maxResponseBytes))) {
        throw new LoadTransportError('RESPONSE_BYTES_EXCEEDED', response.status);
      }
      const mediaType = (response.headers.get('content-type') || '').split(';')[0].trim().toLowerCase();
      let shardLossReport;
      if (response.ok && responseKind === 'gzip-archive') {
        if (mediaType !== 'application/gzip') throw new LoadTransportError('ARCHIVE_MEDIA_TYPE_REQUIRED', response.status);
        const encoding = response.headers.get('content-encoding');
        if (encoding && encoding.toLowerCase() !== 'identity') throw new LoadTransportError('ARCHIVE_CONTENT_ENCODING_REJECTED', response.status);
        shardLossReport = response.headers.get('x-fortemi-shard-loss-report');
        if (shardLossReport !== null && Buffer.byteLength(shardLossReport) > 4096) throw new LoadTransportError('ARCHIVE_HEADER_LIMIT_EXCEEDED', response.status);
      }
      reader = response.body?.getReader(); const chunks = [];
      if (reader) while (true) {
        const { value, done } = await reader.read(); if (done) break;
        received += value.length;
        if (received > maxResponseBytes) throw new LoadTransportError('RESPONSE_BYTES_EXCEEDED', response.status);
        chunks.push(value);
      }
      raw = Buffer.concat(chunks);
      if (!response.ok) throw new LoadTransportError('HTTP_REJECTED', response.status);
      if (responseKind === 'gzip-archive') result = { bytes: raw, contentType: mediaType, shardLossReport };
      else {
        if (!/^application\/json(?:\s*;|$)/i.test(response.headers.get('content-type') || '')) throw new LoadTransportError('JSON_RESPONSE_REQUIRED', response.status);
        try { result = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(raw)); }
        catch { throw new LoadTransportError('INVALID_JSON_RESPONSE', response.status); }
      }
    } catch (error) {
      failure = error instanceof LoadTransportError ? error
        : new LoadTransportError(controller.signal.aborted ? 'REQUEST_ABORTED' : 'TRANSPORT_OR_OBSERVER_FAILED', response?.status);
    }
    try {
      // One observation per dispatched fetch, including network/abort/parse failures.
      // No paths, credentials, response content, or raw exception messages are persisted.
      const metadata = { method, startNs, endNs: process.hrtime.bigint().toString(), status: response?.status ?? null,
        requestBytes: bytes?.length || 0, responseBytes: received,
        responseDigest: raw ? `sha256:${createHash('sha256').update(raw).digest('hex')}` : null,
        ...ids, outcome: failure ? 'failed' : 'succeeded', errorCode: failure?.code ?? null,
        ...(reservation ? { reservedCostUsd: reservation.reservedCostUsd } : {}) };
      await Promise.race([Promise.resolve().then(() => observe(metadata)), new Promise((_, reject) => {
        observerAbort = () => reject(new Error('observer-aborted'));
        if (controller.signal.aborted) observerAbort();
        else controller.signal.addEventListener('abort', observerAbort, { once: true });
      })]);
      if (failure) throw failure;
      return result;
    } catch (error) {
      if (error instanceof LoadTransportError) throw error;
      throw new LoadTransportError(controller.signal.aborted ? 'REQUEST_ABORTED' : 'TRANSPORT_OR_OBSERVER_FAILED', response?.status);
    } finally {
      controller.abort(); clearTimeout(timer); options.signal?.removeEventListener('abort', relay);
      if (observerAbort) controller.signal.removeEventListener('abort', observerAbort);
      if (reader) await reader.cancel().catch(() => {});
      else if (response?.body) await response.body.cancel().catch(() => {});
    }
  };
}
