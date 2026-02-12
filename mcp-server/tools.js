// MCP Tool Definitions — JSON Schema draft 2020-12
//
// IMPORTANT: inputSchema must be valid JSON Schema draft 2020-12.
// Common pitfalls when defining tool schemas:
//
//   1. NULLABLE TYPES: Do NOT use `type: ["string", "null"]` (draft-04/07 syntax).
//      Instead use: `anyOf: [{ type: "string" }, { type: "null" }]`
//
//   2. EXCLUSIVE BOUNDS: Do NOT use `exclusiveMinimum: true` (draft-04 boolean syntax).
//      Instead use: `exclusiveMinimum: <number>` (the actual exclusive bound value)
//      Example: `exclusiveMinimum: 0` means "must be > 0"
//
//   3. These constraints are enforced by the Claude API at connection time.
//      Invalid schemas cause "JSON schema is invalid" errors that prevent
//      ALL tools from loading, not just the broken one.
//
//   Run `npm run validate:schemas` to check all schemas before committing.
//

export default [
  // ============================================================================
  // READ OPERATIONS - No processing triggered
  // ============================================================================
  {
    name: "list_notes",
    description: `List notes from the active memory only.

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
    description: `Search notes using hybrid full-text and semantic search within the active memory only.

**IMPORTANT**: Currently search is limited to the default (public) archive. Non-default archives return an error for search operations.

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
        lat: { type: "number", description: "Latitude in decimal degrees (-90 to 90)", minimum: -90, maximum: 90 },
        lon: { type: "number", description: "Longitude in decimal degrees (-180 to 180)", minimum: -180, maximum: 180 },
        radius: { type: "number", description: "Search radius in meters (default: 1000)", default: 1000, exclusiveMinimum: 0 },
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
        lat: { type: "number", description: "Latitude in decimal degrees (-90 to 90)", minimum: -90, maximum: 90 },
        lon: { type: "number", description: "Longitude in decimal degrees (-180 to 180)", minimum: -180, maximum: 180 },
        radius: { type: "number", description: "Search radius in meters (default: 1000)", default: 1000, exclusiveMinimum: 0 },
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
    name: "create_provenance_location",
    description: `Create a provenance location record with GPS coordinates.

Records a geographic location that can be linked to file attachments via file provenance.
Use this to establish spatial context for captured content.

**Parameters:**
- latitude/longitude: GPS coordinates (required)
- source: How location was obtained (gps_exif, device_api, user_manual, geocoded, ai_estimated)
- confidence: Location accuracy level (high, medium, low, unknown)
- altitude_m: Altitude in meters (optional)
- horizontal_accuracy_m: GPS accuracy in meters (optional)

**Returns:** { id: "uuid" } — use this ID when creating file provenance records.`,
    inputSchema: {
      type: "object",
      properties: {
        latitude: { type: "number", minimum: -90, maximum: 90, description: "Latitude in decimal degrees" },
        longitude: { type: "number", minimum: -180, maximum: 180, description: "Longitude in decimal degrees" },
        altitude_m: { type: "number", description: "Altitude in meters" },
        horizontal_accuracy_m: { type: "number", description: "Horizontal accuracy in meters" },
        vertical_accuracy_m: { type: "number", description: "Vertical accuracy in meters" },
        heading_degrees: { type: "number", minimum: 0, maximum: 360, description: "Compass heading in degrees" },
        speed_mps: { type: "number", minimum: 0, description: "Speed in meters per second" },
        named_location_id: { type: "string", format: "uuid", description: "Link to a named location" },
        source: { type: "string", enum: ["gps_exif", "device_api", "user_manual", "geocoded", "ai_estimated"], description: "How location was obtained" },
        confidence: { type: "string", enum: ["high", "medium", "low", "unknown"], description: "Location accuracy level" },
      },
      required: ["latitude", "longitude", "source", "confidence"],
    },
    annotations: { destructiveHint: false },
  },
  {
    name: "create_named_location",
    description: `Create a named location (landmark, address, place).

Registers a semantic place name that can be linked to provenance location records.
Named locations provide human-readable context for geographic coordinates.

**Parameters:**
- name: Display name for the location (required)
- location_type: Category — home, work, poi, city, region, country (required)
- latitude/longitude: GPS coordinates (required)
- address fields: Optional address components
- radius_m: Location boundary radius in meters

**Returns:** Created location record with ID.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Location display name" },
        location_type: { type: "string", enum: ["home", "work", "poi", "city", "region", "country"], description: "Location category" },
        latitude: { type: "number", minimum: -90, maximum: 90, description: "Latitude" },
        longitude: { type: "number", minimum: -180, maximum: 180, description: "Longitude" },
        radius_m: { type: "number", minimum: 0, description: "Boundary radius in meters" },
        address_line: { type: "string", description: "Street address" },
        locality: { type: "string", description: "City/town" },
        admin_area: { type: "string", description: "State/province" },
        country: { type: "string", description: "Country name" },
        country_code: { type: "string", description: "ISO country code" },
        postal_code: { type: "string", description: "Postal/ZIP code" },
        timezone: { type: "string", description: "IANA timezone" },
        altitude_m: { type: "number", description: "Altitude in meters" },
        is_private: { type: "boolean", description: "Whether location is private" },
        metadata: { type: "object", description: "Additional metadata" },
      },
      required: ["name", "location_type", "latitude", "longitude"],
    },
    annotations: { destructiveHint: false },
  },
  {
    name: "create_provenance_device",
    description: `Register a capture device for provenance tracking.

Records device information (camera, phone, scanner) that can be linked to file provenance.
Devices are automatically deduplicated by make + model.

**Parameters:**
- device_make: Manufacturer (required, e.g., "Apple", "Samsung", "Canon")
- device_model: Model name (required, e.g., "iPhone 15 Pro", "Galaxy S24")
- device_os/software: Optional OS and software details
- has_gps/has_accelerometer: Sensor capabilities

**Returns:** Device record with ID (returns existing device if make+model already registered).`,
    inputSchema: {
      type: "object",
      properties: {
        device_make: { type: "string", description: "Device manufacturer" },
        device_model: { type: "string", description: "Device model name" },
        device_os: { type: "string", description: "Operating system" },
        device_os_version: { type: "string", description: "OS version" },
        software: { type: "string", description: "Capture software name" },
        software_version: { type: "string", description: "Software version" },
        has_gps: { type: "boolean", description: "Device has GPS" },
        has_accelerometer: { type: "boolean", description: "Device has accelerometer" },
        sensor_metadata: { type: "object", description: "Additional sensor details" },
        device_name: { type: "string", description: "User-friendly device name" },
      },
      required: ["device_make", "device_model"],
    },
    annotations: { destructiveHint: false },
  },
  {
    name: "create_file_provenance",
    description: `Create a file provenance record linking an attachment to spatial-temporal context.

Establishes the W3C PROV chain for a file attachment — when it was captured,
where it was captured, and what device was used. This enables temporal-spatial
memory search (search_memories_by_location, search_memories_by_time).

**Workflow:**
1. Create a location with create_provenance_location (if spatial context known)
2. Register a device with create_provenance_device (if device known)
3. Create file provenance linking the attachment to location + device + time

**Parameters:**
- attachment_id: The file attachment UUID (required)
- location_id: UUID from create_provenance_location (optional)
- device_id: UUID from create_provenance_device (optional)
- capture_time_start/end: When the file was captured (ISO 8601)
- event_type: photo, video, audio, scan, screenshot, recording
- event_title: Human-readable event description

**Returns:** { id: "uuid" } — the provenance record ID.`,
    inputSchema: {
      type: "object",
      properties: {
        attachment_id: { type: "string", format: "uuid", description: "Attachment UUID to link provenance to" },
        note_id: { type: "string", format: "uuid", description: "Optional note UUID to directly associate provenance with a note" },
        capture_time_start: { type: "string", format: "date-time", description: "Capture start time (ISO 8601)" },
        capture_time_end: { type: "string", format: "date-time", description: "Capture end time (ISO 8601)" },
        capture_timezone: { type: "string", description: "Capture timezone (e.g., America/New_York)" },
        capture_duration_seconds: { type: "number", minimum: 0, description: "Duration in seconds" },
        time_source: { type: "string", enum: ["exif", "file_mtime", "user_manual", "ai_estimated"], description: "How capture time was determined" },
        time_confidence: { type: "string", enum: ["high", "medium", "low", "unknown"], description: "Time accuracy level" },
        location_id: { type: "string", format: "uuid", description: "Location UUID from create_provenance_location" },
        device_id: { type: "string", format: "uuid", description: "Device UUID from create_provenance_device" },
        event_type: { type: "string", enum: ["photo", "video", "audio", "scan", "screenshot", "recording"], description: "Type of capture event" },
        event_title: { type: "string", description: "Human-readable event title" },
        event_description: { type: "string", description: "Detailed event description" },
        raw_metadata: { type: "object", description: "Raw EXIF/XMP/IPTC metadata" },
      },
      required: ["attachment_id"],
    },
    annotations: { destructiveHint: false },
  },
  {
    name: "create_note_provenance",
    description: `Create a note provenance record linking a note directly to spatial-temporal context.

Unlike create_file_provenance (which requires an attachment), this tool attaches
location/time/device context directly to a note. Use this when a note itself has
spatial-temporal meaning without needing a file attachment.

**Examples:**
- Meeting notes taken at a specific location
- Travel journal entries with GPS coordinates
- Field observations with time and place context

**Workflow:**
1. Create a location with create_provenance_location (if spatial context known)
2. Optionally register a device with create_provenance_device
3. Create note provenance linking the note to location + device + time

**Parameters:**
- note_id: The note UUID (required)
- location_id: UUID from create_provenance_location (optional)
- device_id: UUID from create_provenance_device (optional)
- capture_time_start/end: When the note content was created (ISO 8601)
- event_type: created, modified, accessed, shared
- event_title: Human-readable event description

**Returns:** { id: "uuid" } — the provenance record ID.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", format: "uuid", description: "Note UUID to attach provenance to" },
        capture_time_start: { type: "string", format: "date-time", description: "Event start time (ISO 8601)" },
        capture_time_end: { type: "string", format: "date-time", description: "Event end time (ISO 8601)" },
        capture_timezone: { type: "string", description: "Timezone (e.g., America/New_York)" },
        time_source: { type: "string", enum: ["gps", "network", "manual", "file_metadata"], description: "How time was determined" },
        time_confidence: { type: "string", enum: ["exact", "approximate", "estimated"], description: "Time accuracy level" },
        location_id: { type: "string", format: "uuid", description: "Location UUID from create_provenance_location" },
        device_id: { type: "string", format: "uuid", description: "Device UUID from create_provenance_device" },
        event_type: { type: "string", enum: ["created", "modified", "accessed", "shared"], description: "Type of note event" },
        event_title: { type: "string", description: "Human-readable event title" },
        event_description: { type: "string", description: "Detailed event description" },
      },
      required: ["note_id"],
    },
    annotations: { destructiveHint: false },
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
    description: `Create a new note in the active memory with FULL AI ENHANCEMENT PIPELINE.

Use select_memory first to target a specific memory. If no memory is selected, the note is created in the default memory.

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
- ["ai/ml/deep-learning", "projects/research"] - hierarchical tags`,
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
              metadata: { type: "object", description: "Optional JSON metadata for the note (arbitrary key-value pairs)" },
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
          description: "Single processing step to run. Valid types: ai_revision, embedding, linking, context_update, title_generation, concept_tagging, re_embed_all"
        },
        priority: { type: "number", description: "Job priority (higher = sooner)" },
        deduplicate: { type: "boolean", description: "When true, skip if a pending job with the same note_id+job_type already exists. Returns status 'already_pending' instead of creating a duplicate." },
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
    description: `Get a collection's metadata (name, description, parent, note count).

Collections organize notes into folders with optional hierarchy (parent/child).
Use list_collections to browse, then this tool for details of a specific collection.

RETURNS: id, name, description, parent_id, note_count, created_at, updated_at`,
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
Child collections will be moved to root level.

