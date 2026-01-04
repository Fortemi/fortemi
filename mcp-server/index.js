#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { SSEServerTransport } from "@modelcontextprotocol/sdk/server/sse.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { AsyncLocalStorage } from "node:async_hooks";

const API_BASE = process.env.MATRIC_MEMORY_URL || "https://memory.integrolabs.net";
const API_KEY = process.env.MATRIC_MEMORY_API_KEY || null;
const MCP_TRANSPORT = process.env.MCP_TRANSPORT || "stdio"; // "stdio" or "http"
const MCP_PORT = parseInt(process.env.MCP_PORT || "3001", 10);

// AsyncLocalStorage for per-request token context
const tokenStorage = new AsyncLocalStorage();

// Helper to make API requests (uses session token in HTTP mode, API_KEY in stdio mode)
async function apiRequest(method, path, body = null) {
  const url = `${API_BASE}${path}`;
  const headers = { "Content-Type": "application/json" };

  // Get token from async context (HTTP mode) or use API_KEY (stdio mode)
  const sessionToken = tokenStorage.getStore()?.token;
  if (sessionToken) {
    headers["Authorization"] = `Bearer ${sessionToken}`;
  } else if (API_KEY) {
    headers["Authorization"] = `Bearer ${API_KEY}`;
  }

  const options = { method, headers };
  if (body) {
    options.body = JSON.stringify(body);
  }

  const response = await fetch(url, options);
  if (!response.ok) {
    const error = await response.text();
    throw new Error(`API error ${response.status}: ${error}`);
  }
  if (response.status === 204) return null;
  return response.json();
}

// Create MCP server
const server = new Server(
  {
    name: "matric-memory",
    version: "0.1.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

// Define available tools
const tools = [
  {
    name: "list_notes",
    description: "List all notes in the memory system. Returns summaries with titles, snippets, and metadata.",
    inputSchema: {
      type: "object",
      properties: {
        limit: { type: "number", description: "Maximum number of notes to return", default: 50 },
        offset: { type: "number", description: "Pagination offset", default: 0 },
        filter: { type: "string", description: "Filter: 'starred' or 'archived'" },
      },
    },
  },
  {
    name: "get_note",
    description: "Get full details of a specific note including original content, AI revisions, tags, and links.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note to retrieve" },
      },
      required: ["id"],
    },
  },
  {
    name: "create_note",
    description: "Create a new note in the memory system. Content should be markdown formatted.",
    inputSchema: {
      type: "object",
      properties: {
        content: { type: "string", description: "Markdown content of the note" },
        tags: { type: "array", items: { type: "string" }, description: "Optional tags" },
      },
      required: ["content"],
    },
  },
  {
    name: "update_note",
    description: "Update an existing note's content or status.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note to update" },
        content: { type: "string", description: "New markdown content" },
        starred: { type: "boolean", description: "Star/unstar the note" },
        archived: { type: "boolean", description: "Archive/unarchive the note" },
      },
      required: ["id"],
    },
  },
  {
    name: "delete_note",
    description: "Soft delete a note. Can be restored later.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note to delete" },
      },
      required: ["id"],
    },
  },
  {
    name: "search_notes",
    description: "Search notes using full-text and semantic search. Returns matching notes ranked by relevance.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Search query" },
        limit: { type: "number", description: "Maximum results", default: 20 },
        mode: { type: "string", enum: ["hybrid", "fts", "semantic"], description: "Search mode", default: "hybrid" },
      },
      required: ["query"],
    },
  },
  {
    name: "list_tags",
    description: "List all tags in the system.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "set_note_tags",
    description: "Set tags for a note (replaces existing tags).",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note" },
        tags: { type: "array", items: { type: "string" }, description: "New tags" },
      },
      required: ["id", "tags"],
    },
  },
  {
    name: "get_note_links",
    description: "Get incoming and outgoing links for a note.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note" },
      },
      required: ["id"],
    },
  },
  {
    name: "create_job",
    description: "Queue a background job for AI processing (embedding, revision, linking, etc).",
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note to process" },
        job_type: {
          type: "string",
          enum: ["ai_revision", "embedding", "linking", "context_update", "title_generation"],
          description: "Type of job to create"
        },
        priority: { type: "number", description: "Job priority (higher = sooner)" },
      },
      required: ["job_type"],
    },
  },
];

