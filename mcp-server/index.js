#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { SSEServerTransport } from "@modelcontextprotocol/sdk/server/sse.js";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { AsyncLocalStorage } from "node:async_hooks";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import tools from "./tools.js";
// execSync removed — all PKE operations now use HTTP API instead of CLI binary
import * as DEFAULTS from "./constants/defaults.js";

// Prevent unhandled errors from crashing the MCP server process (issue #131)
process.on("uncaughtException", (err) => {
  console.error("[mcp] Uncaught exception (process kept alive):", err.message);
});
process.on("unhandledRejection", (reason) => {
  console.error("[mcp] Unhandled rejection (process kept alive):", reason);
});

const API_BASE = process.env.FORTEMI_URL || process.env.ISSUER_URL || "https://fortemi.com";
const API_KEY = process.env.FORTEMI_API_KEY || null;
const MCP_TRANSPORT = process.env.MCP_TRANSPORT || "stdio"; // "stdio" or "http"
const MCP_PORT = parseInt(process.env.MCP_PORT || String(DEFAULTS.MCP_DEFAULT_PORT), 10);
const MCP_BASE_URL = process.env.MCP_BASE_URL || `http://localhost:${MCP_PORT}`;
const MAX_UPLOAD_SIZE = parseInt(process.env.MATRIC_MAX_UPLOAD_SIZE_BYTES || String(DEFAULTS.MAX_UPLOAD_SIZE_BYTES), 10);

// AsyncLocalStorage for per-request token context
const tokenStorage = new AsyncLocalStorage();

// Per-session active memory storage (sessionId -> memory name)
const sessionMemories = new Map();

// Helper to read a public key file — supports both JSON keyset format (from create_keyset)
// and raw binary format (from generate_keypair with output_dir)
function readPublicKeyAsBase64(keyPath) {
  const content = fs.readFileSync(keyPath, "utf8");
  try {
    const keyFile = JSON.parse(content);
    if (keyFile.public_key) return keyFile.public_key; // Already base64
  } catch {
    // Not JSON — treat as raw binary key
  }
  const rawBytes = fs.readFileSync(keyPath);
  return rawBytes.toString("base64");
}

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

  // Add X-Fortemi-Memory header if active memory is set for this session
  const sessionId = tokenStorage.getStore()?.sessionId;
  if (sessionId) {
    const activeMemory = sessionMemories.get(sessionId);
    if (activeMemory) {
      headers["X-Fortemi-Memory"] = activeMemory;
    }
  }

  const options = { method, headers };
  if (body) {
    options.body = JSON.stringify(body);
  }

  const response = await fetch(url, options);
  if (!response.ok) {
    const error = await response.text();
    // Surface token expiry clearly so MCP clients can re-authenticate (fixes #239)
    if (response.status === 401) {
      throw new Error(
        "MCP server requires re-authorization (token expired). " +
        "Please obtain a new access token and reconnect. " +
        `Details: ${error}`
      );
    }
    throw new Error(`API error ${response.status}: ${error}`);
  }
  if (response.status === 204) return null;
  const text = await response.text();
  if (!text || text.trim() === '') return null;
  return JSON.parse(text);
}

// Format bytes to human-readable string (e.g., "1.23 GB")
function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 2 : 0) + " " + units[i];
}

/**
 * Create a new MCP server instance.
 * Each connection gets its own server (required for proper session isolation).
 */