By default, deletion fails if the collection contains notes. Use force=true to delete anyway.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Collection UUID to delete" },
        force: { type: "boolean", description: "Force delete even if collection contains notes (default: false)" },
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
        parent_id: { anyOf: [{ type: "string", format: "uuid" }, { type: "null" }], description: "New parent collection UUID, or null to move to root" },
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
    description: `Get a note template with its content, variables, and defaults.

Returns the full template including content with {{variable}} placeholders, default tags, default collection, and format.
The MCP server automatically extracts variable names from {{variable}} patterns in the content.
Use list_templates to browse available templates, then this tool for the full content before instantiating.

RETURNS: id, name, description, content, format, default_tags, collection_id, variables (extracted array), created_at, updated_at`,
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
    description: `Delete a note template permanently.

Removes the template definition. Notes previously created from this template are NOT affected.
This action cannot be undone.`,
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
        collection_id: { anyOf: [{ type: "string", format: "uuid" }, { type: "null" }], description: "New default collection UUID, or null for none" },
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
- index_status: empty/pending/building/ready/stale/disabled

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

Backup operations respect the active memory context.

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
    description: `Same as export_all_notes but with download headers. Respects the active memory context.

Use for file saving.

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
    description: `Create and download a knowledge shard (.tar.gz) from the active memory.

