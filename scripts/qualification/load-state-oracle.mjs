import { jsonDigest } from './canonical-json.mjs';

// Pure comparison only. Collection, role independence, scope discovery and durable
// timestamps must be established by adapters; a caller-supplied inventory proves none.
const token = value => typeof value === 'string' && value.length > 0 && value.length <= 512;
const time = value => Number.isSafeInteger(value) && value >= 0;
const key = (namespace, surface) => JSON.stringify([namespace, surface]);
const status = (failures, missing) => failures.length ? 'FAIL' : missing.length ? 'MISSING' : 'PASS';

function names(values, label) {
  if (!Array.isArray(values) || !values.length || values.length > 1000
    || values.some(value => !token(value)) || new Set(values).size !== values.length) {
    throw new Error(`invalid or duplicate ${label}`);
  }
  return [...values].sort();
}

function scope(plan) {
  const surfaces = names(plan?.requiredSurfaces, 'required surfaces');
  const namespaces = names(plan?.namespaces, 'namespaces');
  const controls = names(plan?.controlNamespaces, 'control namespaces');
  if (controls.some(value => !namespaces.includes(value)) || namespaces.every(value => controls.includes(value))
    || surfaces.length * namespaces.length > 10000) throw new Error('invalid bounded namespace/control scope');
  return { surfaces, namespaces, controls };
}

function inventory(rows, declared) {
  if (!Array.isArray(rows) || rows.length > 10000) throw new Error('invalid inventory');
  const result = new Map();
  let count = 0;
  for (const row of rows) {
    if (!row || !declared.namespaces.includes(row.namespace) || !declared.surfaces.includes(row.surface)) {
      throw new Error('unbound namespace or surface');
    }
    const binding = key(row.namespace, row.surface);
    if (result.has(binding)) throw new Error('duplicate namespace/surface inventory');
    if (!Array.isArray(row.resources) || (count += row.resources.length) > 100000) throw new Error('invalid bounded resources');
    const resources = new Map();
    for (const resource of row.resources) {
      if (!resource || !token(resource.id) || resources.has(resource.id)) throw new Error('invalid or duplicate resource ID');
      const hasContent = Object.hasOwn(resource, 'content');
      const hasDigest = Object.hasOwn(resource, 'digest');
      if (!hasContent && !hasDigest) throw new Error('resource requires content or digest invariant');
      if (hasDigest && (typeof resource.digest !== 'string' || !/^sha256:[0-9a-f]{64}$/.test(resource.digest))) {
        throw new Error('invalid resource digest');
      }
      const digest = hasContent ? jsonDigest(resource.content) : resource.digest;
      if (hasDigest && digest !== resource.digest) throw new Error('resource content/digest mismatch');
      resources.set(resource.id, digest);
    }
    result.set(binding, resources);
  }
  return result;
}

function requireComplete(rows, declared) {
  const parsed = inventory(rows, declared);
  for (const namespace of declared.namespaces) for (const surface of declared.surfaces) {
    if (!parsed.has(key(namespace, surface))) throw new Error('expected inventory omits required namespace/surface');
  }
  return parsed;
}

function compare(expected, observed, declared) {
  const failures = [], missing = [];
  let actual;
  if (observed === undefined || observed === null) missing.push({ reason: 'inventory absent' });
  else {
    try { actual = inventory(observed, declared); }
    catch (error) { failures.push({ reason: error.message }); }
  }
  if (actual) for (const namespace of declared.namespaces) for (const surface of declared.surfaces) {
    const binding = key(namespace, surface), wanted = expected.get(binding), found = actual.get(binding);
    if (!found) { missing.push({ namespace, surface, reason: 'required surface absent' }); continue; }
    for (const id of [...new Set([...wanted.keys(), ...found.keys()])].sort()) {
      const reason = !found.has(id) ? 'expected resource absent' : !wanted.has(id) ? 'unexpected resource'
        : wanted.get(id) !== found.get(id) ? 'resource invariant mismatch' : null;
      if (reason) failures.push({ namespace, surface, id, reason });
    }
  }
  return { status: status(failures, missing), failures, missing };
}

/** Compare explicit namespace × surface inventories; invalid expectations throw.
 * Missing collection is MISSING; a complete collection omitting a known ID is FAIL.
 * Resources carry {id, content} or {id, digest}, or both with matching SHA-256.
 * Content uses canonicalJson (RFC 8785 ordering/serialization) through jsonDigest;
 * row/resource order is irrelevant. Each ID is scoped to namespace and surface.
 * Bounds: 1,000 names per dimension, 10,000 namespace/surface pairs and rows,
 * 100,000 resources per inventory, 512 characters per scope name/resource ID.
 * Adapters must also bound serialized input bytes before parsing/collecting.
 */
export function compareLoadStateInventory(plan, expected, observed) {
  const declared = scope(plan);
  return { ...compare(requireComplete(expected, declared), observed, declared), admitted: false, executionAuthorized: false };
}

/**
 * plan: {requiredSurfaces, namespaces, controlNamespaces, expectedBaseline,
 * expectedAfter, minimumSettleMs}. Each inventory row is
 * {namespace, surface, resources:[{id, content?, digest?}]} including empty surfaces.
 * after/cleanup/settled: {observedAtMs, inventory} on one monotonic clock.
 * Both cleanup observations must match baseline; settled must be sufficiently later.
 * Validate the pre-run observation separately with compareLoadStateInventory using
 * expectedBaseline, before issuing workload operations.
 * PASS means only these supplied inventories match, never runtime admission.
 */
export function evaluateLoadState({ plan, after, cleanup, settled }) {
  const declared = scope(plan);
  if (!time(plan.minimumSettleMs) || plan.minimumSettleMs === 0) throw new Error('invalid minimum settling interval');
  const baseline = requireComplete(plan.expectedBaseline, declared);
  const expectedAfter = requireComplete(plan.expectedAfter, declared);
  for (const namespace of declared.controls) for (const surface of declared.surfaces) {
    const binding = key(namespace, surface);
    if (jsonDigest([...baseline.get(binding)].sort()) !== jsonDigest([...expectedAfter.get(binding)].sort())) {
      throw new Error('control namespace expectations must remain unchanged');
    }
  }
  const snapshots = { after, cleanup, settled }, checks = {};
  for (const name of ['after', 'cleanup', 'settled']) {
    const snapshot = snapshots[name];
    const check = compare(name === 'after' ? expectedAfter : baseline, snapshot?.inventory, declared);
    if (snapshot != null && (!time(snapshot.observedAtMs) || typeof snapshot !== 'object' || Array.isArray(snapshot))) {
      check.failures.push({ reason: 'invalid observation timestamp or snapshot' });
    }
    checks[name] = check;
  }
  if (time(after?.observedAtMs) && time(cleanup?.observedAtMs) && cleanup.observedAtMs < after.observedAtMs) {
    checks.cleanup.failures.push({ reason: 'cleanup observation precedes after observation' });
  }
  if (time(cleanup?.observedAtMs) && time(settled?.observedAtMs)
    && settled.observedAtMs - cleanup.observedAtMs < plan.minimumSettleMs) {
    checks.settled.failures.push({ reason: 'settling interval not satisfied' });
  }
  for (const check of Object.values(checks)) check.status = status(check.failures, check.missing);
  const failures = Object.values(checks).flatMap(check => check.failures);
  const missing = Object.values(checks).flatMap(check => check.missing);
  return { status: status(failures, missing), checks, admitted: false, executionAuthorized: false };
}
