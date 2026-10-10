// Spawns the MCP server under an egress guard to prove it never dials a non-local host when URLs are unset (#1171).

import { strict as assert } from "node:assert";
import { once } from "node:events";
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { after, before, describe, test } from "node:test";
import { fileURLToPath } from "node:url";

const serverDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const guard = path.join(serverDir, "tests", "helpers", "egress-guard.mjs");
const LOOPBACK = new Set(["127.0.0.1", "::1", "localhost", "::ffff:127.0.0.1"]);

// Only what node needs; every Fortemi URL and auth variable is deliberately absent.
function baseEnv(extra, logFile) {
  return { PATH: process.env.PATH, HOME: os.tmpdir(), FORTEMI_EGRESS_LOG: logFile, ...extra };
}

function tempLog() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mcp-egress-"));
  return path.join(dir, "egress.jsonl");
}

function readLog(file) {
  if (!fs.existsSync(file)) return [];
  return fs.readFileSync(file, "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line));
}

function nonLocal(entries) {
  return entries.filter((entry) => !LOOPBACK.has(entry.host));
}

function startServer(extra, logFile, stdio = ["ignore", "pipe", "pipe"]) {
  const child = spawn(process.execPath, ["--import", guard, "index.js"], {
    cwd: serverDir,
    env: baseEnv(extra, logFile),
    stdio,
  });
  child.output = "";
  child.stdout?.on("data", (chunk) => { child.output += chunk; });
  child.stderr.on("data", (chunk) => { child.output += chunk; });
  return child;
}

async function exitOf(child, timeoutMs = 5000) {
  const timer = setTimeout(() => child.kill("SIGKILL"), timeoutMs);
  const [code] = await once(child, "exit");
  clearTimeout(timer);
  return code;
}

async function freePort() {
  const server = net.createServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  await new Promise((resolve) => server.close(resolve));
  return port;
}

describe("with no API URL configured the server fails closed", () => {
  for (const transport of ["stdio", "http"]) {
    test(`${transport} transport exits non-zero without any outbound connection`, async () => {
      const log = tempLog();
      const child = startServer({ MCP_TRANSPORT: transport }, log);
      const code = await exitOf(child);
      assert.equal(code, 1, child.output);
      assert.match(child.output, /No Fortemi API URL configured/);
      assert.doesNotMatch(child.output, /fortemi\.com/);
      assert.deepEqual(readLog(log), [], "no socket or DNS attempt may happen before the startup check");
    });
  }

  test("ISSUER_URL alone does not become the API base", async () => {
    const log = tempLog();
    const child = startServer({ MCP_TRANSPORT: "http", ISSUER_URL: "https://memory.example.com" }, log);
    assert.equal(await exitOf(child), 1, child.output);
    assert.deepEqual(readLog(log), []);
  });
});

describe("HTTP transport requires authentication by default", () => {
  test("REQUIRE_AUTH=false without I_UNDERSTAND_NO_AUTH refuses to start", async () => {
    const log = tempLog();
    const child = startServer({ MCP_TRANSPORT: "http", MCP_API_LAYOUT: "bundle", REQUIRE_AUTH: "false" }, log);
    assert.equal(await exitOf(child), 1, child.output);
    assert.match(child.output, /I_UNDERSTAND_NO_AUTH=true/);
  });

  test("multi-tenant refuses anonymous mode even with the acknowledgment", async () => {
    const log = tempLog();
    const child = startServer({
      MCP_TRANSPORT: "http", MCP_API_LAYOUT: "bundle", REQUIRE_AUTH: "false",
      I_UNDERSTAND_NO_AUTH: "true", FORTEMI_MULTI_TENANT: "true",
    }, log);
    assert.equal(await exitOf(child), 1, child.output);
  });

  describe("default posture", () => {
    let child;
    let baseUrl;
    const log = tempLog();

    before(async () => {
      const port = await freePort();
      baseUrl = `http://127.0.0.1:${port}`;
      child = startServer({ MCP_TRANSPORT: "http", MCP_API_LAYOUT: "bundle", MCP_PORT: String(port), MCP_BASE_URL: baseUrl }, log);
      const deadline = Date.now() + 5000;
      while (Date.now() < deadline) {
        if (child.exitCode !== null) throw new Error(`server exited:\n${child.output}`);
        try {
          if ((await fetch(`${baseUrl}/health`)).ok) return;
        } catch { /* not listening yet */ }
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      throw new Error(`timed out:\n${child.output}`);
    });

    after(async () => {
      if (child && child.exitCode === null) {
        child.kill("SIGTERM");
        await exitOf(child, 2000);
      }
    });

    test("an unauthenticated MCP request gets 401", async () => {
      const response = await fetch(`${baseUrl}/`, {
        method: "POST",
        headers: { "Content-Type": "application/json", Accept: "application/json, text/event-stream" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} }),
      });
      assert.equal(response.status, 401);
      assert.match(response.headers.get("www-authenticate") || "", /resource_metadata=/);
    });

    test("protected-resource metadata never names a hosted default", async () => {
      const body = await (await fetch(`${baseUrl}/.well-known/oauth-protected-resource`)).json();
      assert.deepEqual(body.authorization_servers, ["http://127.0.0.1:3000"]);
    });

    test("nothing dialed a non-local host", () => {
      assert.deepEqual(nonLocal(readLog(log)), []);
    });
  });
});

