export const DEFAULT_RESOURCE_DOCUMENTATION_URL =
  "https://docs.fortemi.com/server/#/developers-mcp";

export function resolveResourceDocumentationUrl(value) {
  const candidate = value?.trim() || DEFAULT_RESOURCE_DOCUMENTATION_URL;
  let parsed;

  try {
    parsed = new URL(candidate);
  } catch {
    throw new Error("MCP_RESOURCE_DOCUMENTATION_URL must be an absolute URL");
  }

  if (!["http:", "https:"].includes(parsed.protocol)) {
    throw new Error("MCP_RESOURCE_DOCUMENTATION_URL must use HTTP or HTTPS");
  }
  if (parsed.username || parsed.password) {
    throw new Error("MCP_RESOURCE_DOCUMENTATION_URL must not contain credentials");
  }

  return parsed.href;
}

export function buildProtectedResourceMetadata({
  resource,
  authorizationServer,
  resourceDocumentation,
}) {
  return {
    resource,
    authorization_servers: [authorizationServer],
    bearer_methods_supported: ["header"],
    scopes_supported: ["mcp"],
    resource_documentation: resolveResourceDocumentationUrl(resourceDocumentation),
  };
}