Returns a curl command to download the shard file. Execute the command to save the file.

COMPONENTS: notes, collections, tags, templates, links, embedding_sets, embeddings (large!), or "all"
DEFAULT: notes,collections,tags,templates,links,embedding_sets (no embeddings)

USE WHEN: Need knowledge shard with semantic links. Embeddings regenerate on import.
USE INSTEAD: database_snapshot for full pg_dump with everything, export_all_notes for simple JSON.
RESTORE: knowledge_shard_import with the saved file path`,
    inputSchema: {
      type: "object",
      properties: {
        include: {
          type: "string",
          description: "Components to include: comma-separated list (notes,collections,tags,templates,links,embedding_sets,embeddings) or 'all'. Default: notes,collections,tags,templates,links,embedding_sets",
        },
        output_dir: { type: "string", description: "Directory prefix for the output filename in the curl command" },
      },
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "knowledge_shard_import",
    description: `Import knowledge shard from a .tar.gz file on disk into the active memory.

Backup operations respect the active memory context.

RETURNS: {status, manifest, imported{}, skipped{}, errors[]}`,
    inputSchema: {
      type: "object",
      properties: {
        file_path: { type: "string", description: "Path to the .tar.gz shard file on disk (from knowledge_shard tool)" },
        include: { type: "string", description: "Components to import (default: all)" },
        dry_run: { type: "boolean", default: false, description: "Preview import without writing data (default: false)" },
        on_conflict: { type: "string", enum: ["skip", "replace", "merge"], default: "skip", description: "Conflict resolution: skip (keep existing), replace (overwrite), merge (add new only)" },
        skip_embedding_regen: { type: "boolean", default: false, description: "Skip embedding regeneration if shard includes embeddings (default: false)" },
      },
      required: ["file_path"],
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
    description: `Download backup + metadata bundled as .archive file.

Returns a curl command to download the archive file. Execute the command to save the file.

FORMAT: Tar containing backup file (.sql.gz/.tar.gz) + metadata.json

USE WHEN: Export backup for transfer to another system, offline storage, sharing.
ADVANTAGE: Metadata travels WITH backup - never lose title/description/context.
WORKFLOW: list_backups → knowledge_archive_download → transfer → knowledge_archive_upload`,
    inputSchema: {
      type: "object",
      properties: {
        filename: { type: "string", description: "Backup filename from list_backups (e.g., snapshot_database_*.sql.gz)" },
        output_dir: { type: "string", description: "Directory to save the archive file (default: system temp dir)" },
      },
      required: ["filename"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "knowledge_archive_upload",
    description: `Upload .archive file from disk. Extracts backup + metadata to backup directory.

RETURNS: {success, filename, path, size_bytes, size_human, metadata}

USE WHEN: Restore backup from another system, import transferred archive.
WORKFLOW: knowledge_archive_download (source) → transfer → knowledge_archive_upload (target) → database_restore
TIP: Metadata is automatically extracted and preserved.`,
    inputSchema: {
      type: "object",
      properties: {
        file_path: { type: "string", description: "Path to the .archive file on disk" },
        filename: { type: "string", description: "Original filename (optional, for logging)" },
      },
      required: ["file_path"],
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
    description: `Get storage sizing and hardware recommendations for the ACTIVE memory.

