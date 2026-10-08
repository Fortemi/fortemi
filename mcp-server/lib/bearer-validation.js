// Validates MCP bearer tokens: Fortemi-issued tokens via introspection, external OIDC tokens via the API's hosted verifier.
//
// External tokens are never parsed or verified here (#1151). They are forwarded to
// GET /api/v1/auth/token-info, which runs the same issuer, audience, JWKS, tenant and
// scope checks as every REST request. Token values are never logged or returned.

const FORTEMI_ACCESS_PREFIXES = ["mm_at_", "mm_key_"];
const FORTEMI_REFRESH_PREFIX = "mm_rt_";
const VERIFY_TIMEOUT_MS = 5000;

/** Classify a raw bearer value without inspecting its contents beyond the prefix. */
export function classifyBearer(token) {
  if (token.startsWith(FORTEMI_REFRESH_PREFIX)) return "refresh";
  if (FORTEMI_ACCESS_PREFIXES.some((prefix) => token.startsWith(prefix))) return "fortemi";
  return "external";
}

function scopeList(scope) {
  return (scope || "").split(/\s+/).filter(Boolean);
}

function reject(kind, status, reason) {
  return { valid: false, kind, status, reason };
}

async function validateFortemiToken(token, options) {
  const { apiBase, clientId, clientSecret, fetchImpl } = options;
  const credentials = Buffer.from(`${clientId}:${clientSecret}`).toString("base64");
  const response = await fetchImpl(`${apiBase}/oauth/introspect`, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Authorization: `Basic ${credentials}`,
    },
    body: `token=${encodeURIComponent(token)}`,
    signal: AbortSignal.timeout(VERIFY_TIMEOUT_MS),
  });
  if (!response.ok) return reject("fortemi", 401, "introspection_failed");
  const introspection = await response.json();
  if (!introspection.active) return reject("fortemi", 401, "inactive");
  // A refresh token is never a bearer credential, even if introspection reports it active.
  if (introspection.token_type && introspection.token_type !== "Bearer") {
    return reject("fortemi", 401, "not_an_access_token");
  }
  // Transport admission only; the API's route/action policy still enforces mutations.
  const scopes = scopeList(introspection.scope);
  if (!["mcp", "read", "admin"].some((scope) => scopes.includes(scope))) {
    return reject("fortemi", 403, "insufficient_scope");
  }
  return { valid: true, kind: "fortemi", token };
}

async function validateExternalToken(token, options) {
  const { apiBase, fetchImpl } = options;
  const response = await fetchImpl(`${apiBase}/api/v1/auth/token-info`, {
    method: "GET",
    headers: { Authorization: `Bearer ${token}`, Accept: "application/json" },
    signal: AbortSignal.timeout(VERIFY_TIMEOUT_MS),
  });
  if (response.status === 401) return reject("external", 401, "invalid_token");
  if (response.status === 403) return reject("external", 403, "forbidden");
  if (response.status === 503) return reject("external", 503, "verifier_unavailable");
  if (!response.ok) return reject("external", 401, "verification_failed");
  const info = await response.json();
  if (!info.active || info.token_class !== "hosted_oidc") {
    return reject("external", 401, "invalid_token");
  }
  const scopes = scopeList(info.scope);
  if (!scopes.includes("mcp") && !scopes.includes("admin")) {
    return reject("external", 403, "insufficient_scope");
  }
  return { valid: true, kind: "external", token, exp: info.exp ?? null };
}

/**
 * Validate an Authorization header value.
 * Resolves to { valid: true, kind, token } or { valid: false, kind, status, reason }.
 */
export async function validateBearer(authHeader, options) {
  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    return reject("none", 401, "missing");
  }
  const token = authHeader.slice(7).trim();
  if (!token) return reject("none", 401, "missing");
  const kind = classifyBearer(token);
  if (kind === "refresh") return reject("refresh", 401, "not_an_access_token");
  try {
    return kind === "fortemi"
      ? await validateFortemiToken(token, options)
      : await validateExternalToken(token, options);
  } catch (error) {
    // Network errors and timeouts: the verifier could not be reached.
    return reject(kind, 503, error?.name === "TimeoutError" ? "verifier_timeout" : "verifier_unreachable");
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