describe("external IdP discovery", () => {
  let child;
  let baseUrl;
  const log = tempLog();

  before(async () => {
    const port = await freePort();
    baseUrl = `http://127.0.0.1:${port}`;
    child = startServer({
      MCP_TRANSPORT: "http",
      MCP_API_LAYOUT: "bundle",
      MCP_PORT: String(port),
      MCP_BASE_URL: baseUrl,
      FORTEMI_AUTH_ISSUER: "https://idp.example.com/realms/acme",
    }, log);
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      if (child.exitCode !== null) throw new Error(`server exited:\n${child.output}`);
      try {
        if ((await fetch(`${baseUrl}/health`)).ok) return;
      } catch { /* not listening yet */ }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    throw new Error(`timed out:\n${child.output}`);
  });

  after(async () => {
    if (child && child.exitCode === null) {
      child.kill("SIGTERM");
      await exitOf(child, 2000);
    }
  });

  test("PRM names the IdP and AS metadata is not proxied", async () => {
    const prm = await (await fetch(`${baseUrl}/.well-known/oauth-protected-resource`)).json();
    assert.deepEqual(prm.authorization_servers, ["https://idp.example.com/realms/acme"]);
    assert.deepEqual(prm.scopes_supported, ["read", "write", "admin", "mcp"]);

    const asMetadata = await fetch(`${baseUrl}/.well-known/oauth-authorization-server`);
    assert.equal(asMetadata.status, 404);
    assert.deepEqual(nonLocal(readLog(log)), []);
  });
});

describe("stdio tools in the bundle layout only reach the local API", () => {
  let api;
  let apiPort;
  const seen = [];

  before(async () => {
    api = http.createServer((req, res) => {
      seen.push(req.url);
      res.setHeader("Content-Type", "application/json");
      res.end(JSON.stringify({ status: "healthy", version: "test" }));
    });
    await new Promise((resolve) => api.listen(0, "127.0.0.1", resolve));
    apiPort = api.address().port;
  });

  after(() => new Promise((resolve) => api.close(resolve)));

  test("health_check calls 127.0.0.1 and nothing else", { timeout: 10_000 }, async () => {
    const log = tempLog();
    const child = startServer(
      { MCP_TRANSPORT: "stdio", MCP_API_LAYOUT: "bundle", MCP_LOCAL_API_PORT: String(apiPort) },
      log,
      ["pipe", "pipe", "pipe"]
    );
    const result = await new Promise((resolve, reject) => {
      let buffer = "";
      const timer = setTimeout(() => reject(new Error(`timed out:\n${child.output}`)), 8000);
      child.stdout.on("data", (chunk) => {
        buffer += chunk;
        const lines = buffer.split("\n");
        buffer = lines.pop();
        for (const line of lines.filter((l) => l.trim())) {
          const message = JSON.parse(line);
          if (message.id === 1) {
            child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`);
            child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "health_check", arguments: {} } })}\n`);
          } else if (message.id === 2) {
            clearTimeout(timer);
            resolve(message);
          }
        }
      });
      child.stdin.write(`${JSON.stringify({
        jsonrpc: "2.0", id: 1, method: "initialize",
        params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "egress-test", version: "1" } },
      })}\n`);
    });
    child.kill("SIGTERM");
    await exitOf(child, 2000);
    assert.ok(!result.error, JSON.stringify(result));
    assert.ok(seen.includes("/health"), `local API was not called: ${seen}`);
    const entries = readLog(log);
    assert.ok(entries.length > 0, "the guard should have observed the local call");
    assert.deepEqual(nonLocal(entries), []);
  });
});