function createMcpServer() {
  const mcpServer = new Server(
    {
      name: "fortemi",
      version: "0.1.0",
    },
    {
      capabilities: {
        tools: {},
      },
    }
  );

  // Handle list tools request
  mcpServer.setRequestHandler(ListToolsRequestSchema, async () => {
    return { tools };
  });

  // Handle tool calls
  mcpServer.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;

    try {
      let result;

      switch (name) {
        case "list_notes": {
          const params = new URLSearchParams();
          if (args.limit !== undefined && args.limit !== null) params.set("limit", args.limit);
          if (args.offset !== undefined && args.offset !== null) params.set("offset", args.offset);
          if (args.filter) params.set("filter", args.filter);
          if (args.tags) params.set("tags", Array.isArray(args.tags) ? args.tags.join(",") : args.tags);
          if (args.collection_id) params.set("collection_id", args.collection_id);
          if (args.created_after) params.set("created_after", args.created_after);
          if (args.created_before) params.set("created_before", args.created_before);
          if (args.updated_after) params.set("updated_after", args.updated_after);
          if (args.updated_before) params.set("updated_before", args.updated_before);
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
            revision_mode: args.revision_mode,
            collection_id: args.collection_id,
            metadata: args.metadata,
          });
          break;

        case "bulk_create_notes": {
          const bulkResult = await apiRequest("POST", "/api/v1/notes/bulk", {
            notes: args.notes,
          });
          // API returns { ids: [...], count: N } — transform to array of { id }
          // for consistency with create_note which returns { id }
          result = (bulkResult.ids || []).map((id) => ({ id }));
          break;
        }

        case "update_note": {
          const body = {};
          if (args.content !== undefined) body.content = args.content;
          if (args.starred !== undefined) body.starred = args.starred;
          if (args.archived !== undefined) body.archived = args.archived;
          if (args.revision_mode !== undefined) body.revision_mode = args.revision_mode;
          if (args.metadata !== undefined) body.metadata = args.metadata;
          await apiRequest("PATCH", `/api/v1/notes/${args.id}`, body);
          result = { success: true };
          break;
        }

        case "delete_note":
          await apiRequest("DELETE", `/api/v1/notes/${args.id}`);
          result = { success: true };
          break;

        case "restore_note":
          result = await apiRequest("POST", `/api/v1/notes/${args.id}/restore`);
          break;

        case "search_notes": {
          const params = new URLSearchParams({ q: args.query });
          if (args.limit !== undefined && args.limit !== null) params.set("limit", args.limit);
          if (args.mode) params.set("mode", args.mode);
          if (args.set) params.set("set", args.set);
          if (args.collection_id) params.set("collection_id", args.collection_id);
          // Build strict_filter JSON from convenience params or direct JSON
          if (args.strict_filter) {
            params.set("strict_filter", args.strict_filter);
          } else if (args.required_tags || args.excluded_tags || args.any_tags) {
            const filter = {};
            if (args.required_tags) filter.required_tags = args.required_tags;
            if (args.excluded_tags) filter.excluded_tags = args.excluded_tags;
            if (args.any_tags) filter.any_tags = args.any_tags;
            params.set("strict_filter", JSON.stringify(filter));
          }
          result = await apiRequest("GET", `/api/v1/search?${params}`);
          break;
        }

        case "search_memories_by_location": {
          const params = new URLSearchParams();
          params.set("lat", args.lat);
          params.set("lon", args.lon);
          if (args.radius !== undefined && args.radius !== null) params.set("radius", args.radius);
          result = await apiRequest("GET", `/api/v1/memories/search?${params}`);
          break;
        }

        case "search_memories_by_time": {
          const params = new URLSearchParams();
          params.set("start", args.start);
          params.set("end", args.end);
          result = await apiRequest("GET", `/api/v1/memories/search?${params}`);
          break;
        }

        case "search_memories_combined": {
          const params = new URLSearchParams();
          params.set("lat", args.lat);
          params.set("lon", args.lon);
          if (args.radius !== undefined && args.radius !== null) params.set("radius", args.radius);
          params.set("start", args.start);
          params.set("end", args.end);
          result = await apiRequest("GET", `/api/v1/memories/search?${params}`);
          break;
        }

        case "create_provenance_location":
          result = await apiRequest("POST", "/api/v1/provenance/locations", {
            latitude: args.latitude,
            longitude: args.longitude,
            altitude_m: args.altitude_m,
            horizontal_accuracy_m: args.horizontal_accuracy_m,
            vertical_accuracy_m: args.vertical_accuracy_m,
            heading_degrees: args.heading_degrees,
            speed_mps: args.speed_mps,
            named_location_id: args.named_location_id,
            source: args.source,
            confidence: args.confidence,
          });
          break;

        case "create_named_location":
          result = await apiRequest("POST", "/api/v1/provenance/named-locations", {
            name: args.name,
            location_type: args.location_type,
            latitude: args.latitude,
            longitude: args.longitude,
            radius_m: args.radius_m,
            address_line: args.address_line,
            locality: args.locality,
            admin_area: args.admin_area,
            country: args.country,
            country_code: args.country_code,
            postal_code: args.postal_code,
            timezone: args.timezone,
            altitude_m: args.altitude_m,
            is_private: args.is_private,
            metadata: args.metadata,
          });
          break;

        case "create_provenance_device":
          result = await apiRequest("POST", "/api/v1/provenance/devices", {
            device_make: args.device_make,
            device_model: args.device_model,
            device_os: args.device_os,
            device_os_version: args.device_os_version,
            software: args.software,
            software_version: args.software_version,
            has_gps: args.has_gps,
            has_accelerometer: args.has_accelerometer,
            sensor_metadata: args.sensor_metadata,
            device_name: args.device_name,
          });
          break;

        case "create_file_provenance":
          result = await apiRequest("POST", "/api/v1/provenance/files", {
            attachment_id: args.attachment_id,
            capture_time_start: args.capture_time_start,
            capture_time_end: args.capture_time_end,
            capture_timezone: args.capture_timezone,
            capture_duration_seconds: args.capture_duration_seconds,
            time_source: args.time_source,
            time_confidence: args.time_confidence,
            location_id: args.location_id,
            device_id: args.device_id,
            event_type: args.event_type,
            event_title: args.event_title,
            event_description: args.event_description,
            raw_metadata: args.raw_metadata,
          });
          break;

        case "create_note_provenance":
          result = await apiRequest("POST", "/api/v1/provenance/notes", {
            note_id: args.note_id,
            capture_time_start: args.capture_time_start,
            capture_time_end: args.capture_time_end,
            capture_timezone: args.capture_timezone,
            time_source: args.time_source,
            time_confidence: args.time_confidence,
            location_id: args.location_id,
            device_id: args.device_id,
            event_type: args.event_type,
            event_title: args.event_title,
            event_description: args.event_description,
          });
          break;

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

        case "export_note": {
          const exportParams = new URLSearchParams();
          if (args.include_frontmatter !== undefined) {
            exportParams.set("include_frontmatter", args.include_frontmatter.toString());
          }
          if (args.content) exportParams.set("content", args.content);
          // Fetch as text since this returns markdown, not JSON
          const exportResponse = await fetch(`${API_BASE}/api/v1/notes/${args.id}/export?${exportParams}`, {
            headers: { "Accept": "text/markdown" },
          });
          if (!exportResponse.ok) {
            throw new Error(`Export failed: ${exportResponse.status}`);
          }
          result = { markdown: await exportResponse.text() };
          break;
        }

        case "list_collections": {
          const collParams = new URLSearchParams();
          if (args.parent_id) collParams.set("parent_id", args.parent_id);
          result = await apiRequest("GET", `/api/v1/collections?${collParams}`);
          break;
        }

        case "create_collection":
          result = await apiRequest("POST", "/api/v1/collections", {
            name: args.name,
            description: args.description,
            parent_id: args.parent_id,
          });
          break;

        case "get_collection":
          result = await apiRequest("GET", `/api/v1/collections/${args.id}`);
          break;

        case "delete_collection":
          await apiRequest("DELETE", `/api/v1/collections/${args.id}`);
          result = { success: true };
          break;

        case "update_collection": {
          const body = {};
          if (args.name !== undefined) body.name = args.name;
          if (args.description !== undefined) body.description = args.description;
          if (args.parent_id !== undefined) body.parent_id = args.parent_id;
          result = await apiRequest("PATCH", `/api/v1/collections/${args.id}`, body);
          break;
        }

        case "get_collection_notes": {
          const noteParams = new URLSearchParams();
          if (args.limit !== undefined && args.limit !== null) noteParams.set("limit", args.limit);
          if (args.offset) noteParams.set("offset", args.offset);
          result = await apiRequest("GET", `/api/v1/collections/${args.id}/notes?${noteParams}`);
          break;
        }

        case "move_note_to_collection":
          await apiRequest("POST", `/api/v1/notes/${args.note_id}/move`, {
            collection_id: args.collection_id || null,
          });
          result = { success: true };
          break;

        case "explore_graph": {
          const graphParams = new URLSearchParams();
          if (args.depth) graphParams.set("depth", args.depth);
          if (args.max_nodes) graphParams.set("max_nodes", args.max_nodes);
          result = await apiRequest("GET", `/api/v1/graph/${args.id}?${graphParams}`);
          break;
        }

        case "list_templates":
          result = await apiRequest("GET", "/api/v1/templates");
          break;

        case "create_template":
          result = await apiRequest("POST", "/api/v1/templates", {
            name: args.name,
            description: args.description,
            content: args.content,
            format: args.format,
            default_tags: args.default_tags,
            collection_id: args.collection_id,
          });
          break;

        case "get_template": {
          result = await apiRequest("GET", `/api/v1/templates/${args.id}`);
          // Extract {{variables}} from template content
          const variables = [];
          if (result.content) {
            const regex = /\{\{(\w+)\}\}/g;
            let match;
            while ((match = regex.exec(result.content)) !== null) {
              if (!variables.includes(match[1])) {
                variables.push(match[1]);
              }
            }
          }
          result.variables = variables;
          break;
        }

        case "delete_template":
          await apiRequest("DELETE", `/api/v1/templates/${args.id}`);
          result = { success: true };
          break;

        case "update_template": {
          const body = {};
          if (args.name !== undefined) body.name = args.name;
          if (args.description !== undefined) body.description = args.description;
          if (args.content !== undefined) body.content = args.content;
          if (args.format !== undefined) body.format = args.format;
          if (args.default_tags !== undefined) body.default_tags = args.default_tags;
          if (args.collection_id !== undefined) body.collection_id = args.collection_id;
          result = await apiRequest("PATCH", `/api/v1/templates/${args.id}`, body);
          break;
        }

        case "instantiate_template":
          result = await apiRequest("POST", `/api/v1/templates/${args.id}/instantiate`, {
            variables: args.variables || {},
            tags: args.tags,
            collection_id: args.collection_id,
            revision_mode: args.revision_mode,
          });
          break;

        case "create_job":
          result = await apiRequest("POST", "/api/v1/jobs", {
            note_id: args.note_id,
            job_type: args.job_type,
            priority: args.priority,
          });
          break;

        case "list_jobs": {
          const jobParams = new URLSearchParams();
          if (args.status) jobParams.set("status", args.status);
          if (args.job_type) jobParams.set("job_type", args.job_type);
          if (args.note_id) jobParams.set("note_id", args.note_id);
          if (args.limit !== undefined && args.limit !== null) jobParams.set("limit", args.limit);
          if (args.offset) jobParams.set("offset", args.offset);
          result = await apiRequest("GET", `/api/v1/jobs?${jobParams}`);
          break;
        }

        case "get_queue_stats":
          result = await apiRequest("GET", "/api/v1/jobs/stats");
          break;

        case "health_check": {
          // Handle both JSON and plain text health responses
          const url = `${API_BASE}/health`;
          const headers = { "Content-Type": "application/json" };
          const sessionToken = tokenStorage.getStore()?.token;
          if (sessionToken) {
            headers["Authorization"] = `Bearer ${sessionToken}`;
          } else if (API_KEY) {
            headers["Authorization"] = `Bearer ${API_KEY}`;
          }
          const response = await fetch(url, { method: "GET", headers });
          const text = await response.text();
          try {
            result = JSON.parse(text);
          } catch {
            // Plain text response (e.g., "healthy" from nginx)
            result = {
              status: text.trim().toLowerCase() === "healthy" ? "healthy" : text.trim(),
              version: process.env.MATRIC_VERSION || "unknown",
              source: "proxy"
            };
          }
          break;
        }

        case "get_system_info": {
          // Fetch health with plain text fallback
          const fetchHealth = async () => {
            const url = `${API_BASE}/health`;
            const headers = { "Content-Type": "application/json" };
            const sessionToken = tokenStorage.getStore()?.token;
            if (sessionToken) headers["Authorization"] = `Bearer ${sessionToken}`;
            else if (API_KEY) headers["Authorization"] = `Bearer ${API_KEY}`;
            try {
              const response = await fetch(url, { method: "GET", headers });
              const text = await response.text();
              try {
                return JSON.parse(text);
              } catch {
                return { status: text.trim().toLowerCase() === "healthy" ? "healthy" : text.trim(), version: process.env.MATRIC_VERSION || "unknown" };
              }
            } catch {
              return { status: "unknown" };
            }
          };
          const [health, memoryInfo, queueStats, embeddingSets] = await Promise.all([
            fetchHealth(),
            apiRequest("GET", "/api/v1/memory/info").catch(() => ({})),
            apiRequest("GET", "/api/v1/jobs/stats").catch(() => ({})),
            apiRequest("GET", "/api/v1/embedding-sets").catch(() => ({ sets: [] })),
          ]);

          // Extract embedding model from default set
          const defaultSet = (embeddingSets.sets || memoryInfo.embedding_sets || []).find(s => s.is_system || s.slug === "default");
          const embeddingModel = defaultSet?.model || DEFAULTS.EMBED_MODEL;
          const embeddingDimension = defaultSet?.dimension || DEFAULTS.EMBED_DIMENSION;

          result = {
            status: health.status || "unknown",
            versions: {
              release: health.version || process.env.MATRIC_VERSION || "unknown",
              git_sha: process.env.MATRIC_GIT_SHA || (process.env.MATRIC_VERSION?.includes("-") ? process.env.MATRIC_VERSION.split("-")[1] : "unknown"),
              build_date: process.env.MATRIC_BUILD_DATE || "unknown",
              postgresql: process.env.PG_VERSION || "16.x",
              mcp_server: "1.0.0",
            },
            infrastructure: {
              database: {
                type: "PostgreSQL",
                version: process.env.PG_VERSION || "16.x",
                extensions: {
                  pgvector: "0.8.x (HNSW vector indexing)",
                  pg_trgm: "1.6 (trigram search for emoji/symbols)",
                  unaccent: "1.1 (diacritics normalization)",
                },
              },
              search: {
                full_text: "PostgreSQL FTS with multilingual configs",
                semantic: `pgvector HNSW (cosine similarity)`,
                trigram: "pg_trgm GIN index (emoji, CJK, symbols)",
                hybrid: "RRF fusion (FTS + semantic)",
              },
              embedding: {
                provider: "Ollama",
                model: embeddingModel,
                dimension: embeddingDimension,
              },
            },
            stats: {
              total_notes: memoryInfo.summary?.total_notes || memoryInfo.total_notes || 0,
              total_embeddings: memoryInfo.summary?.total_embeddings || memoryInfo.total_embeddings || 0,
              total_links: memoryInfo.summary?.total_links || 0,
              total_collections: memoryInfo.summary?.total_collections || 0,
              total_tags: memoryInfo.summary?.total_tags || 0,
              embedding_sets: (embeddingSets.sets || memoryInfo.embedding_sets || []).length,
              pending_jobs: queueStats.pending || 0,
            },
            storage: memoryInfo.storage || {},
            components: health.components || {},
          };
          break;
        }

        // ============================================================================
        // EMBEDDING SETS
        // ============================================================================
        case "list_embedding_sets":
          result = await apiRequest("GET", "/api/v1/embedding-sets");
          break;

        case "get_embedding_set":
          result = await apiRequest("GET", `/api/v1/embedding-sets/${args.slug}`);
          break;

        case "create_embedding_set":
          result = await apiRequest("POST", "/api/v1/embedding-sets", {
            name: args.name,
            slug: args.slug,
            description: args.description,
            purpose: args.purpose,
            usage_hints: args.usage_hints,
            keywords: args.keywords || [],
            mode: args.mode || "auto",
            criteria: args.criteria || {},
          });
          break;

        case "list_set_members": {
          const memberParams = new URLSearchParams();
          if (args.limit !== undefined && args.limit !== null) memberParams.set("limit", args.limit);
          if (args.offset) memberParams.set("offset", args.offset);
          result = await apiRequest("GET", `/api/v1/embedding-sets/${args.slug}/members?${memberParams}`);
          break;
        }

        case "add_set_members":
          result = await apiRequest("POST", `/api/v1/embedding-sets/${args.slug}/members`, {
            note_ids: args.note_ids,
            added_by: args.added_by,
          });
          break;

        case "remove_set_member":
          await apiRequest("DELETE", `/api/v1/embedding-sets/${args.slug}/members/${args.note_id}`);
          result = { success: true };
          break;

        case "update_embedding_set": {
          const body = {};
          if (args.name !== undefined) body.name = args.name;
          if (args.description !== undefined) body.description = args.description;
          if (args.purpose !== undefined) body.purpose = args.purpose;
          if (args.usage_hints !== undefined) body.usage_hints = args.usage_hints;
          if (args.keywords !== undefined) body.keywords = args.keywords;
          if (args.criteria !== undefined) body.criteria = args.criteria;
          if (args.mode !== undefined) body.mode = args.mode;
          result = await apiRequest("PATCH", `/api/v1/embedding-sets/${args.slug}`, body);
          break;
        }

        case "delete_embedding_set":
          await apiRequest("DELETE", `/api/v1/embedding-sets/${args.slug}`);
          result = { success: true };
          break;

        case "refresh_embedding_set":
          result = await apiRequest("POST", `/api/v1/embedding-sets/${args.slug}/refresh`);
          break;

        case "reembed_all":
          // Queue a bulk re-embedding job
          const payload = {
            job_type: "re_embed_all",
          };
          if (args.embedding_set_slug) {
            payload.embedding_set = args.embedding_set_slug;
          }
          if (args.force !== undefined) {
            payload.force = args.force;
          }
          result = await apiRequest("POST", "/api/v1/jobs", payload);
          break;


        case "purge_note":
          // Queue a purge job for permanent deletion
          result = await apiRequest("POST", `/api/v1/notes/${args.id}/purge`);
          break;

        case "purge_notes":
          // Batch purge multiple notes
          const purgeResults = { queued: [], failed: [] };
          for (const noteId of args.note_ids) {
            try {
              await apiRequest("POST", `/api/v1/notes/${noteId}/purge`);
              purgeResults.queued.push(noteId);
            } catch (e) {
              purgeResults.failed.push({ id: noteId, error: e.message });
            }
          }
          result = purgeResults;
          break;

        case "purge_all_notes":
          // Require explicit confirmation
          if (!args.confirm) {
            throw new Error("Must set confirm=true to purge all notes");
          }
          // Get all notes and purge them
          const allNotes = await apiRequest("GET", `/api/v1/notes?limit=${DEFAULTS.INTERNAL_FETCH_LIMIT}`);
          const purgeAllResults = { queued: [], failed: [], total: (allNotes.notes || []).length };
          for (const note of allNotes.notes || []) {
            try {
              await apiRequest("POST", `/api/v1/notes/${note.id}/purge`);
              purgeAllResults.queued.push(note.id);
            } catch (e) {
              purgeAllResults.failed.push({ id: note.id, error: e.message });
            }
          }
          result = purgeAllResults;
          break;

        // ============================================================================
        // BACKUP & EXPORT - Calls API endpoints
        // ============================================================================
        case "export_all_notes": {
          // Export all notes via API endpoint
          const exportParams = new URLSearchParams();
          if (args.filter?.starred_only) exportParams.set("starred_only", "true");
          if (args.filter?.tags) exportParams.set("tags", args.filter.tags.join(","));
          if (args.filter?.created_after) exportParams.set("created_after", args.filter.created_after);
          if (args.filter?.created_before) exportParams.set("created_before", args.filter.created_before);

          result = await apiRequest("GET", `/api/v1/backup/export?${exportParams}`);
          break;
        }

        case "backup_now": {
          // Trigger backup via API endpoint
          const body = {};
          if (args.destinations) body.destinations = args.destinations;
          if (args.dry_run) body.dry_run = args.dry_run;

          result = await apiRequest("POST", "/api/v1/backup/trigger", Object.keys(body).length > 0 ? body : null);
          break;
        }

        case "backup_status": {
          // Get backup status via API endpoint
          result = await apiRequest("GET", "/api/v1/backup/status");
          break;
        }

        case "backup_download": {
          // Download backup as file (returns same data as export)
          const downloadParams = new URLSearchParams();
          if (args.starred_only) downloadParams.set("starred_only", "true");
          if (args.tags) downloadParams.set("tags", args.tags.join(","));
          if (args.created_after) downloadParams.set("created_after", args.created_after);
          if (args.created_before) downloadParams.set("created_before", args.created_before);

          result = await apiRequest("GET", `/api/v1/backup/download?${downloadParams}`);
          break;
        }

        case "backup_import": {
          // Import backup data
          const importBody = {
            backup: args.backup,
            dry_run: args.dry_run || false,
            on_conflict: args.on_conflict || "skip",
          };

          result = await apiRequest("POST", "/api/v1/backup/import", importBody);
          break;
        }

        case "knowledge_shard": {
          // Create full knowledge shard with all data including embeddings and links
          const shardParams = new URLSearchParams();
          if (args.include) {
            shardParams.set("include", Array.isArray(args.include) ? args.include.join(",") : args.include);
          }

          // Get token from context for authorization
          const sessionToken = tokenStorage.getStore()?.token;
          const headers = { "Accept": "application/gzip" };
          if (sessionToken) {
            headers["Authorization"] = `Bearer ${sessionToken}`;
          } else if (API_KEY) {
            headers["Authorization"] = `Bearer ${API_KEY}`;
          }

          const shardResponse = await fetch(`${API_BASE}/api/v1/backup/knowledge-shard?${shardParams}`, { headers });
          if (!shardResponse.ok) {
            throw new Error(`Shard creation failed: ${shardResponse.status}`);
          }

          const shardArrayBuffer = await shardResponse.arrayBuffer();

          const shardContentDisposition = shardResponse.headers.get('content-disposition');
          const shardFilenameMatch = shardContentDisposition?.match(/filename="([^"]+)"/);
          const shardFilename = shardFilenameMatch ? shardFilenameMatch[1] : `matric-backup-${new Date().toISOString().slice(0,10)}.tar.gz`;

          const shardOutputDir = args.output_dir || os.tmpdir();
          if (!fs.existsSync(shardOutputDir)) {
            fs.mkdirSync(shardOutputDir, { recursive: true });
          }
          const shardOutputPath = path.join(shardOutputDir, shardFilename);
          fs.writeFileSync(shardOutputPath, Buffer.from(shardArrayBuffer));

          result = {
            success: true,
            saved_to: shardOutputPath,
            filename: shardFilename,
            size_bytes: shardArrayBuffer.byteLength,
            size_human: shardArrayBuffer.byteLength > 1024*1024
              ? `${(shardArrayBuffer.byteLength / (1024*1024)).toFixed(2)} MB`
              : `${(shardArrayBuffer.byteLength / 1024).toFixed(2)} KB`,
            content_type: "application/gzip",
            message: `Knowledge shard saved to: ${shardOutputPath}`,
          };
          break;
        }

        case "knowledge_shard_import": {
          const shardPath = args.file_path;
          if (!fs.existsSync(shardPath)) {
            throw new Error(`File not found: ${shardPath}`);
          }
          const shardData = fs.readFileSync(shardPath);
          const shardBase64 = shardData.toString('base64');

          const importBody = {
            shard_base64: shardBase64,
            include: args.include,
            dry_run: args.dry_run || false,
            on_conflict: args.on_conflict || "skip",
            skip_embedding_regen: args.skip_embedding_regen || false,
          };

          result = await apiRequest("POST", "/api/v1/backup/knowledge-shard/import", importBody);
          break;
        }

        case "database_snapshot": {
          // Create a named database snapshot with metadata
          const snapshotBody = {
            name: args.name,
            title: args.title,
            description: args.description,
          };
          result = await apiRequest("POST", "/api/v1/backup/database/snapshot", snapshotBody);
          break;
        }

        case "database_restore": {
          // Restore from a database backup
          const restoreBody = {
            filename: args.filename,
            skip_snapshot: args.skip_snapshot || false,
          };
          result = await apiRequest("POST", "/api/v1/backup/database/restore", restoreBody);
          break;
        }

        case "knowledge_archive_download": {
          const dlHeaders = {};
          const dlToken = tokenStorage.getStore()?.token;
          if (dlToken) {
            dlHeaders["Authorization"] = `Bearer ${dlToken}`;
          } else if (API_KEY) {
            dlHeaders["Authorization"] = `Bearer ${API_KEY}`;
          }
          const response = await fetch(`${API_BASE}/api/v1/backup/knowledge-archive/${encodeURIComponent(args.filename)}`, { headers: dlHeaders });
          if (!response.ok) {
            throw new Error(`Download failed: ${response.status}`);
          }
          const archiveArrayBuffer = await response.arrayBuffer();
          const archiveContentDisposition = response.headers.get('content-disposition');
          const archiveFilenameMatch = archiveContentDisposition?.match(/filename="([^"]+)"/);
          const archiveFilename = archiveFilenameMatch ? archiveFilenameMatch[1] : `${args.filename}.archive`;

          const archiveOutputDir = args.output_dir || os.tmpdir();
          if (!fs.existsSync(archiveOutputDir)) {
            fs.mkdirSync(archiveOutputDir, { recursive: true });
          }
          const archiveOutputPath = path.join(archiveOutputDir, archiveFilename);
          fs.writeFileSync(archiveOutputPath, Buffer.from(archiveArrayBuffer));

          result = {
            success: true,
            saved_to: archiveOutputPath,
            filename: archiveFilename,
            size_bytes: archiveArrayBuffer.byteLength,
            message: `Knowledge archive saved to: ${archiveOutputPath}`,
          };
          break;
        }

        case "knowledge_archive_upload": {
          const archivePath = args.file_path;
          if (!fs.existsSync(archivePath)) {
            throw new Error(`File not found: ${archivePath}`);
          }
          const archiveBuffer = fs.readFileSync(archivePath);
          const uploadFilename = args.filename || path.basename(archivePath);

          const boundary = '----KnowledgeArchiveBoundary' + Date.now();
          const body = Buffer.concat([
            Buffer.from(`--${boundary}\r\n`),
            Buffer.from(`Content-Disposition: form-data; name="file"; filename="${uploadFilename}"\r\n`),
            Buffer.from('Content-Type: application/x-tar\r\n\r\n'),
            archiveBuffer,
            Buffer.from(`\r\n--${boundary}--\r\n`),
          ]);

          const ulHeaders = {};
          const ulToken = tokenStorage.getStore()?.token;
          if (ulToken) {
            ulHeaders["Authorization"] = `Bearer ${ulToken}`;
          } else if (API_KEY) {
            ulHeaders["Authorization"] = `Bearer ${API_KEY}`;
          }
          ulHeaders['Content-Type'] = `multipart/form-data; boundary=${boundary}`;
          ulHeaders['Content-Length'] = body.length.toString();

          const uploadResponse = await fetch(`${API_BASE}/api/v1/backup/knowledge-archive`, {
            method: 'POST',
            headers: ulHeaders,
            body: body,
          });

          if (!uploadResponse.ok) {
            const errorText = await uploadResponse.text();
            throw new Error(`Upload failed: ${uploadResponse.status} - ${errorText}`);
          }
          result = await uploadResponse.json();
          break;
        }

        case "list_backups": {
          // List all backup files
          result = await apiRequest("GET", "/api/v1/backup/list");
          break;
        }

        case "get_backup_info": {
          // Get detailed info about a specific backup
          result = await apiRequest("GET", `/api/v1/backup/list/${encodeURIComponent(args.filename)}`);
          break;
        }

        case "get_backup_metadata": {
          // Get metadata for a backup
          result = await apiRequest("GET", `/api/v1/backup/metadata/${encodeURIComponent(args.filename)}`);
          break;
        }

        case "update_backup_metadata": {
          // Update metadata for a backup
          const metaBody = {
            title: args.title,
            description: args.description,
          };
          result = await apiRequest("PUT", `/api/v1/backup/metadata/${encodeURIComponent(args.filename)}`, metaBody);
          break;
        }

        case "memory_info": {
          // Get detailed memory/storage sizing info
          result = await apiRequest("GET", "/api/v1/memory/info");
          break;
        }

        // ============================================================================
        // SKOS CONCEPT OPERATIONS (Hierarchical Tags)
        // ============================================================================

        case "list_concept_schemes":
          result = await apiRequest("GET", "/api/v1/concepts/schemes");
          break;

        case "create_concept_scheme":
          result = await apiRequest("POST", "/api/v1/concepts/schemes", {
            notation: args.notation,
            title: args.title,
            description: args.description,
            uri: args.uri,
          });
          break;

        case "get_concept_scheme":
          result = await apiRequest("GET", `/api/v1/concepts/schemes/${args.id}`);
          break;

        case "delete_concept_scheme":
          await apiRequest("DELETE", `/api/v1/concepts/schemes/${args.id}${args.force ? "?force=true" : ""}`);
          result = { success: true };
          break;

        case "search_concepts": {
          const conceptParams = new URLSearchParams();
          if (args.q) conceptParams.set("q", args.q);
          if (args.scheme_id) conceptParams.set("scheme_id", args.scheme_id);
          if (args.status) conceptParams.set("status", args.status);
          if (args.top_only) conceptParams.set("top_only", "true");
          if (args.limit !== undefined && args.limit !== null) conceptParams.set("limit", args.limit);
          if (args.offset) conceptParams.set("offset", args.offset);
          result = await apiRequest("GET", `/api/v1/concepts?${conceptParams}`);
          break;
        }

        case "create_concept":
          result = await apiRequest("POST", "/api/v1/concepts", {
            scheme_id: args.scheme_id,
            pref_label: args.pref_label,
            notation: args.notation,
            alt_labels: args.alt_labels || [],
            definition: args.definition,
            scope_note: args.scope_note,
            broader_ids: args.broader_ids || [],
            related_ids: args.related_ids || [],
            facet_type: args.facet_type,
            facet_domain: args.facet_domain,
          });
          break;

        case "get_concept":
          result = await apiRequest("GET", `/api/v1/concepts/${args.id}`);
          break;

        case "get_concept_full":
          result = await apiRequest("GET", `/api/v1/concepts/${args.id}/full`);
          break;

        case "update_concept":
          result = await apiRequest("PATCH", `/api/v1/concepts/${args.id}`, {
            notation: args.notation,
            status: args.status,
            deprecation_reason: args.deprecation_reason,
            replaced_by_id: args.replaced_by_id,
            facet_type: args.facet_type,
          });
          break;

        case "delete_concept":
          await apiRequest("DELETE", `/api/v1/concepts/${args.id}`);
          result = { success: true };
          break;

        case "autocomplete_concepts": {
          const acParams = new URLSearchParams();
          acParams.set("q", args.q);
          if (args.limit !== undefined && args.limit !== null) acParams.set("limit", args.limit);
          result = await apiRequest("GET", `/api/v1/concepts/autocomplete?${acParams}`);
          break;
        }

        case "get_broader":
          result = await apiRequest("GET", `/api/v1/concepts/${args.id}/broader`);
          break;

        case "add_broader":
          result = await apiRequest("POST", `/api/v1/concepts/${args.id}/broader`, {
            target_id: args.target_id,
          });
          break;

        case "get_narrower":
          result = await apiRequest("GET", `/api/v1/concepts/${args.id}/narrower`);
          break;

        case "add_narrower":
          result = await apiRequest("POST", `/api/v1/concepts/${args.id}/narrower`, {
            target_id: args.target_id,
          });
          break;

        case "get_related":
          result = await apiRequest("GET", `/api/v1/concepts/${args.id}/related`);
          break;

        case "add_related":
          result = await apiRequest("POST", `/api/v1/concepts/${args.id}/related`, {
            target_id: args.target_id,
          });
          break;

        case "tag_note_concept":
          result = await apiRequest("POST", `/api/v1/notes/${args.note_id}/concepts`, {
            concept_id: args.concept_id,
            is_primary: args.is_primary || false,
          });
          break;

        case "untag_note_concept":
          await apiRequest("DELETE", `/api/v1/notes/${args.note_id}/concepts/${args.concept_id}`);
          result = { success: true };
          break;

        case "get_note_concepts":
          result = await apiRequest("GET", `/api/v1/notes/${args.note_id}/concepts`);
          break;

        case "get_governance_stats": {
          const govParams = new URLSearchParams();
          if (args.scheme_id) govParams.set("scheme_id", args.scheme_id);
          result = await apiRequest("GET", `/api/v1/concepts/governance?${govParams}`);
          break;
        }

        case "get_top_concepts":
          result = await apiRequest("GET", `/api/v1/concepts/schemes/${args.scheme_id}/top-concepts`);
          break;

        // =======================================================================
        // NOTE VERSIONING (#104)
        // =======================================================================

        case "list_note_versions":
          result = await apiRequest("GET", `/api/v1/notes/${args.note_id}/versions`);
          break;

        case "get_note_version": {
          const versionParams = new URLSearchParams();
          if (args.track) versionParams.set("track", args.track);
          result = await apiRequest(
            "GET",
            `/api/v1/notes/${args.note_id}/versions/${args.version}?${versionParams}`
          );
          break;
        }

        case "restore_note_version":
          result = await apiRequest(
            "POST",
            `/api/v1/notes/${args.note_id}/versions/${args.version}/restore`,
            { restore_tags: args.restore_tags || false }
          );
          break;

        case "delete_note_version":
          await apiRequest(
            "DELETE",
            `/api/v1/notes/${args.note_id}/versions/${args.version}`
          );
          result = { success: true };
          break;

        case "diff_note_versions": {
          const diffParams = new URLSearchParams();
          diffParams.set("from", args.from_version);
          diffParams.set("to", args.to_version);
          // API returns plain text (unified diff format), not JSON
          const sessionToken = tokenStorage.getStore()?.token;
          const diffHeaders = { "Accept": "text/plain" };
          if (sessionToken) {
            diffHeaders["Authorization"] = `Bearer ${sessionToken}`;
          } else if (API_KEY) {
            diffHeaders["Authorization"] = `Bearer ${API_KEY}`;
          }
          const diffResponse = await fetch(
            `${API_BASE}/api/v1/notes/${args.note_id}/versions/diff?${diffParams}`,
            { headers: diffHeaders }
          );
          if (!diffResponse.ok) {
            throw new Error(`Diff failed: ${diffResponse.status}`);
          }
          result = { diff: await diffResponse.text() };
          break;
        }


        // ============================================================================
        // CHUNK-AWARE DOCUMENT HANDLING (Ticket #113)
        // ============================================================================
        case "get_full_document":
          result = await apiRequest("GET", `/api/v1/notes/${args.id}/full`);
          break;

        case "search_with_dedup": {
          const dedupParams = new URLSearchParams({ q: args.query });
          if (args.limit !== undefined && args.limit !== null) dedupParams.set("limit", args.limit);
          if (args.mode) dedupParams.set("mode", args.mode);
          if (args.set) dedupParams.set("set", args.set);
          // Deduplication is enabled by default in the API
          result = await apiRequest("GET", `/api/v1/search?${dedupParams}`);
          break;
        }

        case "get_chunk_chain": {
          const chainParams = new URLSearchParams();
          if (args.include_content !== undefined) {
            chainParams.set("include_content", args.include_content.toString());
          }
          // Note: The API endpoint needs to be implemented as /api/v1/notes/:chain_id/chain
          // For now, we'll use the /full endpoint which provides chunk metadata
          result = await apiRequest("GET", `/api/v1/notes/${args.chain_id}/full?${chainParams}`);
          break;
        }
        case "get_documentation": {
          const topic = args.topic || "overview";
          const content = DOCUMENTATION[topic];
          if (!content) {
            throw new Error(`Unknown documentation topic: ${topic}. Available: ${Object.keys(DOCUMENTATION).join(", ")}`);
          }
          result = { topic, content };
          break;
        }

        // ============================================================================
        // PUBLIC KEY ENCRYPTION (PKE) - Wallet-style encryption via HTTP API
        // All operations use /api/v1/pke/* endpoints (no CLI binary required)
        // ============================================================================
        case "pke_generate_keypair": {
          const apiResult = await apiRequest("POST", "/api/v1/pke/keygen", {
            passphrase: args.passphrase,
            label: args.label || null,
          });
          // Write key files to disk if output_dir specified
          if (args.output_dir) {
            fs.mkdirSync(args.output_dir, { recursive: true });
            fs.writeFileSync(
              path.join(args.output_dir, "public.key"),
              Buffer.from(apiResult.public_key, "base64")
            );
            fs.writeFileSync(
              path.join(args.output_dir, "private.key.enc"),
              Buffer.from(apiResult.encrypted_private_key, "base64")
            );
            fs.writeFileSync(
              path.join(args.output_dir, "address.txt"),
              apiResult.address
            );
          }
          result = {
            address: apiResult.address,
            public_key: apiResult.public_key,
            encrypted_private_key: apiResult.encrypted_private_key,
            label: apiResult.label,
            output_dir: args.output_dir || null,
          };
          break;
        }

        case "pke_get_address": {
          // Accept base64 directly (preferred) or read from filesystem (fallback)
          const pubKeyB64 = args.public_key
            ? args.public_key
            : args.public_key_path
              ? readPublicKeyAsBase64(args.public_key_path)
              : null;
          if (!pubKeyB64) throw new Error("Provide either public_key (base64) or public_key_path");
          const apiResult = await apiRequest("POST", "/api/v1/pke/address", {
            public_key: pubKeyB64,
          });
          result = { address: apiResult.address };
          break;
        }

        case "pke_encrypt": {
          const apiResult = await apiRequest("POST", "/api/v1/pke/encrypt", {
            plaintext: args.plaintext,
            recipients: args.recipient_keys,
            original_filename: args.original_filename || null,
          });
          result = {
            ciphertext: apiResult.ciphertext,
            recipients: apiResult.recipients,
            size_bytes: Buffer.from(apiResult.ciphertext, "base64").length,
          };
          break;
        }

        case "pke_decrypt": {
          const apiResult = await apiRequest("POST", "/api/v1/pke/decrypt", {
            ciphertext: args.ciphertext,
            encrypted_private_key: args.encrypted_private_key,
            passphrase: args.passphrase,
          });
          result = {
            plaintext: apiResult.plaintext,
            original_filename: apiResult.original_filename,
          };
          break;
        }

        case "pke_list_recipients": {
          const apiResult = await apiRequest("POST", "/api/v1/pke/recipients", {
            ciphertext: args.ciphertext,
          });
          result = { recipients: apiResult.recipients };
          break;
        }

        case "pke_verify_address": {
          const apiResult = await apiRequest("GET", `/api/v1/pke/verify/${encodeURIComponent(args.address)}`);
          result = apiResult;
          break;
        }

        // ============================================================================
        // PKE KEYSET MANAGEMENT - Manage named keysets with auto-provisioning
        // ============================================================================
        case "pke_list_keysets": {
          try {
            const keysDir = path.join(os.homedir(), '.matric', 'keys');

            // If directory doesn't exist, return empty array
            if (!fs.existsSync(keysDir)) {
              result = [];
              break;
            }

            // Get all subdirectories
            const entries = fs.readdirSync(keysDir, { withFileTypes: true });
            const keysets = [];

            for (const entry of entries) {
              if (!entry.isDirectory()) continue;

              const keysetDir = path.join(keysDir, entry.name);
              const publicKeyPath = path.join(keysetDir, 'public.key');
              const privateKeyPath = path.join(keysetDir, 'private.key.enc');

              // Verify this is a valid keyset directory
              if (!fs.existsSync(publicKeyPath) || !fs.existsSync(privateKeyPath)) {
                continue;
              }

              // Get address from public key via HTTP API (no CLI binary required)
              let address = null;
              try {
                const pubKeyB64 = readPublicKeyAsBase64(publicKeyPath);
                const addrResult = await apiRequest("POST", "/api/v1/pke/address", {
                  public_key: pubKeyB64,
                });
                address = addrResult.address;
              } catch (e) {
                // Skip if we can't get address
                continue;
              }

              // Get created timestamp from directory
              const stats = fs.statSync(keysetDir);
              const pubKeyB64 = readPublicKeyAsBase64(publicKeyPath);

              keysets.push({
                name: entry.name,
                address,
                public_key: pubKeyB64,
                created: stats.birthtime.toISOString(),
              });
            }

            result = keysets;
          } catch (e) {
            throw new Error(`Failed to list keysets: ${e.message}`);
          }
          break;
        }

        case "pke_create_keyset": {
          try {
            const keysDir = path.join(os.homedir(), '.matric', 'keys');
            const keysetDir = path.join(keysDir, args.name);

            // Validate keyset name
            if (!/^[a-zA-Z0-9_-]+$/.test(args.name)) {
              throw new Error('Keyset name must contain only alphanumeric characters, hyphens, and underscores');
            }

            // Check if keyset already exists
            if (fs.existsSync(keysetDir)) {
              throw new Error(`Keyset '${args.name}' already exists`);
            }

            // Generate keypair via HTTP API (no CLI binary required)
            const keygenResult = await apiRequest("POST", "/api/v1/pke/keygen", {
              passphrase: args.passphrase,
              label: args.name,
            });

            // Create directory and write key files
            fs.mkdirSync(keysetDir, { recursive: true });
            fs.writeFileSync(
              path.join(keysetDir, 'public.key'),
              Buffer.from(keygenResult.public_key, 'base64')
            );
            fs.writeFileSync(
              path.join(keysetDir, 'private.key.enc'),
              Buffer.from(keygenResult.encrypted_private_key, 'base64')
            );

            result = {
              name: args.name,
              address: keygenResult.address,
              public_key: keygenResult.public_key,
              created: new Date().toISOString(),
            };
          } catch (e) {
            throw new Error(`Failed to create keyset: ${e.message}`);
          }
          break;
        }

        case "pke_get_active_keyset": {
          try {
            const keysDir = path.join(os.homedir(), '.matric', 'keys');
            const activeFile = path.join(keysDir, 'active');

            // If no active file, return null
            if (!fs.existsSync(activeFile)) {
              result = null;
              break;
            }

            // Read active keyset name
            const activeKeyset = fs.readFileSync(activeFile, 'utf8').trim();
            if (!activeKeyset) { result = null; break; }

            const keysetDir = path.join(keysDir, activeKeyset);
            const publicKeyPath = path.join(keysetDir, 'public.key');
            const privateKeyPath = path.join(keysetDir, 'private.key.enc');

            // Verify keyset exists
            if (!fs.existsSync(keysetDir) || !fs.existsSync(publicKeyPath) || !fs.existsSync(privateKeyPath)) {
              result = null;
              break;
            }

            // Get address via HTTP API (no CLI binary required)
            const pubKeyB64 = readPublicKeyAsBase64(publicKeyPath);
            const addrResult = await apiRequest("POST", "/api/v1/pke/address", {
              public_key: pubKeyB64,
            });

            // Get created timestamp
            const stats = fs.statSync(keysetDir);

            result = {
              name: activeKeyset,
              address: addrResult.address,
              public_key: pubKeyB64,
              created: stats.birthtime.toISOString(),
            };
          } catch (e) {
            throw new Error(`Failed to get active keyset: ${e.message}`);
          }
          break;
        }

        case "pke_set_active_keyset": {
          try {
            const keysDir = path.join(os.homedir(), '.matric', 'keys');
            const keysetDir = path.join(keysDir, args.name);
            const publicKeyPath = path.join(keysetDir, 'public.key');
            const privateKeyPath = path.join(keysetDir, 'private.key.enc');

            // Verify keyset exists
            if (!fs.existsSync(keysetDir) || !fs.existsSync(publicKeyPath) || !fs.existsSync(privateKeyPath)) {
              throw new Error(`Keyset '${args.name}' not found`);
            }

            // Ensure keys directory exists
            if (!fs.existsSync(keysDir)) {
              fs.mkdirSync(keysDir, { recursive: true });
            }

            // Write active file
            const activeFile = path.join(keysDir, 'active');
            fs.writeFileSync(activeFile, args.name, 'utf8');

            result = {
              success: true,
              active_keyset: args.name,
            };
          } catch (e) {
            throw new Error(`Failed to set active keyset: ${e.message}`);
          }
          break;
        }

        case "pke_export_keyset": {
          try {
            const keysDir = path.join(os.homedir(), '.matric', 'keys');
            const keysetDir = path.join(keysDir, args.name);
            const publicKeyPath = path.join(keysetDir, 'public.key');
            const privateKeyPath = path.join(keysetDir, 'private.key.enc');

            // Verify keyset exists
            if (!fs.existsSync(keysetDir) || !fs.existsSync(publicKeyPath) || !fs.existsSync(privateKeyPath)) {
              throw new Error(`Keyset '${args.name}' not found`);
            }

            // Determine export directory (use provided or default to ~/.matric/exports/)
            const exportDir = args.output_dir || path.join(os.homedir(), '.matric', 'exports');
            fs.mkdirSync(exportDir, { recursive: true });

            // Create timestamped export folder
            const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
            const exportName = `${args.name}-${timestamp}`;
            const exportPath = path.join(exportDir, exportName);
            fs.mkdirSync(exportPath, { recursive: true });

            // Copy key files
            const exportedPublicKey = path.join(exportPath, 'public.key');
            const exportedPrivateKey = path.join(exportPath, 'private.key.enc');
            fs.copyFileSync(publicKeyPath, exportedPublicKey);
            fs.copyFileSync(privateKeyPath, exportedPrivateKey);

            // Write metadata
            const metadata = {
              keyset_name: args.name,
              exported_at: new Date().toISOString(),
              files: ['public.key', 'private.key.enc'],
            };
            fs.writeFileSync(path.join(exportPath, 'keyset.json'), JSON.stringify(metadata, null, 2));

            result = {
              success: true,
              keyset_name: args.name,
              export_path: exportPath,
              files: {
                public_key: exportedPublicKey,
                private_key: exportedPrivateKey,
                metadata: path.join(exportPath, 'keyset.json'),
              },
              message: `Keyset '${args.name}' exported to ${exportPath}`,
            };
          } catch (e) {
            throw new Error(`Failed to export keyset: ${e.message}`);
          }
          break;
        }

        case "pke_import_keyset": {
          try {
            // Validate new keyset name
            if (!/^[a-zA-Z0-9_-]+$/.test(args.name)) {
              throw new Error('Keyset name must contain only alphanumeric characters, hyphens, and underscores');
            }

            const keysDir = path.join(os.homedir(), '.matric', 'keys');
            const keysetDir = path.join(keysDir, args.name);

            // Check if keyset already exists
            if (fs.existsSync(keysetDir)) {
              throw new Error(`Keyset '${args.name}' already exists. Choose a different name or delete the existing keyset.`);
            }

            // Determine source paths
            let sourcePublicKey, sourcePrivateKey;

            if (args.import_path) {
              // Import from directory (exported keyset)
              const importDir = args.import_path;
              if (!fs.existsSync(importDir)) {
                throw new Error(`Import path not found: ${importDir}`);
              }

              sourcePublicKey = path.join(importDir, 'public.key');
              sourcePrivateKey = path.join(importDir, 'private.key.enc');

              if (!fs.existsSync(sourcePublicKey) || !fs.existsSync(sourcePrivateKey)) {
                throw new Error(`Invalid keyset directory. Expected public.key and private.key.enc in ${importDir}`);
              }
            } else if (args.public_key_path && args.private_key_path) {
              // Import from explicit paths
              sourcePublicKey = args.public_key_path;
              sourcePrivateKey = args.private_key_path;

              if (!fs.existsSync(sourcePublicKey)) {
                throw new Error(`Public key not found: ${sourcePublicKey}`);
              }
              if (!fs.existsSync(sourcePrivateKey)) {
                throw new Error(`Private key not found: ${sourcePrivateKey}`);
              }
            } else {
              throw new Error('Must provide either import_path (directory) or both public_key_path and private_key_path');
            }

            // Create keyset directory
            fs.mkdirSync(keysetDir, { recursive: true });

            // Copy key files
            const destPublicKey = path.join(keysetDir, 'public.key');
            const destPrivateKey = path.join(keysetDir, 'private.key.enc');
            fs.copyFileSync(sourcePublicKey, destPublicKey);
            fs.copyFileSync(sourcePrivateKey, destPrivateKey);

            // Get address from imported public key via HTTP API (no CLI binary required)
            const importedPubKeyB64 = readPublicKeyAsBase64(destPublicKey);
            const addrResult = await apiRequest("POST", "/api/v1/pke/address", {
              public_key: importedPubKeyB64,
            });

            result = {
              success: true,
              keyset_name: args.name,
              address: addrResult.address,
              public_key: importedPubKeyB64,
              message: `Keyset imported as '${args.name}'`,
            };
          } catch (e) {
            throw new Error(`Failed to import keyset: ${e.message}`);
          }
          break;
        }

        case "pke_delete_keyset": {
          try {
            const keysDir = path.join(os.homedir(), '.matric', 'keys');
            const keysetDir = path.join(keysDir, args.name);

            // Verify keyset exists
            if (!fs.existsSync(keysetDir)) {
              throw new Error(`Keyset '${args.name}' not found`);
            }

            // Check if this is the active keyset
            const activeFile = path.join(keysDir, 'active');
            if (fs.existsSync(activeFile)) {
              const activeKeyset = fs.readFileSync(activeFile, 'utf8').trim();
              if (activeKeyset === args.name) {
                // Clear the active file
                fs.writeFileSync(activeFile, '', 'utf8');
              }
            }

            // Delete the keyset directory
            fs.rmSync(keysetDir, { recursive: true, force: true });

            result = {
              success: true,
              deleted_keyset: args.name,
              message: `Keyset '${args.name}' has been deleted`,
            };
          } catch (e) {
            throw new Error(`Failed to delete keyset: ${e.message}`);
          }
          break;
        }

        // ============================================================================
        // DOCUMENT TYPES - Document type management
        // ============================================================================
        case "list_document_types": {
          const params = new URLSearchParams();
          if (args.category) params.set("category", args.category);
          const queryString = params.toString();
          const path = queryString ? `/api/v1/document-types?${queryString}` : "/api/v1/document-types";
          const apiResult = await apiRequest("GET", path);
          
          // Transform response based on detail parameter (default: false)
          if (args.detail === true) {
            // Return full response with all document type details
            result = apiResult;
          } else {
            // Return only names array (default behavior)
            if (apiResult && apiResult.types && Array.isArray(apiResult.types)) {
              result = apiResult.types.map(t => t.name);
            } else {
              result = apiResult;
            }
          }
          break;
        }

        case "get_document_type": {
          result = await apiRequest("GET", `/api/v1/document-types/${encodeURIComponent(args.name)}`);
          break;
        }

        case "create_document_type": {
          result = await apiRequest("POST", "/api/v1/document-types", args);
          break;
        }

        case "update_document_type": {
          const { name: typeName, ...updates } = args;
          result = await apiRequest("PATCH", `/api/v1/document-types/${encodeURIComponent(typeName)}`, updates);
          break;
        }

        case "delete_document_type": {
          await apiRequest("DELETE", `/api/v1/document-types/${encodeURIComponent(args.name)}`);
          result = { success: true, deleted: args.name };
          break;
        }

        case "detect_document_type": {
          result = await apiRequest("POST", "/api/v1/document-types/detect", args);
          break;
        }

        // Archive Management
        case "list_archives":
          result = await apiRequest("GET", "/api/v1/archives");
          break;

        case "create_archive":
          result = await apiRequest("POST", "/api/v1/archives", args);
          break;

        case "get_archive":
          result = await apiRequest("GET", `/api/v1/archives/${args.name}`);
          break;

        case "update_archive": {
          await apiRequest("PATCH", `/api/v1/archives/${args.name}`, {
            description: args.description
          });
          result = { success: true, updated: args.name };
          break;
        }

        case "delete_archive": {
          await apiRequest("DELETE", `/api/v1/archives/${args.name}`);
          result = { success: true, deleted: args.name };
          break;
        }

        case "set_default_archive": {
          await apiRequest("POST", `/api/v1/archives/${args.name}/set-default`);
          result = { success: true, default_archive: args.name };
          break;
        }

        case "get_archive_stats":
          result = await apiRequest("GET", `/api/v1/archives/${args.name}/stats`);
          break;

        // ============================================================================
        // SKOS COLLECTIONS (#450) - Grouped concept management
        // ============================================================================
        case "list_skos_collections": {
          const params = new URLSearchParams();
          if (args.scheme_id) params.set("scheme_id", args.scheme_id);
          if (args.limit !== undefined && args.limit !== null) params.set("limit", args.limit);
          if (args.offset) params.set("offset", args.offset);
          result = await apiRequest("GET", `/api/v1/concepts/collections?${params}`);
          break;
        }

        case "create_skos_collection":
          result = await apiRequest("POST", "/api/v1/concepts/collections", {
            scheme_id: args.scheme_id,
            pref_label: args.pref_label,
            notation: args.notation,
            definition: args.definition,
            is_ordered: args.ordered || false,
          });
          break;

        case "get_skos_collection":
          result = await apiRequest("GET", `/api/v1/concepts/collections/${args.id}`);
          break;

        case "update_skos_collection": {
          const body = {};
          if (args.pref_label !== undefined) body.pref_label = args.pref_label;
          if (args.notation !== undefined) body.notation = args.notation;
          if (args.definition !== undefined) body.definition = args.definition;
          if (args.ordered !== undefined) body.is_ordered = args.ordered;
          result = await apiRequest("PATCH", `/api/v1/concepts/collections/${args.id}`, body);
          break;
        }

        case "delete_skos_collection":
          await apiRequest("DELETE", `/api/v1/concepts/collections/${args.id}`);
          result = { success: true };
          break;

        case "add_skos_collection_member":
          result = await apiRequest("POST", `/api/v1/concepts/collections/${args.id}/members/${args.concept_id}`, {
            position: args.position,
          });
          break;

        case "remove_skos_collection_member":
          await apiRequest("DELETE", `/api/v1/concepts/collections/${args.id}/members/${args.concept_id}`);
          result = { success: true };
          break;

        // ============================================================================
        // SKOS RELATION REMOVAL (#451) - Remove semantic relations
        // ============================================================================
        case "remove_broader":
          await apiRequest("DELETE", `/api/v1/concepts/${args.id}/broader/${args.target_id}`);
          result = { success: true };
          break;

        case "remove_narrower":
          await apiRequest("DELETE", `/api/v1/concepts/${args.id}/narrower/${args.target_id}`);
          result = { success: true };
          break;

        case "remove_related":
          await apiRequest("DELETE", `/api/v1/concepts/${args.id}/related/${args.target_id}`);
          result = { success: true };
          break;

        // ============================================================================
        // KNOWLEDGE HEALTH (#452) - Knowledge base health monitoring
        // ============================================================================
        case "get_knowledge_health":
          result = await apiRequest("GET", "/api/v1/health/knowledge");
          break;

        case "get_orphan_tags":
          result = await apiRequest("GET", "/api/v1/health/orphan-tags");
          break;

        case "get_stale_notes": {
          const params = new URLSearchParams();
          if (args.days) params.set("stale_days", args.days);
          if (args.limit !== undefined && args.limit !== null) params.set("limit", args.limit);
          result = await apiRequest("GET", `/api/v1/health/stale-notes?${params}`);
          break;
        }

        case "get_unlinked_notes": {
          const params = new URLSearchParams();
          if (args.limit !== undefined && args.limit !== null) params.set("limit", args.limit);
          result = await apiRequest("GET", `/api/v1/health/unlinked-notes?${params}`);
          break;
        }

        case "get_tag_cooccurrence": {
          const params = new URLSearchParams();
          if (args.min_count) params.set("min_count", args.min_count);
          if (args.limit !== undefined && args.limit !== null) params.set("limit", args.limit);
          result = await apiRequest("GET", `/api/v1/health/tag-cooccurrence?${params}`);
          break;
        }

        // ============================================================================
        // NOTE PROVENANCE & BACKLINKS (#453)
        // ============================================================================
        case "get_note_backlinks":
          result = await apiRequest("GET", `/api/v1/notes/${args.id}/backlinks`);
          break;

        case "get_note_provenance":
          result = await apiRequest("GET", `/api/v1/notes/${args.id}/provenance`);

          break;
        case "get_memory_provenance":
          result = await apiRequest("GET", `/api/v1/notes/${args.note_id}/memory-provenance`);
          break;

        // ============================================================================
        // JOB MANAGEMENT (#454)
        // ============================================================================
        case "get_job":
          result = await apiRequest("GET", `/api/v1/jobs/${args.id}`);
          break;

        case "get_pending_jobs_count":
          result = await apiRequest("GET", "/api/v1/jobs/pending");
          break;

        // ============================================================================
        // NOTE REPROCESS (#455)
        // ============================================================================
        case "reprocess_note":
          result = await apiRequest("POST", `/api/v1/notes/${args.id}/reprocess`, {
            steps: args.steps,
            force: args.force || false,
          });
          break;

        // ============================================================================
        // TIMELINE & ACTIVITY (#456)
        // ============================================================================
        case "get_notes_timeline": {
          const params = new URLSearchParams();
          if (args.granularity) params.set("granularity", args.granularity);
          if (args.start_date) params.set("start_date", args.start_date);
          if (args.end_date) params.set("end_date", args.end_date);
          result = await apiRequest("GET", `/api/v1/notes/timeline?${params}`);
          break;
        }

        case "get_notes_activity": {
          const params = new URLSearchParams();
          if (args.limit !== undefined && args.limit !== null) params.set("limit", args.limit);
          if (args.offset) params.set("offset", args.offset);
          if (args.event_types) params.set("event_types", args.event_types.join(","));
          result = await apiRequest("GET", `/api/v1/notes/activity?${params}`);
          break;
        }

        // ============================================================================
        // EMBEDDING CONFIG MANAGEMENT (#457)
        // ============================================================================
        case "list_embedding_configs":
          result = await apiRequest("GET", "/api/v1/embedding-configs");
          break;

        case "get_default_embedding_config":
          result = await apiRequest("GET", "/api/v1/embedding-configs/default");
          break;

        case "get_embedding_config":
          result = await apiRequest("GET", `/api/v1/embedding-configs/${args.id}`);
          break;

        case "create_embedding_config":
          result = await apiRequest("POST", "/api/v1/embedding-configs", {
            name: args.name,
            model: args.model,
            dimension: args.dimension,
            provider: args.provider,
            is_default: args.is_default || false,
            chunk_size: args.chunk_size,
            chunk_overlap: args.chunk_overlap,
          });
          break;

        case "update_embedding_config": {
          const body = {};
          if (args.name !== undefined) body.name = args.name;
          if (args.model !== undefined) body.model = args.model;
          if (args.dimension !== undefined) body.dimension = args.dimension;
          if (args.provider !== undefined) body.provider = args.provider;
          if (args.is_default !== undefined) body.is_default = args.is_default;
          if (args.chunk_size !== undefined) body.chunk_size = args.chunk_size;
          if (args.chunk_overlap !== undefined) body.chunk_overlap = args.chunk_overlap;
          result = await apiRequest("PATCH", `/api/v1/embedding-configs/${args.id}`, body);
          break;
        }

        case "delete_embedding_config":
          await apiRequest("DELETE", `/api/v1/embedding-configs/${args.id}`);
          result = { success: true };
          break;

        // ============================================================================
        // SKOS TURTLE EXPORT (#460)
        // ============================================================================
        case "export_skos_turtle": {
          // Fetch as text since this returns Turtle format, not JSON
          const sessionToken = tokenStorage.getStore()?.token;
          const turtleHeaders = { "Accept": "text/turtle" };
          if (sessionToken) {
            turtleHeaders["Authorization"] = `Bearer ${sessionToken}`;
          } else if (API_KEY) {
            turtleHeaders["Authorization"] = `Bearer ${API_KEY}`;
          }
          // If scheme_id provided, export single scheme; otherwise export all
          const turtleUrl = args.scheme_id
            ? `${API_BASE}/api/v1/concepts/schemes/${args.scheme_id}/export/turtle`
            : `${API_BASE}/api/v1/concepts/schemes/export/turtle`;
          const turtleResponse = await fetch(turtleUrl, { headers: turtleHeaders });
          if (!turtleResponse.ok) {
            throw new Error(`Turtle export failed: ${turtleResponse.status}`);
          }
          result = { turtle: await turtleResponse.text() };
          break;
        }


        // ============================================================================
        // FILE ATTACHMENTS (#14)
        // ============================================================================
        case "upload_attachment": {
          const uploadUrl = `${API_BASE}/api/v1/notes/${args.note_id}/attachments/upload`;
          const filename = args.filename || "FILE_PATH";
          const curlParts = [`curl -X POST`];
          curlParts.push(`-F "file=@${filename}"`);
          if (args.document_type_id) {
            curlParts.push(`-F "document_type_id=${args.document_type_id}"`);
          }
          if (args.content_type) {
            curlParts.push(`-F "file=@${filename};type=${args.content_type}"`);
            // Replace the first -F with the typed version
            curlParts.splice(1, 1);
          }

          // Add auth header if available
          const sessionToken = tokenStorage.getStore()?.token;
          const authToken = sessionToken || API_KEY;
          if (authToken) {
            curlParts.push(`-H "Authorization: Bearer ${authToken}"`);
          }

          // Add memory header if set
          const sid = tokenStorage.getStore()?.sessionId;
          const activeMem = sid ? sessionMemories.get(sid) : null;
          if (activeMem) {
            curlParts.push(`-H "X-Fortemi-Memory: ${activeMem}"`);
          }

          curlParts.push(`"${uploadUrl}"`);

          result = {
            upload_url: uploadUrl,
            method: "POST",
            content_type: "multipart/form-data",
            max_size: `${Math.round(MAX_UPLOAD_SIZE / (1024 * 1024))}MB`,
            curl_command: curlParts.join(" \\\n  "),
            instructions: "Execute the curl command to upload the file. Replace the filename with the actual file path. " +
              "The API accepts multipart/form-data — no base64 encoding needed. " +
              "The response will contain the attachment metadata (id, filename, status, etc.).",
          };
          if (args.filename) {
            result.filename_hint = args.filename;
          }
          break;
        }

        case "list_attachments":
          result = await apiRequest("GET", `/api/v1/notes/${args.note_id}/attachments`);
          break;

        case "get_attachment":
          result = await apiRequest("GET", `/api/v1/attachments/${args.id}`);
          if (result && result.id) {
            result._api_urls = {
              download: `${API_BASE}/api/v1/attachments/${result.id}/download`,
              download_curl: `curl -o "${result.filename || result.original_filename || `attachment-${result.id}`}" "${API_BASE}/api/v1/attachments/${result.id}/download"`,
            };
          }
          break;

        case "download_attachment": {
          const meta = await apiRequest("GET", `/api/v1/attachments/${args.id}`);
          const downloadUrl = `${API_BASE}/api/v1/attachments/${args.id}/download`;
          const outputFilename = meta?.filename || meta?.original_filename || `attachment-${args.id}`;

          result = {
            filename: outputFilename,
            size_bytes: meta?.size_bytes,
            content_type: meta?.content_type,
            download_url: downloadUrl,
            curl_command: `curl -o "${outputFilename}" "${downloadUrl}"`,
            instructions: "Execute the curl_command above (or equivalent HTTP GET) to download the file.",
          };
          break;
        }

        case "delete_attachment":
          await apiRequest("DELETE", `/api/v1/attachments/${args.id}`);
          result = { success: true };
          break;
        // ============================================================================
        // MEMORY MANAGEMENT TOOLS
        // ============================================================================
        case "select_memory": {
          // Store the active memory for this session
          const store = tokenStorage.getStore();
          const sessionId = store?.sessionId;
          if (sessionId) {
            sessionMemories.set(sessionId, args.name);
            result = {
              success: true,
              message: `Active memory set to: ${args.name}`,
              active_memory: args.name
            };
          } else {
            throw new Error("Session context not available - memory selection requires HTTP transport");
          }
          break;
        }

        case "get_active_memory": {
          const store = tokenStorage.getStore();
          const sessionId = store?.sessionId;
          const activeMemory = sessionId ? sessionMemories.get(sessionId) : null;
          result = {
            active_memory: activeMemory || "public (default)",
            is_explicit: !!activeMemory
          };
          break;
        }

        case "list_memories": {
          result = await apiRequest("GET", "/api/v1/archives");
          break;
        }

        case "create_memory": {
          const body = { name: args.name };
          if (args.description) {
            body.description = args.description;
          }
          result = await apiRequest("POST", "/api/v1/archives", body);
          break;
        }

        case "delete_memory": {
          result = await apiRequest("DELETE", `/api/v1/archives/${encodeURIComponent(args.name)}`);
          break;
        }

        case "clone_memory": {
          const body = { new_name: args.new_name };
          if (args.description) {
            body.description = args.description;
          }
          result = await apiRequest("POST", `/api/v1/archives/${encodeURIComponent(args.source_name)}/clone`, body);
          break;
        }

        case "get_memories_overview": {
          // Overview includes database_size_bytes (pg_database_size) which covers
          // ALL data on disk: all schemas, tables, indexes, attachment blobs, etc.
          result = await apiRequest("GET", "/api/v1/memories/overview");
          break;
        }

        case "search_memories_federated": {
          const body = { q: args.q, memories: args.memories };
          if (args.limit) {
            body.limit = args.limit;
          }
          result = await apiRequest("POST", "/api/v1/search/federated", body);
          break;
        }

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

  return mcpServer;
}

