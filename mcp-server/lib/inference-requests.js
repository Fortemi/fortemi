const hasOwn = (value, key) => Object.prototype.hasOwnProperty.call(value, key);

export function buildInferenceUpdateRequest(args) {
  const body = {};
  for (const provider of ["ollama", "openai", "llamacpp", "openrouter"]) {
    if (args[provider] !== undefined) body[provider] = args[provider];
  }
  if (hasOwn(args, "embedding_backend")) {
    body.embedding_backend = args.embedding_backend;
  }

  const params = new URLSearchParams();
  for (const flag of ["validate", "dry_run", "atomic"]) {
    if (args[flag] !== undefined) params.set(flag, String(args[flag]));
  }
  const query = params.toString();
  return {
    path: `/api/v1/inference/config${query ? `?${query}` : ""}`,
    body,
  };
}

export function buildInferenceAuditPath(args) {
  const params = new URLSearchParams();
  for (const field of ["limit", "changed_by", "audit_action"]) {
    if (args[field] !== undefined) {
      params.set(field === "audit_action" ? "action" : field, String(args[field]));
    }
  }
  const query = params.toString();
  return `/api/v1/inference/config/audit${query ? `?${query}` : ""}`;
}

export function buildInferenceConnectionRequest(args) {
  const body = { base_url: args.base_url };
  for (const field of ["provider", "api_key", "timeout_secs"]) {
    if (args[field] !== undefined) body[field] = args[field];
  }
  return body;
}
