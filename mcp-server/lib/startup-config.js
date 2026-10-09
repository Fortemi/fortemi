// Resolves the MCP server's API base, public URL and auth posture at startup; fails closed (#1171).
//
// There is no external default. The API base is MATRIC_API_URL, then FORTEMI_URL, then the
// co-located API when MCP_API_LAYOUT declares the bundle/sidecar layout. Anything else is a
// startup error, so a forgotten variable never sends requests or bearer tokens off-host.

import fs from "node:fs";

const LOCAL_LAYOUTS = new Set(["bundle", "sidecar"]);
const DEFAULT_LOCAL_API_PORT = 3000;

export class StartupConfigError extends Error {
  constructor(message) {
    super(message);
    this.name = "StartupConfigError";
  }
}

/** Parse a security boolean like the API does: true/false/1/0 only, unset uses the default. */
export function parseStrictBool(name, raw, fallback) {
  if (raw === undefined || raw === "") return fallback;
  const value = String(raw).trim().toLowerCase();
  if (value === "true" || value === "1") return true;
  if (value === "false" || value === "0") return false;
  throw new StartupConfigError(`${name} must be one of true, false, 1 or 0 (got "${raw}")`);
}

function normalizeBaseUrl(name, raw) {
  let url;
  try {
    url = new URL(raw);
  } catch {
    throw new StartupConfigError(`${name} is not a valid URL: "${raw}"`);
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new StartupConfigError(`${name} must use http or https (got "${url.protocol}")`);
  }
  if (url.username || url.password) {
    throw new StartupConfigError(`${name} must not embed credentials`);
  }
  return raw.replace(/\/+$/, "");
}

function normalizeAbsoluteHttpsUrl(name, raw, { allowLocalHttp = false } = {}) {
  let url;
  try {
    url = new URL(raw);
  } catch {
    throw new StartupConfigError(`${name} is not a valid URL: "${raw}"`);
  }
  if (url.username || url.password) {
    throw new StartupConfigError(`${name} must not embed credentials`);
  }
  if (url.protocol === "https:") return url.href.replace(/\/$/, url.pathname === "/" ? "" : "/");
  const isLocalHost = ["localhost", "127.0.0.1", "::1"].includes(url.hostname);
  if (url.protocol === "http:" && allowLocalHttp && isLocalHost) return raw.replace(/\/+$/, "");
  throw new StartupConfigError(`${name} must use https outside explicit local development mode`);
}

function splitAudiences(env) {
  const raw = (env.FORTEMI_AUTH_AUDIENCES || env.FORTEMI_AUTH_AUDIENCE || "").trim();
  if (!raw) return [];
  return raw.split(",").map((value) => value.trim()).filter(Boolean);
}

function readSecret(env, valueName, fileName) {
  const direct = (env[valueName] || "").trim();
  const file = (env[fileName] || "").trim();
  if (direct && file) {
    throw new StartupConfigError(`${valueName} and ${fileName} are mutually exclusive`);
  }
  if (direct) return direct;
  if (!file) return "";
  try {
    return fs.readFileSync(file, "utf8").trim();
  } catch (error) {
    throw new StartupConfigError(`${fileName} could not be read: ${error.message}`);
  }
}

function resolveResourcePolicy(env, transport) {
  const audiences = splitAudiences(env);
  const resourceUri = (env.MCP_RESOURCE_URI || env.MCP_BASE_URL || "").trim();
  if (transport !== "http" || audiences.length === 0) {
    return { resourceUri: resourceUri || null, acceptedAudiences: audiences };
  }
  if (!resourceUri) {
    throw new StartupConfigError("MCP_RESOURCE_URI is required when FORTEMI_AUTH_AUDIENCE(S) is configured");
  }
  const allowLocalHttp = parseStrictBool("FORTEMI_ALLOW_LOCAL_ISSUER", env.FORTEMI_ALLOW_LOCAL_ISSUER, false);
  const normalizedResource = normalizeAbsoluteHttpsUrl("MCP_RESOURCE_URI", resourceUri, { allowLocalHttp });
  if (!audiences.includes(resourceUri) && !audiences.includes(normalizedResource)) {
    throw new StartupConfigError(
      "MCP_RESOURCE_URI must exactly match one of FORTEMI_AUTH_AUDIENCES " +
        "(or FORTEMI_AUTH_AUDIENCE fallback), including any path"
    );
  }
  return { resourceUri, acceptedAudiences: audiences };
}

