export const CORE_TOOL_NAMES = Object.freeze([
  // Notes CRUD
  "list_notes", "get_note", "update_note", "delete_note", "restore_note",
  // Consolidated tools
  "capture_knowledge", "search", "record_provenance",
  "manage_tags", "manage_collection", "manage_concepts", "manage_embeddings",
  "manage_archives", "manage_encryption", "manage_backups",
  // Graph and links
  "explore_graph", "get_topology_stats", "get_graph_diagnostics",
  "capture_diagnostics_snapshot", "list_diagnostics_snapshots", "compare_diagnostics_snapshots",
  "recompute_snn_scores", "pfnet_sparsify", "coarse_community_detection",
  "trigger_graph_maintenance", "get_cold_spots", "get_note_links", "get_related_notes",
  // Export
  "export_note",
  // System and docs
  "get_documentation", "get_system_info", "health_check",
  // Multi-memory
  "select_memory", "get_active_memory",
  // Attachments
  "manage_attachments",
  // Observability
  "get_knowledge_health", "get_access_frequency",
  // Jobs and inference
  "manage_jobs", "manage_inference",
  // Bulk operations
  "bulk_reprocess_notes",
  // Permanent deletion of soft-deleted notes
  "purge_note", "purge_notes", "purge_all_notes",
]);

export const CORE_TOOLS = new Set(CORE_TOOL_NAMES);
