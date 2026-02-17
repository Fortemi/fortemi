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
  {
    name: "list_notes",
    description: `List notes with optional filters (tags, dates, starred, archived). See \`get_documentation(topic='notes')\` for return format.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "limit": {
          "type": "number",
          "description": "Maximum notes to return (default: 50)",
          "default": 50
        },
        "offset": {
          "type": "number",
          "description": "Pagination offset (default: 0)",
          "default": 0
        },
        "filter": {
          "type": "string",
          "description": "Filter: 'starred' or 'archived'",
          "enum": [
            "starred",
            "archived"
          ]
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Filter by tags - use hierarchical paths like 'topic/subtopic' (notes must have ALL specified tags)"
        },
        "collection_id": {
          "type": "string",
          "format": "uuid",
          "description": "Filter notes to this collection (optional)"
        },
        "created_after": {
          "type": "string",
          "description": "Filter notes created after this date (ISO 8601 format, e.g. '2024-01-01T00:00:00Z')"
        },
        "created_before": {
          "type": "string",
          "description": "Filter notes created before this date (ISO 8601 format)"
        },
        "updated_after": {
          "type": "string",
          "description": "Filter notes updated after this date (ISO 8601 format)"
        },
        "updated_before": {
          "type": "string",
          "description": "Filter notes updated before this date (ISO 8601 format)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_note",
    description: `Get complete note details including original content, AI revision, tags, and semantic links.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the note"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "search",
    description: `Search the knowledge base by text, location, time, or across archives. See \`get_documentation(topic='search')\` for query syntax and operators.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "text",
            "spatial",
            "temporal",
            "spatial_temporal",
            "federated"
          ],
          "description": "Search type: 'text' (keyword/semantic), 'spatial' (by location), 'temporal' (by time), 'spatial_temporal' (both), 'federated' (cross-archive)"
        },
        "query": {
          "type": "string",
          "description": "Search query (required for 'text'/'federated')"
        },
        "mode": {
          "type": "string",
          "enum": [
            "hybrid",
            "fts",
            "semantic"
          ],
          "description": "Search mode: 'hybrid' (default), 'fts' (exact match), 'semantic' (conceptual)",
          "default": "hybrid"
        },
        "set": {
          "type": "string",
          "description": "Embedding set slug to restrict semantic search"
        },
        "collection_id": {
          "type": "string",
          "format": "uuid",
          "description": "Filter to notes in this collection"
        },
        "required_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "ALL results MUST have these tags (AND logic)"
        },
        "excluded_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "NO results should have these tags"
        },
        "any_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Results must have at least ONE of these tags (OR logic)"
        },
        "lat": {
          "type": "number",
          "minimum": -90,
          "maximum": 90,
          "description": "Latitude (required for 'spatial'/'spatial_temporal')"
        },
        "lon": {
          "type": "number",
          "minimum": -180,
          "maximum": 180,
          "description": "Longitude (required for 'spatial'/'spatial_temporal')"
        },
        "radius": {
          "type": "number",
          "exclusiveMinimum": 0,
          "description": "Search radius in meters (default: 1000)",
          "default": 1000
        },
        "start": {
          "type": "string",
          "format": "date-time",
          "description": "Start of time range (ISO 8601, required for 'temporal'/'spatial_temporal')"
        },
        "end": {
          "type": "string",
          "format": "date-time",
          "description": "End of time range (ISO 8601, required for 'temporal'/'spatial_temporal')"
        },
        "memories": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Memory names to search, or ['all'] for all memories (required for 'federated')"
        },
        "limit": {
          "type": "integer",
          "description": "Maximum results (default: 20)",
          "default": 20
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "capture_knowledge",
    description: `Add knowledge to the active memory. Each note triggers the full NLP pipeline: AI revision, title generation, SKOS concept tagging (8-15 tags), metadata extraction, tag-enriched embedding, and semantic linking — all automatic. Set revision_mode='none' to skip AI revision but keep auto-tagging/embedding/linking. See \`get_documentation(topic='notes')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "create",
            "bulk_create",
            "from_template",
            "upload"
          ],
          "description": "Creation method: 'create' for single note, 'bulk_create' for batch, 'from_template' for template instantiation, 'upload' for file attachment"
        },
        "content": {
          "type": "string",
          "description": "Note content in markdown format (required for 'create')"
        },
        "notes": {
          "type": "array",
          "description": "Array of notes for 'bulk_create' (max 100). Each: { content, tags?, metadata?, revision_mode? }",
          "items": {
            "type": "object",
            "properties": {
              "content": {
                "type": "string"
              },
              "tags": {
                "type": "array",
                "items": {
                  "type": "string"
                }
              },
              "metadata": {
                "type": "object"
              },
              "revision_mode": {
                "type": "string",
                "enum": [
                  "full",
                  "light",
                  "none"
                ],
                "default": "light"
              }
            },
            "required": [
              "content"
            ]
          }
        },
        "template_id": {
          "type": "string",
          "description": "Template UUID for 'from_template' action"
        },
        "variables": {
          "type": "object",
          "additionalProperties": {
            "type": "string"
          },
          "description": "Variable substitutions for template: { 'placeholder': 'value' }"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID to attach file to (required for 'upload')"
        },
        "filename": {
          "type": "string",
          "description": "Filename hint for upload (e.g., 'photo.jpg')"
        },
        "content_type": {
          "type": "string",
          "description": "MIME type hint for upload (e.g., 'image/jpeg')"
        },
        "document_type_id": {
          "type": "string",
          "format": "uuid",
          "description": "Explicit document type UUID override for upload"
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Tags using hierarchical paths like 'topic/subtopic' (max 5 levels)"
        },
        "revision_mode": {
          "type": "string",
          "enum": [
            "full",
            "light",
            "none"
          ],
          "description": "AI revision mode: 'full' (default), 'light' (format only), 'none' (skip)",
          "default": "light"
        },
        "collection_id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID to place the note in"
        },
        "metadata": {
          "type": "object",
          "description": "Arbitrary key-value metadata (e.g., { source: 'meeting' })"
        },
        "model": {
          "type": "string",
          "description": "Language model slug for AI operations. Supports provider-qualified slugs (e.g. 'qwen3:8b', 'openai:gpt-4o', 'openrouter:anthropic/claude-sonnet-4-20250514'). If omitted, uses the globally configured default. Bare slugs route to the default provider (Ollama). Use get_available_models to discover available slugs and providers."
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "record_provenance",
    description: `Record spatial-temporal provenance for files or notes. Supports locations, devices, and linking content to where/when it was captured. See \`get_documentation(topic='provenance')\` for W3C PROV details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "location",
            "named_location",
            "device",
            "file",
            "note"
          ],
          "description": "Provenance type: 'location' (GPS coords), 'named_location' (place name), 'device' (camera/phone), 'file' (attach to file), 'note' (attach to note)"
        },
        "latitude": {
          "type": "number",
          "minimum": -90,
          "maximum": 90,
          "description": "Latitude in decimal degrees (required for 'location'/'named_location')"
        },
        "longitude": {
          "type": "number",
          "minimum": -180,
          "maximum": 180,
          "description": "Longitude in decimal degrees (required for 'location'/'named_location')"
        },
        "altitude_m": {
          "type": "number",
          "description": "Altitude in meters"
        },
        "horizontal_accuracy_m": {
          "type": "number",
          "description": "GPS horizontal accuracy in meters"
        },
        "vertical_accuracy_m": {
          "type": "number",
          "description": "GPS vertical accuracy in meters"
        },
        "heading_degrees": {
          "type": "number",
          "minimum": 0,
          "maximum": 360,
          "description": "Compass heading in degrees"
        },
        "speed_mps": {
          "type": "number",
          "minimum": 0,
          "description": "Speed in meters per second"
        },
        "source": {
          "type": "string",
          "enum": [
            "gps_exif",
            "device_api",
            "user_manual",
            "geocoded",
            "ai_estimated"
          ],
          "description": "How location was obtained"
        },
        "confidence": {
          "type": "string",
          "enum": [
            "high",
            "medium",
            "low",
            "unknown"
          ],
          "description": "Location accuracy level"
        },
        "name": {
          "type": "string",
          "description": "Location display name (required for 'named_location')"
        },
        "location_type": {
          "type": "string",
          "enum": [
            "home",
            "work",
            "poi",
            "city",
            "region",
            "country"
          ],
          "description": "Location category (required for 'named_location')"
        },
        "radius_m": {
          "type": "number",
          "minimum": 0,
          "description": "Boundary radius in meters"
        },
        "address_line": {
          "type": "string",
          "description": "Street address"
        },
        "locality": {
          "type": "string",
          "description": "City/town"
        },
        "admin_area": {
          "type": "string",
          "description": "State/province"
        },
        "country": {
          "type": "string",
          "description": "Country name"
        },
        "country_code": {
          "type": "string",
          "description": "ISO country code"
        },
        "postal_code": {
          "type": "string",
          "description": "Postal/ZIP code"
        },
        "timezone": {
          "type": "string",
          "description": "IANA timezone"
        },
        "is_private": {
          "type": "boolean",
          "description": "Whether location is private"
        },
        "device_make": {
          "type": "string",
          "description": "Device manufacturer (required for 'device')"
        },
        "device_model": {
          "type": "string",
          "description": "Device model name (required for 'device')"
        },
        "device_os": {
          "type": "string",
          "description": "Operating system"
        },
        "device_os_version": {
          "type": "string",
          "description": "OS version"
        },
        "software": {
          "type": "string",
          "description": "Capture software name"
        },
        "software_version": {
          "type": "string",
          "description": "Software version"
        },
        "has_gps": {
          "type": "boolean",
          "description": "Device has GPS"
        },
        "has_accelerometer": {
          "type": "boolean",
          "description": "Device has accelerometer"
        },
        "sensor_metadata": {
          "type": "object",
          "description": "Additional sensor details"
        },
        "device_name": {
          "type": "string",
          "description": "User-friendly device name"
        },
        "attachment_id": {
          "type": "string",
          "format": "uuid",
          "description": "Attachment UUID (required for 'file')"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID (required for 'note', optional for 'file')"
        },
        "location_id": {
          "type": "string",
          "format": "uuid",
          "description": "Location UUID from a prior 'location' action"
        },
        "device_id": {
          "type": "string",
          "format": "uuid",
          "description": "Device UUID from a prior 'device' action"
        },
        "capture_time_start": {
          "type": "string",
          "format": "date-time",
          "description": "Capture start time (ISO 8601)"
        },
        "capture_time_end": {
          "type": "string",
          "format": "date-time",
          "description": "Capture end time (ISO 8601)"
        },
        "capture_timezone": {
          "type": "string",
          "description": "Capture timezone (e.g., America/New_York)"
        },
        "capture_duration_seconds": {
          "type": "number",
          "minimum": 0,
          "description": "Duration in seconds (for 'file')"
        },
        "time_source": {
          "type": "string",
          "enum": [
            "exif",
            "file_mtime",
            "user_manual",
            "ai_estimated",
            "gps",
            "network",
            "manual",
            "file_metadata",
            "device_clock"
          ],
          "description": "How capture time was determined"
        },
        "time_confidence": {
          "type": "string",
          "enum": [
            "high",
            "medium",
            "low",
            "unknown",
            "exact",
            "approximate",
            "estimated"
          ],
          "description": "Time accuracy level"
        },
        "event_type": {
          "type": "string",
          "description": "Event type: photo/video/audio/scan for file; created/modified/accessed/shared for note"
        },
        "event_title": {
          "type": "string",
          "description": "Human-readable event title"
        },
        "event_description": {
          "type": "string",
          "description": "Detailed event description"
        },
        "raw_metadata": {
          "type": "object",
          "description": "Raw EXIF/XMP/IPTC metadata (for 'file')"
        },
        "metadata": {
          "type": "object",
          "description": "Additional metadata"
        },
        "named_location_id": {
          "type": "string",
          "format": "uuid",
          "description": "Link to a named location (for 'location')"
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "manage_tags",
    description: `Review and curate tags on notes. SKOS concept tags are auto-generated by the NLP pipeline — use this to review auto-tags, make corrections, or add organizational tags (project names, status). See \`get_documentation(topic='concepts')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "list",
            "set",
            "tag_concept",
            "untag_concept",
            "get_concepts"
          ],
          "description": "Action: 'list' (all tags), 'set' (replace note tags), 'tag_concept'/'untag_concept' (SKOS concept tagging), 'get_concepts' (concepts on a note)"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID (required for 'set'/'tag_concept'/'untag_concept'/'get_concepts')"
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Tags to set — hierarchical paths like 'topic/subtopic' (required for 'set')"
        },
        "concept_id": {
          "type": "string",
          "format": "uuid",
          "description": "Concept UUID (required for 'tag_concept'/'untag_concept')"
        },
        "is_primary": {
          "type": "boolean",
          "description": "Mark as primary concept tag (for 'tag_concept')",
          "default": false
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "manage_collection",
    description: `Manage collections (folders) for organizing notes. Supports CRUD, listing notes, moving notes, and export. See \`get_documentation(topic='collections')\` for hierarchy details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "list",
            "create",
            "get",
            "update",
            "delete",
            "list_notes",
            "move_note",
            "export"
          ],
          "description": "Action: 'list', 'create', 'get', 'update', 'delete', 'list_notes', 'move_note', 'export'"
        },
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID (required for get/update/delete/list_notes/export)"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID (required for 'move_note')"
        },
        "name": {
          "type": "string",
          "description": "Collection name (required for 'create')"
        },
        "description": {
          "type": "string",
          "description": "Collection description"
        },
        "parent_id": {
          "anyOf": [
            {
              "type": "string",
              "format": "uuid"
            },
            {
              "type": "null"
            }
          ],
          "description": "Parent collection UUID (null for root)"
        },
        "force": {
          "type": "boolean",
          "description": "Force delete even if collection has notes (for 'delete')"
        },
        "collection_id": {
          "type": "string",
          "format": "uuid",
          "description": "Target collection UUID for 'move_note' (omit for uncategorized)"
        },
        "limit": {
          "type": "integer",
          "description": "Max results for 'list_notes' (default: 50)"
        },
        "offset": {
          "type": "integer",
          "description": "Pagination offset for 'list_notes'"
        },
        "include_frontmatter": {
          "type": "boolean",
          "description": "Include YAML frontmatter in export (default: true)"
        },
        "content": {
          "type": "string",
          "enum": [
            "revised",
            "original"
          ],
          "description": "Content version for export: 'revised' (default) or 'original'"
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "manage_concepts",
    description: `Browse and curate the SKOS concept vocabulary, and manage concept schemes. Concepts are auto-created by the NLP pipeline as notes are tagged — use this to search, review candidates, monitor vocabulary health, and manage taxonomies. Actions: search, autocomplete, get, get_full, stats, top (concepts) | list_schemes, create_scheme, get_scheme, update_scheme, delete_scheme (schemes).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "search",
            "autocomplete",
            "get",
            "get_full",
            "stats",
            "top",
            "list_schemes",
            "create_scheme",
            "get_scheme",
            "update_scheme",
            "delete_scheme"
          ],
          "description": "Action: 'search' (find concepts), 'autocomplete' (quick lookup), 'get'/'get_full' (details), 'stats' (governance), 'top' (root concepts), 'list_schemes'/'create_scheme'/'get_scheme'/'update_scheme'/'delete_scheme' (scheme management)"
        },
        "q": {
          "type": "string",
          "description": "Search query or prefix (required for 'search'/'autocomplete')"
        },
        "scheme_id": {
          "type": "string",
          "format": "uuid",
          "description": "Filter by scheme UUID (for 'search'/'stats'/'top'), or target scheme (for 'get_scheme'/'update_scheme'/'delete_scheme')"
        },
        "status": {
          "type": "string",
          "enum": [
            "candidate",
            "approved",
            "deprecated"
          ],
          "description": "Filter by status (for 'search')"
        },
        "top_only": {
          "type": "boolean",
          "description": "Only return top-level concepts (for 'search')"
        },
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Concept UUID (required for 'get'/'get_full')"
        },
        "limit": {
          "type": "integer",
          "description": "Max results (default: 50 for search, 10 for autocomplete)"
        },
        "offset": {
          "type": "integer",
          "description": "Pagination offset (for 'search')"
        },
        "notation": {
          "type": "string",
          "description": "Short code for scheme (e.g., 'topics', 'domains') — required for 'create_scheme'"
        },
        "title": {
          "type": "string",
          "description": "Human-readable title — required for 'create_scheme', optional for 'update_scheme'"
        },
        "description": {
          "type": "string",
          "description": "Purpose and scope — optional for 'create_scheme'/'update_scheme'"
        },
        "uri": {
          "type": "string",
          "description": "Optional canonical URI (for 'create_scheme')"
        },
        "creator": {
          "type": "string",
          "description": "Creator attribution (for 'create_scheme'/'update_scheme')"
        },
        "publisher": {
          "type": "string",
          "description": "Publisher attribution (for 'create_scheme'/'update_scheme')"
        },
        "rights": {
          "type": "string",
          "description": "Rights/license information (for 'create_scheme'/'update_scheme')"
        },
        "version": {
          "type": "string",
          "description": "Version string (for 'create_scheme'/'update_scheme')"
        },
        "is_active": {
          "type": "boolean",
          "description": "Whether the scheme is active (for 'update_scheme')"
        },
        "force": {
          "type": "boolean",
          "description": "Delete even if scheme has concepts (for 'delete_scheme')"
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "manage_embeddings",
    description: `Manage embedding sets — curated subsets of notes for focused semantic search. Actions: list, get, create, update, delete, list_members, add_members, remove_member, refresh.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "list",
            "get",
            "create",
            "update",
            "delete",
            "list_members",
            "add_members",
            "remove_member",
            "refresh"
          ],
          "description": "Action: 'list' (all sets), 'get' (by slug), 'create'/'update'/'delete' (CRUD), 'list_members'/'add_members'/'remove_member' (membership), 'refresh' (re-embed)"
        },
        "slug": {
          "type": "string",
          "description": "Embedding set slug (required for get/update/delete/list_members/add_members/remove_member/refresh)"
        },
        "name": {
          "type": "string",
          "description": "Display name (required for create, optional for update)"
        },
        "description": {
          "type": "string",
          "description": "Set description (for create/update)"
        },
        "purpose": {
          "type": "string",
          "description": "Intended purpose of the set (for create/update)"
        },
        "usage_hints": {
          "type": "string",
          "description": "Guidance for agents on when to use this set (for create/update)"
        },
        "keywords": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Keywords for set discovery (for create/update)"
        },
        "mode": {
          "type": "string",
          "enum": ["auto", "manual", "mixed"],
          "description": "Membership mode: 'auto' (criteria-based), 'manual' (explicit), 'mixed' (both). Default: auto"
        },
        "criteria": {
          "type": "object",
          "properties": {
            "tags": {
              "type": "array",
              "items": { "type": "string" },
              "description": "Include notes with any of these tags"
            },
            "collections": {
              "type": "array",
              "items": { "type": "string" },
              "description": "Include notes in these collection IDs"
            },
            "fts_query": {
              "type": "string",
              "description": "Full-text search query for auto-inclusion"
            },
            "include_all": {
              "type": "boolean",
              "description": "Include all notes (universal set)"
            },
            "exclude_archived": {
              "type": "boolean",
              "description": "Exclude archived notes"
            }
          },
          "description": "Auto-membership criteria (for create/update with mode=auto/mixed)"
        },
        "note_ids": {
          "type": "array",
          "items": { "type": "string", "format": "uuid" },
          "description": "Note UUIDs to add (required for add_members)"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID to remove (required for remove_member)"
        },
        "added_by": {
          "type": "string",
          "description": "Attribution for who added members (for add_members)"
        },
        "limit": {
          "type": "integer",
          "description": "Max results for list_members (default: 50)"
        },
        "offset": {
          "type": "integer",
          "description": "Pagination offset for list_members"
        }
      },
      "required": [
        "action"
      ]
    },
  },
  {
    name: "manage_attachments",
    description: `Manage file attachments on notes. Upload, list, get metadata, download, and delete attachments. Image/audio/video attachments are automatically processed by the extraction pipeline.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "list",
            "upload",
            "get",
            "download",
            "delete"
          ],
          "description": "Action: 'list' (note's attachments), 'upload' (returns curl cmd), 'get' (metadata), 'download' (returns curl cmd), 'delete'"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID (required for 'list' and 'upload')"
        },
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Attachment UUID (required for 'get', 'download', 'delete')"
        },
        "filename": {
          "type": "string",
          "description": "Filename hint for upload curl command (e.g., 'photo.jpg')"
        },
        "content_type": {
          "type": "string",
          "description": "MIME type hint (e.g., 'image/jpeg'). If omitted, auto-detected from file extension."
        },
        "document_type_id": {
          "type": "string",
          "format": "uuid",
          "description": "Optional: explicit document type UUID override for upload (skips auto-classification)"
        }
      },
      "required": [
        "action"
      ]
    },
  },
  {
    name: "search_notes",
    description: `Search notes using hybrid, FTS, or semantic mode with tag filtering. See \`get_documentation(topic='search')\` for query syntax.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "query": {
          "type": "string",
          "description": "Search query (natural language or keywords)"
        },
        "limit": {
          "type": "number",
          "description": "Maximum results (default: 20)",
          "default": 20
        },
        "mode": {
          "type": "string",
          "enum": [
            "hybrid",
            "fts",
            "semantic"
          ],
          "description": "Search mode",
          "default": "hybrid"
        },
        "set": {
          "type": "string",
          "description": "Embedding set slug to restrict semantic search (optional)"
        },
        "collection_id": {
          "type": "string",
          "format": "uuid",
          "description": "Filter results to notes in this collection (optional)"
        },
        "required_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Strict filter: ALL results MUST have these tags (AND logic). Example: ['programming/rust']"
        },
        "excluded_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Strict filter: NO results should have these tags (NOT logic). Example: ['draft']"
        },
        "any_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Strict filter: results must have at least ONE of these tags (OR logic). Example: ['ai/ml', 'ai/nlp']"
        },
        "strict_filter": {
          "type": "string",
          "description": "Advanced: raw JSON strict filter string. Example: '{\"required_tags\":[\"tag1\"],\"excluded_tags\":[\"tag2\"]}'. Use required_tags/excluded_tags/any_tags params instead for convenience."
        }
      },
      "required": [
        "query"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "search_memories_by_location",
    description: `Find memories near GPS coordinates within a radius. See \`get_documentation(topic='search')\` for spatial queries.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "lat": {
          "type": "number",
          "description": "Latitude in decimal degrees (-90 to 90)",
          "minimum": -90,
          "maximum": 90
        },
        "lon": {
          "type": "number",
          "description": "Longitude in decimal degrees (-180 to 180)",
          "minimum": -180,
          "maximum": 180
        },
        "radius": {
          "type": "number",
          "description": "Search radius in meters (default: 1000)",
          "default": 1000,
          "exclusiveMinimum": 0
        }
      },
      "required": [
        "lat",
        "lon"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "search_memories_by_time",
    description: `Find memories captured within a time range. See \`get_documentation(topic='search')\` for temporal queries.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "start": {
          "type": "string",
          "description": "Start of time range (ISO 8601 format, e.g. '2024-01-15T00:00:00Z')"
        },
        "end": {
          "type": "string",
          "description": "End of time range (ISO 8601 format)"
        }
      },
      "required": [
        "start",
        "end"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "search_memories_combined",
    description: `Find memories by both location and time range. See \`get_documentation(topic='search')\` for combined queries.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "lat": {
          "type": "number",
          "description": "Latitude in decimal degrees (-90 to 90)",
          "minimum": -90,
          "maximum": 90
        },
        "lon": {
          "type": "number",
          "description": "Longitude in decimal degrees (-180 to 180)",
          "minimum": -180,
          "maximum": 180
        },
        "radius": {
          "type": "number",
          "description": "Search radius in meters (default: 1000)",
          "default": 1000,
          "exclusiveMinimum": 0
        },
        "start": {
          "type": "string",
          "description": "Start of time range (ISO 8601 format, e.g. '2024-01-15T00:00:00Z')"
        },
        "end": {
          "type": "string",
          "description": "End of time range (ISO 8601 format)"
        }
      },
      "required": [
        "lat",
        "lon",
        "start",
        "end"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_provenance_location",
    description: `Create a GPS location record for provenance tracking. See \`get_documentation(topic='provenance')\` for workflow.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "latitude": {
          "type": "number",
          "minimum": -90,
          "maximum": 90,
          "description": "Latitude in decimal degrees"
        },
        "longitude": {
          "type": "number",
          "minimum": -180,
          "maximum": 180,
          "description": "Longitude in decimal degrees"
        },
        "altitude_m": {
          "type": "number",
          "description": "Altitude in meters"
        },
        "horizontal_accuracy_m": {
          "type": "number",
          "description": "Horizontal accuracy in meters"
        },
        "vertical_accuracy_m": {
          "type": "number",
          "description": "Vertical accuracy in meters"
        },
        "heading_degrees": {
          "type": "number",
          "minimum": 0,
          "maximum": 360,
          "description": "Compass heading in degrees"
        },
        "speed_mps": {
          "type": "number",
          "minimum": 0,
          "description": "Speed in meters per second"
        },
        "named_location_id": {
          "type": "string",
          "format": "uuid",
          "description": "Link to a named location"
        },
        "source": {
          "type": "string",
          "enum": [
            "gps_exif",
            "device_api",
            "user_manual",
            "geocoded",
            "ai_estimated"
          ],
          "description": "How location was obtained"
        },
        "confidence": {
          "type": "string",
          "enum": [
            "high",
            "medium",
            "low",
            "unknown"
          ],
          "description": "Location accuracy level"
        }
      },
      "required": [
        "latitude",
        "longitude",
        "source",
        "confidence"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "create_named_location",
    description: `Register a named place (home, work, POI) with coordinates. See \`get_documentation(topic='provenance')\` for details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Location display name"
        },
        "location_type": {
          "type": "string",
          "enum": [
            "home",
            "work",
            "poi",
            "city",
            "region",
            "country"
          ],
          "description": "Location category"
        },
        "latitude": {
          "type": "number",
          "minimum": -90,
          "maximum": 90,
          "description": "Latitude"
        },
        "longitude": {
          "type": "number",
          "minimum": -180,
          "maximum": 180,
          "description": "Longitude"
        },
        "radius_m": {
          "type": "number",
          "minimum": 0,
          "description": "Boundary radius in meters"
        },
        "address_line": {
          "type": "string",
          "description": "Street address"
        },
        "locality": {
          "type": "string",
          "description": "City/town"
        },
        "admin_area": {
          "type": "string",
          "description": "State/province"
        },
        "country": {
          "type": "string",
          "description": "Country name"
        },
        "country_code": {
          "type": "string",
          "description": "ISO country code"
        },
        "postal_code": {
          "type": "string",
          "description": "Postal/ZIP code"
        },
        "timezone": {
          "type": "string",
          "description": "IANA timezone"
        },
        "altitude_m": {
          "type": "number",
          "description": "Altitude in meters"
        },
        "is_private": {
          "type": "boolean",
          "description": "Whether location is private"
        },
        "metadata": {
          "type": "object",
          "description": "Additional metadata"
        }
      },
      "required": [
        "name",
        "location_type",
        "latitude",
        "longitude"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "create_provenance_device",
    description: `Register a capture device (camera, phone). Auto-deduplicates by make+model. See \`get_documentation(topic='provenance')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "device_make": {
          "type": "string",
          "description": "Device manufacturer"
        },
        "device_model": {
          "type": "string",
          "description": "Device model name"
        },
        "device_os": {
          "type": "string",
          "description": "Operating system"
        },
        "device_os_version": {
          "type": "string",
          "description": "OS version"
        },
        "software": {
          "type": "string",
          "description": "Capture software name"
        },
        "software_version": {
          "type": "string",
          "description": "Software version"
        },
        "has_gps": {
          "type": "boolean",
          "description": "Device has GPS"
        },
        "has_accelerometer": {
          "type": "boolean",
          "description": "Device has accelerometer"
        },
        "sensor_metadata": {
          "type": "object",
          "description": "Additional sensor details"
        },
        "device_name": {
          "type": "string",
          "description": "User-friendly device name"
        }
      },
      "required": [
        "device_make",
        "device_model"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "create_file_provenance",
    description: `Link a file attachment to spatial-temporal context (location, device, time). See \`get_documentation(topic='provenance')\` for workflow.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "attachment_id": {
          "type": "string",
          "format": "uuid",
          "description": "Attachment UUID to link provenance to"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Optional note UUID to directly associate provenance with a note"
        },
        "capture_time_start": {
          "type": "string",
          "format": "date-time",
          "description": "Capture start time (ISO 8601)"
        },
        "capture_time_end": {
          "type": "string",
          "format": "date-time",
          "description": "Capture end time (ISO 8601)"
        },
        "capture_timezone": {
          "type": "string",
          "description": "Capture timezone (e.g., America/New_York)"
        },
        "capture_duration_seconds": {
          "type": "number",
          "minimum": 0,
          "description": "Duration in seconds"
        },
        "time_source": {
          "type": "string",
          "enum": [
            "exif",
            "file_mtime",
            "user_manual",
            "ai_estimated",
            "device_clock"
          ],
          "description": "How capture time was determined"
        },
        "time_confidence": {
          "type": "string",
          "enum": [
            "high",
            "medium",
            "low",
            "unknown"
          ],
          "description": "Time accuracy level"
        },
        "location_id": {
          "type": "string",
          "format": "uuid",
          "description": "Location UUID from create_provenance_location"
        },
        "device_id": {
          "type": "string",
          "format": "uuid",
          "description": "Device UUID from create_provenance_device"
        },
        "event_type": {
          "type": "string",
          "enum": [
            "photo",
            "video",
            "audio",
            "scan",
            "screenshot",
            "recording"
          ],
          "description": "Type of capture event"
        },
        "event_title": {
          "type": "string",
          "description": "Human-readable event title"
        },
        "event_description": {
          "type": "string",
          "description": "Detailed event description"
        },
        "raw_metadata": {
          "type": "object",
          "description": "Raw EXIF/XMP/IPTC metadata"
        }
      },
      "required": [
        "attachment_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "create_note_provenance",
    description: `Link a note directly to spatial-temporal context without requiring an attachment. See \`get_documentation(topic='provenance')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID to attach provenance to"
        },
        "capture_time_start": {
          "type": "string",
          "format": "date-time",
          "description": "Event start time (ISO 8601)"
        },
        "capture_time_end": {
          "type": "string",
          "format": "date-time",
          "description": "Event end time (ISO 8601)"
        },
        "capture_timezone": {
          "type": "string",
          "description": "Timezone (e.g., America/New_York)"
        },
        "time_source": {
          "type": "string",
          "enum": [
            "gps",
            "network",
            "manual",
            "file_metadata",
            "device_clock"
          ],
          "description": "How time was determined"
        },
        "time_confidence": {
          "type": "string",
          "enum": [
            "exact",
            "approximate",
            "estimated"
          ],
          "description": "Time accuracy level"
        },
        "location_id": {
          "type": "string",
          "format": "uuid",
          "description": "Location UUID from create_provenance_location"
        },
        "device_id": {
          "type": "string",
          "format": "uuid",
          "description": "Device UUID from create_provenance_device"
        },
        "event_type": {
          "type": "string",
          "enum": [
            "created",
            "modified",
            "accessed",
            "shared"
          ],
          "description": "Type of note event"
        },
        "event_title": {
          "type": "string",
          "description": "Human-readable event title"
        },
        "event_description": {
          "type": "string",
          "description": "Detailed event description"
        }
      },
      "required": [
        "note_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "list_tags",
    description: `List all tags with usage counts. Tags use hierarchical '/' separator.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_note_links",
    description: `Get semantic links between a note and related notes.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the note"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "export_note",
    description: `Export a note as markdown with optional YAML frontmatter. See \`get_documentation(topic='versioning')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the note to export"
        },
        "include_frontmatter": {
          "type": "boolean",
          "description": "Include YAML metadata header (default: true)",
          "default": true
        },
        "content": {
          "type": "string",
          "enum": [
            "revised",
            "original"
          ],
          "description": "Content version to export (default: revised)",
          "default": "revised"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_note",
    description: `Create a single note with optional tags, revision mode, and collection placement. See \`get_documentation(topic='notes')\` for revision modes.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "content": {
          "type": "string",
          "description": "Note content in markdown format"
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Optional tags. Use hierarchical paths like 'topic/subtopic' (max 5 levels). Examples: 'archive', 'programming/rust', 'ai/ml/transformers'"
        },
        "revision_mode": {
          "type": "string",
          "enum": [
            "full",
            "light",
            "none"
          ],
          "description": "AI revision mode: 'full' (default) for contextual expansion, 'light' for formatting only without inventing details, 'none' to skip AI revision entirely",
          "default": "light"
        },
        "collection_id": {
          "type": "string",
          "format": "uuid",
          "description": "Optional collection UUID to place the note in"
        },
        "metadata": {
          "type": "object",
          "description": "Optional arbitrary key-value metadata to attach to the note (e.g., { source: 'meeting', priority: 'high' })"
        }
      },
      "required": [
        "content"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "bulk_create_notes",
    description: `Create multiple notes at once (max 100). See \`get_documentation(topic='notes')\` for batch creation.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "notes": {
          "type": "array",
          "description": "Array of notes to create (max 100)",
          "items": {
            "type": "object",
            "properties": {
              "content": {
                "type": "string",
                "description": "Note content in markdown format"
              },
              "tags": {
                "type": "array",
                "items": {
                  "type": "string"
                },
                "description": "Optional hierarchical tags (e.g., 'topic/subtopic', max 5 levels)"
              },
              "metadata": {
                "type": "object",
                "description": "Optional JSON metadata for the note (arbitrary key-value pairs)"
              },
              "revision_mode": {
                "type": "string",
                "enum": [
                  "full",
                  "light",
                  "none"
                ],
                "description": "AI revision mode for this note",
                "default": "light"
              }
            },
            "required": [
              "content"
            ]
          }
        }
      },
      "required": [
        "notes"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "update_note",
    description: `Update an existing note's content, tags, or metadata. Creates a new version. See \`get_documentation(topic='notes')\` for details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the note to update"
        },
        "content": {
          "type": "string",
          "description": "New markdown content (triggers full AI pipeline)"
        },
        "starred": {
          "type": "boolean",
          "description": "Mark as important (no processing)"
        },
        "archived": {
          "type": "boolean",
          "description": "Archive the note (no processing)"
        },
        "revision_mode": {
          "type": "string",
          "enum": [
            "full",
            "light",
            "none"
          ],
          "description": "AI revision mode: 'full' (default) for contextual expansion, 'light' for formatting only without inventing details, 'none' to skip AI revision entirely",
          "default": "light"
        },
        "metadata": {
          "type": "object",
          "description": "Optional arbitrary key-value metadata to update (e.g., { source: 'meeting', priority: 'high' })"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_note",
    description: `Soft-delete a note (recoverable via restore_note).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the note to delete"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "restore_note",
    description: `Restore a previously soft-deleted note.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "UUID of the deleted note to restore"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "set_note_tags",
    description: `Replace all user tags on a note. AI-generated tags are preserved separately. See \`get_documentation(topic='concepts')\` for tag format.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the note"
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "New tags (replaces existing). Use hierarchical paths like 'topic/subtopic' (max 5 levels)"
        }
      },
      "required": [
        "id",
        "tags"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "create_job",
    description: `Queue a single processing step (revision, embedding, linking, extraction). See \`get_documentation(topic='jobs')\` for job types.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note to process"
        },
        "job_type": {
          "type": "string",
          "description": "Single processing step to run. Valid types: ai_revision, embedding, linking, context_update, title_generation, concept_tagging, re_embed_all, extraction, exif_extraction"
        },
        "priority": {
          "type": "number",
          "description": "Job priority (higher = sooner)"
        },
        "payload": {
          "type": "object",
          "description": "Optional JSON payload for the job. Required for extraction jobs: { strategy, attachment_id, filename, mime_type }. Example: { \"strategy\": \"video_multimodal\", \"attachment_id\": \"uuid\", \"filename\": \"clip.mp4\", \"mime_type\": \"video/mp4\" }"
        },
        "deduplicate": {
          "type": "boolean",
          "description": "When true, skip if a pending or running job with the same note_id+job_type already exists. Returns status 'already_pending' instead of creating a duplicate."
        }
      },
      "required": [
        "job_type"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "list_jobs",
    description: `List background processing jobs with optional status/type filters. See \`get_documentation(topic='jobs')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "status": {
          "type": "string",
          "enum": [
            "pending",
            "running",
            "completed",
            "failed"
          ],
          "description": "Filter by job status"
        },
        "job_type": {
          "type": "string",
          "enum": [
            "ai_revision",
            "embedding",
            "linking",
            "context_update",
            "title_generation",
            "concept_tagging",
            "re_embed_all",
            "extraction",
            "exif_extraction"
          ],
          "description": "Filter by job type"
        },
        "note_id": {
          "type": "string",
          "description": "Filter by specific note UUID"
        },
        "limit": {
          "type": "number",
          "description": "Max results (default: 50)",
          "default": 50
        },
        "offset": {
          "type": "number",
          "description": "Pagination offset",
          "default": 0
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_queue_stats",
    description: `Get job queue statistics: pending, running, completed, failed counts.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "health_check",
    description: `Check API health status.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_system_info",
    description: `Get system capabilities, version, and enabled features.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_available_models",
    description: `List available LLM models with capability metadata. Returns all models from configured providers (Ollama, OpenAI, OpenRouter) and Whisper (transcription), including which model is the default for each capability. Also returns registered providers and their health status. Model slugs can be provider-qualified (e.g. "openai:gpt-4o", "openrouter:anthropic/claude-sonnet-4-20250514") — bare slugs route to the default provider (Ollama).`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_collections",
    description: `List collections (folders). Use parent_id to list children of a specific collection.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "parent_id": {
          "type": "string",
          "description": "Parent collection UUID (omit for root collections)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_collection",
    description: `Create a new collection (folder). Supports nesting via parent_id.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Collection name"
        },
        "description": {
          "type": "string",
          "description": "Optional description"
        },
        "parent_id": {
          "type": "string",
          "description": "Parent collection UUID for nesting"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_collection",
    description: `Get collection metadata (name, description, parent, note count).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Collection UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "delete_collection",
    description: `Delete a collection. Notes are moved to uncategorized. Use force=true if collection has notes.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Collection UUID to delete"
        },
        "force": {
          "type": "boolean",
          "description": "Force delete even if collection contains notes (default: false)"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "update_collection",
    description: `Update collection name, description, or parent (to reorganize hierarchy).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID to update"
        },
        "name": {
          "type": "string",
          "description": "New collection name"
        },
        "description": {
          "type": "string",
          "description": "New collection description"
        },
        "parent_id": {
          "anyOf": [
            {
              "type": "string",
              "format": "uuid"
            },
            {
              "type": "null"
            }
          ],
          "description": "New parent collection UUID, or null to move to root"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_collection_notes",
    description: `List all notes in a specific collection with pagination.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Collection UUID"
        },
        "limit": {
          "type": "number",
          "description": "Maximum results (default: 50)",
          "default": 50
        },
        "offset": {
          "type": "number",
          "description": "Pagination offset",
          "default": 0
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "move_note_to_collection",
    description: `Move a note to a different collection. Omit collection_id for uncategorized.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "Note UUID to move"
        },
        "collection_id": {
          "type": "string",
          "description": "Target collection UUID (omit for uncategorized)"
        }
      },
      "required": [
        "note_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "explore_graph",
    description: `Traverse the knowledge graph from a starting note up to N hops. Returns nodes and edges. See \`get_documentation(topic='notes')\` for graph details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Starting note UUID"
        },
        "depth": {
          "type": "number",
          "description": "Maximum hops to traverse (default: 2)",
          "default": 2
        },
        "max_nodes": {
          "type": "number",
          "description": "Maximum total nodes to return, including the starting node (default: 50)",
          "default": 50
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_topology_stats",
    description: `Get graph topology statistics including degree distribution, connected components, isolated nodes, and current linking strategy. Useful for monitoring graph health after auto-linking.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_templates",
    description: `List all available note templates.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_template",
    description: `Create a reusable note template with {{variable}} placeholders. See \`get_documentation(topic='templates')\` for syntax.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Unique template name"
        },
        "description": {
          "type": "string",
          "description": "What this template is for"
        },
        "content": {
          "type": "string",
          "description": "Template content with {{variable}} placeholders"
        },
        "format": {
          "type": "string",
          "description": "Content format (default: markdown)",
          "default": "markdown"
        },
        "default_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Tags to apply by default"
        },
        "collection_id": {
          "type": "string",
          "description": "Default collection for instantiated notes"
        }
      },
      "required": [
        "name",
        "content"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_template",
    description: `Get template details including content and variable list.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Template UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "delete_template",
    description: `Delete a note template.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Template UUID to delete"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "update_template",
    description: `Update a template's content, name, tags, or collection assignment.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Template UUID to update"
        },
        "name": {
          "type": "string",
          "description": "New template name"
        },
        "description": {
          "type": "string",
          "description": "New template description"
        },
        "content": {
          "type": "string",
          "description": "New template content with {{variable}} placeholders"
        },
        "format": {
          "type": "string",
          "description": "Content format (e.g., markdown, plain)"
        },
        "default_tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "New default tags"
        },
        "collection_id": {
          "anyOf": [
            {
              "type": "string",
              "format": "uuid"
            },
            {
              "type": "null"
            }
          ],
          "description": "New default collection UUID, or null for none"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "instantiate_template",
    description: `Create a note from a template with variable substitutions. See \`get_documentation(topic='templates')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Template UUID to instantiate"
        },
        "variables": {
          "type": "object",
          "additionalProperties": {
            "type": "string"
          },
          "description": "Variable substitutions: { 'placeholder': 'value' }"
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Additional tags to merge with template default_tags"
        },
        "collection_id": {
          "type": "string",
          "description": "Override default collection"
        },
        "revision_mode": {
          "type": "string",
          "enum": [
            "full",
            "light",
            "none"
          ],
          "description": "AI revision mode (default: light)",
          "default": "light"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "list_embedding_sets",
    description: `List all embedding sets. See \`get_documentation(topic='embedding_configs')\` for set types.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_embedding_set",
    description: `Get embedding set details including member count and configuration.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "slug": {
          "type": "string",
          "description": "Embedding set slug"
        }
      },
      "required": [
        "slug"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_embedding_set",
    description: `Create a new embedding set (filter or full). See \`get_documentation(topic='embedding_configs')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Display name for the set"
        },
        "slug": {
          "type": "string",
          "description": "URL-friendly identifier (auto-generated if omitted)"
        },
        "description": {
          "type": "string",
          "description": "What this set is for"
        },
        "purpose": {
          "type": "string",
          "description": "Detailed purpose (helps AI agents decide when to use)"
        },
        "usage_hints": {
          "type": "string",
          "description": "When and how to use this set"
        },
        "keywords": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Discovery keywords"
        },
        "mode": {
          "type": "string",
          "enum": [
            "auto",
            "manual",
            "mixed"
          ],
          "description": "Membership mode",
          "default": "auto"
        },
        "criteria": {
          "type": "object",
          "description": "Auto-membership criteria",
          "properties": {
            "include_all": {
              "type": "boolean",
              "description": "Include all notes"
            },
            "tags": {
              "type": "array",
              "items": {
                "type": "string"
              },
              "description": "Include notes with these tags"
            },
            "collections": {
              "type": "array",
              "items": {
                "type": "string"
              },
              "description": "Include notes in these collection UUIDs"
            },
            "fts_query": {
              "type": "string",
              "description": "Include notes matching this FTS query"
            },
            "exclude_archived": {
              "type": "boolean",
              "description": "Exclude archived notes",
              "default": true
            }
          }
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "list_set_members",
    description: `List notes in an embedding set with pagination.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "slug": {
          "type": "string",
          "description": "Embedding set slug"
        },
        "limit": {
          "type": "number",
          "description": "Maximum results",
          "default": 50
        },
        "offset": {
          "type": "number",
          "description": "Pagination offset",
          "default": 0
        }
      },
      "required": [
        "slug"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "add_set_members",
    description: `Add notes to an embedding set.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "slug": {
          "type": "string",
          "description": "Embedding set slug"
        },
        "note_ids": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Note UUIDs to add"
        },
        "added_by": {
          "type": "string",
          "description": "Who/what added these notes"
        }
      },
      "required": [
        "slug",
        "note_ids"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "remove_set_member",
    description: `Remove a note from an embedding set.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "slug": {
          "type": "string",
          "description": "Embedding set slug"
        },
        "note_id": {
          "type": "string",
          "description": "Note UUID to remove"
        }
      },
      "required": [
        "slug",
        "note_id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "update_embedding_set",
    description: `Update embedding set configuration or membership criteria.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "slug": {
          "type": "string",
          "description": "Embedding set slug to update"
        },
        "name": {
          "type": "string",
          "description": "New display name"
        },
        "description": {
          "type": "string",
          "description": "New description"
        },
        "purpose": {
          "type": "string",
          "description": "New detailed purpose"
        },
        "usage_hints": {
          "type": "string",
          "description": "New usage hints"
        },
        "keywords": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "New discovery keywords"
        },
        "criteria": {
          "type": "object",
          "description": "New auto-inclusion criteria"
        },
        "mode": {
          "type": "string",
          "enum": [
            "auto",
            "manual",
            "mixed"
          ],
          "description": "New mode"
        }
      },
      "required": [
        "slug"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_embedding_set",
    description: `Delete an embedding set (notes are preserved).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "slug": {
          "type": "string",
          "description": "Embedding set slug to delete (cannot be 'default')"
        }
      },
      "required": [
        "slug"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "refresh_embedding_set",
    description: `Refresh embeddings in a set (re-embed all members).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "slug": {
          "type": "string",
          "description": "Embedding set slug"
        }
      },
      "required": [
        "slug"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "reembed_all",
    description: `Re-embed all notes or a specific embedding set.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "embedding_set_slug": {
          "type": "string",
          "description": "Optional: Limit re-embedding to specific embedding set"
        },
        "force": {
          "type": "boolean",
          "description": "If true, regenerate even if embeddings exist (future use)",
          "default": false
        }
      }
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "purge_note",
    description: `Permanently delete a single soft-deleted note. This cannot be undone.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "Note UUID to permanently delete"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "purge_notes",
    description: `Permanently delete multiple soft-deleted notes by ID. This cannot be undone.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_ids": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Array of note UUIDs to permanently delete"
        }
      },
      "required": [
        "note_ids"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "purge_all_notes",
    description: `Permanently delete ALL soft-deleted notes. Requires confirmation. This cannot be undone.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "confirm": {
          "type": "boolean",
          "description": "Must be true to confirm this destructive operation"
        }
      },
      "required": [
        "confirm"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "export_all_notes",
    description: `Export all notes with optional filters. See \`get_documentation(topic='backup')\` for export options.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filter": {
          "type": "object",
          "description": "Optional filters to scope the export (starred, tags, date range)",
          "properties": {
            "starred_only": {
              "type": "boolean",
              "description": "Only export starred notes"
            },
            "tags": {
              "type": "array",
              "items": {
                "type": "string"
              },
              "description": "Only export notes with these tags"
            },
            "created_after": {
              "type": "string",
              "description": "Only export notes created after this date (ISO 8601)"
            },
            "created_before": {
              "type": "string",
              "description": "Only export notes created before this date (ISO 8601)"
            }
          }
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "backup_now",
    description: `Trigger an immediate database backup.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "destinations": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": [
              "local",
              "s3",
              "rsync"
            ]
          },
          "description": "Limit to specific destinations (default: all configured)"
        },
        "dry_run": {
          "type": "boolean",
          "default": false,
          "description": "Preview backup without executing (default: false)"
        }
      }
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "backup_status",
    description: `Get current backup status and last backup time.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "backup_download",
    description: `Download a backup file. Returns curl command.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "starred_only": {
          "type": "boolean",
          "description": "Only include starred notes"
        },
        "tags": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Only include notes with these tags"
        },
        "created_after": {
          "type": "string",
          "description": "Only include notes created after this date (ISO 8601)"
        },
        "created_before": {
          "type": "string",
          "description": "Only include notes created before this date (ISO 8601)"
        },
        "output_dir": {
          "type": "string",
          "description": "Directory prefix for the output filename in the curl command"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "backup_import",
    description: `Import notes from a backup file.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "file_path": {
          "type": "string",
          "description": "Path to the JSON backup file on disk (from backup_download or export_all_notes)"
        },
        "dry_run": {
          "type": "boolean",
          "description": "Validate without importing",
          "default": false
        },
        "on_conflict": {
          "type": "string",
          "enum": [
            "skip",
            "replace",
            "merge"
          ],
          "description": "Conflict resolution strategy",
          "default": "skip"
        }
      },
      "required": [
        "file_path"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "knowledge_shard",
    description: `Export a subset of knowledge as a portable shard.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "include": {
          "type": "string",
          "description": "Components to include: comma-separated list (notes,collections,tags,templates,links,embedding_sets,embeddings) or 'all'. Default: notes,collections,tags,templates,links,embedding_sets"
        },
        "output_dir": {
          "type": "string",
          "description": "Directory prefix for the output filename in the curl command"
        }
      }
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "knowledge_shard_import",
    description: `Import a knowledge shard into the current memory via multipart file upload.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "file_path": {
          "type": "string",
          "description": "Path to the .tar.gz shard file on disk (from knowledge_shard tool)"
        },
        "include": {
          "type": "string",
          "description": "Components to import (default: all)"
        },
        "dry_run": {
          "type": "boolean",
          "default": false,
          "description": "Preview import without writing data (default: false)"
        },
        "on_conflict": {
          "type": "string",
          "enum": [
            "skip",
            "replace",
            "merge"
          ],
          "default": "skip",
          "description": "Conflict resolution: skip (keep existing), replace (overwrite), merge (add new only)"
        },
        "skip_embedding_regen": {
          "type": "boolean",
          "default": false,
          "description": "Skip embedding regeneration if shard includes embeddings (default: false)"
        }
      },
      "required": [
        "file_path"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "database_snapshot",
    description: `Create a full database snapshot.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Filename suffix (alphanumeric/-/_)"
        },
        "title": {
          "type": "string",
          "description": "Human-readable title"
        },
        "description": {
          "type": "string",
          "description": "Why this backup was created"
        }
      }
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "database_restore",
    description: `Restore from a database snapshot. See \`get_documentation(topic='backup')\` for restore process.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filename": {
          "type": "string",
          "description": "Backup file from list_backups (e.g., snapshot_database_*.sql.gz)"
        },
        "skip_snapshot": {
          "type": "boolean",
          "default": false,
          "description": "DANGEROUS: Skip prerestore backup"
        }
      },
      "required": [
        "filename"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "knowledge_archive_download",
    description: `Download a full knowledge archive. Returns curl command.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filename": {
          "type": "string",
          "description": "Backup filename from list_backups (e.g., snapshot_database_*.sql.gz)"
        },
        "output_dir": {
          "type": "string",
          "description": "Directory to save the archive file (default: system temp dir)"
        }
      },
      "required": [
        "filename"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "knowledge_archive_upload",
    description: `Upload a full knowledge archive.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "file_path": {
          "type": "string",
          "description": "Path to the .archive file on disk"
        },
        "filename": {
          "type": "string",
          "description": "Original filename (optional, for logging)"
        }
      },
      "required": [
        "file_path"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "list_backups",
    description: `List all available backup files.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_backup_info",
    description: `Get metadata for a specific backup file.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filename": {
          "type": "string",
          "description": "From list_backups"
        }
      },
      "required": [
        "filename"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_backup_metadata",
    description: `Get extended metadata for a backup file.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filename": {
          "type": "string",
          "description": "From list_backups"
        }
      },
      "required": [
        "filename"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "update_backup_metadata",
    description: `Update metadata on a backup file.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filename": {
          "type": "string",
          "description": "Backup filename from list_backups"
        },
        "title": {
          "type": "string",
          "description": "Human-readable title for the backup"
        },
        "description": {
          "type": "string",
          "description": "Description of backup contents or purpose"
        }
      },
      "required": [
        "filename"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "memory_info",
    description: `Get metadata about the current memory archive.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_concept_schemes",
    description: `List all SKOS concept schemes. See \`get_documentation(topic='concepts')\` for taxonomy management.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_concept_scheme",
    description: `Create a new SKOS concept scheme. See \`get_documentation(topic='concepts')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "notation": {
          "type": "string",
          "description": "Short code (e.g., 'topics', 'domains')"
        },
        "title": {
          "type": "string",
          "description": "Human-readable title"
        },
        "description": {
          "type": "string",
          "description": "Purpose and scope of this vocabulary"
        },
        "uri": {
          "type": "string",
          "description": "Optional canonical URI"
        }
      },
      "required": [
        "notation",
        "title"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_concept_scheme",
    description: `Get concept scheme details including statistics.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept scheme"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "update_concept_scheme",
    description: `Update scheme metadata (title, description, namespace). See \`get_documentation(topic='concepts')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept scheme to update"
        },
        "title": {
          "type": "string",
          "description": "New human-readable title"
        },
        "description": {
          "type": "string",
          "description": "New description/purpose"
        },
        "creator": {
          "type": "string",
          "description": "Creator attribution"
        },
        "publisher": {
          "type": "string",
          "description": "Publisher attribution"
        },
        "rights": {
          "type": "string",
          "description": "Rights/license information"
        },
        "version": {
          "type": "string",
          "description": "Version string"
        },
        "is_active": {
          "type": "boolean",
          "description": "Whether the scheme is active"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_concept_scheme",
    description: `Delete a concept scheme and all its concepts. This cannot be undone.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "UUID of the concept scheme to delete"
        },
        "force": {
          "type": "boolean",
          "description": "Delete even if scheme has concepts",
          "default": false
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "search_concepts",
    description: `Search SKOS concepts across labels. See \`get_documentation(topic='concepts')\` for filtering.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "q": {
          "type": "string",
          "description": "Search query (matches labels)"
        },
        "scheme_id": {
          "type": "string",
          "description": "Filter by scheme UUID"
        },
        "status": {
          "type": "string",
          "enum": [
            "candidate",
            "approved",
            "deprecated"
          ],
          "description": "Filter by status"
        },
        "top_only": {
          "type": "boolean",
          "description": "Only return top-level concepts (no broader)"
        },
        "limit": {
          "type": "number",
          "default": 50,
          "description": "Maximum results to return (default: 50)"
        },
        "offset": {
          "type": "number",
          "default": 0,
          "description": "Pagination offset (default: 0)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_concept",
    description: `Create a new SKOS concept with labels, definition, and relationships. See \`get_documentation(topic='concepts')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "scheme_id": {
          "type": "string",
          "description": "UUID of the scheme"
        },
        "pref_label": {
          "type": "string",
          "description": "Primary label (required)"
        },
        "notation": {
          "type": "string",
          "description": "Short code within scheme"
        },
        "alt_labels": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Alternative labels/synonyms"
        },
        "definition": {
          "type": "string",
          "description": "Formal definition"
        },
        "scope_note": {
          "type": "string",
          "description": "Usage guidance"
        },
        "broader_ids": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Parent concept UUIDs (max 3)"
        },
        "related_ids": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Related concept UUIDs"
        },
        "facet_type": {
          "type": "string",
          "enum": [
            "personality",
            "matter",
            "energy",
            "space",
            "time"
          ],
          "description": "PMEST facet"
        },
        "facet_domain": {
          "type": "string",
          "description": "Domain context"
        }
      },
      "required": [
        "scheme_id",
        "pref_label"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_concept",
    description: `Get basic concept info (scheme, labels, definition, status).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_concept_full",
    description: `Get full concept details including hierarchy, relationships, and mappings.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "update_concept",
    description: `Update concept properties (notation, status, facet, deprecation). See \`get_documentation(topic='concepts')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        },
        "notation": {
          "type": "string",
          "description": "Short code/identifier for the concept (e.g., 'ML', 'NLP')"
        },
        "status": {
          "type": "string",
          "enum": [
            "candidate",
            "approved",
            "deprecated",
            "obsolete"
          ],
          "description": "Concept lifecycle status"
        },
        "deprecation_reason": {
          "type": "string",
          "description": "Reason for deprecation (required when setting status to deprecated)"
        },
        "replaced_by_id": {
          "type": "string",
          "description": "UUID of replacement concept when deprecating"
        },
        "facet_type": {
          "type": "string",
          "enum": [
            "personality",
            "matter",
            "energy",
            "space",
            "time"
          ],
          "description": "PMEST facet classification for the concept"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_concept",
    description: `Delete a concept permanently. Must not be tagged on any notes.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "autocomplete_concepts",
    description: `Fast autocomplete for concept labels across pref/alt/hidden labels.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "q": {
          "type": "string",
          "description": "Prefix to match"
        },
        "limit": {
          "type": "number",
          "default": 10,
          "description": "Maximum suggestions to return (default: 10)"
        }
      },
      "required": [
        "q"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_broader",
    description: `Get broader (parent) concepts in the hierarchy.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "add_broader",
    description: `Add a broader (parent) relationship between concepts.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the child concept"
        },
        "target_id": {
          "type": "string",
          "description": "UUID of the parent concept"
        }
      },
      "required": [
        "id",
        "target_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_narrower",
    description: `Get narrower (child) concepts in the hierarchy.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "add_narrower",
    description: `Add a narrower (child) relationship between concepts.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the parent concept"
        },
        "target_id": {
          "type": "string",
          "description": "UUID of the child concept"
        }
      },
      "required": [
        "id",
        "target_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_related",
    description: `Get non-hierarchical related concepts.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "add_related",
    description: `Add a non-hierarchical related relationship between concepts.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the concept"
        },
        "target_id": {
          "type": "string",
          "description": "UUID of the related concept"
        }
      },
      "required": [
        "id",
        "target_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "tag_note_concept",
    description: `Tag a note with a SKOS concept. Use is_primary to mark the main concept.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        },
        "concept_id": {
          "type": "string",
          "description": "UUID of the concept"
        },
        "is_primary": {
          "type": "boolean",
          "default": false,
          "description": "Mark as primary tag"
        }
      },
      "required": [
        "note_id",
        "concept_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "untag_note_concept",
    description: `Remove a SKOS concept tag from a note.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        },
        "concept_id": {
          "type": "string",
          "description": "UUID of the concept"
        }
      },
      "required": [
        "note_id",
        "concept_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_note_concepts",
    description: `Get all SKOS concepts tagged on a note with their labels.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        }
      },
      "required": [
        "note_id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_governance_stats",
    description: `Get taxonomy health statistics (total, candidates, orphans, usage). See \`get_documentation(topic='concepts')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "scheme_id": {
          "type": "string",
          "description": "UUID of the scheme (uses default if not provided)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_top_concepts",
    description: `Get top-level (root) concepts in a scheme for hierarchy navigation.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "scheme_id": {
          "type": "string",
          "description": "UUID of the scheme"
        }
      },
      "required": [
        "scheme_id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_note_versions",
    description: `List all versions of a note. See \`get_documentation(topic='versioning')\` for dual-track versioning.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        }
      },
      "required": [
        "note_id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_note_version",
    description: `Get a specific version's content (both original and revised tracks).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        },
        "version": {
          "type": "integer",
          "description": "Version number to retrieve"
        },
        "track": {
          "type": "string",
          "enum": [
            "original",
            "revision"
          ],
          "default": "original",
          "description": "Which track: original (user content) or revision (AI enhanced)"
        }
      },
      "required": [
        "note_id",
        "version"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "restore_note_version",
    description: `Restore a note to a previous version (creates a new version).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        },
        "version": {
          "type": "integer",
          "description": "Version number to restore"
        },
        "restore_tags": {
          "type": "boolean",
          "default": false,
          "description": "Also restore tags from the version snapshot"
        }
      },
      "required": [
        "note_id",
        "version"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_note_version",
    description: `Delete a specific version from history.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        },
        "version": {
          "type": "integer",
          "description": "Version number to delete"
        }
      },
      "required": [
        "note_id",
        "version"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "diff_note_versions",
    description: `Compare two versions showing additions, deletions, and changes.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "description": "UUID of the note"
        },
        "from_version": {
          "type": "integer",
          "description": "Version to diff from (older)"
        },
        "to_version": {
          "type": "integer",
          "description": "Version to diff to (newer)"
        }
      },
      "required": [
        "note_id",
        "from_version",
        "to_version"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_full_document",
    description: `Reconstruct a chunked document by stitching all chunks together. For regular notes, returns content as-is. See \`get_documentation(topic='chunking')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "description": "UUID of the note or chain ID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "search_with_dedup",
    description: `Search with explicit chunk deduplication — one result per document. Same as search_notes with dedup emphasis. See \`get_documentation(topic='chunking')\` for details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "query": {
          "type": "string",
          "description": "Search query (natural language or keywords)"
        },
        "limit": {
          "type": "number",
          "description": "Maximum results (default: 20)",
          "default": 20
        },
        "mode": {
          "type": "string",
          "enum": [
            "hybrid",
            "fts",
            "semantic"
          ],
          "description": "Search mode",
          "default": "hybrid"
        },
        "set": {
          "type": "string",
          "description": "Embedding set slug to restrict semantic search (optional)"
        }
      },
      "required": [
        "query"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_chunk_chain",
    description: `Get all chunks in a document chain with sequence metadata. See \`get_documentation(topic='chunking')\` for chunk structure.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "chain_id": {
          "type": "string",
          "description": "UUID of the chain (first chunk ID or any chunk in chain)"
        },
        "include_content": {
          "type": "boolean",
          "description": "Include full reconstructed content (default: true)",
          "default": true
        }
      },
      "required": [
        "chain_id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_documentation",
    description: `Get detailed documentation for a topic. Use topic='all' for complete reference.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "topic": {
          "type": "string",
          "enum": [
            "overview",
            "notes",
            "search",
            "concepts",
            "skos_collections",
            "chunking",
            "versioning",
            "collections",
            "archives",
            "templates",
            "document_types",
            "backup",
            "encryption",
            "jobs",
            "observability",
            "provenance",
            "embedding_configs",
            "vision",
            "audio",
            "video",
            "3d-models",
            "workflows",
            "troubleshooting",
            "contributing",
            "all"
          ],
          "description": "Documentation topic to retrieve",
          "default": "overview"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "pke_generate_keypair",
    description: `Generate a new PKE keypair for note encryption. See \`get_documentation(topic='encryption')\` for workflow.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "passphrase": {
          "type": "string",
          "description": "Passphrase to protect the private key (minimum 12 characters)"
        },
        "output_dir": {
          "type": "string",
          "description": "Directory to save keys (default: current directory)"
        },
        "label": {
          "type": "string",
          "description": "Optional label for the key (e.g., 'Work Key')"
        }
      },
      "required": [
        "passphrase"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "pke_get_address",
    description: `Derive a PKE address from a public key.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "public_key": {
          "type": "string",
          "description": "Base64-encoded public key bytes (preferred — no filesystem access needed)"
        },
        "public_key_path": {
          "type": "string",
          "description": "Path to the public key file (fallback for local CLI workflows)"
        }
      },
      "required": []
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "pke_encrypt",
    description: `Encrypt content for specific recipients using their public keys.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "plaintext": {
          "type": "string",
          "description": "Base64-encoded plaintext to encrypt"
        },
        "recipient_keys": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Base64-encoded recipient public keys"
        },
        "original_filename": {
          "type": "string",
          "description": "Original filename to embed in encrypted header (optional)"
        }
      },
      "required": [
        "plaintext",
        "recipient_keys"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "pke_decrypt",
    description: `Decrypt content using an encrypted private key and passphrase.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "ciphertext": {
          "type": "string",
          "description": "Base64-encoded ciphertext to decrypt"
        },
        "encrypted_private_key": {
          "type": "string",
          "description": "Base64-encoded encrypted private key"
        },
        "passphrase": {
          "type": "string",
          "description": "Passphrase for the private key"
        }
      },
      "required": [
        "ciphertext",
        "encrypted_private_key",
        "passphrase"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "pke_list_recipients",
    description: `List the recipient addresses that can decrypt a ciphertext.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "ciphertext": {
          "type": "string",
          "description": "Base64-encoded ciphertext"
        }
      },
      "required": [
        "ciphertext"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "pke_verify_address",
    description: `Verify a PKE address format is valid.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "address": {
          "type": "string",
          "description": "The address to verify (mm:...)"
        }
      },
      "required": [
        "address"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "pke_list_keysets",
    description: `List all named keysets stored locally.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "pke_create_keyset",
    description: `Create a new named keyset with passphrase protection.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Keyset name (alphanumeric, hyphens, underscores only)"
        },
        "passphrase": {
          "type": "string",
          "description": "Strong passphrase to protect the private key (minimum 12 characters)"
        }
      },
      "required": [
        "name",
        "passphrase"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "pke_get_active_keyset",
    description: `Get the currently active keyset name and address.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "pke_set_active_keyset",
    description: `Set the active keyset for encryption operations.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Name of the keyset to activate"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "pke_export_keyset",
    description: `Export a keyset to a filesystem directory.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Name of the keyset to export"
        },
        "output_dir": {
          "type": "string",
          "description": "Directory to export to (default: ~/.matric/exports/)"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "pke_import_keyset",
    description: `Import a keyset from a filesystem directory.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Name for the imported keyset (must be unique)"
        },
        "import_path": {
          "type": "string",
          "description": "Path to exported keyset directory (contains public.key, private.key.enc)"
        },
        "public_key_path": {
          "type": "string",
          "description": "Path to public key file (use with private_key_path)"
        },
        "private_key_path": {
          "type": "string",
          "description": "Path to encrypted private key file (use with public_key_path)"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "pke_delete_keyset",
    description: `Delete a named keyset from local storage.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Name of the keyset to delete"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "list_document_types",
    description: `List all document types (131+ built-in). See \`get_documentation(topic='document_types')\` for registry details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "detail": {
          "type": "boolean",
          "description": "Return full document type objects (true) or just names (false, default). Default false returns ~500 tokens, true returns ~14k tokens.",
          "default": false
        },
        "category": {
          "type": "string",
          "description": "Filter by category: code, prose, config, markup, data, api-spec, iac, database, shell, docs, package, observability, legal, communication, research, creative, media, personal, custom"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_document_type",
    description: `Get document type details including chunking strategy and patterns.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Document type name (e.g., 'rust', 'markdown', 'openapi')"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_document_type",
    description: `Create a custom document type with chunking configuration.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Unique type identifier (lowercase, alphanumeric with hyphens)"
        },
        "display_name": {
          "type": "string",
          "description": "Human-readable name"
        },
        "category": {
          "type": "string",
          "description": "Category for organization (code, prose, config, etc.)"
        },
        "description": {
          "type": "string",
          "description": "Description of the type and its use cases"
        },
        "file_extensions": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "File extensions for detection (e.g., ['.rs', '.rust'])"
        },
        "filename_patterns": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Filename patterns for detection (e.g., ['Cargo.toml'])"
        },
        "content_patterns": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Regex patterns for content-based detection"
        },
        "chunking_strategy": {
          "type": "string",
          "enum": [
            "semantic",
            "syntactic",
            "fixed",
            "hybrid",
            "per_section",
            "per_unit",
            "whole"
          ],
          "description": "How to split documents of this type into chunks"
        }
      },
      "required": [
        "name",
        "display_name",
        "category"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "update_document_type",
    description: `Update a custom document type's configuration.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Type name to update (must be custom, not system type)"
        },
        "display_name": {
          "type": "string",
          "description": "Human-readable name"
        },
        "description": {
          "type": "string",
          "description": "Description of the type"
        },
        "file_extensions": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "File extensions for detection"
        },
        "filename_patterns": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Filename patterns for detection"
        },
        "content_patterns": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Regex patterns for content-based detection"
        },
        "chunking_strategy": {
          "type": "string",
          "enum": [
            "semantic",
            "syntactic",
            "fixed",
            "hybrid",
            "per_section",
            "per_unit",
            "whole"
          ],
          "description": "How to split documents into chunks"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_document_type",
    description: `Delete a custom document type (built-in types cannot be deleted).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Type name to delete (must be custom, not system type)"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "detect_document_type",
    description: `Auto-detect document type from filename, content, or MIME type.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filename": {
          "type": "string",
          "description": "Filename to detect from (e.g., 'main.rs', 'docker-compose.yml')"
        },
        "content": {
          "type": "string",
          "description": "Content snippet for magic pattern detection (first 1000 chars recommended)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "process_video",
    description: `Get video processing workflow guidance. See \`get_documentation(topic='video')\` for pipeline details.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "anyOf": [
            {
              "type": "string"
            },
            {
              "type": "null"
            }
          ],
          "description": "Optional existing note UUID. If provided, the guidance will reference this note. If omitted, the guidance includes a note creation step."
        },
        "filename": {
          "type": "string",
          "description": "Optional filename for the video (used in guidance output)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "process_3d_model",
    description: `Get 3D model processing workflow guidance. See \`get_documentation(topic='3d-models')\` for requirements.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "anyOf": [
            {
              "type": "string"
            },
            {
              "type": "null"
            }
          ],
          "description": "Optional existing note UUID. If provided, the guidance will reference this note. If omitted, the guidance includes a note creation step."
        },
        "filename": {
          "type": "string",
          "description": "Optional filename for the 3D model (used in guidance output)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_archives",
    description: `List all memory archives. See \`get_documentation(topic='archives')\` for multi-memory architecture.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_archive",
    description: `Create a new memory archive with automatic schema cloning.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Archive name (alphanumeric with hyphens/underscores)"
        },
        "description": {
          "type": "string",
          "description": "Optional archive description"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_archive",
    description: `Get archive details including status and note count.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Archive name"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "update_archive",
    description: `Update archive metadata (name, description).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Archive name"
        },
        "description": {
          "anyOf": [
            {
              "type": "string"
            },
            {
              "type": "null"
            }
          ],
          "description": "New description (null to clear)"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_archive",
    description: `Delete a memory archive and all its data. This cannot be undone.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Archive name to delete"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "set_default_archive",
    description: `Set the default archive for new connections.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Archive name to set as default"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_archive_stats",
    description: `Get detailed statistics for a memory archive.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Archive name"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_skos_collections",
    description: `List SKOS collections within a scheme. See \`get_documentation(topic='skos_collections')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "scheme_id": {
          "type": "string",
          "format": "uuid",
          "description": "Filter by concept scheme"
        },
        "limit": {
          "type": "number",
          "default": 50,
          "description": "Maximum collections to return (default: 50)"
        },
        "offset": {
          "type": "number",
          "default": 0,
          "description": "Pagination offset (default: 0)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_skos_collection",
    description: `Create a SKOS collection to group concepts. See \`get_documentation(topic='skos_collections')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "scheme_id": {
          "type": "string",
          "format": "uuid",
          "description": "Parent concept scheme UUID"
        },
        "pref_label": {
          "type": "string",
          "description": "Collection name"
        },
        "notation": {
          "type": "string",
          "description": "Short code (optional)"
        },
        "definition": {
          "type": "string",
          "description": "What this collection groups"
        },
        "ordered": {
          "type": "boolean",
          "default": false,
          "description": "Whether members have explicit order"
        }
      },
      "required": [
        "scheme_id",
        "pref_label"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_skos_collection",
    description: `Get SKOS collection details and its members.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "update_skos_collection",
    description: `Update SKOS collection metadata.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID"
        },
        "pref_label": {
          "type": "string",
          "description": "New collection name"
        },
        "notation": {
          "type": "string",
          "description": "New short code"
        },
        "definition": {
          "type": "string",
          "description": "New definition"
        },
        "ordered": {
          "type": "boolean",
          "description": "Change ordered status"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_skos_collection",
    description: `Delete a SKOS collection (concepts are preserved).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID to delete"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "add_skos_collection_member",
    description: `Add a concept to a SKOS collection.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID"
        },
        "concept_id": {
          "type": "string",
          "format": "uuid",
          "description": "Concept UUID to add"
        },
        "position": {
          "type": "number",
          "description": "Position in ordered collection (0-indexed)"
        }
      },
      "required": [
        "id",
        "concept_id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "remove_skos_collection_member",
    description: `Remove a concept from a SKOS collection.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Collection UUID"
        },
        "concept_id": {
          "type": "string",
          "format": "uuid",
          "description": "Concept UUID to remove"
        }
      },
      "required": [
        "id",
        "concept_id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "remove_broader",
    description: `Remove a broader (parent) relationship between concepts.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Child concept UUID"
        },
        "target_id": {
          "type": "string",
          "format": "uuid",
          "description": "Parent concept UUID to unlink"
        }
      },
      "required": [
        "id",
        "target_id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "remove_narrower",
    description: `Remove a narrower (child) relationship between concepts.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Parent concept UUID"
        },
        "target_id": {
          "type": "string",
          "format": "uuid",
          "description": "Child concept UUID to unlink"
        }
      },
      "required": [
        "id",
        "target_id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "remove_related",
    description: `Remove a non-hierarchical related relationship between concepts.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "First concept UUID"
        },
        "target_id": {
          "type": "string",
          "format": "uuid",
          "description": "Related concept UUID to unlink"
        }
      },
      "required": [
        "id",
        "target_id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "get_knowledge_health",
    description: `Get overall knowledge health metrics. See \`get_documentation(topic='observability')\` for dashboard details.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_orphan_tags",
    description: `Find tags not associated with any notes.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_stale_notes",
    description: `Find notes not updated within a threshold period.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "days": {
          "type": "number",
          "default": 90,
          "description": "Days since last update to consider stale"
        },
        "limit": {
          "type": "number",
          "default": 50,
          "description": "Maximum results"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_unlinked_notes",
    description: `Find notes with no semantic links to other notes.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "limit": {
          "type": "number",
          "default": 50,
          "description": "Maximum results"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_tag_cooccurrence",
    description: `Get tag co-occurrence matrix showing which tags appear together.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "min_count": {
          "type": "number",
          "default": 2,
          "description": "Minimum co-occurrence count"
        },
        "limit": {
          "type": "number",
          "default": 50,
          "description": "Maximum results"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_note_backlinks",
    description: `Get notes that link to this note (reverse semantic links).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_note_provenance",
    description: `Get W3C PROV provenance chain for a note. See \`get_documentation(topic='provenance')\` for chain structure.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_memory_provenance",
    description: `Get file provenance chain for a note's attachments including location, device, and capture time.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "The note ID"
        }
      },
      "required": [
        "note_id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_job",
    description: `Get details of a specific background job including status and result.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Job UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_pending_jobs_count",
    description: `Get count of pending jobs. Useful to check if new content has finished processing.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "reprocess_note",
    description: `Re-run the full AI processing pipeline on a note (revision, embedding, linking).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID"
        },
        "steps": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": [
              "ai_revision",
              "embedding",
              "linking",
              "title_generation",
              "all"
            ]
          },
          "description": "Pipeline steps to run (omit for all)"
        },
        "force": {
          "type": "boolean",
          "default": false,
          "description": "Force reprocessing even if already processed"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "bulk_reprocess_notes",
    description: `Bulk reprocess multiple notes through the AI pipeline — the "Make Smarter" operation.

Queues AI revision, embedding, and linking jobs for many notes at once. Use revision_mode "full" for deep contextual enhancement that cross-references related notes, or omit for light formatting cleanup.

When called without note_ids, processes all notes in the current archive (up to the limit).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "revision_mode": {
          "type": "string",
          "enum": [
            "full",
            "light",
            "none"
          ],
          "default": "light",
          "description": "AI revision mode: 'full' for deep enhancement, 'light' for formatting only (default), 'none' to skip revision"
        },
        "note_ids": {
          "type": "array",
          "items": {
            "type": "string",
            "format": "uuid"
          },
          "description": "Specific note UUIDs to process. Omit to process all notes."
        },
        "steps": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": [
              "ai_revision",
              "embedding",
              "linking",
              "title_generation",
              "concept_tagging",
              "all"
            ]
          },
          "description": "Pipeline steps to run (omit for all)"
        },
        "limit": {
          "type": "integer",
          "default": 500,
          "description": "Maximum notes to process (safety limit, max 5000)"
        },
        "model": {
          "type": "string",
          "description": "Language model slug for AI operations. Supports provider-qualified slugs (e.g. 'qwen3:8b', 'openai:gpt-4o', 'openrouter:anthropic/claude-sonnet-4-20250514'). If omitted, uses the globally configured default. Bare slugs route to the default provider (Ollama). Use get_available_models to discover available slugs and providers."
        }
      }
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_notes_timeline",
    description: `Get note creation/update timeline for a date range.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "granularity": {
          "type": "string",
          "enum": [
            "hour",
            "day",
            "week",
            "month"
          ],
          "default": "day",
          "description": "Time bucket size: hour, day, week, or month (default: day)"
        },
        "start_date": {
          "type": "string",
          "description": "Start date (ISO 8601)"
        },
        "end_date": {
          "type": "string",
          "description": "End date (ISO 8601)"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_notes_activity",
    description: `Get recent note activity feed.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "limit": {
          "type": "number",
          "default": 50,
          "description": "Maximum events to return (default: 50)"
        },
        "offset": {
          "type": "number",
          "default": 0,
          "description": "Pagination offset (default: 0)"
        },
        "event_types": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": [
              "created",
              "updated",
              "deleted",
              "restored",
              "tagged",
              "linked"
            ]
          },
          "description": "Filter by event types"
        }
      }
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_embedding_configs",
    description: `List embedding model configurations. See \`get_documentation(topic='embedding_configs')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_default_embedding_config",
    description: `Get the default embedding model configuration.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_embedding_config",
    description: `Get a specific embedding configuration by ID.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Config UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_embedding_config",
    description: `Create a new embedding model configuration. See \`get_documentation(topic='embedding_configs')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Display name"
        },
        "model": {
          "type": "string",
          "description": "Model identifier"
        },
        "dimension": {
          "type": "number",
          "description": "Vector dimension (e.g., 768, 384, 1536)"
        },
        "provider": {
          "type": "string",
          "description": "Provider (ollama, openai, etc.)"
        },
        "is_default": {
          "type": "boolean",
          "default": false,
          "description": "Set as default config"
        },
        "chunk_size": {
          "type": "integer",
          "description": "Maximum characters per chunk for text splitting (default: 1000)"
        },
        "chunk_overlap": {
          "type": "integer",
          "description": "Overlap characters between chunks for context preservation (default: 100)"
        }
      },
      "required": [
        "name",
        "model",
        "dimension"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "update_embedding_config",
    description: `Update an embedding configuration's parameters.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Config UUID"
        },
        "name": {
          "type": "string",
          "description": "New display name"
        },
        "model": {
          "type": "string",
          "description": "New model identifier"
        },
        "dimension": {
          "type": "number",
          "description": "New vector dimension"
        },
        "provider": {
          "type": "string",
          "description": "New provider"
        },
        "is_default": {
          "type": "boolean",
          "description": "Set as default"
        },
        "chunk_size": {
          "type": "integer",
          "description": "Maximum characters per chunk for text splitting"
        },
        "chunk_overlap": {
          "type": "integer",
          "description": "Overlap characters between chunks for context preservation"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_embedding_config",
    description: `Delete an embedding configuration.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Config UUID to delete"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "upload_attachment",
    description: `Upload a file attachment to a note. Returns curl command for multipart upload.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID to attach the file to"
        },
        "filename": {
          "type": "string",
          "description": "Filename hint for the curl command (e.g., 'photo.jpg')"
        },
        "content_type": {
          "type": "string",
          "description": "MIME type hint (e.g., 'image/jpeg'). If omitted, auto-detected from file extension."
        },
        "document_type_id": {
          "type": "string",
          "format": "uuid",
          "description": "Optional: explicit document type UUID override (skips auto-classification)"
        }
      },
      "required": [
        "note_id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_attachments",
    description: `List all attachments for a note.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID"
        }
      },
      "required": [
        "note_id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_attachment",
    description: `Get attachment metadata (filename, size, content type).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Attachment UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "download_attachment",
    description: `Download an attachment file. Returns curl command.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Attachment UUID"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "delete_attachment",
    description: `Delete a file attachment from a note.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Attachment UUID to delete"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "export_skos_turtle",
    description: `Export a concept scheme as RDF/Turtle format. See \`get_documentation(topic='concepts')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "scheme_id": {
          "type": "string",
          "format": "uuid",
          "description": "Concept scheme UUID to export. Omit to export ALL schemes."
        }
      },
      "required": []
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "select_memory",
    description: `Switch active memory context for this session. See \`get_documentation(topic='archives')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Memory name to select (e.g., 'personal', 'work', 'public')"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false,"idempotentHint":true},
  },
  {
    name: "get_active_memory",
    description: `Get the currently active memory name for this session.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_memories",
    description: `Alias for list_archives. See \`get_documentation(topic='archives')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_memory",
    description: `Alias for create_archive.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Memory name (alphanumeric, hyphens, underscores)"
        },
        "description": {
          "type": "string",
          "description": "Purpose or description (optional)"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "delete_memory",
    description: `Alias for delete_archive.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Memory name to delete"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "clone_memory",
    description: `Clone an existing archive to create a copy.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "source_name": {
          "type": "string",
          "description": "Name of the memory to clone from"
        },
        "new_name": {
          "type": "string",
          "description": "Name for the cloned memory"
        },
        "description": {
          "type": "string",
          "description": "Optional description for the clone"
        }
      },
      "required": [
        "source_name",
        "new_name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_memories_overview",
    description: `Get overview of all memories with note counts and sizes.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "search_memories_federated",
    description: `Search across multiple memory archives simultaneously. See \`get_documentation(topic='archives')\` for federated search.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "q": {
          "type": "string",
          "description": "Search query string"
        },
        "memories": {
          "type": "array",
          "items": {
            "type": "string"
          },
          "description": "Memory names to search (use [\"all\"] for all memories)"
        },
        "limit": {
          "type": "integer",
          "description": "Maximum results per memory (default 10)"
        }
      },
      "required": [
        "q",
        "memories"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "list_api_keys",
    description: `List all API keys (metadata only, values not returned).`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "create_api_key",
    description: `Create a new API key. The key value is returned ONCE — store it securely.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Human-readable name for the API key"
        },
        "description": {
          "type": "string",
          "description": "Optional description of the key's purpose"
        },
        "scope": {
          "type": "string",
          "description": "Access scope (default: 'admin')"
        },
        "expires_in_days": {
          "type": "integer",
          "description": "Days until expiration (omit for no expiration)"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "get_rate_limit_status",
    description: `Check current rate limit status and remaining quota.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "get_extraction_stats",
    description: `Get extraction pipeline statistics and adapter status.`,
    inputSchema: {
      "type": "object",
      "properties": {}
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "export_collection",
    description: `Export all notes in a collection as concatenated markdown. See \`get_documentation(topic='collections')\` for options.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "UUID of the collection to export"
        },
        "include_frontmatter": {
          "type": "boolean",
          "description": "Include YAML frontmatter metadata (default: true)"
        },
        "content": {
          "type": "string",
          "enum": [
            "revised",
            "original"
          ],
          "description": "Content version: 'revised' (default) or 'original'"
        }
      },
      "required": [
        "id"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  {
    name: "swap_backup",
    description: `Swap current database state with a backup. See \`get_documentation(topic='backup')\` for strategies.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "filename": {
          "type": "string",
          "description": "Shard filename (e.g., 'shard_20260210.tar.gz')"
        },
        "dry_run": {
          "type": "boolean",
          "description": "Preview without restoring (default: false)"
        },
        "strategy": {
          "type": "string",
          "enum": [
            "wipe",
            "merge"
          ],
          "description": "Restore strategy: 'wipe' (default) or 'merge'"
        }
      },
      "required": [
        "filename"
      ]
    },
    annotations: {"destructiveHint":true},
  },
  {
    name: "memory_backup_download",
    description: `Download a memory-specific backup. Returns curl command.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "name": {
          "type": "string",
          "description": "Memory archive name to back up"
        }
      },
      "required": [
        "name"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
  // ==========================================================================
  // CONSOLIDATED CORE TOOLS — Archives, Encryption, Backups (Issue #441)
  // ==========================================================================
  {
    name: "manage_archives",
    description: `Manage parallel memory archives (schema-level data isolation). See \`get_documentation(topic='archives')\` for multi-memory architecture.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "list",
            "create",
            "get",
            "update",
            "delete",
            "set_default",
            "stats",
            "clone"
          ],
          "description": "Action: 'list' (all archives), 'create' (new archive), 'get' (details), 'update' (metadata), 'delete' (permanent!), 'set_default', 'stats', 'clone' (deep copy)"
        },
        "name": {
          "type": "string",
          "description": "Archive name (required for create/get/update/delete/set_default/stats/clone)"
        },
        "description": {
          "anyOf": [
            { "type": "string" },
            { "type": "null" }
          ],
          "description": "Archive description (for create/update; null to clear)"
        },
        "new_name": {
          "type": "string",
          "description": "Name for cloned archive (required for 'clone')"
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "manage_encryption",
    description: `PKE (Public Key Encryption) operations — keypair generation, encrypt/decrypt, keyset management. See \`get_documentation(topic='encryption')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "generate_keypair",
            "get_address",
            "encrypt",
            "decrypt",
            "list_recipients",
            "verify_address",
            "list_keysets",
            "create_keyset",
            "get_active_keyset",
            "set_active_keyset",
            "export_keyset",
            "import_keyset",
            "delete_keyset"
          ],
          "description": "Action: crypto ops (generate_keypair, get_address, encrypt, decrypt, list_recipients, verify_address) or keyset management (list_keysets, create_keyset, get_active_keyset, set_active_keyset, export_keyset, import_keyset, delete_keyset)"
        },
        "passphrase": {
          "type": "string",
          "description": "Passphrase for private key (required for generate_keypair, create_keyset, decrypt)"
        },
        "label": {
          "type": "string",
          "description": "Optional label for keypair (for generate_keypair)"
        },
        "output_dir": {
          "type": "string",
          "description": "Directory for key file output (generate_keypair) or keyset export (export_keyset)"
        },
        "public_key": {
          "type": "string",
          "description": "Base64-encoded public key (for get_address, encrypt)"
        },
        "public_key_path": {
          "type": "string",
          "description": "Path to public key file (fallback for get_address)"
        },
        "plaintext": {
          "type": "string",
          "description": "Base64-encoded plaintext to encrypt (for encrypt)"
        },
        "recipient_keys": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Base64-encoded recipient public keys (for encrypt)"
        },
        "original_filename": {
          "type": "string",
          "description": "Filename to embed in encrypted header (for encrypt)"
        },
        "ciphertext": {
          "type": "string",
          "description": "Base64-encoded ciphertext (for decrypt, list_recipients)"
        },
        "encrypted_private_key": {
          "type": "string",
          "description": "Base64-encoded encrypted private key (for decrypt)"
        },
        "address": {
          "type": "string",
          "description": "PKE address to verify (for verify_address, format: mm:...)"
        },
        "name": {
          "type": "string",
          "description": "Keyset name (for create_keyset, set_active_keyset, export_keyset, import_keyset, delete_keyset)"
        },
        "import_path": {
          "type": "string",
          "description": "Path to exported keyset directory (for import_keyset)"
        },
        "private_key_path": {
          "type": "string",
          "description": "Path to encrypted private key file (for import_keyset with public_key_path)"
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "manage_backups",
    description: `Backup, restore, shard export/import, and memory swap operations. See \`get_documentation(topic='backup')\`.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "export_shard",
            "import_shard",
            "snapshot",
            "restore",
            "list",
            "get_info",
            "get_metadata",
            "update_metadata",
            "download_archive",
            "upload_archive",
            "swap",
            "download_memory"
          ],
          "description": "Action: shards (export_shard, import_shard), database (snapshot, restore), backups (list, get_info, get_metadata, update_metadata), archives (download_archive, upload_archive), swap, download_memory"
        },
        "filename": {
          "type": "string",
          "description": "Backup/shard filename (for restore, get_info, get_metadata, update_metadata, download_archive, swap)"
        },
        "file_path": {
          "type": "string",
          "description": "Path to file on disk (for import_shard, upload_archive)"
        },
        "output_dir": {
          "type": "string",
          "description": "Directory prefix for download output (for export_shard, download_archive, download_memory)"
        },
        "include": {
          "type": "string",
          "description": "Components to include: comma-separated (notes,collections,tags,templates,links,embedding_sets,embeddings) or 'all' (for export_shard, import_shard)"
        },
        "dry_run": {
          "type": "boolean",
          "description": "Preview without writing data (for import_shard, swap)"
        },
        "on_conflict": {
          "type": "string",
          "enum": ["skip", "replace", "merge"],
          "description": "Conflict resolution: skip, replace, merge (for import_shard)"
        },
        "skip_embedding_regen": {
          "type": "boolean",
          "description": "Skip embedding regeneration on import (for import_shard)"
        },
        "name": {
          "type": "string",
          "description": "Snapshot name suffix or memory archive name (for snapshot, download_memory)"
        },
        "title": {
          "type": "string",
          "description": "Human-readable title (for snapshot, update_metadata)"
        },
        "description": {
          "type": "string",
          "description": "Description (for snapshot, update_metadata)"
        },
        "skip_snapshot": {
          "type": "boolean",
          "description": "DANGEROUS: Skip pre-restore backup (for restore)"
        },
        "strategy": {
          "type": "string",
          "enum": ["wipe", "merge"],
          "description": "Restore strategy (for swap)"
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"destructiveHint":false},
  },
  {
    name: "manage_jobs",
    description: `Monitor and manage background processing jobs. Actions: list (filter by status/type/note), get (single job details), create (queue a processing step), stats (queue statistics), pending_count (quick pending check), extraction_stats (extraction pipeline analytics). See \`get_documentation(topic='jobs')\` for job types.`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "list",
            "get",
            "create",
            "stats",
            "pending_count",
            "extraction_stats"
          ],
          "description": "Action: 'list' (jobs with filters), 'get' (single job), 'create' (queue job), 'stats' (queue statistics), 'pending_count' (pending count), 'extraction_stats' (extraction analytics)"
        },
        "id": {
          "type": "string",
          "format": "uuid",
          "description": "Job UUID (required for get)"
        },
        "note_id": {
          "type": "string",
          "format": "uuid",
          "description": "Note UUID (required for create, optional filter for list)"
        },
        "job_type": {
          "type": "string",
          "description": "Job type: ai_revision, embedding, linking, context_update, title_generation, concept_tagging, re_embed_all, extraction, exif_extraction (required for create, optional filter for list)"
        },
        "status": {
          "type": "string",
          "enum": ["pending", "running", "completed", "failed"],
          "description": "Filter by job status (for list)"
        },
        "priority": {
          "type": "integer",
          "description": "Job priority — higher = sooner (for create)"
        },
        "payload": {
          "type": "object",
          "description": "Optional JSON payload for the job (for create). Required for extraction jobs: { strategy, attachment_id, filename, mime_type }"
        },
        "deduplicate": {
          "type": "boolean",
          "description": "Skip if duplicate pending/running job exists (for create, default: true)"
        },
        "model": {
          "type": "string",
          "description": "Provider-qualified model slug, e.g. 'openai:gpt-4o' (for create)"
        },
        "limit": {
          "type": "integer",
          "description": "Max results (for list, default: 50)"
        },
        "offset": {
          "type": "integer",
          "description": "Pagination offset (for list)"
        }
      },
      "required": [
        "action"
      ]
    },
  },
  {
    name: "manage_inference",
    description: `Discover available LLM models, providers, and embedding configurations. Actions: list_models (all models with capabilities and provider health), get_embedding_config (current default embedding config), list_embedding_configs (all embedding configurations).`,
    inputSchema: {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": [
            "list_models",
            "get_embedding_config",
            "list_embedding_configs"
          ],
          "description": "Action: 'list_models' (all models from all providers), 'get_embedding_config' (default embedding config), 'list_embedding_configs' (all embedding configs)"
        }
      },
      "required": [
        "action"
      ]
    },
    annotations: {"readOnlyHint":true},
  },
];
