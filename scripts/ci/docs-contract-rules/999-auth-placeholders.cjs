"use strict";

module.exports = {
  id: "issue-999-auth-placeholders",
  ownerIssue: "#999",
  profiles: [
    "local_dev",
    "test_fixture",
    "self_hosted_operator",
    "native_distribution",
    "hosted_strict",
    "compatibility",
  ],
  positiveFixtures: [
    'curl -H "Authorization: Bearer mm_at_realisticExample"',
    'curl "https://example.test/events?token=mm_key_realisticExample"',
    "OPENAI_API_KEY=sk-proj-realisticExample",
    "MCP_CLIENT_SECRET=secret_xyz789",
    'passphrase: "secure-passphrase-123"',
    'save_private_key(&keypair.private, "/path/to/private.key", "your-passphrase")?;',
    '-p "your-secure-passphrase-123"',
    '"secret": "my-webhook-secret"',
  ],
  negativeFixtures: [
    "Use Authorization: Bearer <ACCESS_TOKEN>",
    "Use token=<STREAM_TOKEN>",
    "OPENAI_API_KEY=<OPENAI_API_KEY>",
    "MCP_CLIENT_SECRET=<MCP_CLIENT_SECRET>",
    'passphrase: "<PKE_PASSPHRASE>"',
    'load_private_key("/path/to/private.key", "<PKE_PASSPHRASE>")?;',
    '-p "<PKE_PASSPHRASE>"',
    '"secret": "<WEBHOOK_SECRET>"',
    "client_secret_basic is an OAuth auth method name",
    "secret_set: true",
    "task-specific embeddings",
  ],
  rules: [
    {
      id: "docs-token-placeholder",
      severity: "high",
      category: "fortemi_token_placeholder",
      detect(line) {
        return /\bmm_(?:at|rt|key)_[A-Za-z0-9._~+/=-]+/.test(line);
      },
      remediation:
        "Use scanner-safe placeholders such as <ACCESS_TOKEN>, <STREAM_TOKEN>, or <API_KEY>.",
    },
    {
      id: "docs-auth-header-secret",
      severity: "high",
      category: "credential_value_in_header",
      detect(line) {
        return /Authorization:\s*Bearer\s+(?!<ACCESS_TOKEN>)[A-Za-z0-9._~+/=-]{6,}/i.test(line);
      },
      remediation:
        "Use Authorization: Bearer <ACCESS_TOKEN> in docs and generated examples.",
    },
    {
      id: "docs-query-token-secret",
      severity: "high",
      category: "credential_value_in_query",
      detect(line) {
        return /[?&]token=(?!<STREAM_TOKEN>|<ACCESS_TOKEN>)[A-Za-z0-9._~+/=-]{6,}/i.test(line);
      },
      remediation:
        "Use token=<STREAM_TOKEN> for EventSource examples or avoid query-token examples.",
    },
    {
      id: "docs-provider-key-placeholder",
      severity: "high",
      category: "provider_key_placeholder",
      detect(line) {
        return /\b(?:OPENAI_API_KEY|OPENROUTER_API_KEY|MATRIC_OPENAI_API_KEY|api_key)["'\s:=]+["']?(?:sk|sk-proj|sk-or|hf)[_-][A-Za-z0-9._~+/=-]+/i.test(
          line
        );
      },
      remediation:
        "Use provider-neutral placeholders such as <OPENAI_API_KEY> or <API_KEY>.",
    },
    {
      id: "docs-client-secret-placeholder",
      severity: "high",
      category: "client_secret_placeholder",
      detect(line) {
        return /\b(?:MCP_CLIENT_SECRET|client_secret)["'\s:=]+["']?secret_[A-Za-z0-9._~+/=-]+/i.test(
          line
        );
      },
      remediation: "Use <MCP_CLIENT_SECRET> for OAuth/MCP client-secret examples.",
    },
    {
      id: "docs-passphrase-placeholder",
      severity: "high",
      category: "passphrase_or_webhook_secret_placeholder",
      appliesTo(relativePath) {
        return (
          relativePath.startsWith("docs/") ||
          relativePath === "mcp-server/index.js" ||
          relativePath === "crates/matric-crypto/src/lib.rs" ||
          relativePath === "crates/matric-crypto/src/pke/mod.rs"
        );
      },
      detect(line) {
        return (
          /\bpassphrase["']?\s*[:=]\s*["'](?!(?:<PKE_PASSPHRASE>|\.\.\.))[^"']{6,}["']/i.test(
            line
          ) ||
          /\bsecret["']?\s*[:=]\s*["'](?!(?:<WEBHOOK_SECRET>|<MCP_CLIENT_SECRET>|\.\.\.))[^"']{6,}["']/i.test(
            line
          ) ||
          /\b(?:save_private_key|load_private_key|encrypt_private_key|decrypt_private_key)\([^)]*["'](?!(?:<PKE_PASSPHRASE>|\.\.\.))[^"']*passphrase[^"']*["']/i.test(
            line
          ) ||
          /(?:^|\s)(?<!mkdir\s)-p\s+["'](?!(?:<PKE_PASSPHRASE>|\.\.\.))[^"']{6,}["']/.test(
            line
          )
        );
      },
      remediation:
        "Use <PKE_PASSPHRASE> or <WEBHOOK_SECRET> instead of realistic passphrase/secret examples.",
    },
  ],
};
