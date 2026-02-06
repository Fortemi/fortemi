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
import { execSync } from "node:child_process";

const API_BASE = process.env.FORTEMI_URL || process.env.ISSUER_URL || "https://fortemi.com";
const API_KEY = process.env.FORTEMI_API_KEY || null;
const MCP_TRANSPORT = process.env.MCP_TRANSPORT || "stdio"; // "stdio" or "http"
const MCP_PORT = parseInt(process.env.MCP_PORT || "3001", 10);
const MCP_BASE_URL = process.env.MCP_BASE_URL || `http://localhost:${MCP_PORT}`;

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

        case "bulk_create_notes":
          result = await apiRequest("POST", "/api/v1/notes/bulk", {
            notes: args.notes,
          });
          break;

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
          params.set("start", encodeURIComponent(args.start));
          params.set("end", encodeURIComponent(args.end));
          result = await apiRequest("GET", `/api/v1/memories/search?${params}`);
          break;
        }

        case "search_memories_combined": {
          const params = new URLSearchParams();
          params.set("lat", args.lat);
          params.set("lon", args.lon);
          if (args.radius !== undefined && args.radius !== null) params.set("radius", args.radius);
          params.set("start", encodeURIComponent(args.start));
          params.set("end", encodeURIComponent(args.end));
          result = await apiRequest("GET", `/api/v1/memories/search?${params}`);
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

        case "get_template":
          result = await apiRequest("GET", `/api/v1/templates/${args.id}`);
          break;

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
          const embeddingModel = defaultSet?.model || "nomic-embed-text";
          const embeddingDimension = defaultSet?.dimension || 768;

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
          const allNotes = await apiRequest("GET", "/api/v1/notes?limit=10000");
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

          // Get shard as binary and convert to base64
          const arrayBuffer = await shardResponse.arrayBuffer();
          const base64Data = Buffer.from(arrayBuffer).toString('base64');

          // Get content-disposition for filename
          const contentDisposition = shardResponse.headers.get('content-disposition');
          const filenameMatch = contentDisposition?.match(/filename="([^"]+)"/);
          const filename = filenameMatch ? filenameMatch[1] : `matric-backup-${new Date().toISOString().slice(0,10)}.tar.gz`;

          result = {
            success: true,
            filename,
            size_bytes: arrayBuffer.byteLength,
            size_human: arrayBuffer.byteLength > 1024*1024
              ? `${(arrayBuffer.byteLength / (1024*1024)).toFixed(2)} MB`
              : `${(arrayBuffer.byteLength / 1024).toFixed(2)} KB`,
            content_type: "application/gzip",
            base64_data: base64Data,
            message: `Archive created: ${filename} (${arrayBuffer.byteLength} bytes). Use base64_data to save the file.`,
          };
          break;
        }

        case "knowledge_shard_import": {
          // Import a full knowledge shard from tar.gz
          const importBody = {
            shard_base64: args.shard_base64,
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
          // Download backup + metadata as bundled .archive file
          const response = await fetch(`${API_BASE}/api/v1/backup/knowledge-archive/${encodeURIComponent(args.filename)}`, { headers });
          if (!response.ok) {
            throw new Error(`Download failed: ${response.status}`);
          }
          const arrayBuffer = await response.arrayBuffer();
          const base64Data = Buffer.from(arrayBuffer).toString('base64');
          const contentDisposition = response.headers.get('content-disposition');
          const filenameMatch = contentDisposition?.match(/filename="([^"]+)"/);
          const archiveFilename = filenameMatch ? filenameMatch[1] : `${args.filename}.archive`;
          result = {
            success: true,
            filename: archiveFilename,
            size_bytes: arrayBuffer.byteLength,
            base64_data: base64Data,
            message: `Knowledge archive downloaded: ${archiveFilename}. Contains backup file + metadata.json.`,
          };
          break;
        }

        case "knowledge_archive_upload": {
          // Upload a .archive file (backup + metadata bundled)
          // This requires FormData which is complex in Node.js, so we use base64
          const boundary = '----KnowledgeArchiveBoundary' + Date.now();
          const archiveBuffer = Buffer.from(args.archive_base64, 'base64');
          const filename = args.filename || 'upload.archive';

          const body = Buffer.concat([
            Buffer.from(`--${boundary}\r\n`),
            Buffer.from(`Content-Disposition: form-data; name="file"; filename="${filename}"\r\n`),
            Buffer.from('Content-Type: application/x-tar\r\n\r\n'),
            archiveBuffer,
            Buffer.from(`\r\n--${boundary}--\r\n`),
          ]);

          const uploadResponse = await fetch(`${API_BASE}/api/v1/backup/knowledge-archive`, {
            method: 'POST',
            headers: {
              ...headers,
              'Content-Type': `multipart/form-data; boundary=${boundary}`,
            },
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
        // PUBLIC KEY ENCRYPTION (PKE) - Local wallet-style encryption
        // These operations run locally via the matric-pke CLI
        // ============================================================================
        case "pke_generate_keypair": {
          const { execSync } = await import("node:child_process");
          const cliArgs = ["keygen", "-p", args.passphrase];
          if (args.output_dir) cliArgs.push("-o", args.output_dir);
          if (args.label) cliArgs.push("-l", args.label);
          const output = execSync(`matric-pke ${cliArgs.join(" ")}`, { encoding: "utf8" });
          result = JSON.parse(output);
          break;
        }

        case "pke_get_address": {
          const { execSync } = await import("node:child_process");
          const output = execSync(`matric-pke address -p "${args.public_key_path}"`, { encoding: "utf8" });
          result = JSON.parse(output);
          break;
        }

        case "pke_encrypt": {
          const { execSync } = await import("node:child_process");
          const recipientArgs = args.recipients.map(r => `-r "${r}"`).join(" ");
          const output = execSync(`matric-pke encrypt -i "${args.input_path}" -o "${args.output_path}" ${recipientArgs}`, { encoding: "utf8" });
          result = JSON.parse(output);
          break;
        }

        case "pke_decrypt": {
          const { execSync } = await import("node:child_process");
          const output = execSync(`matric-pke decrypt -i "${args.input_path}" -o "${args.output_path}" -k "${args.private_key_path}" -p "${args.passphrase}"`, { encoding: "utf8" });
          result = JSON.parse(output);
          break;
        }

        case "pke_list_recipients": {
          const { execSync } = await import("node:child_process");
          const output = execSync(`matric-pke recipients -i "${args.input_path}"`, { encoding: "utf8" });
          result = JSON.parse(output);
          break;
        }

        case "pke_verify_address": {
          const { execSync } = await import("node:child_process");
          try {
            const output = execSync(`matric-pke verify "${args.address}"`, { encoding: "utf8" });
            result = JSON.parse(output);
          } catch (e) {
            // Parse the JSON output even on error (verification failure)
            if (e.stdout) {
              result = JSON.parse(e.stdout);
            } else {
              throw e;
            }
          }
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

              // Get address from public key
              let address = null;
              try {
                const addrOutput = execSync(`matric-pke address -p "${publicKeyPath}"`, { encoding: 'utf8' });
                const addrData = JSON.parse(addrOutput);
                address = addrData.address;
              } catch (e) {
                // Skip if we can't get address
                continue;
              }

              // Get created timestamp from directory
              const stats = fs.statSync(keysetDir);

              keysets.push({
                name: entry.name,
                address,
                public_key_path: publicKeyPath,
                private_key_path: privateKeyPath,
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

            // Create directory
            fs.mkdirSync(keysetDir, { recursive: true });

            // Generate keypair using matric-pke
            const output = execSync(`matric-pke keygen -p "${args.passphrase}" -o "${keysetDir}"`, { encoding: 'utf8' });
            const keygenData = JSON.parse(output);

            // Return keyset info with normalized paths
            result = {
              name: args.name,
              address: keygenData.address,
              public_key_path: path.join(keysetDir, 'public.key'),
              private_key_path: path.join(keysetDir, 'private.key.enc'),
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

            const keysetDir = path.join(keysDir, activeKeyset);
            const publicKeyPath = path.join(keysetDir, 'public.key');
            const privateKeyPath = path.join(keysetDir, 'private.key.enc');

            // Verify keyset exists
            if (!fs.existsSync(keysetDir) || !fs.existsSync(publicKeyPath) || !fs.existsSync(privateKeyPath)) {
              result = null;
              break;
            }

            // Get address from public key
            const addrOutput = execSync(`matric-pke address -p "${publicKeyPath}"`, { encoding: 'utf8' });
            const addrData = JSON.parse(addrOutput);

            // Get created timestamp
            const stats = fs.statSync(keysetDir);

            result = {
              name: activeKeyset,
              address: addrData.address,
              public_key_path: publicKeyPath,
              private_key_path: privateKeyPath,
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

            // Get address from imported public key
            const addrOutput = execSync(`matric-pke address -p "${destPublicKey}"`, { encoding: 'utf8' });
            const addrData = JSON.parse(addrOutput);

            result = {
              success: true,
              keyset_name: args.name,
              address: addrData.address,
              public_key_path: destPublicKey,
              private_key_path: destPrivateKey,
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
          result = await apiRequest("POST", `/api/v1/concepts/collections/${args.id}/members`, {
            concept_id: args.concept_id,
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
          if (args.days) params.set("days", args.days);
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
          if (!args.scheme_id) {
            throw new Error("scheme_id is required for SKOS Turtle export");
          }
          // Fetch as text since this returns Turtle format, not JSON
          const sessionToken = tokenStorage.getStore()?.token;
          const headers = { "Accept": "text/turtle" };
          if (sessionToken) {
            headers["Authorization"] = `Bearer ${sessionToken}`;
          } else if (API_KEY) {
            headers["Authorization"] = `Bearer ${API_KEY}`;
          }
          const turtleResponse = await fetch(`${API_BASE}/api/v1/concepts/schemes/${args.scheme_id}/export/turtle`, { headers });
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
          const uploadBody = {
            filename: args.filename,
            content_type: args.content_type,
            data: args.data,
          };
          if (args.document_type_id) {
            uploadBody.document_type_id = args.document_type_id;
          }
          result = await apiRequest("POST", `/api/v1/notes/${args.note_id}/attachments`, uploadBody);
          break;
        }

        case "list_attachments":
          result = await apiRequest("GET", `/api/v1/notes/${args.note_id}/attachments`);
          break;

        case "get_attachment":
          result = await apiRequest("GET", `/api/v1/attachments/${args.id}`);
          break;

        case "download_attachment":
          result = await apiRequest("GET", `/api/v1/attachments/${args.id}/download`);
          break;

        case "delete_attachment":
          await apiRequest("DELETE", `/api/v1/attachments/${args.id}`);
          result = { success: true };
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

  return mcpServer;
}

// Define available tools with comprehensive documentation for consuming agents
const tools = [
  // ============================================================================
  // READ OPERATIONS - No processing triggered
  // ============================================================================
  {
    name: "list_notes",
    description: `List notes from the memory system.

Returns note summaries with titles, snippets, tags, and metadata. Notes are returned with both their original content and AI-enhanced revisions.

Use cases:
- Browse recent notes
- Get an overview of stored knowledge
- Filter by starred or archived status
- Filter by specific tags
- Filter by date range (created_after/before, updated_after/before)`,
    inputSchema: {
      type: "object",
      properties: {
        limit: { type: "number", description: "Maximum notes to return (default: 50)", default: 50 },
        offset: { type: "number", description: "Pagination offset (default: 0)", default: 0 },
        filter: { type: "string", description: "Filter: 'starred' or 'archived'", enum: ["starred", "archived"] },
        tags: { type: "array", items: { type: "string" }, description: "Filter by tags - use hierarchical paths like 'topic/subtopic' (notes must have ALL specified tags)" },
        collection_id: { type: "string", format: "uuid", description: "Filter notes to this collection (optional)" },
        created_after: { type: "string", description: "Filter notes created after this date (ISO 8601 format, e.g. '2024-01-01T00:00:00Z')" },
        created_before: { type: "string", description: "Filter notes created before this date (ISO 8601 format)" },
        updated_after: { type: "string", description: "Filter notes updated after this date (ISO 8601 format)" },
        updated_before: { type: "string", description: "Filter notes updated before this date (ISO 8601 format)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_note",
    description: `Get complete details for a specific note.

Returns the full note including:
- Original content (as submitted)
- AI-enhanced revision (structured, contextual)
- Generated title
- Tags (user + AI-generated)
- Semantic links to related notes
- Metadata and timestamps

Use this to retrieve the full context of a note for analysis or reference.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "search_notes",
    description: `Search notes using hybrid full-text and semantic search.

Search modes:
- 'hybrid' (default): Combines keyword matching with semantic similarity for best results
- 'fts': Full-text search only - exact keyword matching
- 'semantic': Vector similarity only - finds conceptually related content

Embedding sets:
- Use 'set' parameter to restrict semantic search to a specific embedding set
- Omit 'set' to search across all embeddings (default behavior)
- Use list_embedding_sets to discover available sets

Returns ranked results with:
- note_id: UUID of the matching note
- score: Relevance score (0.0-1.0)
- snippet: Text excerpt showing matching content
- title: Note title (for quick identification)
- tags: Associated tags (for context)

Use semantic mode when looking for conceptually related content even if exact keywords don't match.`,
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Search query (natural language or keywords)" },
        limit: { type: "number", description: "Maximum results (default: 20)", default: 20 },
        mode: { type: "string", enum: ["hybrid", "fts", "semantic"], description: "Search mode", default: "hybrid" },
        set: { type: "string", description: "Embedding set slug to restrict semantic search (optional)" },
        collection_id: { type: "string", format: "uuid", description: "Filter results to notes in this collection (optional)" },
        required_tags: {
          type: "array",
          items: { type: "string" },
          description: "Strict filter: ALL results MUST have these tags (AND logic). Example: ['programming/rust']"
        },
        excluded_tags: {
          type: "array",
          items: { type: "string" },
          description: "Strict filter: NO results should have these tags (NOT logic). Example: ['draft']"
        },
        any_tags: {
          type: "array",
          items: { type: "string" },
          description: "Strict filter: results must have at least ONE of these tags (OR logic). Example: ['ai/ml', 'ai/nlp']"
        },
        strict_filter: {
          type: "string",
          description: "Advanced: raw JSON strict filter string. Example: '{\"required_tags\":[\"tag1\"],\"excluded_tags\":[\"tag2\"]}'. Use required_tags/excluded_tags/any_tags params instead for convenience."
        },
      },
      required: ["query"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "search_memories_by_location",
    description: `Search for memories near a geographic location. Returns attachments captured within a radius of the given coordinates, ordered by distance.

Use this to find photos, documents, or other attachments that were created or captured at specific places.`,
    inputSchema: {
      type: "object",
      properties: {
        lat: { type: "number", description: "Latitude in decimal degrees (-90 to 90)" },
        lon: { type: "number", description: "Longitude in decimal degrees (-180 to 180)" },
        radius: { type: "number", description: "Search radius in meters (default: 1000)", default: 1000 },
      },
      required: ["lat", "lon"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "search_memories_by_time",
    description: `Search for memories captured within a time range. Returns attachments with capture times overlapping the given range.

Use this to find photos or files created during specific events or time periods.`,
    inputSchema: {
      type: "object",
      properties: {
        start: { type: "string", description: "Start of time range (ISO 8601 format, e.g. '2024-01-15T00:00:00Z')" },
        end: { type: "string", description: "End of time range (ISO 8601 format)" },
      },
      required: ["start", "end"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "search_memories_combined",
    description: `Search for memories by both location and time. Returns attachments captured within a radius AND time range.

Use this to find content from specific events at known locations and times.`,
    inputSchema: {
      type: "object",
      properties: {
        lat: { type: "number", description: "Latitude in decimal degrees (-90 to 90)" },
        lon: { type: "number", description: "Longitude in decimal degrees (-180 to 180)" },
        radius: { type: "number", description: "Search radius in meters (default: 1000)", default: 1000 },
        start: { type: "string", description: "Start of time range (ISO 8601 format, e.g. '2024-01-15T00:00:00Z')" },
        end: { type: "string", description: "End of time range (ISO 8601 format)" },
      },
      required: ["lat", "lon", "start", "end"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "list_tags",
    description: `List all tags (SKOS concepts) in the knowledge base with usage counts.

Tags are organized hierarchically using "/" separator (e.g., "programming/rust", "ai/ml/transformers").
This returns the flattened list of all tag paths with their note counts.`,
    inputSchema: { type: "object", properties: {} },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_note_links",
    description: `Get semantic links and backlinks for a note.

Returns two arrays:
- outgoing: Notes this note links TO (related concepts it references)
- incoming: BACKLINKS - Notes that link TO this note (other notes that reference this concept)

Each link includes:
- id: Link UUID
- from_note_id / to_note_id: The connected notes
- kind: Link type (e.g., "semantic")
- score: Similarity score (0.0-1.0)

Use backlinks (incoming) to discover:
- What notes reference this concept
- How this note fits into the broader knowledge graph
- Entry points for exploring related knowledge

Links are automatically created based on semantic similarity (>70%) and are bidirectional.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "export_note",
    description: `Export a note as markdown with optional YAML frontmatter.

Perfect for:
- Backing up notes to local files
- Sharing notes in standard format
- Importing into other tools (Obsidian, Notion, etc.)

Options:
- include_frontmatter: Add YAML metadata (id, title, dates, tags) at top (default: true)
- content: "revised" (default, AI-enhanced) or "original" (raw input)

Returns the complete markdown text ready to save as a .md file.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note to export" },
        include_frontmatter: { type: "boolean", description: "Include YAML metadata header (default: true)", default: true },
        content: { type: "string", enum: ["revised", "original"], description: "Content version to export (default: revised)", default: "revised" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // WRITE OPERATIONS WITH FULL PIPELINE
  // These automatically trigger the complete NLP enhancement pipeline:
  // 1. AI Revision - Enhances content with context from related notes
  // 2. Embedding - Generates vector embeddings for semantic search
  // 3. Title Generation - Creates descriptive title from content
  // 4. Linking - Creates bidirectional semantic links to related notes
  // ============================================================================
  {
    name: "create_note",
    description: `Create a new note with FULL AI ENHANCEMENT PIPELINE.

This is the primary method for adding knowledge. After creation, the note automatically goes through:

1. AI REVISION: Content is enhanced using context from related notes in the knowledge base. The revision adds structure, clarity, connections to related concepts, and proper markdown formatting.

2. EMBEDDING: Vector embeddings are generated for semantic search. Content is chunked and each chunk is embedded for fine-grained retrieval.

3. TITLE GENERATION: A descriptive, unique title is generated based on content and related notes.

4. LINKING: Bidirectional semantic links are created to related notes (similarity >70%), connecting this note to the broader knowledge graph.

The enhanced version preserves all original information while adding structure and context. Both original and enhanced versions are stored.

**REVISION MODE SELECTION GUIDE:**

Use revision_mode="full" (default) when:
- Recording technical concepts, research, or complex ideas that benefit from connections
- Building a knowledge base where cross-referencing adds value
- The note has enough detail for meaningful enhancement

Use revision_mode="light" when:
- Recording facts, opinions, or observations that should stay as-is
- The note is short/simple and shouldn't be expanded
- You want formatting improvements without invented details
- Recording personal notes or quick thoughts

Use revision_mode="none" when:
- Storing exact quotes or citations
- Recording data that must remain unmodified
- Bulk importing content that shouldn't be processed

Best practices:
- Write in markdown format for best results
- Include context and specifics - the more detail, the better the enhancement
- Use #tags inline for explicit categorization
- For factual/personal notes, use "light" mode to prevent hallucination

**TAG FORMAT (SKOS-compliant hierarchical tags):**

Tags support hierarchical paths using "/" separator (max 5 levels):
- Simple: "archive", "reviewed", "important"
- Hierarchical: "programming/rust", "ai/ml/transformers"
- Multi-level: "projects/matric/features/search"

Examples:
- ["archive"] - flat tag
- ["programming/rust", "learning"] - mixed tags
- ["ai/ml/deep-learning", "projects/research"] - hierarchical tags

Tags are automatically converted to W3C SKOS concepts with proper broader/narrower relationships.`,
    inputSchema: {
      type: "object",
      properties: {
        content: { type: "string", description: "Note content in markdown format" },
        tags: {
          type: "array",
          items: { type: "string" },
          description: "Optional tags. Use hierarchical paths like 'topic/subtopic' (max 5 levels). Examples: 'archive', 'programming/rust', 'ai/ml/transformers'"
        },
        revision_mode: {
          type: "string",
          enum: ["full", "light", "none"],
          description: "AI revision mode: 'full' (default) for contextual expansion, 'light' for formatting only without inventing details, 'none' to skip AI revision entirely",
          default: "full"
        },
        collection_id: {
          type: "string",
          format: "uuid",
          description: "Optional collection UUID to place the note in"
        },
        metadata: {
          type: "object",
          description: "Optional arbitrary key-value metadata to attach to the note (e.g., { source: 'meeting', priority: 'high' })"
        },
      },
      required: ["content"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "bulk_create_notes",
    description: `Create multiple notes in a single batch operation.

Use this for efficient batch import of multiple notes. All notes are inserted in a single transaction for atomicity.

Each note in the batch:
- Goes through the same AI enhancement pipeline as create_note
- Can have its own revision_mode setting
- Can have its own tags

Limits:
- Maximum 100 notes per batch
- Large batches may take longer to process (AI pipeline runs for each)

Returns:
- ids: Array of created note UUIDs (in same order as input)
- count: Total notes created

Best practices:
- Use revision_mode="none" for raw imports that shouldn't be AI-enhanced
- Use revision_mode="light" for lightly processed content
- Group similar content types in batches for consistent processing`,
    inputSchema: {
      type: "object",
      properties: {
        notes: {
          type: "array",
          description: "Array of notes to create (max 100)",
          items: {
            type: "object",
            properties: {
              content: { type: "string", description: "Note content in markdown format" },
              tags: { type: "array", items: { type: "string" }, description: "Optional hierarchical tags (e.g., 'topic/subtopic', max 5 levels)" },
              revision_mode: {
                type: "string",
                enum: ["full", "light", "none"],
                description: "AI revision mode for this note",
                default: "full"
              }
            },
            required: ["content"]
          }
        }
      },
      required: ["notes"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "update_note",
    description: `Update a note's content or status.

If CONTENT is updated, the FULL AI ENHANCEMENT PIPELINE runs automatically:
- AI revision regenerated with new content
- Embeddings updated for semantic search
- Title regenerated if content changed significantly
- Links recalculated based on new content

If only STATUS (starred/archived) is updated, no processing occurs.

Use this to:
- Correct or expand note content (triggers full pipeline)
- Star important notes for quick access (no processing)
- Archive outdated notes (no processing)`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note to update" },
        content: { type: "string", description: "New markdown content (triggers full AI pipeline)" },
        starred: { type: "boolean", description: "Mark as important (no processing)" },
        archived: { type: "boolean", description: "Archive the note (no processing)" },
        revision_mode: {
          type: "string",
          enum: ["full", "light", "none"],
          description: "AI revision mode: 'full' (default) for contextual expansion, 'light' for formatting only without inventing details, 'none' to skip AI revision entirely",
          default: "full"
        },
        metadata: {
          type: "object",
          description: "Optional arbitrary key-value metadata to update (e.g., { source: 'meeting', priority: 'high' })"
        },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "delete_note",
    description: `Soft delete a note (can be restored later).

The note is marked as deleted but not permanently removed. Use restore endpoint to recover.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note to delete" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "restore_note",
    description: `Restore a soft-deleted note.

Recovers a previously deleted note, making it accessible again. The note retains all its original metadata, tags, and content.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "UUID of the deleted note to restore" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "set_note_tags",
    description: `Set tags for a note (replaces all existing user tags).

AI-generated tags are preserved separately. This only affects user-defined tags.

**TAG FORMAT (SKOS-compliant hierarchical tags):**
- Simple: "archive", "reviewed"
- Hierarchical: "programming/rust", "ai/ml/transformers" (max 5 levels)
- Tags are auto-converted to SKOS concepts with broader/narrower relationships`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note" },
        tags: {
          type: "array",
          items: { type: "string" },
          description: "New tags (replaces existing). Use hierarchical paths like 'topic/subtopic' (max 5 levels)"
        },
      },
      required: ["id", "tags"],
    },
    annotations: {
      destructiveHint: false,
    },
  },

  // ============================================================================
  // SINGLE-STEP PROCESSING
  // For fine-grained control when you need to run specific pipeline steps
  // ============================================================================
  {
    name: "create_job",
    description: `Queue a SINGLE processing step (for fine-grained control).

Unlike create_note/update_note which run the FULL pipeline, this queues just ONE step. Use this for:
- Reprocessing specific notes after model updates
- Debugging pipeline issues
- Bulk reprocessing with control over which steps run

Job types:
- 'ai_revision': Re-enhance content with context from related notes
- 'embedding': Regenerate vector embeddings for semantic search
- 'title_generation': Regenerate the note title
- 'linking': Recalculate semantic links to related notes
- 'context_update': Add "Related Context" section based on links

Priority: Higher values run sooner. Default priorities:
- ai_revision: 8 (highest - should run first)
- embedding: 5
- linking: 3
- title_generation: 2
- context_update: 1 (lowest - runs after links exist)

NOTE: For normal operations, prefer create_note/update_note which handle the full pipeline automatically. Use create_job only when you need single-step control.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note to process" },
        job_type: {
          type: "string",
          enum: ["ai_revision", "embedding", "linking", "context_update", "title_generation"],
          description: "Single processing step to run"
        },
        priority: { type: "number", description: "Job priority (higher = sooner)" },
      },
      required: ["job_type"],
    },
    annotations: {
      destructiveHint: false,
    },
  },

  // ============================================================================
  // JOB VISIBILITY
  // Monitor background job queue status and processing progress
  // ============================================================================
  {
    name: "list_jobs",
    description: `List and filter background jobs in the processing queue.

Use this to monitor job progress after triggering updates:
- Confirm jobs were queued successfully
- Track processing progress across multiple notes
- Identify failed or stuck jobs
- Wait for bulk operations to complete

Returns job list with queue statistics summary.

Common workflows:
1. After bulk update: list_jobs(status="pending") → confirm all queued
2. Monitor progress: list_jobs(status="processing") → see what's running
3. Check failures: list_jobs(status="failed") → surface errors
4. Track specific note: list_jobs(note_id="uuid") → see all jobs for one note`,
    inputSchema: {
      type: "object",
      properties: {
        status: {
          type: "string",
          enum: ["pending", "processing", "completed", "failed"],
          description: "Filter by job status"
        },
        job_type: {
          type: "string",
          enum: ["ai_revision", "embedding", "linking", "context_update", "title_generation"],
          description: "Filter by job type"
        },
        note_id: { type: "string", description: "Filter by specific note UUID" },
        limit: { type: "number", description: "Max results (default: 50)", default: 50 },
        offset: { type: "number", description: "Pagination offset", default: 0 },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_queue_stats",
    description: `Get a quick summary of queue health without listing individual jobs.

Returns:
- pending: Jobs waiting to be processed
- processing: Jobs currently running
- completed_last_hour: Successfully finished in last hour
- failed_last_hour: Failed in last hour
- total: Total jobs in queue

Use this for quick status checks or progress bars when you don't need full job details.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "health_check",
    description: `Check system health status.

Returns a simple health check indicating if the system is operational.
Includes version info and component health status.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_system_info",
    description: `Get comprehensive system diagnostic information.

Returns aggregated system information including:
- version: API version
- status: Overall health status
- configuration: Chunking and AI revision settings
- stats: Note counts, embedding counts, job queue status
- components: Individual component health status

Use for monitoring and troubleshooting system issues.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // COLLECTIONS (FOLDERS) - Hierarchical organization
  // Organize notes into nested collections/folders for better structure
  // ============================================================================
  {
    name: "list_collections",
    description: `List all collections (folders) for organizing notes.

Collections provide hierarchical organization with nested folders.
Use parent_id to list children of a specific collection, or omit for root collections.

Returns:
- id: Collection UUID
- name: Collection name
- description: Optional description
- parent_id: Parent collection UUID (null for root)
- note_count: Number of notes in this collection
- created_at_utc: Creation timestamp`,
    inputSchema: {
      type: "object",
      properties: {
        parent_id: { type: "string", description: "Parent collection UUID (omit for root collections)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_collection",
    description: `Create a new collection (folder) for organizing notes.

Collections can be nested to create a folder hierarchy.
Set parent_id to create a subcollection within an existing collection.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Collection name" },
        description: { type: "string", description: "Optional description" },
        parent_id: { type: "string", description: "Parent collection UUID for nesting" },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_collection",
    description: `Get details of a specific collection by ID.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Collection UUID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "delete_collection",
    description: `Delete a collection.

Notes in the collection will be moved to uncategorized (not deleted).
Child collections will be moved to root level.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Collection UUID to delete" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "update_collection",
    description: `Update collection metadata including name, description, and parent.

Use to rename collections, add descriptions, or reorganize hierarchy by changing the parent.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Collection UUID to update" },
        name: { type: "string", description: "New collection name" },
        description: { type: "string", description: "New collection description" },
        parent_id: { type: ["string", "null"], format: "uuid", description: "New parent collection UUID, or null to move to root" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_collection_notes",
    description: `List all notes in a specific collection.

Returns paginated list of note summaries in the collection.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Collection UUID" },
        limit: { type: "number", description: "Maximum results (default: 50)", default: 50 },
        offset: { type: "number", description: "Pagination offset", default: 0 },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "move_note_to_collection",
    description: `Move a note to a different collection.

Set collection_id to move to a specific collection, or omit/null to move to uncategorized.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "Note UUID to move" },
        collection_id: { type: "string", description: "Target collection UUID (omit for uncategorized)" },
      },
      required: ["note_id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "explore_graph",
    description: `Explore the knowledge graph starting from a note.

Traverses semantic links to discover connected notes up to N hops away.
Returns a graph structure with:
- nodes: Discovered notes with id, title, and depth from start
- edges: Links between discovered notes with score and kind

Use for:
- Visualizing the neighborhood around a concept
- Finding clusters of related knowledge
- Discovering indirect connections between ideas

Parameters:
- id: Starting note UUID
- depth: How many hops to traverse (default: 2, max recommended: 3)
- max_nodes: Limit total nodes returned (default: 50)`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Starting note UUID" },
        depth: { type: "number", description: "Maximum hops to traverse (default: 2)", default: 2 },
        max_nodes: { type: "number", description: "Maximum nodes to return (default: 50)", default: 50 },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // NOTE TEMPLATES - Reusable note structures
  // ============================================================================
  {
    name: "list_templates",
    description: `List all available note templates.

Templates are reusable note structures with:
- Pre-defined content with {{variable}} placeholders
- Default tags and collection assignment
- Consistent formatting

Returns all templates sorted by name.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_template",
    description: `Create a new note template.

Templates support {{variable}} placeholders that get replaced during instantiation.
Example: "# Meeting Notes: {{topic}}\\n\\nDate: {{date}}\\n\\n## Attendees\\n{{attendees}}"

Set default_tags and collection_id to automatically apply them to notes created from this template.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Unique template name" },
        description: { type: "string", description: "What this template is for" },
        content: { type: "string", description: "Template content with {{variable}} placeholders" },
        format: { type: "string", description: "Content format (default: markdown)", default: "markdown" },
        default_tags: { type: "array", items: { type: "string" }, description: "Tags to apply by default" },
        collection_id: { type: "string", description: "Default collection for instantiated notes" },
      },
      required: ["name", "content"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_template",
    description: `Get a template by ID.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Template UUID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "delete_template",
    description: `Delete a template.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Template UUID to delete" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "update_template",
    description: `Update template metadata and content.

Use to modify template name, description, content, format, default tags, or default collection.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Template UUID to update" },
        name: { type: "string", description: "New template name" },
        description: { type: "string", description: "New template description" },
        content: { type: "string", description: "New template content with {{variable}} placeholders" },
        format: { type: "string", description: "Content format (e.g., markdown, plain)" },
        default_tags: { type: "array", items: { type: "string" }, description: "New default tags" },
        collection_id: { type: ["string", "null"], format: "uuid", description: "New default collection UUID, or null for none" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "instantiate_template",
    description: `Create a new note from a template.

Substitutes {{variable}} placeholders with provided values.
The resulting note goes through the full NLP enhancement pipeline.

Example:
  template content: "# Meeting: {{topic}}\\nDate: {{date}}"
  variables: { "topic": "Sprint Planning", "date": "2024-01-15" }
  result: "# Meeting: Sprint Planning\\nDate: 2024-01-15"`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Template UUID to instantiate" },
        variables: {
          type: "object",
          additionalProperties: { type: "string" },
          description: "Variable substitutions: { 'placeholder': 'value' }",
        },
        tags: { type: "array", items: { type: "string" }, description: "Override default tags" },
        collection_id: { type: "string", description: "Override default collection" },
        revision_mode: {
          type: "string",
          enum: ["full", "light", "none"],
          description: "AI revision mode (default: full)",
          default: "full",
        },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },

  // ============================================================================
  // EMBEDDING SETS - Focused semantic search collections
  // Create curated embedding sets for domain-specific semantic search
  // ============================================================================
  {
    name: "list_embedding_sets",
    description: `List all embedding sets available for semantic search.

Embedding sets are curated collections of notes optimized for focused semantic search.
The 'default' set contains all notes (global search). Power users can create focused sets
for specific domains or use cases.

Returns:
- id: Set UUID
- name: Display name
- slug: URL-friendly identifier (use this in search_notes)
- description: What this set is for
- purpose: Detailed purpose description
- usage_hints: When to use this set
- keywords: Discovery keywords
- document_count: Number of notes in set
- embedding_count: Number of embedding chunks
- index_status: pending/building/ready/stale/disabled

Use slug as the 'set' parameter in search_notes for focused semantic search.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_embedding_set",
    description: `Get detailed information about an embedding set.

Returns full set metadata including:
- All fields from list_embedding_sets
- criteria: Auto-membership rules (tags, collections, fts_query, etc.)
- agent_metadata: Information for AI agents about set usage

Use this to understand what's in a set before searching it.`,
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string", description: "Embedding set slug" },
      },
      required: ["slug"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_embedding_set",
    description: `Create a new embedding set for focused semantic search.

Embedding sets allow you to create curated collections for domain-specific queries.
For example:
- "ml-research" - Notes about machine learning
- "project-alpha" - Notes for a specific project
- "meeting-notes" - All meeting-related content

Modes:
- 'auto': Automatically include notes matching criteria
- 'manual': Only explicitly added notes
- 'mixed': Auto criteria + manual additions/exclusions

Criteria options (for auto/mixed modes):
- include_all: Include all notes (default set behavior)
- tags: Notes with any of these tags
- collections: Notes in any of these collections
- fts_query: Notes matching this full-text search
- created_after/before: Date range filters
- exclude_archived: Skip archived notes (default: true)

After creation, a background job builds the embedding index.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Display name for the set" },
        slug: { type: "string", description: "URL-friendly identifier (auto-generated if omitted)" },
        description: { type: "string", description: "What this set is for" },
        purpose: { type: "string", description: "Detailed purpose (helps AI agents decide when to use)" },
        usage_hints: { type: "string", description: "When and how to use this set" },
        keywords: { type: "array", items: { type: "string" }, description: "Discovery keywords" },
        mode: { type: "string", enum: ["auto", "manual", "mixed"], description: "Membership mode", default: "auto" },
        criteria: {
          type: "object",
          description: "Auto-membership criteria",
          properties: {
            include_all: { type: "boolean", description: "Include all notes" },
            tags: { type: "array", items: { type: "string" }, description: "Include notes with these tags" },
            collections: { type: "array", items: { type: "string" }, description: "Include notes in these collection UUIDs" },
            fts_query: { type: "string", description: "Include notes matching this FTS query" },
            exclude_archived: { type: "boolean", description: "Exclude archived notes", default: true },
          },
        },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "list_set_members",
    description: `List notes that are members of an embedding set.

Returns paginated list of notes in the set with their membership details.`,
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string", description: "Embedding set slug" },
        limit: { type: "number", description: "Maximum results", default: 50 },
        offset: { type: "number", description: "Pagination offset", default: 0 },
      },
      required: ["slug"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "add_set_members",
    description: `Add notes to an embedding set.

For manual or mixed mode sets, explicitly add notes to the set.
Added notes will be embedded and indexed for semantic search within the set.`,
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string", description: "Embedding set slug" },
        note_ids: { type: "array", items: { type: "string" }, description: "Note UUIDs to add" },
        added_by: { type: "string", description: "Who/what added these notes" },
      },
      required: ["slug", "note_ids"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "remove_set_member",
    description: `Remove a note from an embedding set.

Removes the note's membership and its embeddings from the set.`,
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string", description: "Embedding set slug" },
        note_id: { type: "string", description: "Note UUID to remove" },
      },
      required: ["slug", "note_id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "update_embedding_set",
    description: `Update embedding set metadata and configuration.

Modify name, description, purpose, usage hints, keywords, criteria, or mode.
Changing criteria or mode triggers a background refresh job.`,
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string", description: "Embedding set slug to update" },
        name: { type: "string", description: "New display name" },
        description: { type: "string", description: "New description" },
        purpose: { type: "string", description: "New detailed purpose" },
        usage_hints: { type: "string", description: "New usage hints" },
        keywords: { type: "array", items: { type: "string" }, description: "New discovery keywords" },
        criteria: { type: "object", description: "New auto-inclusion criteria" },
        mode: { type: "string", enum: ["auto", "manual", "mixed"], description: "New mode" },
      },
      required: ["slug"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "delete_embedding_set",
    description: `Delete an embedding set.

Removes the embedding set and all its associated embeddings. The default set cannot be deleted.
Notes remain in the database, only the embedding set index is removed.`,
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string", description: "Embedding set slug to delete (cannot be 'default')" },
      },
      required: ["slug"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "refresh_embedding_set",
    description: `Refresh an embedding set.

For auto/mixed mode sets, re-evaluates criteria to find matching notes.
Queues background jobs to update membership and rebuild embeddings.

Use this after adding notes that should match the criteria, or periodically
to ensure the set is current.`,
    inputSchema: {
      type: "object",
      properties: {
        slug: { type: "string", description: "Embedding set slug" },
      },
      required: ["slug"],
    },
    annotations: {
      destructiveHint: false,
    },
  },

  {
    name: "reembed_all",
    description: `Regenerate embeddings for all notes or a specific embedding set.

This queues a bulk re-embedding job that will process notes in the background.
Useful after changing embedding models or fixing embedding issues.

**Use cases:**
- After upgrading to a new embedding model
- To fix corrupted or missing embeddings
- To regenerate embeddings for a specific embedding set

**Parameters:**
- embedding_set_slug: Optional. If provided, only re-embed notes in this set.
- force: If true, regenerate even if embeddings already exist (future use).

**Returns:**
Job ID for tracking progress via list_jobs or get_queue_stats.

**Note:** This operation can take time for large knowledge bases.
Monitor job status to track completion.`,
    inputSchema: {
      type: "object",
      properties: {
        embedding_set_slug: {
          type: "string",
          description: "Optional: Limit re-embedding to specific embedding set"
        },
        force: {
          type: "boolean",
          description: "If true, regenerate even if embeddings exist (future use)",
          default: false
        },
      },
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "purge_note",
    description: `Permanently delete a note and ALL related data.

CAUTION: This is irreversible! Unlike soft delete, this permanently removes:
- The note itself
- All embeddings for the note
- All links (from and to this note)
- All tags associations
- All revision history
- Membership in all embedding sets

Queues a high-priority background job to perform the deletion.
Use delete_note for recoverable deletion, purge_note for permanent removal.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Note UUID to permanently delete" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "purge_notes",
    description: `Batch permanently delete multiple notes.

CAUTION: This is irreversible! Permanently deletes all specified notes
and their related data (embeddings, links, tags, revisions, set memberships).

Returns a summary of queued and failed operations.`,
    inputSchema: {
      type: "object",
      properties: {
        note_ids: {
          type: "array",
          items: { type: "string" },
          description: "Array of note UUIDs to permanently delete",
        },
      },
      required: ["note_ids"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "purge_all_notes",
    description: `Permanently delete ALL notes in the system.

EXTREME CAUTION: This wipes the entire knowledge base!
Use only for development cleanup or complete system reset.

Queues purge jobs for every note in the system.
Returns count of queued and failed operations.`,
    inputSchema: {
      type: "object",
      properties: {
        confirm: {
          type: "boolean",
          description: "Must be true to confirm this destructive operation",
        },
      },
      required: ["confirm"],
    },
    annotations: {
      destructiveHint: true,
    },
  },

  // ============================================================================
  // BACKUP & EXPORT
  // Tools for backing up and exporting the knowledge base
  // ============================================================================
  {
    name: "export_all_notes",
    description: `Export notes to portable JSON format. NO embeddings (regenerated on import).

RETURNS: {manifest, notes[], collections[], tags[], templates[]}

USE WHEN: Need portable backup, migration, or filtered export.
USE INSTEAD: knowledge_shard for tar.gz with links/embeddings, database_snapshot for full pg_dump.

FILTERS: starred_only, tags[], created_after/before (ISO 8601)`,
    inputSchema: {
      type: "object",
      properties: {
        filter: {
          type: "object",
          description: "Optional filters to scope the export (starred, tags, date range)",
          properties: {
            starred_only: { type: "boolean", description: "Only export starred notes" },
            tags: { type: "array", items: { type: "string" }, description: "Only export notes with these tags" },
            created_after: { type: "string", description: "Only export notes created after this date (ISO 8601)" },
            created_before: { type: "string", description: "Only export notes created before this date (ISO 8601)" },
          },
        },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "backup_now",
    description: `Run backup script: pg_dump → compress → ship to destinations (local/s3/rsync).

RETURNS: {status, output, timestamp} - Check output for success/failure details.

USE WHEN: Need automated backup with compression and remote shipping.
USE INSTEAD: database_snapshot for manual named backup with metadata.
NEXT: backup_status to verify, list_backups to see result.`,
    inputSchema: {
      type: "object",
      properties: {
        destinations: {
          type: "array",
          items: { type: "string", enum: ["local", "s3", "rsync"] },
          description: "Limit to specific destinations (default: all configured)",
        },
        dry_run: { type: "boolean", default: false, description: "Preview backup without executing (default: false)" },
      },
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "backup_status",
    description: `Get backup system health: total size, backup count, latest backup info.

RETURNS: {backup_directory, total_size_bytes, total_size_human, backup_count, latest_backup{path,size,modified}, status}
STATUS: "healthy" | "no_backups" | "error"

USE WHEN: Check if backups exist, verify system health, monitor disk usage.
NEXT: list_backups for full file listing, memory_info for storage breakdown.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "backup_download",
    description: `Same as export_all_notes but with download headers. Use for file saving.

USE INSTEAD: export_all_notes for in-memory processing, knowledge_shard for tar.gz format.`,
    inputSchema: {
      type: "object",
      properties: {
        starred_only: { type: "boolean", description: "Only include starred notes" },
        tags: { type: "array", items: { type: "string" }, description: "Only include notes with these tags" },
        created_after: { type: "string", description: "Only include notes created after this date (ISO 8601)" },
        created_before: { type: "string", description: "Only include notes created before this date (ISO 8601)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "backup_import",
    description: `Import notes from JSON backup (from export_all_notes/backup_download).

RETURNS: {status, imported{notes,collections,templates}, skipped, errors[]}
CONFLICTS: "skip" (keep existing) | "replace" (overwrite) | "merge" (add new only)

USE WHEN: Restore from JSON export, migrate between instances.
USE INSTEAD: knowledge_shard_import for tar.gz, database_restore for pg_dump.
TIP: Use dry_run=true first to validate.`,
    inputSchema: {
      type: "object",
      properties: {
        backup: {
          type: "object",
          description: "Data from export_all_notes",
          properties: {
            manifest: { type: "object" },
            notes: {
              type: "array",
              items: {
                type: "object",
                properties: {
                  id: { type: "string", description: "Original note UUID (optional)" },
                  original_content: { type: "string", description: "Original note content" },
                  content: { type: "string", description: "Note content (fallback if original_content missing)" },
                  revised_content: { type: "string", description: "AI-revised content (optional)" },
                  format: { type: "string", description: "Content format (default: markdown)" },
                  starred: { type: "boolean", description: "Star the note" },
                  archived: { type: "boolean", description: "Archive the note" },
                  tags: { type: "array", items: { type: "string" }, description: "Tags to apply" },
                },
              },
            },
            collections: { type: "array", description: "Collections to import" },
            templates: { type: "array", description: "Templates to import" },
          },
          required: ["notes"],
        },
        dry_run: { type: "boolean", description: "Validate without importing", default: false },
        on_conflict: {
          type: "string",
          enum: ["skip", "replace", "merge"],
          description: "Conflict resolution strategy",
          default: "skip",
        },
      },
      required: ["backup"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "knowledge_shard",
    description: `Create knowledge shard with notes, links, collections, tags, templates. Optionally include embeddings.

RETURNS: {filename, size_bytes, size_human, base64_data} - Decode base64_data and save as .tar.gz

COMPONENTS: notes, collections, tags, templates, links, embedding_sets, embeddings (large!), or "all"
DEFAULT: notes,collections,tags,templates,links,embedding_sets (no embeddings)

USE WHEN: Need knowledge shard with semantic links. Embeddings regenerate on import.
USE INSTEAD: database_snapshot for full pg_dump with everything, export_all_notes for simple JSON.
RESTORE: knowledge_shard_import with the base64_data`,
    inputSchema: {
      type: "object",
      properties: {
        include: {
          type: "string",
          description: "Components to include: comma-separated list (notes,collections,tags,templates,links,embedding_sets,embeddings) or 'all'. Default: notes,collections,tags,templates,links,embedding_sets",
        },
      },
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "knowledge_shard_import",
    description: `Import knowledge shard from knowledge_shard.

RETURNS: {status, manifest, imported{}, skipped{}, errors[]}
CONFLICTS: "skip" | "replace" | "merge"

USE WHEN: Restore from knowledge shard created by knowledge_shard.
USE INSTEAD: backup_import for JSON, database_restore for pg_dump.
TIP: dry_run=true first, skip_embedding_regen=true if shard has embeddings.`,
    inputSchema: {
      type: "object",
      properties: {
        shard_base64: { type: "string", description: "Base64 shard data from knowledge_shard.base64_data" },
        include: { type: "string", description: "Components to import (default: all)" },
        dry_run: { type: "boolean", default: false, description: "Preview import without writing data (default: false)" },
        on_conflict: { type: "string", enum: ["skip", "replace", "merge"], default: "skip", description: "Conflict resolution: skip (keep existing), replace (overwrite), merge (add new only)" },
        skip_embedding_regen: { type: "boolean", default: false, description: "Skip embedding regeneration if shard includes embeddings (default: false)" },
      },
      required: ["shard_base64"],
    },
    annotations: {
      destructiveHint: true,
    },
  },

  // ============================================================================
  // DATABASE BACKUP MANAGEMENT
  // Full pg_dump backups with metadata
  // ============================================================================
  {
    name: "database_snapshot",
    description: `Create full pg_dump backup with metadata. INCLUDES embeddings (unlike shard exports).

RETURNS: {success, filename, path, size_bytes, size_human, backup_type, created_at}
FILENAME: snapshot_database_YYYYMMDD_HHMMSS_[name].sql.gz
METADATA: Saved to .meta.json sidecar (title, description, note_count)

USE WHEN: Before major changes, manual checkpoint, disaster recovery prep.
USE INSTEAD: backup_now for automated/scheduled backups with shipping.
NEXT: list_backups to see it, get_backup_metadata to read metadata, database_restore to restore.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Filename suffix (alphanumeric/-/_)" },
        title: { type: "string", description: "Human-readable title" },
        description: { type: "string", description: "Why this backup was created" },
      },
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "database_restore",
    description: `DESTRUCTIVE: Replace entire database from backup file. Auto-creates prerestore snapshot.

RETURNS: {success, message, prerestore_backup, restored_from, reconnect_delay_ms}

PROCESS: prerestore snapshot → drop tables → restore → reconnect
RECOVERY: If restore fails, use prerestore_backup filename to restore again.

USE WHEN: Disaster recovery, rollback to previous state.
FIRST: list_backups to find filename, get_backup_metadata to verify correct backup.
WARNING: skip_snapshot=true is dangerous, always keep prerestore backup.`,
    inputSchema: {
      type: "object",
      properties: {
        filename: { type: "string", description: "Backup file from list_backups (e.g., snapshot_database_*.sql.gz)" },
        skip_snapshot: { type: "boolean", default: false, description: "DANGEROUS: Skip prerestore backup" },
      },
      required: ["filename"],
    },
    annotations: {
      destructiveHint: true,
    },
  },

  // ============================================================================
  // KNOWLEDGE ARCHIVES (.archive format)
  // Bundles backup file + metadata.json into a portable tar archive
  // ============================================================================
  {
    name: "knowledge_archive_download",
    description: `Download backup + metadata bundled as .archive file (portable tar archive).

RETURNS: {success, filename, size_bytes, base64_data}
FORMAT: Tar containing backup file (.sql.gz/.tar.gz) + metadata.json
EXTENSION: .archive (knowledge archive)

USE WHEN: Export backup for transfer to another system, offline storage, sharing.
ADVANTAGE: Metadata travels WITH backup - never lose title/description/context.
WORKFLOW: list_backups → knowledge_archive_download → transfer → knowledge_archive_upload`,
    inputSchema: {
      type: "object",
      properties: {
        filename: { type: "string", description: "Backup filename from list_backups (e.g., snapshot_database_*.sql.gz)" },
      },
      required: ["filename"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "knowledge_archive_upload",
    description: `Upload .archive file (backup + metadata bundled). Extracts both to backup directory.

RETURNS: {success, filename, path, size_bytes, size_human, metadata}
EXTRACTS: Backup file → backup directory, metadata.json → .meta.json sidecar

USE WHEN: Restore backup from another system, import transferred archive.
WORKFLOW: knowledge_archive_download (source) → transfer → knowledge_archive_upload (target) → database_restore
TIP: Metadata is automatically extracted and preserved.`,
    inputSchema: {
      type: "object",
      properties: {
        archive_base64: { type: "string", description: "Base64-encoded .archive file" },
        filename: { type: "string", description: "Original filename (optional, for logging)" },
      },
      required: ["archive_base64"],
    },
    annotations: {
      destructiveHint: false,
    },
  },

  {
    name: "list_backups",
    description: `List all backup files with size, hash, type. Sorted newest first.

RETURNS: {shards: [{filename, path, size_bytes, size_human, modified, sha256, backup_type}]}
TYPES: snapshot, upload, prerestore, auto, shard (tar.gz), unknown

USE WHEN: Browse backups before restore, verify integrity, check disk usage.
NEXT: get_backup_info for details, get_backup_metadata for title/description, database_restore to restore.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_backup_info",
    description: `Get file details for a specific backup: size, sha256, type, manifest (for shards).

RETURNS: {filename, path, size_bytes, size_human, sha256, backup_type, manifest?}

USE WHEN: Verify backup integrity before restore, check shard contents.
USE INSTEAD: get_backup_metadata for title/description, list_backups for all files.`,
    inputSchema: {
      type: "object",
      properties: {
        filename: { type: "string", description: "From list_backups" },
      },
      required: ["filename"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_backup_metadata",
    description: `Get human-readable metadata from .meta.json sidecar: title, description, note_count.

RETURNS: {has_metadata, filename, metadata?: {title, description, backup_type, created_at, note_count, source}}
NO METADATA: Returns {has_metadata: false, backup_type, message} - use update_backup_metadata to add.

USE WHEN: Identify what a backup contains, verify correct backup before restore.
WORKFLOW: list_backups → get_backup_metadata → database_restore`,
    inputSchema: {
      type: "object",
      properties: {
        filename: { type: "string", description: "From list_backups" },
      },
      required: ["filename"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "update_backup_metadata",
    description: `Set/update title and description for a backup. Creates .meta.json if missing.

RETURNS: {success, filename, metadata}

USE WHEN: Document old backups, fix missing descriptions, organize backup library.
TIP: database_snapshot auto-creates metadata if title/description provided.`,
    inputSchema: {
      type: "object",
      properties: {
        filename: { type: "string", description: "Backup filename from list_backups" },
        title: { type: "string", description: "Human-readable title for the backup" },
        description: { type: "string", description: "Description of backup contents or purpose" },
      },
      required: ["filename"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "memory_info",
    description: `Get storage sizing and hardware recommendations for capacity planning.

RETURNS: {summary, embedding_sets[], storage, recommendations}
SUMMARY: total_notes, total_embeddings, total_links, total_collections, total_tags, total_templates
STORAGE: database_total_bytes, embedding_table_bytes, notes_table_bytes, estimated_memory_for_search
RECOMMENDATIONS: min_ram_gb, recommended_ram_gb, notes[] (GPU vs CPU usage explained)

USE WHEN: Plan hardware, estimate scaling costs, understand storage breakdown.
KEY INSIGHT: GPU = embedding generation (Ollama), CPU = vector search (pgvector). More RAM = faster search.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  // ============================================================================
  // SKOS CONCEPTS - W3C SKOS-compliant hierarchical tag system
  // ============================================================================
  {
    name: "list_concept_schemes",
    description: `List all SKOS concept schemes (vocabularies/namespaces).

A concept scheme is a container for related concepts, like "topics", "domains", or "imported:library_of_congress".

USE WHEN: Discover available vocabularies, check which schemes exist.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_concept_scheme",
    description: `Create a new concept scheme (vocabulary namespace).

Use schemes to organize related concepts, e.g., "projects", "technologies", "domains".

RETURNS: {id} - UUID of the new scheme.`,
    inputSchema: {
      type: "object",
      properties: {
        notation: { type: "string", description: "Short code (e.g., 'topics', 'domains')" },
        title: { type: "string", description: "Human-readable title" },
        description: { type: "string", description: "Purpose and scope of this vocabulary" },
        uri: { type: "string", description: "Optional canonical URI" },
      },
      required: ["notation", "title"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_concept_scheme",
    description: "Get details of a specific concept scheme by ID.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept scheme" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "delete_concept_scheme",
    description: `Delete a concept scheme.

Removes a concept scheme. If the scheme has concepts, use force=true to delete them as well.
System and default schemes are protected from deletion.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "UUID of the concept scheme to delete" },
        force: { type: "boolean", description: "Delete even if scheme has concepts", default: false },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "search_concepts",
    description: `Search and filter SKOS concepts (hierarchical tags).

Searches across prefLabel, altLabel, and hiddenLabel. Returns concepts with their preferred labels and metadata.

USE WHEN: Find existing concepts before creating new ones, browse taxonomy.`,
    inputSchema: {
      type: "object",
      properties: {
        q: { type: "string", description: "Search query (matches labels)" },
        scheme_id: { type: "string", description: "Filter by scheme UUID" },
        status: { type: "string", enum: ["candidate", "approved", "deprecated"], description: "Filter by status" },
        top_only: { type: "boolean", description: "Only return top-level concepts (no broader)" },
        limit: { type: "number", default: 50, description: "Maximum results to return (default: 50)" },
        offset: { type: "number", default: 0, description: "Pagination offset (default: 0)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_concept",
    description: `Create a new SKOS concept (hierarchical tag).

Concepts support:
- prefLabel: Primary display name (required)
- altLabel: Alternative names/synonyms
- hiddenLabel: Hidden search terms (typos, codes)
- definition: Formal definition
- scope_note: Usage guidance
- broader_ids: Parent concepts (max 3 for polyhierarchy)
- related_ids: Non-hierarchical associations
- facet_type: PMEST classification (personality, matter, energy, space, time)

RETURNS: {id} - UUID of the new concept.`,
    inputSchema: {
      type: "object",
      properties: {
        scheme_id: { type: "string", description: "UUID of the scheme" },
        pref_label: { type: "string", description: "Primary label (required)" },
        notation: { type: "string", description: "Short code within scheme" },
        alt_labels: { type: "array", items: { type: "string" }, description: "Alternative labels/synonyms" },
        definition: { type: "string", description: "Formal definition" },
        scope_note: { type: "string", description: "Usage guidance" },
        broader_ids: { type: "array", items: { type: "string" }, description: "Parent concept UUIDs (max 3)" },
        related_ids: { type: "array", items: { type: "string" }, description: "Related concept UUIDs" },
        facet_type: { type: "string", enum: ["personality", "matter", "energy", "space", "time"], description: "PMEST facet" },
        facet_domain: { type: "string", description: "Domain context" },
      },
      required: ["scheme_id", "pref_label"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_concept",
    description: "Get a concept with its preferred label.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_concept_full",
    description: `Get full concept details including all labels, notes, and relationships.

RETURNS: concept + labels[] + notes[] + broader[] + narrower[] + related[] + mappings[] + schemes[]

USE WHEN: Need complete context about a concept including its position in the hierarchy.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "update_concept",
    description: "Update a concept's properties (notation, status, facet).",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
        notation: { type: "string", description: "Short code/identifier for the concept (e.g., 'ML', 'NLP')" },
        status: { type: "string", enum: ["candidate", "approved", "deprecated", "obsolete"], description: "Concept lifecycle status" },
        deprecation_reason: { type: "string", description: "Reason for deprecation (required when setting status to deprecated)" },
        replaced_by_id: { type: "string", description: "UUID of replacement concept when deprecating" },
        facet_type: { type: "string", enum: ["personality", "matter", "energy", "space", "time"], description: "PMEST facet classification for the concept" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "delete_concept",
    description: "Delete a concept (must have no tags applied to notes).",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "autocomplete_concepts",
    description: `Fast autocomplete for concept labels. Searches across pref/alt/hidden labels.

USE WHEN: Building tag input UIs, quick lookup while typing.`,
    inputSchema: {
      type: "object",
      properties: {
        q: { type: "string", description: "Prefix to match" },
        limit: { type: "number", default: 10, description: "Maximum suggestions to return (default: 10)" },
      },
      required: ["q"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_broader",
    description: "Get broader (parent) concepts for a concept.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "add_broader",
    description: `Add a broader (parent) relationship. Max 3 parents allowed (polyhierarchy limit).

Example: add_broader({id: rust_concept, target_id: programming_languages_concept})`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the child concept" },
        target_id: { type: "string", description: "UUID of the parent concept" },
      },
      required: ["id", "target_id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_narrower",
    description: "Get narrower (child) concepts for a concept.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "add_narrower",
    description: "Add a narrower (child) relationship (inverse of broader).",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the parent concept" },
        target_id: { type: "string", description: "UUID of the child concept" },
      },
      required: ["id", "target_id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_related",
    description: "Get related (associative, non-hierarchical) concepts.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "add_related",
    description: `Add a related (associative) relationship. Symmetric - both concepts will be related to each other.

Example: Python related to Data Science (not hierarchical, just associated).`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept" },
        target_id: { type: "string", description: "UUID of the related concept" },
      },
      required: ["id", "target_id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "tag_note_concept",
    description: `Tag a note with a SKOS concept.

is_primary: Mark as the primary/main concept for this note.

RETURNS: {success: true}`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
        concept_id: { type: "string", description: "UUID of the concept" },
        is_primary: { type: "boolean", default: false, description: "Mark as primary tag" },
      },
      required: ["note_id", "concept_id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "untag_note_concept",
    description: "Remove a concept tag from a note.",
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
        concept_id: { type: "string", description: "UUID of the concept" },
      },
      required: ["note_id", "concept_id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_note_concepts",
    description: "Get all SKOS concepts tagged on a note with their labels.",
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
      },
      required: ["note_id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_governance_stats",
    description: `Get taxonomy governance statistics for a scheme.

RETURNS: {total_concepts, candidates, approved, deprecated, orphans, under_used, avg_note_count, max_depth}

USE WHEN: Audit taxonomy health, find issues like orphan tags or under-used concepts.`,
    inputSchema: {
      type: "object",
      properties: {
        scheme_id: { type: "string", description: "UUID of the scheme (uses default if not provided)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_top_concepts",
    description: "Get top-level concepts in a scheme (concepts with no broader relations).",
    inputSchema: {
      type: "object",
      properties: {
        scheme_id: { type: "string", description: "UUID of the scheme" },
      },
      required: ["scheme_id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  // ============================================================================
  // NOTE VERSIONING (#104) - Dual-track version history
  // ============================================================================
  {
    name: "list_note_versions",
    description: `List all versions for a note (both original and AI revision tracks).

Returns version history for both user content (original track) and AI-enhanced content (revision track).

RETURNS: {
  note_id, current_original_version, current_revision_number,
  original_versions: [{version_number, created_at_utc, created_by, is_current}],
  revised_versions: [{id, revision_number, created_at_utc, model, is_user_edited}]
}

USE WHEN: Review edit history, find when content changed, prepare for restore.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
      },
      required: ["note_id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_note_version",
    description: `Get a specific version of a note content.

track: "original" for user content history, "revision" for AI-enhanced history

RETURNS: Version content with metadata (hash, created_at, created_by for original; model, summary for revision).

USE WHEN: View a previous version before deciding to restore.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
        version: { type: "integer", description: "Version number to retrieve" },
        track: {
          type: "string",
          enum: ["original", "revision"],
          default: "original",
          description: "Which track: original (user content) or revision (AI enhanced)"
        },
      },
      required: ["note_id", "version"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "restore_note_version",
    description: `Restore a note to a previous version (creates new version, doesn't overwrite history).

restore_tags: If true, also restore the tags that were present at that version snapshot.

WARNING: This modifies the note content! A new version is created from the restored content.

RETURNS: {success, restored_from_version, new_version, restore_tags}`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
        version: { type: "integer", description: "Version number to restore" },
        restore_tags: {
          type: "boolean",
          default: false,
          description: "Also restore tags from the version snapshot"
        },
      },
      required: ["note_id", "version"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "delete_note_version",
    description: `Delete a specific version from history (cannot delete current version).

WARNING: This permanently removes the version from history!

RETURNS: {success, deleted_version}`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
        version: { type: "integer", description: "Version number to delete" },
      },
      required: ["note_id", "version"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "diff_note_versions",
    description: `Generate a unified diff between two versions of a note.

RETURNS: Plain text unified diff (--- version N / +++ version M format).

USE WHEN: See exactly what changed between versions.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", description: "UUID of the note" },
        from_version: { type: "integer", description: "Version to diff from (older)" },
        to_version: { type: "integer", description: "Version to diff to (newer)" },
      },
      required: ["note_id", "from_version", "to_version"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // CHUNK-AWARE DOCUMENT HANDLING (Ticket #113)
  // ============================================================================
  {
    name: "get_full_document",
    description: `Get the full reconstructed document for a note.

For chunked documents (large documents split during ingestion), this stitches all chunks back together in order, removing overlaps to reconstruct the original content.

For regular notes, returns the content as-is.

Returns:
- id: Note ID (or chain ID for chunked documents)
- title: Document title (with chunk suffixes removed)
- content: Full reconstructed content
- is_chunked: Whether this is a chunked document
- chunks: Array of chunk metadata (null for regular notes)
  - id: Chunk note ID
  - sequence: Chunk number in sequence
  - title: Chunk title
  - byte_range: [start, end] byte positions
- total_chunks: Number of chunks (null for regular notes)
- tags: All tags from all chunks (deduplicated)
- created_at, updated_at: Timestamps

Use cases:
- Downloading complete documents that were split during ingestion
- Viewing full original content before chunking
- Exporting documents with chunk metadata`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note or chain ID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "search_with_dedup",
    description: `Search notes with explicit deduplication enabled (same as search_notes but more explicit about chunk handling).

When searching chunked documents, multiple chunks from the same document can match. Deduplication groups these chunks and returns only the best-scoring chunk per document, with metadata about how many chunks matched.

This is the default behavior of search_notes, but this tool makes it explicit for clarity.

Search modes:
- 'hybrid' (default): Combines keyword matching with semantic similarity
- 'fts': Full-text search only
- 'semantic': Vector similarity only

Returns:
- results: Array of deduplicated search hits with chunk metadata
  - note_id: Best matching chunk ID
  - score: Relevance score
  - snippet: Text excerpt
  - title: Note title
  - tags: Associated tags
  - chain_info: Chunk metadata (if chunked)
    - chain_id: Document chain ID
    - total_chunks: Total chunks in document
    - chunks_matched: How many chunks matched
- query: Original search query
- total: Number of results

Use when you want to:
- Search large documents without duplicate results
- Understand which chunks matched from chunked documents
- Get document-level results rather than chunk-level`,
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Search query (natural language or keywords)" },
        limit: { type: "number", description: "Maximum results (default: 20)", default: 20 },
        mode: { type: "string", enum: ["hybrid", "fts", "semantic"], description: "Search mode", default: "hybrid" },
        set: { type: "string", description: "Embedding set slug to restrict semantic search (optional)" },
      },
      required: ["query"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_chunk_chain",
    description: `Get all chunks in a document chain with metadata.

For chunked documents, this returns information about all chunks in the chain, including their sequence, titles, and byte ranges in the original document.

For regular (non-chunked) notes, this returns the single note with is_chunked: false.

Returns same structure as get_full_document:
- id: Chain ID
- title: Original document title
- content: Full reconstructed content (if include_content=true)
- is_chunked: true for chunked documents
- chunks: Array of all chunks with:
  - id: Chunk note ID
  - sequence: Position in chain (1, 2, 3...)
  - title: Chunk title with "Part X/Y" suffix
  - byte_range: [start, end] positions in original
- total_chunks: Number of chunks
- tags: Deduplicated tags from all chunks
- created_at, updated_at: Timestamps

Use cases:
- Inspecting how a document was chunked
- Getting individual chunk IDs for targeted retrieval
- Understanding chunk boundaries and overlap
- Debugging chunking strategy`,
    inputSchema: {
      type: "object",
      properties: {
        chain_id: { type: "string", description: "UUID of the chain (first chunk ID or any chunk in chain)" },
        include_content: { type: "boolean", description: "Include full reconstructed content (default: true)", default: true },
      },
      required: ["chain_id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // DOCUMENTATION - Expanded help for AI agents
  // ============================================================================
  {
    name: "get_documentation",
    description: `Get expanded documentation and usage guidance for Matric Memory.

Returns detailed documentation on specific topics to help agents use the system effectively. Start with "overview" to understand the system, then drill into specific topics.

Available topics:

**Core Features:**
- "overview" - System overview, capabilities, and tool categories
- "notes" - Note creation, revision modes, lifecycle, and best practices
- "search" - Search modes, multilingual support, query syntax, embedding sets
- "chunking" - Document chunking strategies for optimal embedding quality

**Organization:**
- "concepts" - SKOS hierarchical tagging system (schemes, concepts, relations)
- "skos_collections" - SKOS concept groupings, Turtle export, relation management
- "collections" - Folder organization for notes
- "archives" - Named archive containers with lifecycle management
- "templates" - Reusable note structures with variable substitution
- "document_types" - Document type registry, auto-detection, chunking strategies

**Data Management:**
- "versioning" - Dual-track version history and restoration
- "backup" - Backup strategies, knowledge shards, snapshots
- "encryption" - PKE public-key encryption (X25519 + AES-256-GCM)

**Operations:**
- "jobs" - Background job monitoring, reprocessing, queue management
- "observability" - Knowledge health, stale notes, orphan tags, timeline, activity
- "provenance" - W3C PROV provenance chains and dedicated backlinks
- "embedding_configs" - Embedding model configuration and MRL support

**Reference:**
- "workflows" - Usage patterns and advanced workflow examples
- "troubleshooting" - Common issues, permission reference, debugging tips
- "all" - Complete documentation (large response)

USE THIS TOOL when you need:
- Detailed guidance on using specific features
- Best practices for content creation
- Understanding how components interact
- Troubleshooting unexpected behavior`,
    inputSchema: {
      type: "object",
      properties: {
        topic: {
          type: "string",
          enum: ["overview", "notes", "search", "concepts", "skos_collections", "chunking", "versioning", "collections", "archives", "templates", "document_types", "backup", "encryption", "jobs", "observability", "provenance", "embedding_configs", "workflows", "troubleshooting", "all"],
          description: "Documentation topic to retrieve",
          default: "overview"
        },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // PUBLIC KEY ENCRYPTION (PKE) - Wallet-style E2E encryption
  // These tools enable secure data sharing using public key addresses
  // ============================================================================
  {
    name: "pke_generate_keypair",
    description: `Generate a new X25519 keypair for public-key encryption.

Creates a wallet-style identity consisting of:
- **Private key** - Stored encrypted with your passphrase (never share this!)
- **Public key** - Can be shared freely
- **Address** - Human-friendly identifier (mm:...) that others use to encrypt data for you

The address is derived from your public key using BLAKE3 hashing with a checksum,
similar to cryptocurrency wallet addresses. Share your address with anyone who
wants to send you encrypted data.

**Security Notes:**
- Use a strong passphrase (12+ characters) to protect your private key
- Back up your private key file - losing it means losing access to encrypted data
- Generate separate keypairs for different purposes (work, personal, etc.)`,
    inputSchema: {
      type: "object",
      properties: {
        passphrase: {
          type: "string",
          description: "Passphrase to protect the private key (minimum 12 characters)"
        },
        output_dir: {
          type: "string",
          description: "Directory to save keys (default: current directory)"
        },
        label: {
          type: "string",
          description: "Optional label for the key (e.g., 'Work Key')"
        },
      },
      required: ["passphrase"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_get_address",
    description: `Get the public address from a public key file.

Returns the mm:... address that can be shared with others.
This address is what senders use to encrypt data for you.`,
    inputSchema: {
      type: "object",
      properties: {
        public_key_path: {
          type: "string",
          description: "Path to the public key file"
        },
      },
      required: ["public_key_path"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "pke_encrypt",
    description: `Encrypt a file for one or more recipients using public-key encryption.

This uses the MMPKE01 format which provides:
- **Multi-recipient support** - Encrypt once for multiple people
- **Forward secrecy** - Each encryption uses fresh ephemeral keys
- **Authenticated encryption** - AES-256-GCM detects tampering

Recipients are specified by their public key files. To encrypt for someone,
you need their public key file (which contains their mm:... address).

The encrypted file can only be decrypted by someone with the corresponding
private key for one of the recipient public keys.`,
    inputSchema: {
      type: "object",
      properties: {
        input_path: {
          type: "string",
          description: "Path to the file to encrypt"
        },
        output_path: {
          type: "string",
          description: "Path for the encrypted output file"
        },
        recipients: {
          type: "array",
          items: { type: "string" },
          description: "Paths to recipient public key files"
        },
      },
      required: ["input_path", "output_path", "recipients"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_decrypt",
    description: `Decrypt a file using your private key.

Decrypts a file that was encrypted for your public key address.
You must have the private key file and its passphrase.

Returns the decrypted content and metadata (original filename, creation date).`,
    inputSchema: {
      type: "object",
      properties: {
        input_path: {
          type: "string",
          description: "Path to the encrypted file"
        },
        output_path: {
          type: "string",
          description: "Path for the decrypted output"
        },
        private_key_path: {
          type: "string",
          description: "Path to your encrypted private key file"
        },
        passphrase: {
          type: "string",
          description: "Passphrase for the private key"
        },
      },
      required: ["input_path", "output_path", "private_key_path", "passphrase"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_list_recipients",
    description: `List the recipient addresses that can decrypt an encrypted file.

Returns the mm:... addresses of all recipients without decrypting the file.
Useful for determining if you can decrypt a file or who it was intended for.`,
    inputSchema: {
      type: "object",
      properties: {
        input_path: {
          type: "string",
          description: "Path to the encrypted file"
        },
      },
      required: ["input_path"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "pke_verify_address",
    description: `Verify that a public key address is valid.

Checks that the mm:... address has:
- Correct prefix
- Valid Base58 encoding
- Correct checksum (catches typos)
- Supported version

Returns validation status and version info.`,
    inputSchema: {
      type: "object",
      properties: {
        address: {
          type: "string",
          description: "The address to verify (mm:...)"
        },
      },
      required: ["address"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  // ============================================================================
  // PKE KEYSET MANAGEMENT - Auto-provisioning for multi-identity workflows
  // ============================================================================
  {
    name: "pke_list_keysets",
    description: `List all available PKE keysets in the local keystore.

Returns an array of keyset information including:
- **name** - The keyset identifier
- **address** - The mm:... public address
- **public_key_path** - Path to the public key file
- **private_key_path** - Path to the encrypted private key file
- **created** - Timestamp when the keyset was created

Keysets are stored in ~/.matric/keys/{name}/ and provide named identities
for different encryption contexts (personal, work, projects, etc.).`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "pke_create_keyset",
    description: `Create a new named PKE keyset.

Creates a new keyset directory at ~/.matric/keys/{name}/ containing:
- public_key.pem - Public key (shareable)
- private_key.enc - Encrypted private key (secured with passphrase)

**Use Cases:**
- Separate work and personal identities
- Project-specific encryption keys
- Team-shared keysets (via secure key exchange)
- Multi-device synchronization (backup/restore)

**Security:**
- Passphrase must be at least 12 characters
- Private key is encrypted with Argon2id + AES-256-GCM
- Each keyset is isolated in its own directory`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Keyset name (alphanumeric, hyphens, underscores only)"
        },
        passphrase: {
          type: "string",
          description: "Strong passphrase to protect the private key (minimum 12 characters)"
        },
      },
      required: ["name", "passphrase"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_get_active_keyset",
    description: `Get the currently active keyset.

Returns the keyset information for the currently active keyset, or null if no
keyset is active. The active keyset is read from ~/.matric/keys/active file.

The active keyset is used as the default identity for encryption/decryption
operations in auto-provisioning workflows.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "pke_set_active_keyset",
    description: `Set the active keyset by name.

Sets the specified keyset as the active identity. This writes the keyset name
to ~/.matric/keys/active file for use by other tools.

**Workflow:**
1. Create or list keysets to see available identities
2. Set active keyset for current context
3. Use encryption/decryption tools with active keyset
4. Switch keysets as needed for different contexts`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Name of the keyset to activate"
        },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_export_keyset",
    description: `Export a keyset to a directory for backup or transfer.

Copies the keyset's public and private key files to an export directory. The
exported keyset can be transferred to another machine and imported using
pke_import_keyset.

**Output:**
- Creates a timestamped directory containing:
  - public.key - The public key file
  - private.key.enc - The encrypted private key file
  - keyset.json - Metadata about the export

**Security:**
- The private key remains encrypted with its original passphrase
- The export directory path is returned for reference
- Users should securely transfer the exported files

**Use cases:**
- Backup keysets before system changes
- Transfer identity to another device
- Share public key with collaborators (public.key only)`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Name of the keyset to export"
        },
        output_dir: {
          type: "string",
          description: "Directory to export to (default: ~/.matric/exports/)"
        },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_import_keyset",
    description: `Import a keyset from files or an exported directory.

Imports key files into the managed keysets directory. Can import from:
1. An exported keyset directory (from pke_export_keyset)
2. Explicit public and private key file paths

**Import from export directory:**
Provide import_path pointing to a directory containing public.key and
private.key.enc files.

**Import from explicit paths:**
Provide both public_key_path and private_key_path pointing to the key files.

**Security:**
- The imported private key retains its original passphrase
- You'll need the original passphrase for decryption operations
- A new keyset name must be provided (cannot overwrite existing)`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Name for the imported keyset (must be unique)"
        },
        import_path: {
          type: "string",
          description: "Path to exported keyset directory (contains public.key, private.key.enc)"
        },
        public_key_path: {
          type: "string",
          description: "Path to public key file (use with private_key_path)"
        },
        private_key_path: {
          type: "string",
          description: "Path to encrypted private key file (use with public_key_path)"
        },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "pke_delete_keyset",
    description: `Delete a keyset from the managed keys directory.

Permanently removes a keyset and its associated key files. This action cannot
be undone - ensure you have a backup if needed.

**Behavior:**
- Deletes both public and private key files
- If the deleted keyset was active, clears the active keyset
- Cannot delete non-existent keysets

**Warning:**
- Data encrypted with this keyset's public key will become unrecoverable
- Export the keyset first if you might need it later`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Name of the keyset to delete"
        },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  // ============================================================================
  // DOCUMENT TYPES - Registry management for code/prose/config types
  // ============================================================================
  {
    name: "list_document_types",
    description: `List all document types with optional category filter and detail level.

By default (detail=false), returns just type names (~500 tokens).
With detail=true, returns full type objects with all fields (~14k tokens).

Returns 131+ pre-configured types across 19 categories including code, prose,
config, markup, data, API specs, IaC, database, shell, docs, package managers,
observability, legal, communication, research, creative, media, and personal.

**Categories:**
- code: Programming languages (rust, python, javascript, etc.)
- prose: Written content (markdown, asciidoc, org-mode, etc.)
- config: Configuration files (yaml, toml, json, ini, etc.)
- markup: Structured markup (html, xml, latex, etc.)
- data: Data formats (csv, json, parquet, etc.)
- api-spec: API specifications (openapi, graphql, protobuf, etc.)
- iac: Infrastructure as Code (terraform, ansible, docker, etc.)
- database: Database schemas and queries (sql, migration, etc.)
- shell: Shell scripts (bash, zsh, fish, powershell, etc.)
- docs: Documentation (README, CHANGELOG, etc.)
- package: Package manifests (package.json, Cargo.toml, etc.)
- observability: Logs, metrics, traces
- legal: Licenses, terms, policies
- communication: Email, chat, memos
- research: Papers, notes, lab notebooks
- creative: Stories, screenplays, lyrics
- media: Subtitles, transcripts
- personal: Journals, diaries, TODO lists
- custom: User-defined types

**Use cases:**
- Discover available document types for chunking strategies
- Filter types by category for specialized workflows
- Understand chunking behavior for different content types`,
    inputSchema: {
      type: "object",
      properties: {
        detail: {
          type: "boolean",
          description: "Return full document type objects (true) or just names (false, default). Default false returns ~500 tokens, true returns ~14k tokens.",
          default: false
        },
        category: {
          type: "string",
          description: "Filter by category: code, prose, config, markup, data, api-spec, iac, database, shell, docs, package, observability, legal, communication, research, creative, media, personal, custom"
        }
      }
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_document_type",
    description: `Get detailed information about a specific document type by name.

Returns comprehensive type details including:
- Display name and category
- Description and use cases
- File extensions and filename patterns
- Content patterns for detection
- Chunking strategy configuration
- System vs. custom type indicator

**Example types:**
- rust: Rust source code (semantic chunking)
- markdown: Markdown prose (per_section chunking)
- openapi: OpenAPI specs (syntactic chunking)
- terraform: Terraform configs (per_unit chunking)

**Use cases:**
- Verify chunking strategy for a file type
- Understand detection patterns
- Check file extension mappings`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Document type name (e.g., 'rust', 'markdown', 'openapi')"
        }
      },
      required: ["name"]
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_document_type",
    description: `Create a custom document type for specialized content.

Define a new type with custom chunking strategies and detection rules.
System types cannot be modified, so create custom variants if needed.

**Chunking strategies:**
- semantic: AST/structure-aware (best for code)
- syntactic: Pattern-based structure detection
- fixed: Fixed-size chunks with overlap
- hybrid: Combines multiple strategies
- per_section: Split on headers/sections (best for docs)
- per_unit: One logical unit per chunk (configs, small files)
- whole: Entire document as one chunk (small files)

**Example:**
Create type for Dockerfiles with per_unit chunking:
{
  "name": "dockerfile-custom",
  "display_name": "Custom Dockerfile",
  "category": "iac",
  "description": "Custom Dockerfile with specialized chunking",
  "file_extensions": [".dockerfile"],
  "filename_patterns": ["Dockerfile.*"],
  "chunking_strategy": "per_unit"
}`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Unique type identifier (lowercase, alphanumeric with hyphens)"
        },
        display_name: {
          type: "string",
          description: "Human-readable name"
        },
        category: {
          type: "string",
          description: "Category for organization (code, prose, config, etc.)"
        },
        description: {
          type: "string",
          description: "Description of the type and its use cases"
        },
        file_extensions: {
          type: "array",
          items: { type: "string" },
          description: "File extensions for detection (e.g., ['.rs', '.rust'])"
        },
        filename_patterns: {
          type: "array",
          items: { type: "string" },
          description: "Filename patterns for detection (e.g., ['Cargo.toml'])"
        },
        content_patterns: {
          type: "array",
          items: { type: "string" },
          description: "Regex patterns for content-based detection"
        },
        chunking_strategy: {
          type: "string",
          enum: ["semantic", "syntactic", "fixed", "hybrid", "per_section", "per_unit", "whole"],
          description: "How to split documents of this type into chunks"
        }
      },
      required: ["name", "display_name", "category"]
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "update_document_type",
    description: `Update a document type's configuration.

Modify an existing custom document type. System types cannot be updated.

**Updatable fields:**
- display_name
- description
- file_extensions
- filename_patterns
- content_patterns
- chunking_strategy

**Note:** Changing chunking_strategy may require re-chunking existing documents.`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Type name to update (must be custom, not system type)"
        },
        display_name: {
          type: "string",
          description: "Human-readable name"
        },
        description: {
          type: "string",
          description: "Description of the type"
        },
        file_extensions: {
          type: "array",
          items: { type: "string" },
          description: "File extensions for detection"
        },
        filename_patterns: {
          type: "array",
          items: { type: "string" },
          description: "Filename patterns for detection"
        },
        content_patterns: {
          type: "array",
          items: { type: "string" },
          description: "Regex patterns for content-based detection"
        },
        chunking_strategy: {
          type: "string",
          enum: ["semantic", "syntactic", "fixed", "hybrid", "per_section", "per_unit", "whole"],
          description: "How to split documents into chunks"
        }
      },
      required: ["name"]
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "delete_document_type",
    description: `Delete a custom document type.

Permanently removes a custom type from the registry. System types cannot be deleted.

**Warning:**
- This action cannot be undone
- Existing documents with this type will revert to auto-detection
- Consider updating instead of deleting if you need to modify behavior`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Type name to delete (must be custom, not system type)"
        }
      },
      required: ["name"]
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "detect_document_type",
    description: `Auto-detect document type from filename and/or content.

Uses pattern matching with confidence scoring to identify the document type.
Checks file extensions, filename patterns, and content patterns in order.

**Detection strategy:**
1. Exact filename match (highest confidence)
2. File extension match
3. Content pattern match (if content provided)
4. Default to generic types if no match

**Use cases:**
- Preview type detection before storing a document
- Validate detection logic
- Debug chunking behavior

**Example:**
Detect from filename only:
{ "filename": "main.rs" } → rust (code/semantic)

Detect from content:
{ "content": "#!/usr/bin/env python3\\nimport..." } → python (code/semantic)

Combined detection (most accurate):
{ "filename": "script.py", "content": "#!/usr/bin/env python..." } → python`,
    inputSchema: {
      type: "object",
      properties: {
        filename: {
          type: "string",
          description: "Filename to detect from (e.g., 'main.rs', 'docker-compose.yml')"
        },
        content: {
          type: "string",
          description: "Content snippet for magic pattern detection (first 1000 chars recommended)"
        }
      }
    },
    annotations: {
      readOnlyHint: true,
    },
  },



  // ============================================================================
  // ARCHIVE MANAGEMENT
  // Manage parallel memory archives with schema-level data isolation
  // ============================================================================
  {
    name: "list_archives",
    description: `List all memory archives.

Archives provide schema-level data isolation, allowing multiple independent memory spaces within the same database.

Returns array of archives with:
- id: Unique identifier
- name: Archive name
- schema_name: PostgreSQL schema name
- description: Optional description
- created_at: Creation timestamp
- note_count: Number of notes in archive
- size_bytes: Total size in bytes
- is_default: Whether this is the default archive

**Use cases:**
- View available memory archives
- Check archive statistics
- Identify default archive`,
    inputSchema: {
      type: "object",
      properties: {}
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  {
    name: "create_archive",
    description: `Create a new memory archive.

Creates a new PostgreSQL schema with complete table structure for isolated memory storage. Each archive maintains its own:
- Notes and embeddings
- Collections and tags
- Links and metadata

**Parameters:**
- name: Archive name (alphanumeric with hyphens/underscores)
- description: Optional description

**Example:**
{ "name": "project-xyz", "description": "XYZ project knowledge base" }`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Archive name (alphanumeric with hyphens/underscores)"
        },
        description: {
          type: "string",
          description: "Optional archive description"
        }
      },
      required: ["name"]
    },
    annotations: {
      destructiveHint: false,
    },
  },

  {
    name: "get_archive",
    description: `Get details for a specific archive.

Returns full archive information including statistics.

**Parameters:**
- name: Archive name

**Returns:**
- Complete archive info with note count and size statistics`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Archive name"
        }
      },
      required: ["name"]
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  {
    name: "update_archive",
    description: `Update archive metadata.

Currently supports updating the archive description.

**Parameters:**
- name: Archive name
- description: New description (or null to clear)

**Example:**
{ "name": "project-xyz", "description": "Updated description" }`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Archive name"
        },
        description: {
          type: ["string", "null"],
          description: "New description (null to clear)"
        }
      },
      required: ["name"]
    },
    annotations: {
      destructiveHint: false,
    },
  },

  {
    name: "delete_archive",
    description: `Delete an archive and all its data.

**WARNING:** This permanently deletes:
- All notes in the archive
- All embeddings
- All collections, tags, and links
- The archive schema itself

This operation cannot be undone.

**Parameters:**
- name: Archive name

**Example:**
{ "name": "old-archive" }`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Archive name to delete"
        }
      },
      required: ["name"]
    },
    annotations: {
      destructiveHint: true,
    },
  },

  {
    name: "set_default_archive",
    description: `Set an archive as the default.

The default archive is used when no specific archive is specified in operations. Only one archive can be default at a time.

**Parameters:**
- name: Archive name to set as default

**Example:**
{ "name": "main-archive" }`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Archive name to set as default"
        }
      },
      required: ["name"]
    },
    annotations: {
      destructiveHint: false,
    },
  },

  {
    name: "get_archive_stats",
    description: `Get current statistics for an archive.

Calculates and returns:
- note_count: Number of non-deleted notes
- size_bytes: Total database size for archive tables
- last_accessed: Timestamp of last access/stats update

This also updates the archive registry with current statistics.

**Parameters:**
- name: Archive name

**Returns:**
- Current archive statistics`,
    inputSchema: {
      type: "object",
      properties: {
        name: {
          type: "string",
          description: "Archive name"
        }
      },
      required: ["name"]
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // SKOS COLLECTIONS (#450) - Grouped concept management
  // ============================================================================
  {
    name: "list_skos_collections",
    description: `List SKOS collections (ordered or unordered groups of concepts).

SKOS Collections allow grouping concepts for:
- Ordered lists (e.g., difficulty levels: beginner → intermediate → advanced)
- Thematic groups (e.g., "Core ML Concepts")
- Custom taxonomic views

Returns array of collections with member counts.`,
    inputSchema: {
      type: "object",
      properties: {
        scheme_id: { type: "string", format: "uuid", description: "Filter by concept scheme" },
        limit: { type: "number", default: 50, description: "Maximum collections to return (default: 50)" },
        offset: { type: "number", default: 0, description: "Pagination offset (default: 0)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_skos_collection",
    description: `Create a new SKOS collection.

Collections can be:
- **Ordered**: Members have explicit sequence (skos:OrderedCollection)
- **Unordered**: Members have no defined order (skos:Collection)

Example: Create an ordered difficulty progression:
{ pref_label: "Learning Path", ordered: true }`,
    inputSchema: {
      type: "object",
      properties: {
        scheme_id: { type: "string", format: "uuid", description: "Parent concept scheme UUID" },
        pref_label: { type: "string", description: "Collection name" },
        notation: { type: "string", description: "Short code (optional)" },
        definition: { type: "string", description: "What this collection groups" },
        ordered: { type: "boolean", default: false, description: "Whether members have explicit order" },
      },
      required: ["scheme_id", "pref_label"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_skos_collection",
    description: `Get a SKOS collection with its members.

Returns collection metadata and all member concepts (in order if ordered collection).`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Collection UUID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "update_skos_collection",
    description: `Update a SKOS collection's metadata.

Can change label, notation, definition, or ordered status.
Note: Changing ordered status may affect member ordering.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Collection UUID" },
        pref_label: { type: "string", description: "New collection name" },
        notation: { type: "string", description: "New short code" },
        definition: { type: "string", description: "New definition" },
        ordered: { type: "boolean", description: "Change ordered status" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "delete_skos_collection",
    description: `Delete a SKOS collection.

Removes the collection but NOT the member concepts.
Concepts remain in the scheme, only the grouping is removed.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Collection UUID to delete" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "add_skos_collection_member",
    description: `Add a concept to a SKOS collection.

For ordered collections, specify position (0-indexed).
Omit position to append at end.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Collection UUID" },
        concept_id: { type: "string", format: "uuid", description: "Concept UUID to add" },
        position: { type: "number", description: "Position in ordered collection (0-indexed)" },
      },
      required: ["id", "concept_id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "remove_skos_collection_member",
    description: `Remove a concept from a SKOS collection.

Removes the membership, not the concept itself.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Collection UUID" },
        concept_id: { type: "string", format: "uuid", description: "Concept UUID to remove" },
      },
      required: ["id", "concept_id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },

  // ============================================================================
  // SKOS RELATION REMOVAL (#451)
  // ============================================================================
  {
    name: "remove_broader",
    description: `Remove a broader (parent) relationship from a concept.

Also removes the inverse narrower relationship from the target.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Child concept UUID" },
        target_id: { type: "string", format: "uuid", description: "Parent concept UUID to unlink" },
      },
      required: ["id", "target_id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "remove_narrower",
    description: `Remove a narrower (child) relationship from a concept.

Also removes the inverse broader relationship from the target.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Parent concept UUID" },
        target_id: { type: "string", format: "uuid", description: "Child concept UUID to unlink" },
      },
      required: ["id", "target_id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "remove_related",
    description: `Remove a related (associative) relationship between concepts.

This is symmetric - removes the relationship in both directions.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "First concept UUID" },
        target_id: { type: "string", format: "uuid", description: "Related concept UUID to unlink" },
      },
      required: ["id", "target_id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },

  // ============================================================================
  // KNOWLEDGE HEALTH (#452)
  // ============================================================================
  {
    name: "get_knowledge_health",
    description: `Get overall knowledge base health metrics.

Returns actionable metrics for maintenance:
- orphan_tags: Tags not used by any notes
- stale_notes: Notes not updated in N days
- unlinked_notes: Notes with no semantic links
- concept_health: SKOS taxonomy health stats
- embedding_coverage: Notes missing embeddings

Use this as a dashboard to identify maintenance needs. Follow up with specific diagnostic tools (get_orphan_tags, get_stale_notes, get_unlinked_notes) for details. See get_documentation({ topic: "observability" }) for maintenance workflows.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_orphan_tags",
    description: `List tags that are not used by any notes.

Returns tags with zero note count, candidates for cleanup or consolidation.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_stale_notes",
    description: `Find notes that haven't been updated recently.

Useful for content refresh initiatives or identifying abandoned knowledge.`,
    inputSchema: {
      type: "object",
      properties: {
        days: { type: "number", default: 90, description: "Days since last update to consider stale" },
        limit: { type: "number", default: 50, description: "Maximum results" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_unlinked_notes",
    description: `Find notes with no semantic links (isolated knowledge).

These notes may need:
- More content to establish connections
- Manual linking to related concepts
- Review for relevance`,
    inputSchema: {
      type: "object",
      properties: {
        limit: { type: "number", default: 50, description: "Maximum results" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_tag_cooccurrence",
    description: `Analyze which tags frequently appear together.

Useful for:
- Discovering implicit tag relationships
- Identifying candidates for SKOS related relationships
- Understanding tagging patterns`,
    inputSchema: {
      type: "object",
      properties: {
        min_count: { type: "number", default: 2, description: "Minimum co-occurrence count" },
        limit: { type: "number", default: 50, description: "Maximum results" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // NOTE PROVENANCE & BACKLINKS (#453)
  // ============================================================================
  {
    name: "get_note_backlinks",
    description: `Get dedicated backlinks for a note (notes that link TO this note).

This is a focused view of incoming links. For both directions, use get_note_links.

Returns array of linking notes with:
- id: Source note UUID
- title: Source note title
- score: Link similarity score
- snippet: Context showing the connection`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Note UUID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_note_provenance",
    description: `Get W3C PROV provenance chain for a note.

Tracks the complete derivation history:
- Original creation (prov:wasGeneratedBy)
- AI revisions (prov:wasDerivedFrom)
- Template instantiation source
- Version history references

Useful for understanding how content evolved and verifying sources. Returns the full chain from creation through all modifications. See get_documentation({ topic: "provenance" }) for detailed usage patterns.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Note UUID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_memory_provenance",
    description: `Get the complete file provenance chain for a note's attachments. Returns temporal-spatial provenance including location, device, and capture time.

This provides the full lifecycle history of files attached to notes:
- Original capture location (GPS coordinates)
- Device information (camera, phone, scanner)
- Capture timestamp (when the photo/file was created)
- File format and technical metadata

Use this to understand the origin and context of media attachments.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", format: "uuid", description: "The note ID" },
      },
      required: ["note_id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // JOB MANAGEMENT (#454)
  // ============================================================================
  {
    name: "get_job",
    description: `Get detailed information about a specific job.

Returns full job details including:
- status: pending/processing/completed/failed
- job_type: ai_revision/embedding/linking/etc.
- result: Output from successful job
- error: Error details if failed
- created_at, started_at, completed_at: Timing info`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Job UUID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_pending_jobs_count",
    description: `Get quick count of pending jobs.

Returns just the count of jobs waiting to be processed.
Faster than list_jobs when you only need the count for status display.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // NOTE REPROCESS (#455)
  // ============================================================================
  {
    name: "reprocess_note",
    description: `Manually trigger NLP pipeline steps on a note.

Use to:
- Re-embed after model changes
- Regenerate links after content fixes
- Force title regeneration
- Fix processing issues

Steps (array of strings):
- "ai_revision": Re-run AI enhancement
- "embedding": Regenerate embeddings
- "linking": Recalculate semantic links
- "title_generation": Regenerate title
- "all": Run complete pipeline

If steps is omitted, runs all steps.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Note UUID" },
        steps: {
          type: "array",
          items: { type: "string", enum: ["ai_revision", "embedding", "linking", "title_generation", "all"] },
          description: "Pipeline steps to run (omit for all)",
        },
        force: { type: "boolean", default: false, description: "Force reprocessing even if already processed" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },

  // ============================================================================
  // TIMELINE & ACTIVITY (#456)
  // ============================================================================
  {
    name: "get_notes_timeline",
    description: `Get note creation/update timeline bucketed by time period.

Returns buckets with counts for visualization:
- bucket: Time period start
- created: Notes created in period
- updated: Notes updated in period

Granularity options: hour, day, week, month`,
    inputSchema: {
      type: "object",
      properties: {
        granularity: { type: "string", enum: ["hour", "day", "week", "month"], default: "day", description: "Time bucket size: hour, day, week, or month (default: day)" },
        start_date: { type: "string", description: "Start date (ISO 8601)" },
        end_date: { type: "string", description: "End date (ISO 8601)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_notes_activity",
    description: `Get activity feed of recent note events.

Returns chronological list of events:
- event_type: created, updated, deleted, restored, tagged, linked
- note_id: Affected note
- timestamp: When it happened
- details: Event-specific data

Use for audit trails and activity dashboards.`,
    inputSchema: {
      type: "object",
      properties: {
        limit: { type: "number", default: 50, description: "Maximum events to return (default: 50)" },
        offset: { type: "number", default: 0, description: "Pagination offset (default: 0)" },
        event_types: {
          type: "array",
          items: { type: "string", enum: ["created", "updated", "deleted", "restored", "tagged", "linked"] },
          description: "Filter by event types",
        },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // EMBEDDING CONFIG MANAGEMENT (#457)
  // ============================================================================
  {
    name: "list_embedding_configs",
    description: `List all embedding model configurations.

Returns available embedding models with:
- id: Config UUID
- name: Display name
- model: Model identifier (e.g., "nomic-embed-text")
- dimensions: Vector dimensions
- provider: Ollama, OpenAI, etc.
- is_default: Whether this is the default config

Use this to discover which embedding models are available before creating embedding sets or changing the default model. See get_documentation({ topic: "embedding_configs" }) for model selection guidance.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_default_embedding_config",
    description: `Get the default embedding configuration.

Returns the config used for new notes when no specific config is specified. Check this to understand what embedding model and dimensions are being used for vector search.

Returns: { id, name, model, dimensions, provider, is_default: true }`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "get_embedding_config",
    description: `Get details of a specific embedding configuration.

Returns full config including model name, dimensions, provider, and whether it's the default. Use list_embedding_configs to find available config IDs.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Config UUID" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_embedding_config",
    description: `Create a new embedding model configuration.

Use to add new embedding models or configure different dimension settings. Supports MRL (Matryoshka) models where dimensions can be reduced for storage savings (e.g., 768 → 256).

After creating a config, use it with embedding sets or set as default for all new notes. Existing notes will need reprocessing to use the new model.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Display name" },
        model: { type: "string", description: "Model identifier" },
        dimension: { type: "number", description: "Vector dimension (e.g., 768, 384, 1536)" },
        provider: { type: "string", description: "Provider (ollama, openai, etc.)" },
        is_default: { type: "boolean", default: false, description: "Set as default config" },
        chunk_size: { type: "integer", description: "Maximum characters per chunk for text splitting (default: 1000)" },
        chunk_overlap: { type: "integer", description: "Overlap characters between chunks for context preservation (default: 100)" },
      },
      required: ["name", "model", "dimension"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "update_embedding_config",
    description: `Update an embedding configuration.

Can change name, model, dimensions, provider, or default status. Setting is_default: true will unset the previous default. After changing model or dimensions, existing notes need reprocessing (use reembed_all or reprocess_note).`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Config UUID" },
        name: { type: "string", description: "New display name" },
        model: { type: "string", description: "New model identifier" },
        dimension: { type: "number", description: "New vector dimension" },
        provider: { type: "string", description: "New provider" },
        is_default: { type: "boolean", description: "Set as default" },
        chunk_size: { type: "integer", description: "Maximum characters per chunk for text splitting" },
        chunk_overlap: { type: "integer", description: "Overlap characters between chunks for context preservation" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
    },
  },

  {
    name: "delete_embedding_config",
    description: `Delete an embedding configuration.

Cannot delete the default config. Remove or reassign default status first.
Existing embeddings using this config are not affected but won't be regenerated with the deleted config.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Config UUID to delete" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: true,
    },
  },


  // ============================================================================
  // FILE ATTACHMENTS (#14)
  // ============================================================================
  {
    name: "upload_attachment",
    description: `Upload a file attachment to a note.

Files are stored with content-hash deduplication using filesystem storage.

The extraction strategy (how to extract content) is automatically determined from the MIME type.
Document type (semantic classification) can be set explicitly or is classified asynchronously after extraction.

The data parameter must be base64-encoded file content.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", format: "uuid", description: "Note UUID to attach the file to" },
        filename: { type: "string", description: "Filename (e.g., 'photo.jpg', 'document.pdf')" },
        content_type: { type: "string", description: "MIME type (e.g., 'image/jpeg', 'application/pdf')" },
        data: { type: "string", description: "Base64-encoded file content" },
        document_type_id: { type: "string", format: "uuid", description: "Optional: explicit document type UUID override (skips auto-classification)" },
      },
      required: ["note_id", "filename", "content_type", "data"],
    },
    annotations: { destructiveHint: false },
  },
  {
    name: "list_attachments",
    description: `List all file attachments for a note.

Returns attachment metadata including filename, content type, size, status, and timestamps.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", format: "uuid", description: "Note UUID" },
      },
      required: ["note_id"],
    },
    annotations: { readOnlyHint: true },
  },
  {
    name: "get_attachment",
    description: `Get metadata for a specific attachment.

Returns full attachment details including extracted metadata (EXIF, etc.) if available.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Attachment UUID" },
      },
      required: ["id"],
    },
    annotations: { readOnlyHint: true },
  },
  {
    name: "download_attachment",
    description: `Download a file attachment.

Returns the file content as base64-encoded data along with content type and filename.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Attachment UUID" },
      },
      required: ["id"],
    },
    annotations: { readOnlyHint: true },
  },
  {
    name: "delete_attachment",
    description: `Delete a file attachment.

Removes the attachment record. If no other attachments reference the same blob (content hash), the underlying blob is also deleted.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "Attachment UUID to delete" },
      },
      required: ["id"],
    },
    annotations: { destructiveHint: true },
  },
  // ============================================================================
  // SKOS TURTLE EXPORT (#460)
  // ============================================================================
  {
    name: "export_skos_turtle",
    description: `Export SKOS taxonomy as W3C RDF/Turtle format.

Returns valid Turtle syntax for interoperability with other SKOS tools:
- Protégé, TopBraid, PoolParty
- RDF visualization tools
- Other knowledge management systems

Includes:
- Concept schemes with metadata
- Concepts with all labels (preferred, alternative, hidden)
- Broader/narrower/related relations
- Collection memberships and ordering

Omit scheme_id to export all schemes. See get_documentation({ topic: "skos_collections" }) for collection details.`,
    inputSchema: {
      type: "object",
      properties: {
        scheme_id: { type: "string", format: "uuid", description: "Export specific scheme (omit for all)" },
      },
    },
    annotations: {
      readOnlyHint: true,
    },
  },

];

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
  shard_base64: data,
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
knowledge_shard_import({ shard_base64: other_data, on_conflict: "replace" })
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

  provenance: `# Note Provenance & Backlinks

Track content origins and discover reverse connections in the knowledge graph.

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

1. Check provenance: Where did this content come from?
2. Check backlinks: What references this content?
3. Use together for complete content lineage and impact analysis`,

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

      // Check for MCP or read scope
      const scopes = (introspection.scope || "").split(" ");
      if (!scopes.includes("mcp") && !scopes.includes("read")) {
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
    });

    // Create a new MCP server for this connection and connect
    const mcpServer = createMcpServer();
    await tokenStorage.run({ token: req.accessToken }, async () => {
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
    await tokenStorage.run({ token: session.token }, async () => {
      await session.transport.handlePostMessage(req, res, req.body);
    });
  });

  // StreamableHTTP transport on root path (newer transport, POST to initialize/send, GET to receive)
  app.post("/", validateToken, async (req, res) => {
    const sessionId = req.headers['mcp-session-id'];
    console.log(`[mcp] POST, sessionId from header: ${sessionId || 'none'}`);

    const existingSession = sessionId ? transports.get(sessionId) : undefined;

    let transport;
    if (existingSession && existingSession.type === 'streamable') {
      // Reuse existing transport for this session
      console.log(`[mcp] Reusing existing transport for session ${sessionId}`);
      transport = existingSession.transport;
    } else {
      // Create new StreamableHTTP transport
      transport = new StreamableHTTPServerTransport({
        sessionIdGenerator: () => crypto.randomUUID(),
      });

      // Create and connect new MCP server for this transport
      const mcpServer = createMcpServer();
      await mcpServer.connect(transport);
      console.log(`[mcp] Transport connected (sessionId set after handleRequest)`);

      // Set up cleanup on close
      transport.onclose = () => {
        console.log(`[mcp] Transport closed: ${transport?.sessionId}`);
        if (transport?.sessionId) {
          transports.delete(transport.sessionId);
        }
      };
    }

    // Handle the request with token context
    try {
      await tokenStorage.run({ token: req.accessToken }, async () => {
        await transport.handleRequest(req, res);
      });

      // Store transport AFTER handleRequest - sessionId is only set after first request
      if (transport.sessionId && !transports.has(transport.sessionId)) {
        console.log(`[mcp] Storing transport with sessionId: ${transport.sessionId}`);
        transports.set(transport.sessionId, { transport, token: req.accessToken, type: 'streamable' });
      }
    } catch (error) {
      console.error(`[mcp] Error handling request:`, error);
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

    await tokenStorage.run({ token: session.token }, async () => {
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
  // Returns this MCP server as the resource, with authorization_servers pointing to main API
  app.get("/.well-known/oauth-protected-resource", (req, res) => {
    res.json({
      resource: MCP_BASE_URL,
      authorization_servers: [process.env.ISSUER_URL || API_BASE],
      bearer_methods_supported: ["header"],
      scopes_supported: ["mcp", "read"],
    });
  });

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