// Tool definitions imported from tools.js
// Run `npm run validate:schemas` to check all schemas before committing.

// Documentation content for get_documentation tool
const DOCUMENTATION = {
  overview: `# Matric Memory Overview

Matric Memory is an AI-enhanced knowledge base with semantic search, automatic linking, and NLP pipelines.

## Core Capabilities

1. **AI-Enhanced Notes**
   - Full revision mode: Contextual expansion using related notes
   - Light revision mode: Formatting without invented details
   - Automatic title generation
   - Semantic link creation

2. **Hybrid Search**
   - Full-text search (exact keywords, operators: OR, NOT, phrase)
   - Semantic search (conceptual similarity via embeddings)
   - Hybrid mode (combined RRF ranking)
   - Multilingual: English, German, French, Spanish, Portuguese, Russian, CJK, Arabic
   - Emoji and symbol search via trigram matching
   - Embedding sets for focused search contexts

3. **Knowledge Graph**
   - Automatic semantic linking (>70% similarity)
   - Bidirectional backlinks (\`get_note_links\`, \`get_note_backlinks\`)
   - Graph exploration with \`explore_graph\`
   - W3C PROV provenance tracking (\`get_note_provenance\`)

4. **SKOS Hierarchical Tags**
   - W3C compliant concept schemes and collections
   - Broader/narrower/related relations
   - Governance workflows and anti-pattern detection
   - RDF/Turtle export (\`export_skos_turtle\`)

5. **Organization**
   - Collections (nested folders)
   - Archives (named containers with lifecycle)
   - Templates with variable substitution
   - Embedding sets for domain isolation
   - Document type registry (131+ types with auto-detection)
   - Version history (dual-track: original + revision)

6. **Observability**
   - Knowledge health metrics (\`get_knowledge_health\`)
   - Orphan tag detection, stale note finding, unlinked note discovery
   - Tag co-occurrence analysis
   - Timeline and activity feed
   - Background job monitoring

## Quick Start

1. Create notes with \`create_note\` - choose appropriate revision_mode
2. Search with \`search_notes\` - use mode="semantic" for conceptual search
3. Explore links with \`get_note_links\` - backlinks show what references your note
4. Build hierarchy with SKOS concepts for structured tagging

## Storage & Capacity Planning

Use \`memory_info\` to understand storage and plan hardware:

**Storage Metrics:**
- \`total_notes\`, \`total_embeddings\`, \`total_links\`, \`total_collections\`, \`total_tags\`, \`total_templates\`
- \`database_total_bytes\`, \`embedding_table_bytes\`, \`notes_table_bytes\`
- \`estimated_memory_for_search\` - RAM needed for vector operations

**Hardware Recommendations:**
- \`min_ram_gb\` - Minimum RAM for basic operation
- \`recommended_ram_gb\` - Recommended RAM for optimal performance

**GPU vs CPU:**
- Ollama (embedding generation): Benefits from GPU for faster processing
- pgvector (search): CPU-bound, benefits from RAM and fast SSD
- For most deployments, prioritize RAM over GPU

## Knowledge Graph

The knowledge graph automatically connects related notes:

**How It Works:**
1. Each note generates vector embeddings
2. System computes cosine similarity with all other notes
3. Notes with >70% similarity get bidirectional links
4. Links update when notes are modified

**Using \`get_note_links\`:**
\`\`\`
get_note_links({ id: "note-uuid" })
// Returns: { incoming: [...], outgoing: [...] }
// incoming = notes that link TO this note (backlinks)
// outgoing = notes this note links TO
\`\`\`

**Using \`explore_graph\`:**
\`\`\`
explore_graph({ id: "note-uuid", depth: 2, max_nodes: 50 })
// Returns: { nodes: [...], edges: [...] }
// Traverses links up to N hops from starting note
\`\`\`

## Tool Categories

### Read-Only Tools (Safe)
- **Search**: \`search_notes\`, \`search_with_dedup\`, \`list_notes\`, \`list_tags\`
- **Retrieval**: \`get_note\`, \`get_note_links\`, \`get_note_backlinks\`, \`get_full_document\`, \`get_chunk_chain\`
- **Provenance**: \`get_note_provenance\`, \`get_notes_timeline\`, \`get_notes_activity\`
- **SKOS**: \`search_concepts\`, \`get_concept\`, \`get_concept_full\`, \`autocomplete_concepts\`, \`get_governance_stats\`, \`export_skos_turtle\`
- **Versioning**: \`list_note_versions\`, \`get_note_version\`, \`diff_note_versions\`
- **Health**: \`get_knowledge_health\`, \`get_orphan_tags\`, \`get_stale_notes\`, \`get_unlinked_notes\`, \`get_tag_cooccurrence\`
- **System**: \`health_check\`, \`get_system_info\`, \`memory_info\`, \`list_jobs\`, \`get_job\`, \`get_queue_stats\`, \`get_pending_jobs_count\`
- **Config**: \`list_embedding_configs\`, \`get_embedding_config\`, \`get_default_embedding_config\`, \`list_document_types\`, \`get_document_type\`, \`detect_document_type\`
- **Export**: \`export_note\`, \`export_all_notes\`, \`list_backups\`, \`get_backup_info\`, \`get_backup_metadata\`

### Mutating Tools (Require Permission)
- **Notes**: \`create_note\`, \`update_note\`, \`set_note_tags\`, \`bulk_create_notes\`, \`reprocess_note\`
- **Collections**: \`create_collection\`, \`update_collection\`, \`move_note_to_collection\`
- **Archives**: \`create_archive\`, \`update_archive\`, \`set_default_archive\`
- **SKOS**: \`create_concept\`, \`update_concept\`, \`add_broader\`, \`add_narrower\`, \`add_related\`, \`tag_note_concept\`
- **SKOS Collections**: \`create_skos_collection\`, \`add_skos_collection_member\`
- **Templates**: \`create_template\`, \`update_template\`, \`instantiate_template\`
- **Embedding**: \`create_embedding_set\`, \`update_embedding_set\`, \`add_set_members\`, \`create_embedding_config\`
- **Backup**: \`backup_now\`, \`knowledge_shard\`, \`knowledge_shard_import\`, \`database_snapshot\`

### Destructive Tools (Usually Restricted)
- \`delete_note\`, \`restore_note\`, \`purge_note\`, \`purge_notes\`, \`purge_all_notes\`
- \`delete_collection\`, \`delete_concept\`, \`delete_archive\`
- \`delete_embedding_set\`, \`delete_document_type\`, \`delete_skos_collection\`
- \`remove_broader\`, \`remove_narrower\`, \`remove_related\`
- \`reembed_all\` (regenerates all embeddings)
- \`database_restore\` (overwrites entire database)`,

  notes: `# Notes: Creation and Management

## Revision Modes

| Mode | When to Use | Behavior |
|------|-------------|----------|
| \`full\` (default) | Technical concepts, research | Full contextual expansion |
| \`light\` | Facts, quick thoughts | Formatting only |
| \`none\` | Exact quotes, data imports | No AI processing |

## Title Generation

Title is extracted automatically **regardless of revision_mode**:
1. **H1 present**: Extracted from first \`# Heading\`
2. **No H1**: Generated from first line (truncated)
3. **Empty content**: Defaults to "Untitled Note"

## Return Formats

### get_note returns:
\`\`\`
{
  id, title,
  original_content,  // User-provided content
  revised_content,   // AI-enhanced (null if revision_mode=none)
  tags: [{ path, is_ai_generated }],
  links: [{ id, to_note_id, kind, score }],
  created_at_utc, updated_at_utc, starred, archived
}
\`\`\`

### list_notes returns:
\`\`\`
{
  notes: [{ id, title, snippet, tags, starred, archived, created_at_utc }],
  total
}
\`\`\`
**Note**: \`snippet\` is first ~200 chars of content, not a summary.

## Note Lifecycle

- **Create**: \`create_note\` → version 1 created
- **Update**: \`update_note\` → new version created
- **Soft Delete**: \`delete_note\` → marked deleted, recoverable
- **Restore**: \`restore_note\` → recovers soft-deleted note
- **Hard Delete**: Not available via MCP (admin only)

## Processing Pipeline

After create_note/update_note:
1. AI Revision (if mode != none)
2. Embedding generation
3. Title extraction
4. Semantic link creation

**Jobs are asynchronous** - use \`list_jobs\` to check progress.

## Best Practices

- Use H1 headings for consistent titles
- Match revision_mode to content type
- Use soft delete for normal workflows
- Check \`list_jobs\` before searching new content`,

  search: `# Search: Finding Knowledge

## Search Modes

| Mode | Best For | How It Works |
|------|----------|--------------|
| \`hybrid\` (default) | General search | Combines keyword + semantic via RRF ranking |
| \`fts\` | Exact matching | Full-text search with operators (OR, NOT, phrase) |
| \`semantic\` | Conceptual search | Vector similarity, finds related concepts |

## Query Syntax (FTS Mode)

\`\`\`
hello world          # Match all words (AND)
apple OR orange      # Match either word
apple -orange        # Exclude word (NOT)
"hello world"        # Exact phrase match
\`\`\`

## Multilingual Search

The system automatically detects query language and routes to the appropriate search strategy:

| Language | Support | Strategy |
|----------|---------|----------|
| English, German, French, Spanish, Portuguese, Russian | Full stemming | Language-specific FTS config |
| Chinese, Japanese, Korean | Bigram tokenization | pg_bigm character matching |
| Arabic, Greek, Hebrew | Basic tokenization | Standard FTS |
| Emoji, symbols (🚀, ∑) | Trigram matching | pg_trgm substring search |

**Accent folding**: Searching "cafe" finds "café", "naive" finds "naïve".

## Embedding Sets

Create focused search contexts:

\`\`\`javascript
// Create a focused set
create_embedding_set({
  name: "AI Research",
  slug: "ai-research",
  purpose: "AI/ML research papers and notes",
  mode: "auto",
  criteria: { tags: ["ai", "ml", "research"] }
})

// Search within the set
search_notes({
  query: "transformer attention mechanisms",
  mode: "semantic",
  set: "ai-research"
})
\`\`\`

## Chunk-Aware Search

For large documents split into chunks:
\`\`\`
// Search with deduplication (one result per document)
search_with_dedup({ query: "neural networks", mode: "hybrid" })
// Returns unique documents, not individual chunks
\`\`\`

## Search Tips

1. **Start broad, then narrow**
   - Begin with hybrid mode
   - Switch to semantic if keywords don't match but concept does
   - Use embedding sets to restrict domain
   - Filter by tags: \`search_notes({ query: "...", tags: ["ml"] })\`

2. **Leverage backlinks**
   - After finding a relevant note, check its links
   - \`get_note_backlinks\` shows what references a note
   - Backlinks often reveal related content you didn't think to search for

3. **Wait for embeddings**
   - Newly created notes need embedding generation
   - Check \`get_pending_jobs_count\` before searching for fresh content

4. **Multilingual queries**
   - CJK: Use 2+ characters for best results
   - Emoji: Search directly with emoji characters
   - Accented text: Unaccented queries match accented content`,

  concepts: `# SKOS Hierarchical Tagging

W3C SKOS-compliant concept taxonomy system for organizing knowledge with semantic relationships.

## Key Concepts

- **Concept Scheme**: A vocabulary/namespace identified by UUID (use \`list_concept_schemes\` to get IDs)
- **Concept**: A tag with semantic meaning, labels, and status
- **Relations**: broader (parent), narrower (child), related (associative)

## Concept Status

| Status | Meaning | Use Case |
|--------|---------|----------|
| \`candidate\` | Auto-created from hashtags, needs review | Initial import, user tags |
| \`approved\` | Reviewed and approved for use | Production vocabulary |
| \`deprecated\` | Replaced by newer concept | Legacy terms |
| \`obsolete\` | No longer valid, retained for history | Archived terms |

**Lifecycle**: candidate → approved → deprecated → obsolete

## Scheme Management

**Important**: \`scheme_id\` must be a valid UUID, not a string like "main".

\`\`\`
// List schemes to get UUIDs
list_concept_schemes()
// Returns: [{ id: "550e8400-...", label: "Main" }, ...]

// Create new scheme
create_concept_scheme({ label: "Projects", description: "Project taxonomy" })

// Get scheme details
get_concept_scheme({ scheme_id: "550e8400-..." })
\`\`\`

## Working with Concepts

\`\`\`
// Search concepts (q is optional - omit to list all)
search_concepts({ scheme_id: "uuid", q: "machine", status: ["approved"] })

// Get concept with all relations
get_concept_full({ concept_id: "uuid" })
// Returns: concept + labels + broader + narrower + related

// Get top-level concepts (no parents)
get_top_concepts({ scheme_id: "uuid" })

// Autocomplete for UIs
autocomplete_concepts({ scheme_id: "uuid", prefix: "mach", limit: 10 })
\`\`\`

## Creating and Updating Concepts

\`\`\`
// Create with hierarchy
create_concept({
  scheme_id: "550e8400-...",  // UUID required
  pref_label: "Machine Learning",
  alt_labels: ["ML", "Statistical Learning"],
  definition: "A field of AI...",
  broader: ["parent-concept-uuid"],
  status: "approved"
})

// Update concept (including labels)
update_concept({
  concept_id: "uuid",
  pref_label: "Updated Label",
  status: "deprecated",
  replaced_by: "new-concept-uuid"  // For deprecation
})
\`\`\`

## Tagging Notes with Concepts

\`\`\`
// Tag note with concept
tag_note_concept({ note_id: "uuid", concept_id: "uuid" })

// Remove concept from note
untag_note_concept({ note_id: "uuid", concept_id: "uuid" })

// List note's concepts
get_note_concepts({ note_id: "uuid" })
\`\`\`

## Governance Workflow

\`\`\`
// Get taxonomy health stats
get_governance_stats({ scheme_id: "uuid" })
// Returns: { candidate: 12, approved: 45, deprecated: 3, orphans: 2 }

// Review candidates
search_concepts({ scheme_id: "uuid", status: ["candidate"] })

// Approve concept
update_concept({ concept_id: "uuid", status: "approved" })

// Deprecate with replacement
update_concept({
  concept_id: "old-uuid",
  status: "deprecated",
  replaced_by: "new-uuid"
})
\`\`\`

## list_tags vs SKOS Concepts

- **\`list_tags\`**: Simple string tags from hashtags - fast, flat, no hierarchy
- **SKOS Concepts**: Rich vocabulary with hierarchy, status, and relations

Both coexist. Notes can have both inline hashtags AND SKOS concept associations.

## Removing Relations

\`\`\`
// Remove broader (parent) relation
remove_broader({ concept_id: "child-uuid", broader_id: "parent-uuid" })

// Remove narrower (child) relation
remove_narrower({ concept_id: "parent-uuid", narrower_id: "child-uuid" })

// Remove related (associative) relation
remove_related({ concept_id: "uuid-a", related_id: "uuid-b" })
\`\`\`

## SKOS Collections & Export

For concept groupings (cross-hierarchy), see \`get_documentation({ topic: "skos_collections" })\`.
For RDF/Turtle export, use \`export_skos_turtle({ scheme_id: "uuid" })\`.

## Best Practices

1. **Use UUIDs for scheme_id** - Never hardcode strings like "main"
2. **Define concepts clearly** - Add definition and alt_labels
3. **Review candidates regularly** - Use \`get_governance_stats\`
4. **Deprecate, don't delete** - Preserve history with replacement links
5. **Build shallow hierarchies** - 3-4 levels max for usability
6. **Use collections for cross-cutting groups** - See \`skos_collections\` topic`,

  chunking: `# Document Chunking

The system splits documents into chunks for optimal embedding quality.

## Chunking Strategies

| Strategy | Best For |
|----------|----------|
| \`SemanticChunker\` | Markdown docs, technical content (recommended) |
| \`ParagraphChunker\` | Blog posts, structured content |
| \`SentenceChunker\` | Narrative, prose |
| \`SlidingWindowChunker\` | Dense text, consistent sizes |
| \`RecursiveChunker\` | Mixed/unknown content |

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| \`max_chunk_size\` | 1000 | Maximum characters per chunk |
| \`min_chunk_size\` | 100 | Minimum size (smaller merged) |
| \`overlap\` | 100 | Overlap between chunks |

## Impact on Search

- **Too small chunks**: Lost context, fragmented concepts
- **Too large chunks**: Mixed concepts, noisy embeddings
- **Good chunking**: Coherent semantic units, accurate retrieval

## Writing for Good Chunks

1. Use clear headings (##) - Creates natural boundaries
2. Separate topics with blank lines - Paragraph breaks
3. Keep code blocks focused - One concept per block
4. Use lists for related items - Keeps them together

## Working with Chunked Documents

When large documents are chunked, use these tools to work with them:

### Retrieving Full Documents

\`\`\`
// Get complete original document from any chunk
get_full_document({ id: "chunk-uuid" })
// Returns: { content, title, is_chunked: true, chunks: [...], total_chunks }
\`\`\`

### Search with Deduplication

\`\`\`
// Search with chunk deduplication (one result per document)
search_with_dedup({ query: "neural networks", mode: "hybrid" })
// Returns: { results: [{ note_id, score, chain_info: { total_chunks, chunks_matched } }] }
\`\`\`

### Inspecting Chunk Chains

\`\`\`
// Get all chunks in a document chain
get_chunk_chain({ chain_id: "any-chunk-uuid", include_content: false })
// Returns: { chunks: [{ id, sequence, byte_range: [start, end] }], total_chunks }
\`\`\`

**Understanding byte_range**: Shows position in original document. Overlap between chunks is intentional for context preservation.`,

  versioning: `# Note Version History

Dual-track versioning preserves both original and AI-enhanced content.

## Version Tracks

| Track | Contains | Field Name |
|-------|----------|------------|
| \`original\` | User-submitted content | \`version_number\` |
| \`revision\` | AI-enhanced content | \`revision_number\` |

**Note**: Field names differ between tracks for historical reasons.

## Operations

\`\`\`
// List versions (returns both tracks)
list_note_versions({ note_id: "uuid" })
// Returns: {
//   original_versions: [{ version_number: 1, created_at_utc, ... }],
//   revised_versions: [{ revision_number: 1, model, summary, ... }]
// }

// Get specific version
get_note_version({ note_id: "uuid", version: 2, track: "original" })

// Restore version (original track only, creates new version)
restore_note_version({ note_id: "uuid", version: 2, restore_tags: false })

// Compare versions
diff_note_versions({ note_id: "uuid", from_version: 1, to_version: 3 })
\`\`\`

## Workflow Patterns

### Safe Editing Workflow
1. Check current version: \`list_note_versions\`
2. Make edits: \`update_note\`
3. Review change: \`diff_note_versions\`
4. If mistake, restore: \`restore_note_version\`

### AI Enhancement Review
1. Create note with \`revision_mode: "full"\`
2. Compare tracks: get original and revised versions
3. If AI added unwanted content, restore original
4. Re-create with \`revision_mode: "light"\` or \`"none"\`

### Bulk Update Recovery
1. Before bulk updates: \`database_snapshot\`
2. Perform bulk operations
3. If issues: restore affected notes individually or use snapshot

### Content Evolution Tracking
1. List versions with timestamps
2. Analyze revision history
3. Track how ideas evolved over time

## Best Practices

- Review before restore: Use \`get_note_version\` to preview
- Use diff for clarity: \`diff_note_versions\` shows exact changes
- Restore creates new version: History is preserved, not overwritten
- Cannot restore revisions: Only original track supports restore`,

  collections: `# Collections (Folders)

Hierarchical folder organization for notes.

## Operations

\`\`\`
// List collections (root or children)
list_collections({ parent_id: null })  // Root collections
list_collections({ parent_id: "uuid" })  // Children

// Create collection
create_collection({ name: "Work", description: "...", parent_id: null })
// Returns: { id, name, description, parent_id, created_at_utc }

// Get collection details
get_collection({ id: "uuid" })
// Returns: { id, name, description, parent_id, note_count, created_at_utc }

// Update collection
update_collection({ id: "uuid", name: "New Name" })

// Delete collection (notes moved to uncategorized)
delete_collection({ id: "uuid" })
// Returns: { success: true, message: "Collection deleted" }

// Move note
move_note_to_collection({ note_id: "uuid", collection_id: "uuid" })
// Or: collection_id: null to uncategorize
// Returns: { success: true, note_id, collection_id }
\`\`\`

## Error Cases

- **"Collection not found"**: Invalid collection ID
- **"Note not found"**: Invalid note ID for move operation

## Best Practices

- Use collections for broad categories
- Use SKOS concepts for detailed tagging
- Don't over-nest (3-4 levels max)
- Combine with embedding sets for search focus`,

  templates: `# Note Templates

Reusable note structures with variable substitution.

## Operations

\`\`\`
// Create template
create_template({
  name: "Meeting Notes",
  content: "# {{topic}}\\n**Date**: {{date}}\\n## Notes\\n{{notes}}",
  default_tags: ["meeting"],
  collection_id: "uuid"  // Optional
})
// Returns: { id, name, content, default_tags, collection_id, created_at_utc }

// List templates
list_templates({ limit: 50 })
// Returns: { templates: [{ id, name, created_at_utc }], total }

// Get template
get_template({ template_id: "uuid" })
// Returns: { id, name, content, default_tags, collection_id, created_at_utc, updated_at_utc }
// Error: "Template not found" if ID doesn't exist

// Delete template
delete_template({ template_id: "uuid" })
// Returns: { success: true, message: "Template deleted" }
// Error: "Template not found" if ID doesn't exist

// Instantiate template
instantiate_template({
  template_id: "uuid",
  variables: { topic: "Sprint Planning", date: "2026-02-02", notes: "..." }
})
// Returns: { note: { id, title, content, tags, created_at_utc } }
\`\`\`

## Variable Handling

- Use \`{{variable_name}}\` in template content
- **Missing variables**: Left as-is (\`{{var}}\` stays in output)
- **Extra variables**: Ignored
- **Variable names**: Case-sensitive (\`{{Date}}\` ≠ \`{{date}}\`)

## Best Practices

- Use descriptive variable names: \`{{project_name}}\` not \`{{x}}\`
- Include default_tags for automatic categorization
- Use \`revision_mode: "light"\` for structured templates`,

  backup: `# Backup & Restore

## Quick Operations

| Task | Tool | Use Case |
|------|------|----------|
| Export JSON | \`export_all_notes\` | Portable, human-readable |
| Knowledge Shard | \`knowledge_shard\` | Tag-scoped archive |
| Database Snapshot | \`database_snapshot\` | Full disaster recovery |
| List Backups | \`list_backups\` | Browse available backups |
| Restore Database | \`database_restore\` | Restore from snapshot |

## Backup Strategies Comparison

| Strategy | Tool | Includes | Use Case |
|----------|------|----------|----------|
| JSON Export | \`export_all_notes\` | Notes, tags, links | Migration, sharing |
| Knowledge Shard | \`knowledge_shard\` | Notes + optional embeddings | Project archive |
| Database Snapshot | \`database_snapshot\` | Everything (full DB) | Disaster recovery |

## Knowledge Shards

Tag-scoped archives for project handoff:

\`\`\`
// Create shard
knowledge_shard({
  include: ["notes", "embeddings", "links"],
  tag: "project:alpha"  // Optional tag filter
})

// Import shard
knowledge_shard_import({
  file_path: "/path/to/shard.tar.gz",
  on_conflict: "skip",  // skip, replace, or merge
  dry_run: true  // Preview first
})
\`\`\`

## Full Database Backup/Restore

\`\`\`
// Create full snapshot
database_snapshot({ description: "Pre-migration backup" })

// List available backups
list_backups({ backup_type: "snapshot" })

// Verify backup
get_backup_info({ backup_id: 123 })

// Restore (DESTRUCTIVE - overwrites current data)
database_restore({ backup_id: 123, force: true })
\`\`\`

## Backup Verification

\`\`\`
// Check backup status
backup_status({ backup_id: 123 })

// Get metadata
get_backup_metadata({ backup_id: 123 })

// Update description
update_backup_metadata({ backup_id: 123, description: "Monthly archive" })
\`\`\`

## Knowledge Archives

Portable bundles for transfer between systems:

\`\`\`
// Download archive
knowledge_archive_download({ archive_id: 456 })

// Upload archive
knowledge_archive_upload({ file_path: "/path/to/archive.tar.gz" })
\`\`\`

## Best Practices

1. **Regular snapshots**: Daily \`database_snapshot\` for disaster recovery
2. **Test restores**: Monthly restore test in isolated environment
3. **Verify backups**: Check \`backup_status\` after creation
4. **Off-site storage**: Download critical backups to external storage
5. **Include embeddings**: Add embeddings to shards for full restore capability
6. **Document backups**: Use \`update_backup_metadata\` for context`,

  workflows: `# Usage Patterns and Workflows

> **Note**: Examples use conceptual pseudo-code to illustrate patterns. Adapt syntax for your integration (MCP, API, CLI).

## Pattern 1: Domain-Isolated Contexts

Use embedding sets for focused search:

\`\`\`
// Create work context
create_embedding_set({
  name: "Work Projects",
  slug: "work",
  criteria: { tags: ["work"] }
})

// Search within context only
search_notes({ query: "api integration", mode: "semantic", set: "work" })
\`\`\`

## Pattern 2: Memory Snapshots

Swap entire knowledge contexts:

\`\`\`
// Save current memory
backup = knowledge_shard({ include: ["notes", "embeddings", "links"] })

// Load different context
knowledge_shard_import({ file_path: "/path/to/other.tar.gz", on_conflict: "replace" })
\`\`\`

## Pattern 3: Research vs Production

- Tag research: \`["research", "unvalidated"]\`
- Tag validated: \`["validated"]\`
- Separate embedding sets for each
- Promote concepts: candidate → approved

## Pattern 4: Dual-Track Mind

- **Raw observations**: \`revision_mode: "none"\`
- **Synthesized insights**: \`revision_mode: "full"\`

## Pattern 5: Graph Exploration

### Knowledge Discovery
\`\`\`
// Start from a note, explore connections
graph = explore_graph({ id: "note-uuid", depth: 2, max_nodes: 50 })
// Analyze: nodes with many incoming links are hub concepts
// Nodes connecting different clusters are bridges
\`\`\`

### Cluster Analysis
\`\`\`
// Get full neighborhood
graph = explore_graph({ id: "note-uuid", depth: 3, max_nodes: 100 })
// Group nodes by connection density
// Create embedding sets for discovered clusters
\`\`\`

### Gap Detection
\`\`\`
// Find notes with few connections
links = get_note_links({ id: "note-uuid" })
if links.incoming.length == 0:
  // Orphaned note - consider adding more context or tags
\`\`\`

## Pattern 6: Template-Driven Capture

### Meeting Notes
\`\`\`
create_template({
  name: "Meeting Notes",
  content: "# {{title}}\\n**Date**: {{date}}\\n**Attendees**: {{attendees}}\\n\\n## Notes\\n{{notes}}\\n\\n## Actions\\n{{actions}}",
  default_tags: ["meeting"]
})

instantiate_template({ id: template_id, variables: { title: "Sprint Planning", date: "2026-02-02", ... } })
\`\`\`

### Research Paper Capture
\`\`\`
create_template({
  name: "Paper Notes",
  content: "# {{title}}\\n**Authors**: {{authors}}\\n**DOI**: {{doi}}\\n\\n## Key Findings\\n{{findings}}\\n\\n## Relevance\\n{{relevance}}",
  default_tags: ["research", "paper"]
})
\`\`\`

### Daily Reviews
\`\`\`
create_template({
  name: "Daily Review",
  content: "# {{date}}\\n\\n## Accomplished\\n{{done}}\\n\\n## Learned\\n{{learned}}\\n\\n## Tomorrow\\n{{tomorrow}}",
  default_tags: ["review", "daily"]
})
\`\`\`

## Pattern 7: AI Agent Memory

1. Search existing knowledge before responding
2. Get context from top results and their links
3. Store new insights with appropriate revision mode
4. Create task-specific embedding sets

## Design Principles

1. **Tag consistently** - Primary organization mechanism
2. **Match revision modes to content** - full for synthesis, none for data
3. **Leverage embedding sets** - Create focused "views"
4. **Backup before major changes** - Snapshot first
5. **Use semantic search for discovery** - FTS for exact matches`,

  troubleshooting: `# Troubleshooting

## Common Issues

### "Note not found" after create
- **Cause**: Pipeline jobs are asynchronous
- **Fix**: Check job status
\`\`\`
list_jobs({ note_id: "uuid", status: "pending" })
\`\`\`
- **Alternative**: Use \`list_notes\` with tag filtering to verify

### Search returns no results
- **Check 1**: Has embedding job completed?
- **Check 2**: Correct search mode? Try \`semantic\`
- **Check 3**: Using embedding set? Verify note is in set

### AI revision seems wrong
- **Try**: Use \`light\` revision mode
- **Check**: Is content too short for context?
- **Review with read-only tools**: Use \`search_notes\` to verify current content
- **Alternative**: \`list_notes\` with tags shows snippet preview

### Rate limit errors (429)
- **Wait**: Implement exponential backoff
- **Batch**: Use \`bulk_create_notes\` for multiple items
- **Paginate**: Use smaller page sizes

### Slow responses
- **Check**: \`get_queue_stats\` - many pending jobs?
- **Paginate**: Add \`limit\` parameter
- **Index**: Ensure searching in appropriate embedding set

## Permission-Restricted Environments

Some Claude Code sessions may restrict certain tools. Use these alternatives:

### If write tools are restricted
- **Verify content exists**: \`search_notes\` or \`list_notes\` instead of \`get_note\`
- **Check relationships**: \`get_note_links\` is usually available
- **Monitor jobs**: \`list_jobs\` and \`get_queue_stats\` are read-only

### If version tools are restricted
- **Alternative**: \`search_notes({ query: "content", mode: "fts" })\` to verify content
- **Alternative**: \`list_notes({ tags: ["tag"] })\` shows snippets

### Tool Permission Reference

**Usually Available (Read-Only):**
- \`search_notes\`, \`list_notes\`, \`list_tags\`
- \`get_note_links\`, \`explore_graph\`
- \`list_jobs\`, \`get_queue_stats\`, \`health_check\`
- \`search_concepts\`, \`get_governance_stats\`
- \`list_note_versions\`, \`diff_note_versions\`

**May Require Permission (Write):**
- \`create_note\`, \`update_note\`, \`set_note_tags\`
- \`create_collection\`, \`move_note_to_collection\`
- \`create_concept\`, \`tag_note_concept\`
- \`backup_now\`, \`knowledge_shard\`

**Usually Restricted (Destructive):**
- \`delete_note\`, \`purge_note\`
- \`database_restore\`

## Debugging Tips

1. Check job status after writes: \`list_jobs\`
2. Verify note state: \`search_notes\` with exact text
3. Review connections: \`get_note_links\`
4. Check taxonomy health: \`get_governance_stats\`
5. System diagnostics: \`health_check\`, \`memory_info\``,

  encryption: `# Encryption (PKE)

Public-key encryption for secure note and file storage using X25519 + AES-256-GCM.

## Overview

- **Key Exchange**: X25519 elliptic curve
- **Encryption**: AES-256-GCM
- **Key Derivation**: Argon2id (passphrase protection)
- **Format**: MMPKE01

## Address Format

Public addresses use \`mm:\` prefix:
\`\`\`
mm:c29tZV9leGFtcGxlX3B1YmxpY19rZXlfZGF0YQ==
\`\`\`

Validate with: \`pke_verify_address({ address: "mm:..." })\`

## Keyset Management

\`\`\`
// Create keyset (min 12 char passphrase)
pke_create_keyset({ name: "personal", passphrase: "secure-pass-12chars" })

// List keysets
pke_list_keysets()

// Activate keyset
pke_set_active_keyset({ name: "personal" })

// Get active keyset
pke_get_active_keyset()

// Export for backup
pke_export_keyset({ name: "personal", output_dir: "/backup" })

// Import from backup
pke_import_keyset({
  name: "personal",
  directory: "/backup"  // OR use public_key_path + private_key_path
})

// Delete keyset
pke_delete_keyset({ name: "old-keyset" })
\`\`\`

**Import Parameter Rules:**
- Use \`directory\` OR (\`public_key_path\` + \`private_key_path\`) - NOT both
- If directory provided, looks for \`{name}.pub\` and \`{name}.key\`

## Encryption Operations

\`\`\`
// Get public address
pke_get_address({ key_path: "/path/to/keypair.key" })

// Encrypt for recipients
pke_encrypt({
  input_path: "/file.txt",
  output_path: "/file.txt.enc",
  recipients: ["mm:abc...", "mm:xyz..."]  // Multi-recipient
})

// Decrypt (requires active keyset)
pke_decrypt({
  input_path: "/file.txt.enc",
  output_path: "/file.txt",
  passphrase: "secure-pass-12chars"
})

// List recipients of encrypted file
pke_list_recipients({ file_path: "/file.txt.enc" })
\`\`\`

## Passphrase Requirements

- **Minimum**: 12 characters
- **Encryption**: Argon2id + AES-256-GCM
- **No recovery**: Backup keysets securely

## Best Practices

1. Use 20+ character passphrases
2. Unique passphrase per keyset
3. Export keysets to encrypted backup storage
4. Test decryption before deleting plaintext
5. Verify addresses with \`pke_verify_address\`
6. Audit recipients with \`pke_list_recipients\`

## Common Errors

- **"Passphrase too short"**: Use 12+ characters
- **"No active keyset"**: Activate with \`pke_set_active_keyset\`
- **"Decryption failed"**: Wrong passphrase or not a recipient
- **"Invalid format"**: File is not MMPKE01 encrypted`,

  document_types: `# Document Type Registry

Automatic document classification with 131+ pre-configured types and custom type support.

## Overview

Every note can have a document type that controls chunking strategy, search behavior, and metadata extraction.

## Auto-Detection

\`\`\`
// Detect type from content and filename
detect_document_type({ content: "def hello():\\n    print('hi')", filename: "script.py" })
// Returns: { type_id: "uuid", slug: "python-source", confidence: 0.95, strategy: "syntactic" }
\`\`\`

Detection uses:
1. **Filename patterns**: Extension matching (.py → Python, .rs → Rust)
2. **Magic content**: Shebang lines, XML declarations, frontmatter
3. **Content analysis**: Code structure, markup patterns

## Working with Types

\`\`\`
// List all registered types
list_document_types({ limit: 50 })
// Returns: { types: [{ id, slug, name, mime_type, chunking_strategy }], total }

// Get type details
get_document_type({ id: "uuid" })
// Returns: { id, slug, name, description, mime_type, extensions, chunking_strategy, ... }

// Create custom type
create_document_type({
  slug: "api-spec",
  name: "API Specification",
  description: "OpenAPI/Swagger documents",
  mime_type: "application/yaml",
  extensions: [".yaml", ".yml"],
  filename_patterns: ["openapi.*", "swagger.*"],
  chunking_strategy: "semantic",
  chunk_size: 1500
})

// Update type
update_document_type({ id: "uuid", chunk_size: 2000 })

// Delete custom type (built-in types cannot be deleted)
delete_document_type({ id: "uuid" })
\`\`\`

## Chunking Strategies by Type

| Type Category | Strategy | Reason |
|---------------|----------|--------|
| Source code | \`syntactic\` | Preserves function/class boundaries |
| Markdown/docs | \`semantic\` | Splits on heading structure |
| Prose/articles | \`paragraph\` | Natural paragraph breaks |
| Data files | \`sliding_window\` | Uniform chunk sizes |
| Unknown | \`recursive\` | Adaptive fallback |

## Best Practices

1. Let auto-detection handle most cases
2. Create custom types for domain-specific formats
3. Match chunking strategy to content structure
4. Use \`detect_document_type\` before manual override`,

  archives: `# Archives

Named archive containers for organizing backup history and note lifecycle management.

## Overview

Archives provide named containers for organizing notes into logical groups with lifecycle policies.

## Operations

\`\`\`
// List archives
list_archives({ limit: 50 })
// Returns: { archives: [{ id, name, description, is_default, note_count, created_at }], total }

// Create archive
create_archive({
  name: "Q1 2026 Research",
  description: "Research notes from Q1 2026"
})
// Returns: { id, name, description, is_default: false, created_at }

// Get archive details
get_archive({ id: "uuid" })
// Returns: { id, name, description, is_default, note_count, total_size_bytes, created_at }

// Update archive
update_archive({ id: "uuid", name: "Updated Name", description: "New description" })

// Delete archive (moves notes to default archive)
delete_archive({ id: "uuid" })

// Set default archive (new notes go here)
set_default_archive({ id: "uuid" })

// Get archive statistics
get_archive_stats({ id: "uuid" })
// Returns: { note_count, total_size_bytes, oldest_note, newest_note, tag_distribution }
\`\`\`

## Best Practices

1. Use archives for temporal or project-based organization
2. Set a meaningful default archive for day-to-day notes
3. Review archive stats periodically for cleanup
4. Combine with tags for fine-grained filtering within archives`,

  observability: `# Knowledge Health & Observability

Tools for monitoring knowledge base quality, identifying maintenance needs, and understanding usage patterns.

## Knowledge Health Dashboard

\`\`\`
// Get comprehensive health metrics
get_knowledge_health()
// Returns: {
//   orphan_tags: 5,          // Tags with no notes
//   stale_notes: 12,         // Notes not updated in 90+ days
//   unlinked_notes: 8,       // Notes with no semantic links
//   concept_health: {...},    // SKOS taxonomy stats
//   embedding_coverage: 0.95  // % of notes with embeddings
// }
\`\`\`

## Diagnostic Tools

### Find Orphan Tags
\`\`\`
get_orphan_tags()
// Returns: [{ path: "old/unused-tag", note_count: 0 }]
// Action: Clean up or consolidate
\`\`\`

### Find Stale Notes
\`\`\`
get_stale_notes({ days: 90, limit: 50 })
// Returns: [{ id, title, updated_at, days_since_update }]
// Action: Review, update, or archive
\`\`\`

### Find Isolated Notes
\`\`\`
get_unlinked_notes({ limit: 50 })
// Returns: [{ id, title, created_at }]
// Action: Add content for better linking, or manually review
\`\`\`

### Tag Co-occurrence Analysis
\`\`\`
get_tag_cooccurrence({ min_count: 3, limit: 20 })
// Returns: [{ tag_a: "ml", tag_b: "python", count: 15 }]
// Action: Create SKOS related relationships, identify implicit taxonomies
\`\`\`

## Timeline & Activity

### Activity Timeline
\`\`\`
get_notes_timeline({ granularity: "day", start_date: "2026-01-01", end_date: "2026-02-01" })
// Returns: [{ bucket: "2026-01-15", created: 3, updated: 7 }]
// Use for: Activity dashboards, productivity tracking
\`\`\`

### Activity Feed
\`\`\`
get_notes_activity({ limit: 50, event_types: ["created", "updated"] })
// Returns: [{ event_type: "created", note_id: "uuid", timestamp, details }]
// Event types: created, updated, deleted, restored, tagged, linked
\`\`\`

## Maintenance Workflow

1. **Weekly**: Run \`get_knowledge_health\` to check overall status
2. **Review orphans**: \`get_orphan_tags\` → clean up or reassign
3. **Review stale**: \`get_stale_notes\` → update, archive, or delete
4. **Review isolated**: \`get_unlinked_notes\` → enrich content or reprocess
5. **Discover patterns**: \`get_tag_cooccurrence\` → refine SKOS taxonomy`,

  jobs: `# Background Jobs & Processing

Monitor and manage the asynchronous NLP processing pipeline.

## How Jobs Work

After \`create_note\` or \`update_note\`, background jobs run:
1. **ai_revision** - Content enhancement (if revision_mode != "none")
2. **embedding** - Vector embedding generation
3. **title_generation** - Automatic title extraction
4. **linking** - Semantic link calculation

Jobs run asynchronously - content may not be immediately searchable.

## Monitoring Jobs

\`\`\`
// List jobs with filters
list_jobs({ status: "pending", limit: 20 })
// Statuses: pending, processing, completed, failed
// Returns: [{ id, job_type, note_id, status, created_at, started_at }]

// Get specific job details
get_job({ id: "job-uuid" })
// Returns: { id, job_type, note_id, status, result, error, created_at, started_at, completed_at }

// Quick pending count (faster than list_jobs)
get_pending_jobs_count()
// Returns: { count: 5 }

// Queue statistics
get_queue_stats()
// Returns: { pending, processing, completed_today, failed_today, avg_duration_ms }
\`\`\`

## Reprocessing Notes

\`\`\`
// Reprocess specific pipeline steps
reprocess_note({ id: "note-uuid", steps: ["embedding", "linking"] })

// Reprocess everything
reprocess_note({ id: "note-uuid", steps: ["all"], force: true })

// Available steps: ai_revision, embedding, linking, title_generation, all
\`\`\`

**When to reprocess:**
- After embedding model changes → \`["embedding"]\`
- After content fixes → \`["linking", "embedding"]\`
- After model upgrade → \`["all"]\`
- Processing failed → \`reprocess_note\` with \`force: true\`

## Bulk Reprocessing

\`\`\`
// Re-embed all notes (after model change)
reembed_all({ confirm: true })
// Warning: This can take a long time for large knowledge bases
\`\`\`

## Best Practices

1. Check \`get_pending_jobs_count\` before searching for newly created content
2. Use \`get_job\` to debug failed processing
3. Monitor \`get_queue_stats\` for system health
4. Reprocess notes after infrastructure changes (model updates, etc.)`,

  provenance: `# Provenance & Backlinks

Track content origins, create spatial-temporal context for files, and discover reverse connections.

## Note Provenance (W3C PROV)

\`\`\`
get_note_provenance({ id: "note-uuid" })
// Returns: {
//   note_id: "uuid",
//   created_by: "agent/user",
//   creation_method: "create_note",
//   derivations: [
//     { type: "wasDerivedFrom", source_id: "template-uuid", activity: "instantiate_template" },
//     { type: "wasRevisionOf", source_version: 1, activity: "ai_revision" }
//   ],
//   activities: [
//     { type: "wasGeneratedBy", agent: "matric-api", timestamp: "..." }
//   ]
// }
\`\`\`

Provenance tracks:
- **Creation**: How and when the note was first created
- **Derivation**: What content it was derived from (templates, imports)
- **Revision**: AI enhancement history
- **Version lineage**: Connection between versions

## File Provenance Creation

Create spatial-temporal context for file attachments. This links files to where, when, and how they were captured.

### Step 1: Create a location (optional)

\`\`\`
create_provenance_location({
  latitude: 48.8584, longitude: 2.2945,
  source: "gps_exif", confidence: "high",
  altitude_m: 35.0, horizontal_accuracy_m: 10.0
})
// Returns: { id: "location-uuid" }
\`\`\`

Sources: gps_exif, device_api, user_manual, geocoded, ai_estimated, unknown
Confidence: high (GPS ±10m), medium (WiFi ±100m), low (IP ±1km+), unknown

### Step 2: Create a named location (optional)

\`\`\`
create_named_location({
  name: "Eiffel Tower", location_type: "poi",
  latitude: 48.8584, longitude: 2.2945,
  locality: "Paris", country: "France", country_code: "FR",
  timezone: "Europe/Paris"
})
// Returns: { id: "named-location-uuid", slug: "eiffel-tower" }
\`\`\`

Location types: home, work, poi, city, region, country

### Step 3: Register a device (optional)

\`\`\`
create_provenance_device({
  device_make: "Apple", device_model: "iPhone 15 Pro",
  device_os: "iOS", device_os_version: "17.2",
  software: "Camera", has_gps: true
})
// Returns: { id: "device-uuid" }
// Deduplicates on make+model — same device returns same ID
\`\`\`

### Step 4a: Create file provenance (for attachments)

\`\`\`
create_file_provenance({
  attachment_id: "attachment-uuid",
  capture_time_start: "2026-01-15T14:30:00Z",
  capture_timezone: "Europe/Paris",
  time_source: "exif", time_confidence: "high",
  location_id: "location-uuid",    // from step 1
  device_id: "device-uuid",        // from step 3
  event_type: "photo",
  event_title: "Sunset at Eiffel Tower"
})
// Returns: { id: "provenance-uuid" }
\`\`\`

### Step 4b: Create note provenance (for notes without attachments)

\`\`\`
create_note_provenance({
  note_id: "note-uuid",
  capture_time_start: "2026-01-15T14:30:00Z",
  capture_timezone: "Europe/Paris",
  time_source: "manual", time_confidence: "exact",
  location_id: "location-uuid",    // from step 1
  device_id: "device-uuid",        // from step 3
  event_type: "created",
  event_title: "Meeting notes at Eiffel Tower"
})
// Returns: { id: "provenance-uuid" }
\`\`\`

Use \`create_note_provenance\` when a note itself has spatial-temporal context
(e.g., meeting notes, travel journal, field observations) without needing a file attachment.

### Retrieval

\`\`\`
get_memory_provenance({ id: "note-uuid" })
// Returns full provenance chain: file provenance + note provenance
\`\`\`

## Dedicated Backlinks

\`\`\`
get_note_backlinks({ id: "note-uuid" })
// Returns: [{ id, title, score, snippet }]
\`\`\`

This is a focused view of incoming links only. For bidirectional links, use \`get_note_links\`.

**When to use backlinks:**
- Discover what references a concept
- Find entry points into a knowledge cluster
- Audit how a note is being referenced
- Build citation networks

## Provenance + Backlinks Workflow

1. Upload attachment: \`upload_attachment\` (or create note without attachment)
2. Create provenance: location → device → \`create_file_provenance\` or \`create_note_provenance\`
3. Search by context: \`search_memories_by_location\`, \`search_memories_by_time\`
4. Check provenance: \`get_memory_provenance\` — where/when was this captured?
5. Check backlinks: \`get_note_backlinks\` — what references this content?`,

  skos_collections: `# SKOS Collections

Labeled groupings of SKOS concepts for organizing related terms across hierarchies.

## Overview

Unlike broader/narrower (which form tree hierarchies), SKOS Collections are flat groupings that can include concepts from different parts of the hierarchy. Think of them as "playlists" for concepts.

## Operations

\`\`\`
// List collections in a scheme
list_skos_collections({ scheme_id: "uuid" })
// Returns: [{ id, label, description, member_count }]

// Create collection
create_skos_collection({
  scheme_id: "uuid",
  label: "Core AI Concepts",
  description: "Essential concepts for AI literacy"
})
// Returns: { id, label, description, scheme_id }

// Get collection with members
get_skos_collection({ collection_id: "uuid" })
// Returns: { id, label, description, members: [{ concept_id, pref_label }] }

// Update collection
update_skos_collection({ collection_id: "uuid", label: "Updated Label" })

// Delete collection (does not delete member concepts)
delete_skos_collection({ collection_id: "uuid" })

// Add concept to collection
add_skos_collection_member({ collection_id: "uuid", concept_id: "concept-uuid" })

// Remove concept from collection
remove_skos_collection_member({ collection_id: "uuid", concept_id: "concept-uuid" })
\`\`\`

## Collections vs Hierarchy

| Feature | Hierarchy (broader/narrower) | Collections |
|---------|------------------------------|-------------|
| Structure | Tree (parent-child) | Flat (list) |
| Concept membership | One parent only | Multiple collections |
| Purpose | Taxonomic classification | Thematic grouping |
| Example | "ML" broader "AI" | "Exam topics" includes ML, Stats, Ethics |

## SKOS Export

\`\`\`
// Export entire taxonomy as W3C RDF/Turtle
export_skos_turtle({ scheme_id: "uuid" })
// Returns valid Turtle syntax for interop with Protégé, TopBraid, PoolParty
\`\`\`

## Relation Removal

\`\`\`
// Remove broader relation
remove_broader({ concept_id: "child-uuid", broader_id: "parent-uuid" })

// Remove narrower relation
remove_narrower({ concept_id: "parent-uuid", narrower_id: "child-uuid" })

// Remove related relation
remove_related({ concept_id: "uuid-a", related_id: "uuid-b" })
\`\`\`

## Best Practices

1. Use collections for cross-cutting concerns (e.g., "exam topics", "project glossary")
2. Use hierarchy for taxonomic structure
3. A concept can be in multiple collections
4. Export to Turtle for interop with other SKOS tools`,

  embedding_configs: `# Embedding Model Configuration

Manage embedding models for vector search and semantic linking.

## Overview

Embedding configs define which models generate vector embeddings for notes. Different configs can be used for different embedding sets.

## Operations

\`\`\`
// List all configs
list_embedding_configs()
// Returns: [{ id, name, model, dimensions, provider, is_default }]

// Get default config
get_default_embedding_config()
// Returns: { id, name, model, dimensions, provider, is_default: true }

// Get specific config
get_embedding_config({ id: "uuid" })

// Create new config
create_embedding_config({
  name: "Nomic Large",
  model: "nomic-embed-text",
  dimensions: 768,
  provider: "ollama",
  is_default: false
})

// Update config
update_embedding_config({
  id: "uuid",
  dimensions: 384,    // For MRL reduced dimensions
  is_default: true
})
\`\`\`

## Model Selection Guide

| Model | Dimensions | Best For | Provider |
|-------|------------|----------|----------|
| nomic-embed-text | 768 | General purpose, good quality | Ollama |
| nomic-embed-text (MRL) | 256/384 | Storage savings, fast search | Ollama |
| text-embedding-3-small | 1536 | High quality, cloud | OpenAI |

## MRL (Matryoshka Representation Learning)

MRL models produce embeddings that can be truncated to lower dimensions while preserving quality:
- 768d → Full quality
- 384d → Good quality, 2× storage savings
- 256d → Acceptable quality, 3× storage savings
- 64d → Coarse search, 12× storage savings

Use lower dimensions for:
- Two-stage retrieval (coarse search → rerank)
- Resource-constrained environments
- Very large knowledge bases

## Best Practices

1. Start with the default config
2. Create separate configs for experimental models
3. Use MRL dimensions for embedding sets that need speed over precision
4. Re-embed notes after changing the config (\`reembed_all\` or \`reprocess_note\`)`,
};

