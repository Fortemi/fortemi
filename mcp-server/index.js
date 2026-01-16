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

const API_BASE = process.env.MATRIC_MEMORY_URL || "https://memory.integrolabs.net";
const API_KEY = process.env.MATRIC_MEMORY_API_KEY || null;
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
      name: "matric-memory",
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
          if (args.limit) params.set("limit", args.limit);
          if (args.offset) params.set("offset", args.offset);
          if (args.filter) params.set("filter", args.filter);
          if (args.tags) params.set("tags", Array.isArray(args.tags) ? args.tags.join(",") : args.tags);
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

        case "get_collection_notes": {
          const noteParams = new URLSearchParams();
          if (args.limit) noteParams.set("limit", args.limit);
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
          if (args.limit) jobParams.set("limit", args.limit);
          if (args.offset) jobParams.set("offset", args.offset);
          result = await apiRequest("GET", `/api/v1/jobs?${jobParams}`);
          break;
        }

        case "get_queue_stats":
          result = await apiRequest("GET", "/api/v1/jobs/stats");
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
        tags: { type: "array", items: { type: "string" }, description: "Filter by tags (notes must have ALL specified tags)" },
        created_after: { type: "string", description: "Filter notes created after this date (ISO 8601 format, e.g. '2024-01-01T00:00:00Z')" },
        created_before: { type: "string", description: "Filter notes created before this date (ISO 8601 format)" },
        updated_after: { type: "string", description: "Filter notes updated after this date (ISO 8601 format)" },
        updated_before: { type: "string", description: "Filter notes updated before this date (ISO 8601 format)" },
      },
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
  },
  {
    name: "search_notes",
    description: `Search notes using hybrid full-text and semantic search.

Search modes:
- 'hybrid' (default): Combines keyword matching with semantic similarity for best results
- 'fts': Full-text search only - exact keyword matching
- 'semantic': Vector similarity only - finds conceptually related content

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
      },
      required: ["query"],
    },
  },
  {
    name: "list_tags",
    description: "List all tags in the knowledge base with usage counts.",
    inputSchema: { type: "object", properties: {} },
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
- For factual/personal notes, use "light" mode to prevent hallucination`,
    inputSchema: {
      type: "object",
      properties: {
        content: { type: "string", description: "Note content in markdown format" },
        tags: { type: "array", items: { type: "string" }, description: "Optional explicit tags" },
        revision_mode: {
          type: "string",
          enum: ["full", "light", "none"],
          description: "AI revision mode: 'full' (default) for contextual expansion, 'light' for formatting only without inventing details, 'none' to skip AI revision entirely",
          default: "full"
        },
      },
      required: ["content"],
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
              tags: { type: "array", items: { type: "string" }, description: "Optional tags" },
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
      },
      required: ["id"],
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
  },
  {
    name: "set_note_tags",
    description: `Set tags for a note (replaces all existing user tags).

AI-generated tags are preserved separately. This only affects user-defined tags.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the note" },
        tags: { type: "array", items: { type: "string" }, description: "New tags (replaces existing)" },
      },
      required: ["id", "tags"],
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
  },
];

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
      authorization_servers: [API_BASE.replace('http://127.0.0.1:3000', 'https://memory.integrolabs.net')],
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