function resolveTokenExchange(env, transport, resourcePolicy) {
  const enabled = parseStrictBool("MCP_TOKEN_EXCHANGE", env.MCP_TOKEN_EXCHANGE, false);
  if (!enabled) return { enabled: false };
  if (transport !== "http") {
    throw new StartupConfigError("MCP_TOKEN_EXCHANGE=true is only supported with MCP_TRANSPORT=http");
  }
  const clientId = (env.MCP_TOKEN_EXCHANGE_CLIENT_ID || "").trim();
  const clientSecret = readSecret(env, "MCP_TOKEN_EXCHANGE_CLIENT_SECRET", "MCP_TOKEN_EXCHANGE_CLIENT_SECRET_FILE");
  const tokenEndpoint = (env.MCP_TOKEN_EXCHANGE_TOKEN_ENDPOINT || "").trim();
  const audience = (env.MCP_TOKEN_EXCHANGE_AUDIENCE || "").trim();
  if (!clientId) throw new StartupConfigError("MCP_TOKEN_EXCHANGE_CLIENT_ID is required when MCP_TOKEN_EXCHANGE=true");
  if (!clientSecret) {
    throw new StartupConfigError(
      "MCP_TOKEN_EXCHANGE_CLIENT_SECRET or MCP_TOKEN_EXCHANGE_CLIENT_SECRET_FILE is required when MCP_TOKEN_EXCHANGE=true"
    );
  }
  if (!tokenEndpoint) {
    throw new StartupConfigError("MCP_TOKEN_EXCHANGE_TOKEN_ENDPOINT is required when MCP_TOKEN_EXCHANGE=true");
  }
  if (!audience) throw new StartupConfigError("MCP_TOKEN_EXCHANGE_AUDIENCE is required when MCP_TOKEN_EXCHANGE=true");
  normalizeAbsoluteHttpsUrl("MCP_TOKEN_EXCHANGE_TOKEN_ENDPOINT", tokenEndpoint, {
    allowLocalHttp: parseStrictBool("FORTEMI_ALLOW_LOCAL_ISSUER", env.FORTEMI_ALLOW_LOCAL_ISSUER, false),
  });
  if (resourcePolicy.acceptedAudiences.length > 0 && !resourcePolicy.acceptedAudiences.includes(audience)) {
    throw new StartupConfigError("MCP_TOKEN_EXCHANGE_AUDIENCE must be one of FORTEMI_AUTH_AUDIENCES");
  }
  return { enabled: true, clientId, clientSecret, tokenEndpoint, audience };
}

function localApiBase(env) {
  const layout = (env.MCP_API_LAYOUT || "").trim().toLowerCase();
  if (!layout) return null;
  if (!LOCAL_LAYOUTS.has(layout)) {
    throw new StartupConfigError(`MCP_API_LAYOUT must be "bundle" or "sidecar" (got "${env.MCP_API_LAYOUT}")`);
  }
  const port = env.MCP_LOCAL_API_PORT || String(DEFAULT_LOCAL_API_PORT);
  if (!/^\d{1,5}$/.test(port) || Number(port) < 1 || Number(port) > 65535) {
    throw new StartupConfigError(`MCP_LOCAL_API_PORT must be a TCP port (got "${port}")`);
  }
  return `http://127.0.0.1:${port}`;
}

/** Resolve the API base URL. Returns { apiBase, source } or throws StartupConfigError. */
export function resolveApiBase(env) {
  for (const name of ["MATRIC_API_URL", "FORTEMI_URL"]) {
    const raw = (env[name] || "").trim();
    if (raw) return { apiBase: normalizeBaseUrl(name, raw), source: name };
  }
  const local = localApiBase(env);
  if (local) return { apiBase: local, source: "MCP_API_LAYOUT" };
  throw new StartupConfigError(
    "No Fortemi API URL configured. Set MATRIC_API_URL or FORTEMI_URL to the API address " +
      "(for example http://127.0.0.1:3000), or MCP_API_LAYOUT=bundle when the API runs in the same " +
      "container. The MCP server no longer falls back to a hosted default."
  );
}

/** Public base for links shown to users: the external issuer when set, else the API base. */
export function resolvePublicUrl(env, apiBase) {
  const raw = (env.ISSUER_URL || "").trim();
  return raw ? normalizeBaseUrl("ISSUER_URL", raw) : apiBase;
}

/**
 * Resolve the HTTP transport auth posture, mirroring the API (ADR-094): REQUIRE_AUTH defaults to
 * true; REQUIRE_AUTH=false needs I_UNDERSTAND_NO_AUTH=true; multi-tenant never runs anonymous.
 */
export function resolveAuthPolicy(env) {
  const requireAuth = parseStrictBool("REQUIRE_AUTH", env.REQUIRE_AUTH, true);
  const acknowledged = parseStrictBool("I_UNDERSTAND_NO_AUTH", env.I_UNDERSTAND_NO_AUTH, false);
  const multiTenant = parseStrictBool("FORTEMI_MULTI_TENANT", env.FORTEMI_MULTI_TENANT, false);
  if (requireAuth) return { requireAuth: true, anonymous: false };
  if (multiTenant) {
    throw new StartupConfigError("REQUIRE_AUTH=false is not allowed with FORTEMI_MULTI_TENANT=true");
  }
  if (!acknowledged) {
    throw new StartupConfigError(
      "REQUIRE_AUTH=false requires I_UNDERSTAND_NO_AUTH=true. The MCP HTTP transport requires " +
        "authentication by default; anonymous mode is for local development only."
    );
  }
  return { requireAuth: false, anonymous: true };
}

/** Load the full startup configuration. HTTP transport also resolves the auth policy. */
export function loadStartupConfig(env) {
  const transport = env.MCP_TRANSPORT || "stdio";
  if (transport !== "stdio" && transport !== "http") {
    throw new StartupConfigError(`MCP_TRANSPORT must be "stdio" or "http" (got "${transport}")`);
  }
  const { apiBase, source } = resolveApiBase(env);
  const publicUrl = resolvePublicUrl(env, apiBase);
  const auth = transport === "http" ? resolveAuthPolicy(env) : { requireAuth: false, anonymous: false };
  const resource = resolveResourcePolicy(env, transport);
  const tokenExchange = resolveTokenExchange(env, transport, resource);
  return { transport, apiBase, apiBaseSource: source, publicUrl, auth, resource, tokenExchange };
}
