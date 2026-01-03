#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

const API_BASE = process.env.MATRIC_MEMORY_URL || "https://memory.integrolabs.net";

// Helper to make API requests
async function apiRequest(method, path, body = null) {
  const url = `${API_BASE}${path}`;
  const options = {
    method,
    headers: { "Content-Type": "application/json" },
  };
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

// Start server
const transport = new StdioServerTransport();
await server.connect(transport);