// Handle list tools request
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools };
});

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  try {
    let result;

    switch (name) {
      case "list_notes": {
        const params = new URLSearchParams();
        if (args.limit) params.set("limit", args.limit);
        if (args.offset) params.set("offset", args.offset);
        if (args.filter) params.set("filter", args.filter);
        result = await apiRequest("GET", `/api/v1/notes?${params}`);
        break;
      }

      case "get_note":
        result = await apiRequest("GET", `/api/v1/notes/${args.id}`);
        break;

      case "create_note":
        result = await apiRequest("POST", "/api/v1/notes", {
          content: args.content,
          tags: args.tags,
        });
        break;

      case "update_note": {
        const body = {};
        if (args.content !== undefined) body.content = args.content;
        if (args.starred !== undefined) body.starred = args.starred;
        if (args.archived !== undefined) body.archived = args.archived;
        await apiRequest("PATCH", `/api/v1/notes/${args.id}`, body);
        result = { success: true };
        break;
      }

      case "delete_note":
        await apiRequest("DELETE", `/api/v1/notes/${args.id}`);
        result = { success: true };
        break;

      case "search_notes": {
        const params = new URLSearchParams({ q: args.query });
        if (args.limit) params.set("limit", args.limit);
        if (args.mode) params.set("mode", args.mode);
        result = await apiRequest("GET", `/api/v1/search?${params}`);
        break;
      }

      case "list_tags":
        result = await apiRequest("GET", "/api/v1/tags");
        break;

      case "set_note_tags":
        await apiRequest("PUT", `/api/v1/notes/${args.id}/tags`, { tags: args.tags });
        result = { success: true };
        break;

      case "get_note_links":
        result = await apiRequest("GET", `/api/v1/notes/${args.id}/links`);
        break;

      case "create_job":
        result = await apiRequest("POST", "/api/v1/jobs", {
          note_id: args.note_id,
          job_type: args.job_type,
          priority: args.priority,
        });
        break;

      default:
        throw new Error(`Unknown tool: ${name}`);
    }

    return {
      content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
    };
  } catch (error) {
    return {
      content: [{ type: "text", text: `Error: ${error.message}` }],
      isError: true,
    };
  }
});

// Start server based on transport mode
if (MCP_TRANSPORT === "http") {
  // HTTP/SSE transport for remote access with OAuth
  const express = (await import("express")).default;
  const cors = (await import("cors")).default;

  const app = express();
  app.use(cors());
  app.use(express.json());

  // Store active sessions with their tokens
  const sessions = new Map();

  // OAuth token validation middleware
  async function validateToken(req, res, next) {
    const authHeader = req.headers.authorization;
    if (!authHeader || !authHeader.startsWith("Bearer ")) {
      return res.status(401).json({ error: "Missing or invalid Authorization header" });
    }

    const token = authHeader.slice(7);

    // Validate token against the API's introspection endpoint
    try {
      const response = await fetch(`${API_BASE}/oauth/introspect`, {
        method: "POST",
        headers: {
          "Content-Type": "application/x-www-form-urlencoded",
          "Authorization": `Basic ${Buffer.from(`${process.env.MCP_CLIENT_ID}:${process.env.MCP_CLIENT_SECRET}`).toString("base64")}`,
        },
        body: `token=${encodeURIComponent(token)}`,
      });

      if (!response.ok) {
        return res.status(401).json({ error: "Token validation failed" });
      }

      const introspection = await response.json();
      if (!introspection.active) {
        return res.status(401).json({ error: "Token is not active" });
      }

      // Check for MCP or read scope
      const scopes = (introspection.scope || "").split(" ");
      if (!scopes.includes("mcp") && !scopes.includes("read")) {
        return res.status(403).json({ error: "Insufficient scope" });
      }

      // Store token in request for session association
      req.accessToken = token;
      next();
    } catch (error) {
      console.error("Token validation error:", error);
      return res.status(500).json({ error: "Token validation error" });
    }
  }

  // SSE endpoint for MCP connections
  app.get("/sse", validateToken, async (req, res) => {
    console.log("New SSE connection with token");

    // Use full path since we're behind /mcp/ proxy
    const messagesPath = process.env.MCP_BASE_PATH ? `${process.env.MCP_BASE_PATH}/messages` : "/messages";
    const transport = new SSEServerTransport(messagesPath, res);
    const sessionId = transport.sessionId;

    // Store the token for this session
    sessions.set(sessionId, { transport, token: req.accessToken });
    console.log(`Session ${sessionId} created with token`);

    res.on("close", () => {
      console.log(`SSE connection closed for session ${sessionId}`);
      sessions.delete(sessionId);
    });

    // Connect with token context
    await tokenStorage.run({ token: req.accessToken }, async () => {
      await server.connect(transport);
    });
  });

  // Messages endpoint for SSE transport
  app.post("/messages", validateToken, async (req, res) => {
    const sessionId = req.query.sessionId;
    const session = sessions.get(sessionId);

    if (!session) {
      console.error(`Session not found: ${sessionId}`);
      return res.status(404).json({ error: "Session not found" });
    }

    // Execute the message handler with the session's token context
    await tokenStorage.run({ token: session.token }, async () => {
      await session.transport.handlePostMessage(req, res);
    });
  });

  // Health check
  app.get("/health", (req, res) => {
    res.json({ status: "ok", transport: "http", sessions: sessions.size });
  });

  // OAuth discovery endpoint - proxy to main API
  app.get("/.well-known/oauth-authorization-server", async (req, res) => {
    try {
      const response = await fetch(`${API_BASE}/.well-known/oauth-authorization-server`);
      const metadata = await response.json();
      res.json(metadata);
    } catch (error) {
      res.status(500).json({ error: "Failed to fetch OAuth metadata" });
    }
  });

  app.listen(MCP_PORT, () => {
    console.log(`MCP HTTP server listening on port ${MCP_PORT}`);
    console.log(`SSE endpoint: http://localhost:${MCP_PORT}/sse`);
    console.log(`OAuth discovery: http://localhost:${MCP_PORT}/.well-known/oauth-authorization-server`);
  });
} else {
  // Stdio transport for local use (default)
  const transport = new StdioServerTransport();
  await server.connect(transport);
}
