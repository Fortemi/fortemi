// Validates MCP bearer tokens through the API's token-info verifier.
//
// GET /api/v1/auth/token-info runs the same issuer, audience, JWKS, tenant and
// scope checks as every REST request. Token values are never logged or returned.

import crypto from "node:crypto";

const FORTEMI_ACCESS_PREFIXES = ["mm_at_", "mm_key_", "mm_pat_"];
const FORTEMI_REFRESH_PREFIX = "mm_rt_";
const VERIFY_TIMEOUT_MS = 5000;
const SUCCESS_CACHE_MAX_MS = 60_000;
const FAILURE_CACHE_MAX_MS = 5_000;
const TOKEN_EXCHANGE_GRANT_TYPE = "urn:ietf:params:oauth:grant-type:token-exchange";
const TOKEN_EXCHANGE_SUBJECT_TOKEN_TYPE = "urn:ietf:params:oauth:token-type:access_token";

/** Classify a raw bearer value without inspecting its contents beyond the prefix. */
export function classifyBearer(token) {
  if (token.startsWith(FORTEMI_REFRESH_PREFIX)) return "refresh";
  if (FORTEMI_ACCESS_PREFIXES.some((prefix) => token.startsWith(prefix))) return "fortemi";
  return "external";
}

function scopeList(scope) {
  return (scope || "").split(/\s+/).filter(Boolean);
}

function tokenHash(token) {
  return crypto.createHash("sha256").update(token).digest("hex");
}

function decodeJwtPayload(token) {
  const parts = token.split(".");
  if (parts.length < 2) return null;
  try {
    return JSON.parse(Buffer.from(parts[1], "base64url").toString("utf8"));
  } catch {
    return null;
  }
}

function principalFingerprint(token, info) {
  const tokenClass = String(info.token_class || "");
  if (tokenClass === "pat" || tokenClass === "personal_access_token") {
    const patId = info.pat_id || info.patId || info.id;
    if (patId) return crypto.createHash("sha256").update(`pat\n${patId}`).digest("hex");
  }
  const iss = info.iss || info.issuer;
  const sub = info.sub || info.subject;
  if (iss && sub) return crypto.createHash("sha256").update(`${iss}\n${sub}`).digest("hex");

  // The current token-info contract intentionally redacts legacy OAuth/API-key ids.
  // Bind those opaque credentials to the presented token value rather than leaving
  // the session unbound; user-bound OIDC/PAT sessions use the stable ids above.
  if (FORTEMI_ACCESS_PREFIXES.some((prefix) => token.startsWith(prefix))) {
    return crypto.createHash("sha256").update(`token\n${token}`).digest("hex");
  }

  const payload = decodeJwtPayload(token);
  if (payload?.iss && payload?.sub) {
    return crypto.createHash("sha256").update(`${payload.iss}\n${payload.sub}`).digest("hex");
  }
  return null;
}

function reject(kind, status, reason, cacheable = true) {
  return { valid: false, kind, status, reason, cacheable };
}

function successCacheTtlMs(info, nowMs) {
  if (info?.exp === undefined || info?.exp === null) return SUCCESS_CACHE_MAX_MS;
  const expMs = Number(info.exp) * 1000;
  if (!Number.isFinite(expMs)) return SUCCESS_CACHE_MAX_MS;
  return Math.max(0, Math.min(expMs - nowMs, SUCCESS_CACHE_MAX_MS));
}

export function createTokenInfoCache({ now = () => Date.now() } = {}) {
  const entries = new Map();
  return {
    get(token) {
      const key = tokenHash(token);
      const entry = entries.get(key);
      if (!entry) return null;
      if (entry.expiresAtMs <= now()) {
        entries.delete(key);
        return null;
      }
      return entry.value;
    },
    set(token, value, ttlMs) {
      if (ttlMs <= 0) return;
      entries.set(tokenHash(token), { value, expiresAtMs: now() + ttlMs });
    },
    size() {
      return entries.size;
    },
    clear() {
      entries.clear();
    },
  };
}

const defaultTokenInfoCache = createTokenInfoCache();

