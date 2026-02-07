/**
 * MCP Test Client Helper
 *
 * Provides a simple HTTP client for testing MCP tools via the
 * StreamableHTTP transport. Handles session management, tool calls,
 * and error parsing for automated testing.
 *
 * Usage:
 *   const client = new MCPTestClient({ baseUrl: "http://localhost:3001" });
 *   await client.initialize();
 *   const result = await client.callTool("create_note", { content: "Hello" });
 *   await client.close();
 */

import crypto from "node:crypto";

export class MCPTestClient {
  constructor({ baseUrl, apiKey } = {}) {
    this.baseUrl = baseUrl || process.env.MCP_BASE_URL || "http://localhost:3001";
    this.apiKey = apiKey || process.env.FORTEMI_API_KEY || null;
    this.sessionId = null;
    this.requestId = 0;
  }

  /**
   * Send a JSON-RPC request to the MCP server.
   * Handles session cookie management.
   */
  async _rpc(method, params = {}) {
    this.requestId++;
    const body = {
      jsonrpc: "2.0",
      method,
      params,
      id: this.requestId,
    };

    const headers = {
      "Content-Type": "application/json",
      "Accept": "application/json, text/event-stream",
    };

    if (this.sessionId) {
      headers["Mcp-Session-Id"] = this.sessionId;
    }

    if (this.apiKey) {
      headers["Authorization"] = `Bearer ${this.apiKey}`;
    }

    const resp = await fetch(`${this.baseUrl}/mcp`, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    });

    // Capture session ID from response headers
    const newSessionId = resp.headers.get("mcp-session-id");
    if (newSessionId) {
      this.sessionId = newSessionId;
    }

    const contentType = resp.headers.get("content-type") || "";

    if (contentType.includes("text/event-stream")) {
      // SSE response — parse the stream for JSON-RPC result
      const text = await resp.text();
      const lines = text.split("\n");
      for (const line of lines) {
        if (line.startsWith("data: ")) {
          try {
            const data = JSON.parse(line.slice(6));
            if (data.id === this.requestId) {
              return data;
            }
          } catch {
            // Skip non-JSON data lines
          }
        }
      }
      throw new Error(`No matching JSON-RPC response found in SSE stream for request ${this.requestId}`);
    }

    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(`MCP HTTP error ${resp.status}: ${text}`);
    }

    return await resp.json();
  }

  /**
   * Initialize the MCP session by sending an initialize request.
   */
  async initialize() {
    const result = await this._rpc("initialize", {
      protocolVersion: "2025-03-26",
      capabilities: {},
      clientInfo: {
        name: "mcp-test-client",
        version: "1.0.0",
      },
    });

    if (result.error) {
      throw new Error(`Initialize failed: ${JSON.stringify(result.error)}`);
    }

    // Send initialized notification
    await this._rpc("notifications/initialized", {});

    return result.result;
  }

  /**
   * List all available MCP tools.
   */
  async listTools() {
    const result = await this._rpc("tools/list", {});
    if (result.error) {
      throw new Error(`List tools failed: ${JSON.stringify(result.error)}`);
    }
    return result.result?.tools || [];
  }

  /**
   * Call an MCP tool and return the parsed result.
   *
   * @param {string} name - Tool name
   * @param {object} args - Tool arguments
   * @returns {object} Parsed tool result (JSON content) or raw content
   * @throws {Error} If tool returns isError or MCP error
   */
  async callTool(name, args = {}) {
    const result = await this._rpc("tools/call", {
      name,
      arguments: args,
    });

    if (result.error) {
      const err = new Error(`MCP error: ${result.error.message || JSON.stringify(result.error)}`);
      err.code = result.error.code;
      err.data = result.error.data;
      throw err;
    }

    const content = result.result?.content;
    if (!content || content.length === 0) {
      return null;
    }

    // Check for tool-level errors
    if (result.result?.isError) {
      const text = content.map(c => c.text || "").join("\n");
      const err = new Error(`Tool error: ${text}`);
      err.isToolError = true;
      throw err;
    }

    // Parse the first text content as JSON (most tools return JSON)
    const firstText = content.find(c => c.type === "text");
    if (firstText) {
      try {
        return JSON.parse(firstText.text);
      } catch {
        // Return raw text if not JSON
        return firstText.text;
      }
    }

    return content;
  }

  /**
   * Call a tool and expect it to fail (return isError or throw).
   * Returns the error result for assertion.
   */
  async callToolExpectError(name, args = {}) {
    try {
      const result = await this.callTool(name, args);
      // Some errors come as successful results with error content
      if (typeof result === "string" && result.includes("error")) {
        return { error: result };
      }
      if (result && result.error) {
        return result;
      }
      throw new Error(`Expected tool "${name}" to fail, but it succeeded with: ${JSON.stringify(result)}`);
    } catch (e) {
      if (e.isToolError || e.message.includes("Tool error") || e.message.includes("MCP error")) {
        return { error: e.message };
      }
      throw e;
    }
  }

  /**
   * Generate a unique test ID using UUID.
   */
  static uniqueId() {
    return crypto.randomUUID();
  }

  /**
   * Generate a unique test tag with prefix.
   */
  static testTag(phase, suffix = "") {
    const id = crypto.randomUUID().slice(0, 8);
    return `test/mcp-${phase}${suffix ? "-" + suffix : ""}-${id}`;
  }

  /**
   * Close the MCP session gracefully.
   */
  async close() {
    // Session cleanup if needed
    this.sessionId = null;
  }
}

export default MCPTestClient;
