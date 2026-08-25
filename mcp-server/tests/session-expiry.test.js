#!/usr/bin/env node

import { strict as assert } from "node:assert";
import { once } from "node:events";
import { spawn } from "node:child_process";
import net from "node:net";
import path from "node:path";
import { after, before, describe, test } from "node:test";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const serverDir = path.resolve(__dirname, "..");

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function allocatePort() {
  const listener = net.createServer();
  listener.unref();
  await new Promise((resolve, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", resolve);
  });
  const address = listener.address();
  await new Promise((resolve, reject) => {
    listener.close((error) => (error ? reject(error) : resolve()));
  });
  return address.port;
}

describe("expired Streamable HTTP sessions", () => {
  let baseUrl;
  let server;
  let serverOutput = "";

  before(async () => {
    const port = await allocatePort();
    baseUrl = `http://127.0.0.1:${port}`;
    server = spawn(process.execPath, ["index.js"], {
      cwd: serverDir,
      env: {
        ...process.env,
        FORTEMI_URL: "http://127.0.0.1:9",
        MCP_BASE_URL: baseUrl,
        MCP_PORT: String(port),
        MCP_TRANSPORT: "http",
        REQUIRE_AUTH: "false",
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    server.stdout.on("data", (chunk) => {
      serverOutput += chunk;
    });
    server.stderr.on("data", (chunk) => {
      serverOutput += chunk;
    });

    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      if (server.exitCode !== null) {
        throw new Error(`MCP server exited during startup:\n${serverOutput}`);
      }
      try {
        const response = await fetch(`${baseUrl}/health`);
        if (response.ok) return;
      } catch {
        // The listener may not be ready yet.
      }
      await delay(50);
    }
    throw new Error(`Timed out waiting for MCP server:\n${serverOutput}`);
  });

  after(async () => {
    if (!server || server.exitCode !== null) return;
    server.kill("SIGTERM");
    await Promise.race([once(server, "exit"), delay(2000)]);
  });

  test("POST returns a JSON-RPC 404 for an unknown session id", async () => {
    const response = await fetch(`${baseUrl}/`, {
      method: "POST",
      headers: {
        Accept: "application/json, text/event-stream",
        "Content-Type": "application/json",
        "Mcp-Session-Id": "expired-session",
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 42,
        method: "tools/list",
        params: {},
      }),
    });

    assert.equal(response.status, 404);
    assert.deepEqual(await response.json(), {
      jsonrpc: "2.0",
      error: {
        code: -32001,
        message: "Session not found; reinitialize",
      },
      id: null,
    });
  });

  test("GET returns 404 for an unknown session id", async () => {
    const response = await fetch(`${baseUrl}/`, {
      headers: { "Mcp-Session-Id": "expired-session" },
    });

    assert.equal(response.status, 404);
    assert.deepEqual(await response.json(), {
      error: "Session not found; reinitialize",
    });
  });
});
