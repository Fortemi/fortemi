import { strict as assert } from "node:assert";
import { describe, test } from "node:test";
import {
  buildInferenceAuditPath,
  buildInferenceConnectionRequest,
  buildInferenceUpdateRequest,
} from "../lib/inference-requests.js";
import tools from "../tools.js";

describe("Inference MCP request mapping", () => {
  test("schema exposes the current inference administration contract", () => {
    const tool = tools.find(candidate => candidate.name === "manage_inference");
    const properties = tool.inputSchema.properties;
    assert.deepEqual(properties.action.enum, [
      "list_models",
      "get_embedding_config",
      "list_embedding_configs",
      "get_config",
      "list_providers",
      "get_config_audit",
      "update_config",
      "reset_config",
      "test_connection",
    ]);
    for (const field of [
      "ollama", "openai", "llamacpp", "openrouter", "embedding_backend",
      "validate", "dry_run", "atomic", "timeout_secs", "limit", "changed_by", "audit_action",
    ]) {
      assert.ok(properties[field], `manage_inference schema is missing ${field}`);
    }
  });

  test("forwards every configurable provider and update flag", () => {
    const request = buildInferenceUpdateRequest({
      ollama: { base_url: "http://ollama:11434" },
      openai: { generation_model: "gpt-4o" },
      llamacpp: { base_url: "http://llama:8080" },
      openrouter: { generation_model: "anthropic/claude-sonnet-4" },
      embedding_backend: "ollama",
      validate: true,
      dry_run: true,
      atomic: false,
    });

    assert.deepEqual(request.body, {
      ollama: { base_url: "http://ollama:11434" },
      openai: { generation_model: "gpt-4o" },
      llamacpp: { base_url: "http://llama:8080" },
      openrouter: { generation_model: "anthropic/claude-sonnet-4" },
      embedding_backend: "ollama",
    });
    assert.equal(request.path, "/api/v1/inference/config?validate=true&dry_run=true&atomic=false");
  });

  test("preserves explicit null when clearing embedding routing", () => {
    const request = buildInferenceUpdateRequest({ embedding_backend: null });
    assert.deepEqual(request.body, { embedding_backend: null });
    assert.equal(request.path, "/api/v1/inference/config");
  });

  test("omits embedding routing when it was not supplied", () => {
    assert.deepEqual(buildInferenceUpdateRequest({}).body, {});
  });

  test("maps audit filters without colliding with the MCP action discriminator", () => {
    assert.equal(
      buildInferenceAuditPath({ limit: 25, changed_by: "operator", audit_action: "reset" }),
      "/api/v1/inference/config/audit?limit=25&changed_by=operator&action=reset",
    );
  });

  test("forwards connection timeout", () => {
    assert.deepEqual(
      buildInferenceConnectionRequest({
        base_url: "http://provider:8080",
        provider: "openai",
        api_key: "secret",
        timeout_secs: 30,
      }),
      {
        base_url: "http://provider:8080",
        provider: "openai",
        api_key: "secret",
        timeout_secs: 30,
      },
    );
  });
});