RETURNS: {summary, embedding_sets[], storage, recommendations}
SUMMARY: total_notes, total_embeddings, total_links, total_collections, total_tags, total_templates
STORAGE: database_total_bytes, embedding_table_bytes, notes_table_bytes, estimated_memory_for_search
RECOMMENDATIONS: min_ram_gb, recommended_ram_gb, notes[] (GPU vs CPU usage explained)

USE WHEN: Plan hardware, estimate scaling costs, understand storage breakdown for the current memory.
NOTE: For aggregate stats across ALL memories, use get_memories_overview instead.
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
    description: `Get a concept scheme with its metadata (title, description, namespace URI, concept count).

A concept scheme is a top-level container for organizing SKOS concepts (hierarchical tags).
Use list_concept_schemes first to find scheme IDs, then this tool for full details.

RETURNS: id, title, description, namespace_uri, created_at, updated_at, concept_count`,
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
    name: "update_concept_scheme",
    description: `Update an existing concept scheme's metadata.

Modify the title, description, or other metadata of a concept scheme.
System schemes may have restrictions on which fields can be modified.

RETURNS: Updated concept scheme object.`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "UUID of the concept scheme to update" },
        title: { type: "string", description: "New human-readable title" },
        description: { type: "string", description: "New description/purpose" },
        creator: { type: "string", description: "Creator attribution" },
        publisher: { type: "string", description: "Publisher attribution" },
        rights: { type: "string", description: "Rights/license information" },
        version: { type: "string", description: "Version string" },
        is_active: { type: "boolean", description: "Whether the scheme is active" },
      },
      required: ["id"],
    },
    annotations: {
      destructiveHint: false,
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
    description: `Get a concept's basic info (ID, preferred label, scheme, status, notation).

For full details including all labels, notes, and hierarchy relationships, use get_concept_full instead.
Use search_concepts to find concept IDs by label text.`,
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
    description: `Update a concept's properties (notation, status, facet).

USE WHEN: Changing lifecycle status (candidate → approved → deprecated), setting short codes (notation),
or classifying via PMEST facets. When deprecating, provide deprecation_reason and optionally replaced_by_id.

RETURNS: Updated concept object`,
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
    description: `Delete a concept permanently.

The concept must not be tagged on any notes. Remove tags first with untag_note_concept if needed.
Broader/narrower/related relationships are automatically cleaned up.
This action cannot be undone.`,
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
    description: `Get broader (parent) concepts in the SKOS hierarchy.

Returns the direct parent concept(s). A concept may have up to 3 broader concepts (polyhierarchy).
Navigate up the taxonomy tree. Use get_narrower for children, get_related for associative links.`,
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
    description: `Get narrower (child) concepts in the SKOS hierarchy.

Returns direct child concepts. Navigate down the taxonomy tree.
Use get_broader for parents, get_related for associative links.`,
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
    description: `Add a narrower (child) relationship (inverse of broader).

Creates the child link from parent → child. Equivalent to add_broader called from child's perspective.
Max 3 parents per concept (polyhierarchy limit).

Example: add_narrower({id: programming_languages_uuid, target_id: rust_uuid})`,
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
    description: `Get related (associative) concepts — non-hierarchical connections.

Returns concepts linked via skos:related (symmetric). These represent "see also" associations
that cross hierarchy boundaries. Use get_broader/get_narrower for hierarchical navigation.`,
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
    description: `Remove a SKOS concept tag from a note.

Removes only the concept association — does not affect the concept itself or note content.
To manage simple text tags, use set_note_tags instead.`,
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
    description: `Get all SKOS concepts tagged on a note with their labels.

Returns formal SKOS concept tags (not simple text tags). Each concept includes its preferred label,
scheme, and notation. For simple text tags, check the tags field from get_note.`,
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
    description: `Get top-level concepts in a scheme (concepts with no broader relations).

These are root nodes of the taxonomy — entry points for hierarchical navigation.
Use get_narrower on these to explore children and build tree views.`,
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
- "vision" - Vision model for image description and extraction
- "audio" - Audio transcription via Whisper-compatible backend
- "video" - Video multimodal extraction (keyframes, transcription, temporal alignment)
- "3d-models" - 3D model understanding via multi-view rendering

**Reference:**
- "workflows" - Usage patterns and advanced workflow examples
- "troubleshooting" - Common issues, permission reference, debugging tips
- "contributing" - How to file bugs, request features, and report issues
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
          enum: ["overview", "notes", "search", "concepts", "skos_collections", "chunking", "versioning", "collections", "archives", "templates", "document_types", "backup", "encryption", "jobs", "observability", "provenance", "embedding_configs", "vision", "audio", "video", "3d-models", "workflows", "troubleshooting", "contributing", "all"],
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

If output_dir is specified, key files are written to disk (public.key, private.key.enc, address.txt).

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
    description: `Get the public address from a public key (base64 or file path).

Returns the mm:... address derived from a public key. This address is what senders
use to encrypt data for you.

Provide EITHER public_key (base64) OR public_key_path (filesystem). Base64 is
preferred for MCP workflows since no filesystem access is needed.`,
    inputSchema: {
      type: "object",
      properties: {
        public_key: {
          type: "string",
          description: "Base64-encoded public key bytes (preferred — no filesystem access needed)"
        },
        public_key_path: {
          type: "string",
          description: "Path to the public key file (fallback for local CLI workflows)"
        },
      },
      required: [],
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "pke_encrypt",
    description: `Encrypt data for one or more recipients using public-key encryption (MMPKE01 format).

Provides multi-recipient support, forward secrecy (ephemeral keys), and authenticated
encryption (AES-256-GCM).

Pass plaintext and recipient_keys as base64 strings.
Returns base64 ciphertext directly.`,
    inputSchema: {
      type: "object",
      properties: {
        plaintext: {
          type: "string",
          description: "Base64-encoded plaintext to encrypt"
        },
        recipient_keys: {
          type: "array",
          items: { type: "string" },
          description: "Base64-encoded recipient public keys"
        },
        original_filename: {
          type: "string",
          description: "Original filename to embed in encrypted header (optional)"
        },
      },
      required: ["plaintext", "recipient_keys"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_decrypt",
    description: `Decrypt data using your private key.

Pass ciphertext and encrypted_private_key as base64 strings plus the passphrase.
Returns base64 plaintext directly.`,
    inputSchema: {
      type: "object",
      properties: {
        ciphertext: {
          type: "string",
          description: "Base64-encoded ciphertext to decrypt"
        },
        encrypted_private_key: {
          type: "string",
          description: "Base64-encoded encrypted private key"
        },
        passphrase: {
          type: "string",
          description: "Passphrase for the private key"
        },
      },
      required: ["ciphertext", "encrypted_private_key", "passphrase"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "pke_list_recipients",
    description: `List the recipient addresses that can decrypt encrypted data.

Reads the MMPKE01 header and returns the mm:... addresses of all recipients
without decrypting. Useful for determining if you can decrypt or who it was intended for.`,
    inputSchema: {
      type: "object",
      properties: {
        ciphertext: {
          type: "string",
          description: "Base64-encoded ciphertext"
        },
      },
      required: ["ciphertext"],
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
- **public_key** - Base64-encoded public key
- **created** - Timestamp when the keyset was created

Keysets are stored in ~/.matric/keys/{name}/ and provide named identities
for different encryption contexts (personal, work, projects, etc.).
Addresses are computed via the HTTP API (no CLI binary required).`,
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
    description: `Create a new named PKE keyset via the HTTP API.

Generates a keypair using the API (no CLI binary required) and stores it locally
at ~/.matric/keys/{name}/ containing:
- public.key - Public key (shareable)
- private.key.enc - Encrypted private key (secured with passphrase)

**Use Cases:**
- Separate work and personal identities
- Project-specific encryption keys
- Team-shared keysets (via secure key exchange)

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
Address is computed via the HTTP API (no CLI binary required).

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
  // VISION - Image description using vision LLMs
  // Requires OLLAMA_VISION_MODEL to be configured
  // ============================================================================
  {
    name: "describe_image",
    description: `Get the upload URL and curl command for describing an image using the vision model.

Returns a ready-to-use curl command that uploads the image file via multipart/form-data.
The agent should execute the curl command to upload the file directly to the API.
Requires OLLAMA_VISION_MODEL to be configured on the server.

Workflow:
1. Call this tool with the file_path of the image you want to describe
2. Execute the returned curl_command
3. The API accepts multipart/form-data — no base64 encoding needed
4. The response JSON contains { description, model, image_size }

**Use cases:**
- Describe uploaded images before storing
- Extract text from screenshots or diagrams
- Generate alt-text for accessibility
- Analyze image content for tagging

**Supported formats:** PNG, JPEG, GIF, WebP

Binary data never passes through the MCP protocol or LLM context window.`,
    inputSchema: {
      type: "object",
      properties: {
        file_path: {
          type: "string",
          description: "Local path to the image file (e.g., '/tmp/photo.png')",
        },
        mime_type: {
          type: "string",
          description: "Image MIME type (default: auto-detected from file). Supported: image/png, image/jpeg, image/gif, image/webp",
        },
        prompt: {
          type: "string",
          description: "Custom prompt for the vision model. If omitted, uses default description prompt.",
        },
      },
      required: ["file_path"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // AUDIO - Audio transcription using Whisper-compatible backends
  // Requires WHISPER_BASE_URL to be configured
  // ============================================================================
  {
    name: "transcribe_audio",
    description: `Get the upload URL and curl command for transcribing audio using the Whisper backend.

Returns a ready-to-use curl command that uploads the audio file via multipart/form-data.
The agent should execute the curl command to upload the file directly to the API.
Requires WHISPER_BASE_URL to be configured on the server.

Workflow:
1. Call this tool with the file_path of the audio you want to transcribe
2. Execute the returned curl_command
3. The API accepts multipart/form-data — no base64 encoding needed
4. The response JSON contains { text, segments, language, duration_secs, model, audio_size }

**Language hints:**
You can provide an ISO 639-1 language code to help the model:
- "en" for English, "es" for Spanish, "zh" for Chinese, "de" for German
If omitted, the model auto-detects the language.

**Supported formats:** MP3, WAV, OGG, FLAC, AAC, WebM

Binary data never passes through the MCP protocol or LLM context window.`,
    inputSchema: {
      type: "object",
      properties: {
        file_path: {
          type: "string",
          description: "Local path to the audio file (e.g., '/tmp/recording.mp3')",
        },
        mime_type: {
          type: "string",
          description: "Audio MIME type (default: auto-detected from file). Supported: audio/mpeg, audio/wav, audio/ogg, audio/flac, audio/aac, audio/webm",
        },
        language: {
          type: "string",
          description: "ISO 639-1 language code hint (e.g., 'en', 'es', 'zh'). If omitted, the model auto-detects the language.",
        },
      },
      required: ["file_path"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // VIDEO - Video processing via attachment pipeline
  // Requires ffmpeg in PATH + OLLAMA_VISION_MODEL and/or WHISPER_BASE_URL
  // ============================================================================
  {
    name: "process_video",
    description: `**Guidance tool** — Video processing runs through the attachment pipeline, not ad-hoc base64.

Video files are too large for base64 transport through the MCP protocol. Instead, use the standard attachment workflow to process video files.

**Workflow to process a video file:**

1. **Ensure a note exists** — If the video has no associated note, create one first:
   \`create_note({ title: "Video: <filename>", body: "Uploaded video for processing" })\`

2. **Upload the video as an attachment:**
   \`upload_attachment({ note_id: "<note_id>", filename: "video.mp4", content_type: "video/mp4" })\`
   Then execute the returned curl command with the actual file path.

3. **Wait for extraction** — The background job worker automatically:
   - Extracts keyframes using scene detection (ffmpeg)
   - Describes each keyframe using the vision model (if OLLAMA_VISION_MODEL is set)
   - Transcribes audio track using Whisper (if WHISPER_BASE_URL is set)
   - Builds temporal context linking frame descriptions with transcript segments
   - Stores all extracted metadata with the note

4. **Check extraction status:**
   \`get_attachment({ id: "<attachment_id>" })\` — look for extraction_metadata in the response

5. **Search by content** — Once extracted, video content is searchable via \`search_notes\`

**Supported video formats:** MP4, WebM, AVI, MOV, MKV, FLV, WMV, OGG
**Requires:** ffmpeg in PATH, plus OLLAMA_VISION_MODEL and/or WHISPER_BASE_URL

This tool returns the workflow instructions. Call it to get a reminder of the steps.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: {
          anyOf: [{ type: "string" }, { type: "null" }],
          description: "Optional existing note UUID. If provided, the guidance will reference this note. If omitted, the guidance includes a note creation step.",
        },
        filename: {
          type: "string",
          description: "Optional filename for the video (used in guidance output)",
        },
      },
    },
    annotations: { readOnlyHint: true },
  },

  // ============================================================================
  // 3D MODELS - 3D model processing via attachment pipeline
  // Requires Blender headless + OLLAMA_VISION_MODEL
  // ============================================================================
  {
    name: "process_3d_model",
    description: `**Guidance tool** — 3D model processing runs through the attachment pipeline, not ad-hoc base64.

3D model files (GLB, GLTF, OBJ, FBX, STL, PLY) are processed via multi-view rendering:
the server uses Blender headless to render the model from multiple angles, then describes
each view using the vision model.

**Workflow to process a 3D model file:**

1. **Ensure a note exists** — If the model has no associated note, create one first:
   \`create_note({ title: "3D Model: <filename>", body: "Uploaded 3D model for processing" })\`

2. **Upload the model as an attachment:**
   \`upload_attachment({ note_id: "<note_id>", filename: "model.glb", content_type: "model/gltf-binary" })\`
   Then execute the returned curl command with the actual file path.

3. **Wait for extraction** — The background job worker automatically:
   - Renders the model from multiple angles using Blender headless
   - Describes each rendered view using the vision model
   - Synthesizes a composite description from all views
   - Stores the multi-view descriptions as extraction metadata

4. **Check extraction status:**
   \`get_attachment({ id: "<attachment_id>" })\` — look for extraction_metadata in the response

5. **Search by content** — Once extracted, 3D model descriptions are searchable via \`search_notes\`

**Supported 3D formats:** GLB, GLTF, OBJ, FBX, STL, PLY
**Requires:** Blender (headless) in PATH + OLLAMA_VISION_MODEL

This tool returns the workflow instructions. Call it to get a reminder of the steps.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: {
          anyOf: [{ type: "string" }, { type: "null" }],
          description: "Optional existing note UUID. If provided, the guidance will reference this note. If omitted, the guidance includes a note creation step.",
        },
        filename: {
          type: "string",
          description: "Optional filename for the 3D model (used in guidance output)",
        },
      },
    },
    annotations: { readOnlyHint: true },
  },

  // ============================================================================
  // ARCHIVE MANAGEMENT
  // Manage parallel memory archives with schema-level data isolation
  // ============================================================================
  {
    name: "list_archives",
    description: `List all memory archives with their names, sizes, note counts, and schema versions.

Use select_memory to switch the active memory for subsequent operations.

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
          anyOf: [{ type: "string" }, { type: "null" }],
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
    description: `Set an archive as the default and switch to it for this session.

The default archive is used when no specific archive is specified in operations. Only one archive can be default at a time.

This also sets the active memory for this MCP session, ensuring subsequent operations target the new default archive. This is equivalent to calling set_default_archive followed by select_memory.

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

Returns tags with zero note count — candidates for cleanup, consolidation, or deprecation.
Part of the knowledge health dashboard. Use delete_concept to remove, or update_concept to deprecate.`,
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

Returns notes not modified within the threshold (default: 90 days). Useful for content freshness
audits and identifying abandoned knowledge. Consider archiving or refreshing stale content.

PARAMS: days (staleness threshold, default 90), limit (default 50)`,
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
    description: `Find notes with no semantic links (isolated knowledge islands).

These notes have zero incoming and outgoing semantic connections — they may need more content
to establish links, or manual review. Use reprocess_note with steps=["linking"] to retry auto-linking.`,
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
    description: `Analyze which tags frequently appear together on notes.

Discovers implicit semantic relationships in your tagging patterns. High co-occurrence suggests
candidates for skos:related links (use add_related) or potential tag consolidation.

PARAMS: min_count (minimum co-occurrence threshold), limit (default 50)`,
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
    description: `Get count of pending jobs in the processing queue.

Faster than list_jobs when you only need the count for status display.
Normal: 0-10, moderate backlog: 10-100 (after bulk ops), heavy: >100.

RETURNS: { pending_count: number }`,
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
    description: `List all embedding model configurations (shared across all memories).

Embedding configurations are memory-independent and shared system-wide.

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
    description: `Get the upload URL and curl command for attaching a file to a note.

Returns the multipart upload endpoint URL and a ready-to-use curl command.
The agent should execute the curl command to upload the file directly to the API.

Workflow:
1. Call this tool with note_id and the filename you want to upload
2. Execute the returned curl_command (replace FILE_PATH with actual path)
3. The API accepts multipart/form-data — no base64 encoding needed
4. Files up to the configured max upload size (default 50MB) are supported with content-hash deduplication

Binary data never passes through the MCP protocol or LLM context window.`,
    inputSchema: {
      type: "object",
      properties: {
        note_id: { type: "string", format: "uuid", description: "Note UUID to attach the file to" },
        filename: { type: "string", description: "Filename hint for the curl command (e.g., 'photo.jpg')" },
        content_type: { type: "string", description: "MIME type hint (e.g., 'image/jpeg'). If omitted, auto-detected from file extension." },
        document_type_id: { type: "string", format: "uuid", description: "Optional: explicit document type UUID override (skips auto-classification)" },
      },
      required: ["note_id"],
    },
    annotations: { readOnlyHint: true },
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

Returns full attachment details including extracted metadata (EXIF, etc.) if available.
Response includes _api_urls with direct download URL and curl command for binary retrieval.`,
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
    description: `Get the download URL and curl command for a file attachment.

Returns the direct HTTP API URL and a ready-to-use curl command for downloading.
The agent should execute the curl command (or equivalent HTTP request) to retrieve the file.

Workflow:
1. Call this tool with the attachment UUID
2. Execute the returned curl command to download the file
3. The file is saved to the current directory (or specify -o path)

Binary data never passes through the LLM context window.`,
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
    description: `Export SKOS concept scheme(s) as W3C RDF/Turtle format.

If scheme_id is provided, exports a single scheme. If omitted, exports ALL active schemes in one Turtle document.

Returns valid Turtle syntax for interoperability with other SKOS tools:
- Protégé, TopBraid, PoolParty
- RDF visualization tools
- Other knowledge management systems

Includes:
- Concept scheme with metadata
- Concepts with all labels (preferred, alternative, hidden)
- Broader/narrower/related relations
- Collection memberships and ordering

Use list_concept_schemes to find available scheme_ids first.`,
    inputSchema: {
      type: "object",
      properties: {
        scheme_id: { type: "string", format: "uuid", description: "Concept scheme UUID to export. Omit to export ALL schemes." },
      },
      required: [],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // MEMORY MANAGEMENT - Session-based archive selection
  // ============================================================================
  {
    name: "select_memory",
    description: `Switch the active memory for all subsequent MCP operations in this session.

All CRUD, search, tag, collection, template, versioning, and attachment operations will target the selected memory. If not called, operations target the default memory.

All subsequent API calls from this session will automatically route to the selected memory using the X-Fortemi-Memory header.

Use list_memories to see available memories. Use "public" to select the default public memory.

The selected memory persists for the duration of this MCP session.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Memory name to select (e.g., 'personal', 'work', 'public')" },
      },
      required: ["name"],
    },
  },
  {
    name: "get_active_memory",
    description: `Get the currently active memory for this MCP session.

Returns the name of the memory that all API calls are routing to, or "public (default)" if no memory has been explicitly selected.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "list_memories",
    description: `List all available memories (archives).

Each memory is an isolated namespace with its own notes, tags, collections, and knowledge graph.

Returns metadata including:
- name: Memory identifier
- description: Purpose/description
- is_default: Whether this is the default memory
- created_at_utc: Creation timestamp
- note_count, size_bytes: Content statistics

For aggregate capacity/overhead across all memories, use get_memories_overview.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_memory",
    description: `Create a new memory (archive) for data isolation.

Memories provide complete data isolation - each memory has its own:
- Notes and content
- Tags and SKOS concepts
- Collections and hierarchies
- Semantic links and embeddings
- Templates and configurations

Use cases:
- Personal vs work separation
- Client/project isolation
- Public vs private knowledge
- Testing/experimentation sandbox`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Memory name (alphanumeric, hyphens, underscores)" },
        description: { type: "string", description: "Purpose or description (optional)" },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "delete_memory",
    description: `Delete a memory and all its data permanently.

WARNING: This is destructive and irreversible. All notes, tags, collections, embeddings, and configuration in this memory will be permanently deleted.

Cannot delete:
- The default memory
- The "public" memory
- Non-existent memories

Use with extreme caution.`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Memory name to delete" },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "clone_memory",
    description: `Clone an existing memory (archive) to a new memory with all data.

Creates a deep copy of the source memory including all notes, tags, collections, embeddings, links, and templates.

Use cases:
- Creating a snapshot before risky operations
- Forking a knowledge base for experimentation
- Duplicating a project template with data`,
    inputSchema: {
      type: "object",
      properties: {
        source_name: { type: "string", description: "Name of the memory to clone from" },
        new_name: { type: "string", description: "Name for the cloned memory" },
        description: { type: "string", description: "Optional description for the clone" },
      },
      required: ["source_name", "new_name"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  {
    name: "get_memories_overview",
    description: `Get aggregate capacity overview across ALL running memories.

This is the primary tool for understanding system overhead and planning memory allocation.

RETURNS:
- memory_count: How many memories are active
- max_memories: Maximum allowed (configurable via MAX_MEMORIES)
- remaining_slots: How many more memories can be created
- total_notes: Aggregate note count across all memories + public
- total_size_bytes / total_size_human: Aggregate table storage across all memories
- database_size_bytes / database_size_human: Total database size on disk (pg_database_size — includes all schemas, indexes, attachment blobs)
- memories[]: Per-memory breakdown (name, note_count, size_bytes, is_default, created_at)

USE WHEN: Check how much capacity is left, plan memory allocation, monitor overhead.
KEY INSIGHT: If system handles 100k docs, 3 memories with ~33k each uses full capacity.
database_size_bytes is the true on-disk footprint for the entire database.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "search_memories_federated",
    description: `Search across multiple memories simultaneously using full-text search.

Runs the query against each specified memory and merges results sorted by relevance score. Each result is annotated with its source memory name.

Use this when you need to find information across knowledge boundaries, e.g., searching both personal and work memories at once.`,
    inputSchema: {
      type: "object",
      properties: {
        q: { type: "string", description: "Search query string" },
        memories: {
          type: "array",
          items: { type: "string" },
          description: "Memory names to search (use [\"all\"] for all memories)",
        },
        limit: { type: "integer", description: "Maximum results per memory (default 10)" },
      },
      required: ["q", "memories"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // API KEY MANAGEMENT
  // ============================================================================
  {
    name: "list_api_keys",
    description: `List all API keys.

Returns all registered API keys with their metadata (name, scope, creation date, expiration).
Key values are NOT returned — only metadata for management purposes.

Use this to audit existing API keys or check expiration dates.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },
  {
    name: "create_api_key",
    description: `Create a new API key for programmatic access.

Returns the full key value ONCE — it cannot be retrieved again after creation.
Store the returned key securely.

Parameters:
- name: Human-readable label for the key
- description: Optional longer description of key purpose
- scope: Access scope (default: "admin")
- expires_in_days: Optional expiration in days (null = never expires)`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Human-readable name for the API key" },
        description: { type: "string", description: "Optional description of the key's purpose" },
        scope: { type: "string", description: "Access scope (default: 'admin')" },
        expires_in_days: { type: "integer", description: "Days until expiration (omit for no expiration)" },
      },
      required: ["name"],
    },
    annotations: {
      destructiveHint: false,
    },
  },
  // revoke_api_key intentionally omitted from MCP tools.
  // API key revocation is an admin-only operation available via REST API directly:
  //   DELETE /api/v1/api-keys/:id
  // Exposing it via MCP would advertise it to all clients regardless of scope.

  // ============================================================================
  // RATE LIMITING
  // ============================================================================
  {
    name: "get_rate_limit_status",
    description: `Check whether rate limiting is enabled and its current status.

Returns:
- enabled: Whether rate limiting is active
- message: Human-readable status description

Use this to diagnose request throttling issues.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // EXTRACTION PIPELINE
  // ============================================================================
  {
    name: "get_extraction_stats",
    description: `Get extraction pipeline statistics.

Returns aggregate statistics about the content extraction pipeline including
document type distribution, processing counts, and strategy usage.

Use this to monitor pipeline health and understand content processing patterns.`,
    inputSchema: {
      type: "object",
      properties: {},
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // COLLECTION EXPORT
  // ============================================================================
  {
    name: "export_collection",
    description: `Export all notes in a collection as concatenated markdown.

Returns all notes in the specified collection rendered as markdown with optional YAML frontmatter.
Useful for bulk export, backup of a specific collection, or generating a document from collected notes.

Parameters:
- id: Collection UUID to export
- include_frontmatter: Include YAML frontmatter metadata per note (default: true)
- content: Which content version to export — "revised" (AI-enhanced, default) or "original"`,
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", format: "uuid", description: "UUID of the collection to export" },
        include_frontmatter: { type: "boolean", description: "Include YAML frontmatter metadata (default: true)" },
        content: { type: "string", enum: ["revised", "original"], description: "Content version: 'revised' (default) or 'original'" },
      },
      required: ["id"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

  // ============================================================================
  // BACKUP SWAP
  // ============================================================================
  {
    name: "swap_backup",
    description: `Swap the current database state with a backup shard file.

Restores from a knowledge shard (.tar.gz) file on disk. Only knowledge shards are supported — use database_restore for .sql.gz files.

WARNING: With strategy "wipe" (default), this replaces ALL existing data. Use dry_run=true first to preview.

Parameters:
- filename: Name of the shard file in the backup directory (no path traversal allowed)
- dry_run: If true, validate without restoring (default: false)
- strategy: "wipe" (replace all data, default) or "merge" (add new only)

USE: list_backups first to find available shard filenames.`,
    inputSchema: {
      type: "object",
      properties: {
        filename: { type: "string", description: "Shard filename (e.g., 'shard_20260210.tar.gz')" },
        dry_run: { type: "boolean", description: "Preview without restoring (default: false)" },
        strategy: { type: "string", enum: ["wipe", "merge"], description: "Restore strategy: 'wipe' (default) or 'merge'" },
      },
      required: ["filename"],
    },
    annotations: {
      destructiveHint: true,
    },
  },
  {
    name: "memory_backup_download",
    description: `Download a pg_dump backup of a specific memory archive.

Returns a curl command to download a compressed SQL dump (.sql.gz) of the specified memory's PostgreSQL schema.
Execute the curl command to save the backup file.

Use this to back up individual memories without affecting other archives.

Parameters:
- name: Memory archive name (use list_memories to find available names)`,
    inputSchema: {
      type: "object",
      properties: {
        name: { type: "string", description: "Memory archive name to back up" },
      },
      required: ["name"],
    },
    annotations: {
      readOnlyHint: true,
    },
  },

];