async function fetchTokenInfo(token, options) {
  const { apiBase, fetchImpl } = options;
  const response = await fetchImpl(`${apiBase}/api/v1/auth/token-info`, {
    method: "GET",
    headers: { Authorization: `Bearer ${token}`, Accept: "application/json" },
    signal: AbortSignal.timeout(VERIFY_TIMEOUT_MS),
  });
  if (response.status === 401) return reject(classifyBearer(token), 401, "invalid_token");
  if (response.status === 403) return reject(classifyBearer(token), 403, "forbidden");
  if (response.status === 503) return reject(classifyBearer(token), 503, "verifier_unavailable", false);
  if (!response.ok) return reject(classifyBearer(token), 401, "verification_failed");
  const info = await response.json();
  if (!info.active) {
    return reject(classifyBearer(token), 401, "invalid_token");
  }
  const scopes = scopeList(info.scope);
  if (!scopes.includes("mcp") && !scopes.includes("admin")) {
    return reject(classifyBearer(token), 403, "insufficient_scope");
  }
  const fingerprint = principalFingerprint(token, info);
  if (!fingerprint) return reject(classifyBearer(token), 401, "principal_unbound");
  return {
    valid: true,
    kind: classifyBearer(token),
    token,
    forwardToken: token,
    tokenInfo: info,
    exp: info.exp ?? null,
    principalFingerprint: fingerprint,
  };
}

async function exchangeToken(subjectToken, options) {
  const { tokenExchange, fetchImpl } = options;
  if (!tokenExchange?.enabled) return subjectToken;
  const body = new URLSearchParams({
    grant_type: TOKEN_EXCHANGE_GRANT_TYPE,
    subject_token: subjectToken,
    subject_token_type: TOKEN_EXCHANGE_SUBJECT_TOKEN_TYPE,
    audience: tokenExchange.audience,
  });
  const response = await fetchImpl(tokenExchange.tokenEndpoint, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Authorization: `Basic ${Buffer.from(`${tokenExchange.clientId}:${tokenExchange.clientSecret}`).toString("base64")}`,
    },
    body: body.toString(),
    signal: AbortSignal.timeout(VERIFY_TIMEOUT_MS),
  });
  if (!response.ok) {
    const error = new Error("token_exchange_failed");
    error.status = response.status;
    throw error;
  }
  const payload = await response.json();
  if (!payload.access_token) {
    const error = new Error("token_exchange_missing_access_token");
    error.status = 502;
    throw error;
  }
  return payload.access_token;
}

/**
 * Validate an Authorization header value.
 * Resolves to { valid: true, kind, token, forwardToken, principalFingerprint }
 * or { valid: false, kind, status, reason }.
 */
export async function validateBearer(authHeader, options) {
  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    return reject("none", 401, "missing");
  }
  const token = authHeader.slice(7).trim();
  if (!token) return reject("none", 401, "missing");
  const kind = classifyBearer(token);
  if (kind === "refresh") return reject("refresh", 401, "not_an_access_token");
  const cache = options.cache || defaultTokenInfoCache;
  const nowMs = options.now ? options.now() : Date.now();
  const cached = cache.get(token);
  if (cached) return cached;
  try {
    const result = await fetchTokenInfo(token, options);
    if (result.valid) {
      const forwardToken = await exchangeToken(token, options);
      const withForwardToken = { ...result, forwardToken };
      cache.set(token, withForwardToken, successCacheTtlMs(result.tokenInfo, nowMs));
      return withForwardToken;
    }
    if (result.cacheable !== false) cache.set(token, result, FAILURE_CACHE_MAX_MS);
    return result;
  } catch (error) {
    // Network errors and timeouts: the verifier could not be reached.
    const result = reject(
      kind,
      error?.status && error.status >= 400 && error.status < 500 ? 401 : 503,
      error?.name === "TimeoutError"
        ? "verifier_timeout"
        : error?.message?.startsWith("token_exchange")
          ? error.message
          : "verifier_unreachable",
      false,
    );
    return result;
  }
}

/**
 * Whether a failed validation must stop the request.
 * External or refresh credentials that fail are always rejected; a presented but invalid
 * credential never downgrades to anonymous access for them. Fortemi tokens keep the
 * self-hosted REQUIRE_AUTH behavior.
 */
export function mustReject(result, requireAuth) {
  if (result.valid) return false;
  if (requireAuth) return true;
  return result.kind === "external" || result.kind === "refresh";
}
