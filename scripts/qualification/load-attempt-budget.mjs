import { randomUUID } from 'node:crypto';

export class LoadAttemptBudgetError extends Error {
  constructor(code) { super(code); this.name = 'LoadAttemptBudgetError'; this.code = code; }
}
const fail = code => { throw new LoadAttemptBudgetError(code); };
const validId = id => typeof id === 'string' && /^[A-Za-z0-9_.:-]{1,200}$/.test(id);
const scale = 10n ** 18n;
function usd(value) {
  if (typeof value !== 'string' || !/^(0|[1-9]\d{0,17})(\.\d{1,18})?$/.test(value)) fail('INVALID_USD_AMOUNT');
  const [whole, fraction = ''] = value.split('.');
  return BigInt(whole) * scale + BigInt(fraction.padEnd(18, '0'));
}
/** Compare explicit USD decimal strings exactly; malformed/missing values fail closed. */
export function compareLoadUsd(actual, limit) {
  const actualUnits = usd(actual), limitUnits = usd(limit);
  return actualUnits < limitUnits ? -1 : actualUnits > limitUnits ? 1 : 0;
}
function format(value) {
  const fractional = (value % scale).toString().padStart(18, '0').replace(/0+$/, '');
  return `${value / scale}${fractional ? `.${fractional}` : ''}`;
}

/** Process-local, synchronous admission. Share one instance across concurrent calls.
 * Logical IDs group requests; request IDs group retry attempts. Counts never refund.
 * USD uses explicit decimal strings, never inferred provider prices. Unreconciled
 * reservations remain charged, including failed/ambiguous attempts. Not a distributed quota.
 */
export function createLoadAttemptBudget({ maxLogicalOperations, maxRequests, maxAttempts, maxCostUsd } = {}) {
  for (const limit of [maxLogicalOperations, maxRequests, maxAttempts]) {
    if (!Number.isSafeInteger(limit) || limit < 1) fail('INVALID_ATTEMPT_LIMIT');
  }
  const cap = maxCostUsd === undefined ? null : usd(maxCostUsd);
  const logical = new Set(), requests = new Map(), attempts = new Map();
  let reserved = 0n, spent = 0n, breached = false;
  return Object.freeze({
    reserve({ logicalOperationId = randomUUID(), requestId = randomUUID(), attemptId = randomUUID(), reserveCostUsd } = {}) {
      if (![logicalOperationId, requestId, attemptId].every(validId)) fail('INVALID_ATTEMPT_ID');
      if (breached) fail('COST_BUDGET_BREACHED');
      if (attempts.has(attemptId)) fail('DUPLICATE_ATTEMPT_ID');
      if (requests.has(requestId) && requests.get(requestId) !== logicalOperationId) fail('REQUEST_ID_OWNER_MISMATCH');
      const amount = reserveCostUsd === undefined ? (cap === null ? 0n : fail('COST_RESERVATION_REQUIRED')) : usd(reserveCostUsd);
      if (!logical.has(logicalOperationId) && logical.size >= maxLogicalOperations) fail('LOGICAL_OPERATION_BUDGET_EXCEEDED');
      if (!requests.has(requestId) && requests.size >= maxRequests) fail('REQUEST_BUDGET_EXCEEDED');
      if (attempts.size >= maxAttempts) fail('ATTEMPT_BUDGET_EXCEEDED');
      if (cap !== null && spent + reserved + amount > cap) fail('COST_BUDGET_EXCEEDED');
      const record = { logicalOperationId, requestId, attemptId, reserved: amount, reconciled: false };
      logical.add(logicalOperationId); requests.set(requestId, logicalOperationId); attempts.set(attemptId, record); reserved += amount;
      return Object.freeze({ logicalOperationId, requestId, attemptId, reservedCostUsd: format(amount) });
    },
    reconcile(attemptId, actualCostUsd) {
      const record = attempts.get(attemptId);
      if (!record) fail('UNKNOWN_ATTEMPT_ID');
      if (record.reconciled) fail('ATTEMPT_ALREADY_RECONCILED');
      const actual = usd(actualCostUsd);
      reserved -= record.reserved; spent += actual; record.reconciled = true;
      // Record the actual bill even when the estimate was wrong, then fail closed.
      if (actual > record.reserved || (cap !== null && spent + reserved > cap)) {
        breached = true; fail('COST_RESERVATION_EXCEEDED');
      }
      return this.snapshot();
    },
    snapshot() {
      return Object.freeze({ logicalOperations: logical.size, requests: requests.size, attempts: attempts.size,
        reservedCostUsd: format(reserved), spentCostUsd: format(spent), chargedCostUsd: format(reserved + spent),
        unresolvedAttempts: [...attempts.values()].filter(r => !r.reconciled).length, breached });
    },
  });
}