// Combine all documentation for "all" topic
DOCUMENTATION.all = Object.entries(DOCUMENTATION)
  .filter(([key]) => key !== 'all')
  .map(([key, value]) => `---\n\n${value}`)
  .join('\n\n');

// Start server based on transport mode
if (MCP_TRANSPORT === "http") {
  // HTTP/SSE transport for remote access with OAuth
  const express = (await import("express")).default;
  const cors = (await import("cors")).default;

  const app = express();

  // CORS with MCP-Session-Id header support (required for StreamableHTTP transport)
  app.use(cors({
    origin: '*',
    methods: ['GET', 'POST', 'DELETE', 'OPTIONS'],
    allowedHeaders: ['Content-Type', 'Authorization', 'MCP-Session-Id'],
    exposedHeaders: ['MCP-Session-Id'],
  }));

  // IMPORTANT: Only use express.json() for routes that need pre-parsed body.
  // StreamableHTTPServerTransport reads the raw body itself, so we must NOT
  // use express.json() on the root path. Apply JSON parsing only to /messages.
  app.use('/messages', express.json());

  // Store active transports by session ID
  const transports = new Map();
  // Track transports being initialized (to prevent race conditions during handleRequest)
  const pendingTransports = new Map();

  /**
   * Send 401 with RFC 9728 compliant WWW-Authenticate header.
   * This helps MCP OAuth clients discover the authorization server.
   */
  function send401(res, message) {
    res.status(401)
      .set('WWW-Authenticate', `Bearer realm="mcp", resource_metadata="${MCP_BASE_URL}/.well-known/oauth-protected-resource"`)
      .json({ error: "unauthorized", error_description: message });
  }

  /**
   * Validate bearer token from Authorization header.
   * Returns { valid: true, token } or { valid: false }.
   */
  async function validateBearerToken(authHeader) {
    if (!authHeader || !authHeader.startsWith("Bearer ")) {
      return { valid: false };
    }

    const token = authHeader.slice(7);

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
        return { valid: false };
      }

      const introspection = await response.json();
      if (!introspection.active) {
        return { valid: false };
      }

      // Check for MCP scope (includes read+write) or at minimum read scope.
      // Tokens with "mcp" scope can perform all operations.
      // Tokens with only "read" scope can list/get but mutations will be
      // rejected by the Fortemi API's scope enforcement.
      const scopes = (introspection.scope || "").split(" ");
      if (!scopes.includes("mcp") && !scopes.includes("read") && !scopes.includes("admin")) {
        return { valid: false };
      }

      return { valid: true, token };
    } catch (error) {
      console.error("Token validation error:", error);
      return { valid: false };
    }
  }

  // OAuth token validation middleware
  async function validateToken(req, res, next) {
    const result = await validateBearerToken(req.headers.authorization);
    if (!result.valid) {
      return send401(res, "Valid bearer token required");
    }
    req.accessToken = result.token;
    next();
  }

  // SSE endpoint for MCP connections (legacy SSE transport)
  app.get("/sse", validateToken, async (req, res) => {
    console.log("[sse] New SSE connection");

    // Use full path since we're behind /mcp/ proxy
    const messagesPath = process.env.MCP_BASE_PATH ? `${process.env.MCP_BASE_PATH}/messages` : "/messages";
    const transport = new SSEServerTransport(messagesPath, res);
    const sessionId = transport.sessionId;

    console.log(`[sse] Transport created with sessionId: ${sessionId}`);
    transports.set(sessionId, { transport, token: req.accessToken, type: 'sse' });

    res.on("close", () => {
      console.log(`[sse] Connection closed for session ${sessionId}`);
      transports.delete(sessionId);
      sessionMemories.delete(sessionId); // Clean up session memory
    });

    // Create a new MCP server for this connection and connect
    const mcpServer = createMcpServer();
    const contextSessionId = transport.sessionId;
    await tokenStorage.run({ token: req.accessToken, sessionId: contextSessionId }, async () => {
      await mcpServer.connect(transport);
    });
    console.log(`[sse] MCP server connected for session ${sessionId}`);
  });

  // Messages endpoint for SSE transport
  app.post("/messages", validateToken, async (req, res) => {
    const sessionId = req.query.sessionId;
    console.log(`[messages] POST with sessionId: ${sessionId}`);

    if (!sessionId) {
      return res.status(400).json({ error: "Missing sessionId parameter" });
    }

    const session = transports.get(sessionId);
    if (!session || session.type !== 'sse') {
      console.error(`[messages] No SSE transport found for session ${sessionId}`);
      return res.status(400).json({ error: "No SSE transport found for sessionId" });
    }

    // Execute the message handler with the session's token context
    console.log(`[messages] Handling message for session ${sessionId}`);
    await tokenStorage.run({ token: session.token, sessionId }, async () => {
      await session.transport.handlePostMessage(req, res, req.body);
    });
  });

  // StreamableHTTP transport on root path (newer transport, POST to initialize/send, GET to receive)
  app.post("/", validateToken, async (req, res) => {
    const sessionId = req.headers['mcp-session-id'];
    console.log(`[mcp] POST, sessionId from header: ${sessionId || 'none'}`);

    const existingSession = sessionId ? transports.get(sessionId) : undefined;

    let transport;
    let isNewTransport = false;
    if (existingSession && existingSession.type === 'streamable') {
      // Reuse existing transport for this session
      console.log(`[mcp] Reusing existing transport for session ${sessionId}`);
      transport = existingSession.transport;
    } else {
      // Create new StreamableHTTP transport
      isNewTransport = true;
      transport = new StreamableHTTPServerTransport({
        sessionIdGenerator: () => crypto.randomUUID(),
      });

      // Create and connect new MCP server for this transport
      const mcpServer = createMcpServer();
      await mcpServer.connect(transport);
      console.log(`[mcp] Transport connected (sessionId will be set during handleRequest)`);

      // Set up cleanup on close
      transport.onclose = () => {
        console.log(`[mcp] Transport closed: ${transport?.sessionId}`);
        if (transport?.sessionId) {
          transports.delete(transport.sessionId);
          pendingTransports.delete(transport.sessionId);
        }
      };
    }

    // Handle the request with token context
    try {
      // For new transports, handleRequest will call the sessionIdGenerator and set transport.sessionId
      // We need to store it SYNCHRONOUSLY after handleRequest completes to avoid race conditions
      const contextSessionId = transport.sessionId;
      await tokenStorage.run({ token: req.accessToken, sessionId: contextSessionId }, async () => {
        await transport.handleRequest(req, res);
      });

      // Store transport IMMEDIATELY after handleRequest - sessionId is now set
      // This prevents the race where concurrent requests arrive before storage
      if (isNewTransport && transport.sessionId && !transports.has(transport.sessionId)) {
        console.log(`[mcp] Storing new transport with sessionId: ${transport.sessionId}`);
        transports.set(transport.sessionId, { transport, token: req.accessToken, type: 'streamable' });
      }
    } catch (error) {
      console.error(`[mcp] Error handling request:`, error);
      // Clean up pending transport on error
      if (isNewTransport && transport?.sessionId) {
        pendingTransports.delete(transport.sessionId);
      }
      if (!res.headersSent) {
        res.status(500).json({ error: error.message });
      }
    }
  });

  // GET on root for StreamableHTTP (server-to-client messages/SSE stream)
  app.get("/", validateToken, async (req, res) => {
    const sessionId = req.headers['mcp-session-id'];
    console.log(`[mcp] GET, sessionId: ${sessionId || 'none'}`);

    const session = sessionId ? transports.get(sessionId) : undefined;

    if (!session || session.type !== 'streamable') {
      return res.status(400).json({
        error: "Bad Request: No valid session. POST to initialize first, or use /sse for SSE transport."
      });
    }

    await tokenStorage.run({ token: session.token, sessionId }, async () => {
      await session.transport.handleRequest(req, res);
    });
  });

  // DELETE on root for StreamableHTTP session termination
  app.delete("/", validateToken, async (req, res) => {
    const sessionId = req.headers['mcp-session-id'];
    const session = sessionId ? transports.get(sessionId) : undefined;

    if (session && session.type === 'streamable') {
      await session.transport.close();
      transports.delete(sessionId);
      sessionMemories.delete(sessionId); // Clean up session memory
    }

    res.status(200).end();
  });

  // Health check
  app.get("/health", (req, res) => {
    const sseCount = [...transports.values()].filter(s => s.type === 'sse').length;
    const streamableCount = [...transports.values()].filter(s => s.type === 'streamable').length;
    res.json({
      status: "ok",
      transport: "http",
      sessions: { sse: sseCount, streamable: streamableCount, total: transports.size }
    });
  });

  // OAuth discovery endpoints - proxy to main API
  app.get("/.well-known/oauth-authorization-server", async (req, res) => {
    try {
      const response = await fetch(`${API_BASE}/.well-known/oauth-authorization-server`);
      const metadata = await response.json();
      res.json(metadata);
    } catch (error) {
      res.status(500).json({ error: "Failed to fetch OAuth metadata" });
    }
  });

  // OAuth Protected Resource Metadata (RFC 9728) - required by MCP OAuth clients
  // Returns this MCP server as the resource, with authorization_servers pointing to main API.
  // "mcp" scope is listed first — it grants read+write access for MCP operations.
  // Clients SHOULD request "mcp" scope to enable full read/write functionality.
  app.get("/.well-known/oauth-protected-resource", (req, res) => {
    res.json({
      resource: MCP_BASE_URL,
      authorization_servers: [process.env.ISSUER_URL || API_BASE],
      bearer_methods_supported: ["header"],
      scopes_supported: ["mcp"],
      resource_documentation: "https://memory.integrolabs.net/api-docs",
    });
  });

  // Validate MCP OAuth credentials on startup
  if (!process.env.MCP_CLIENT_ID || !process.env.MCP_CLIENT_SECRET) {
    console.warn("WARNING: MCP_CLIENT_ID or MCP_CLIENT_SECRET not set");
    console.warn("  Token introspection will fail — all authenticated requests will be rejected");
    console.warn("  Fix: register an OAuth client via POST /oauth/register and set credentials");
  } else {
    console.log(`MCP OAuth credentials configured (client_id: ${process.env.MCP_CLIENT_ID})`);
    // Verify credentials are valid by testing introspection
    try {
      const testResp = await fetch(`${API_BASE}/oauth/introspect`, {
        method: "POST",
        headers: {
          "Content-Type": "application/x-www-form-urlencoded",
          "Authorization": `Basic ${Buffer.from(`${process.env.MCP_CLIENT_ID}:${process.env.MCP_CLIENT_SECRET}`).toString("base64")}`,
        },
        body: "token=startup_check",
      });
      if (testResp.ok) {
        console.log("  OAuth credential validation: OK");
      } else {
        console.warn(`  WARNING: OAuth credential validation failed (HTTP ${testResp.status})`);
        console.warn("  MCP client_id/secret may be stale — re-register via POST /oauth/register");
      }
    } catch (e) {
      console.warn(`  WARNING: Could not reach API for credential validation: ${e.message}`);
      console.warn("  Ensure the API is running at ${API_BASE}");
    }
  }

  app.listen(MCP_PORT, () => {
    console.log(`MCP HTTP server listening on port ${MCP_PORT}`);
    console.log(`Endpoints:`);
    console.log(`  StreamableHTTP: POST/GET ${MCP_BASE_URL}/`);
    console.log(`  SSE: GET ${MCP_BASE_URL}/sse + POST ${MCP_BASE_URL}/messages`);
    console.log(`  OAuth: ${MCP_BASE_URL}/.well-known/oauth-authorization-server`);
    console.log(`  Resource: ${MCP_BASE_URL}/.well-known/oauth-protected-resource`);
  });
} else {
  // Stdio transport for local use (default)
  const mcpServer = createMcpServer();
  const transport = new StdioServerTransport();
  await mcpServer.connect(transport);
}

// Export for testing
export default createMcpServer;
