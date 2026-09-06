import test from 'node:test';
import assert from 'node:assert/strict';
import { createLoadAttemptBudget, compareLoadUsd } from './load-attempt-budget.mjs';
const config = { maxLogicalOperations: 2, maxRequests: 3, maxAttempts: 4 };
const ids = (n, request = n, logical = 'operation') => ({ logicalOperationId: logical, requestId: `request-${request}`, attemptId: `attempt-${n}` });
test('counts logical operations, requests, and retries independently without refunds', () => {
  const b = createLoadAttemptBudget(config);
  b.reserve(ids(1)); b.reserve(ids(2, 1)); b.reserve(ids(3, 2)); b.reserve(ids(4, 3));
  assert.deepEqual([b.snapshot().logicalOperations, b.snapshot().requests, b.snapshot().attempts], [1, 3, 4]);
  assert.throws(() => b.reserve(ids(5, 3)), { code: 'ATTEMPT_BUDGET_EXCEEDED' });
});
test('concurrent reservations cannot oversubscribe exact decimal cost cap', async () => {
  const b = createLoadAttemptBudget({ ...config, maxCostUsd: '0.3' });
  const results = await Promise.allSettled([1, 2, 3, 4].map(n => Promise.resolve().then(() => b.reserve({ ...ids(n, 1), reserveCostUsd: '0.1' }))));
  assert.equal(results.filter(r => r.status === 'fulfilled').length, 3);
  assert.equal(results[3].reason.code, 'COST_BUDGET_EXCEEDED');
  assert.equal(b.snapshot().chargedCostUsd, '0.3');
  b.reconcile('attempt-1', '0.000000000000000001');
  assert.equal(b.snapshot().chargedCostUsd, '0.200000000000000001');
  b.reserve({ ...ids(4, 1), reserveCostUsd: '0.099999999999999999' });
  assert.equal(b.snapshot().chargedCostUsd, '0.3');
});
test('unresolved reservations persist, underestimation records bill and blocks new attempts', () => {
  const b = createLoadAttemptBudget({ ...config, maxCostUsd: '1' });
  b.reserve({ ...ids(1), reserveCostUsd: '0.2' });
  assert.equal(b.snapshot().unresolvedAttempts, 1);
  assert.throws(() => b.reconcile('attempt-1', '0.3'), { code: 'COST_RESERVATION_EXCEEDED' });
  assert.equal(b.snapshot().spentCostUsd, '0.3');
  assert.throws(() => b.reserve({ ...ids(2), reserveCostUsd: '0' }), { code: 'COST_BUDGET_BREACHED' });
  assert.throws(() => b.reconcile('attempt-1', '0'), { code: 'ATTEMPT_ALREADY_RECONCILED' });
});
test('invalid policy, omitted prices and ID reuse reject without partial admission', () => {
  for (const maxCostUsd of [0.1, '-1', '1e-3', '0.0000000000000000001']) {
    assert.throws(() => createLoadAttemptBudget({ ...config, maxCostUsd }), { code: 'INVALID_USD_AMOUNT' });
  }
  const b = createLoadAttemptBudget({ ...config, maxCostUsd: '1' });
  assert.throws(() => b.reserve(ids(1)), { code: 'COST_RESERVATION_REQUIRED' });
  assert.equal(b.snapshot().attempts, 0);
  b.reserve({ ...ids(1), reserveCostUsd: '0' });
  assert.throws(() => b.reserve({ ...ids(1), reserveCostUsd: '0' }), { code: 'DUPLICATE_ATTEMPT_ID' });
  assert.throws(() => b.reserve({ ...ids(2, 1, 'other'), reserveCostUsd: '0' }), { code: 'REQUEST_ID_OWNER_MISMATCH' });
});
test('logical and request caps reject independently and can be reconfigured', () => {
  const b = createLoadAttemptBudget({ ...config, maxLogicalOperations: 1, maxRequests: 1 });
  b.reserve(ids(1));
  assert.throws(() => b.reserve(ids(2, 1, 'other')), { code: 'REQUEST_ID_OWNER_MISMATCH' });
  assert.throws(() => b.reserve(ids(2, 2, 'other')), { code: 'LOGICAL_OPERATION_BUDGET_EXCEEDED' });
  assert.throws(() => b.reserve(ids(2)), { code: 'REQUEST_BUDGET_EXCEEDED' });
  assert.equal(b.snapshot().attempts, 1);
});

test('USD comparison preserves exact large values, fractional boundaries and equivalent representations', () => {
  assert.equal(compareLoadUsd('0.30', '0.3'), 0);
  assert.equal(compareLoadUsd('0.299999999999999999', '0.3'), -1);
  assert.equal(compareLoadUsd('0.300000000000000001', '0.3'), 1);
  assert.equal(compareLoadUsd('999999999999999999.000000000000000001', '999999999999999999'), 1);
  assert.equal(compareLoadUsd('0', '0.000000000000000000'), 0);
  for (const invalid of [undefined, null, 0, 'NaN', 'Infinity', '1e-3', '-1', '01', '0.0000000000000000001']) {
    assert.throws(() => compareLoadUsd(invalid, '1'), { code: 'INVALID_USD_AMOUNT' });
    assert.throws(() => compareLoadUsd('1', invalid), { code: 'INVALID_USD_AMOUNT' });
  }
});
