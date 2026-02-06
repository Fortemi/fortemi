// Centralized defaults — mirrors crates/matric-core/src/defaults.rs
// Keep in sync when Rust constants change.

// =============================================================================
// CHUNKING
// =============================================================================

export const CHUNK_SIZE = 1000;
export const CHUNK_MIN_SIZE = 100;
export const CHUNK_OVERLAP = 100;

// =============================================================================
// EMBEDDING
// =============================================================================

export const EMBED_MODEL = 'nomic-embed-text';
export const EMBED_DIMENSION = 768;

// =============================================================================
// PAGINATION
// =============================================================================

export const PAGE_LIMIT = 50;
export const PAGE_LIMIT_LARGE = 100;
export const PAGE_LIMIT_SEARCH = 20;
export const PAGE_LIMIT_AUTOCOMPLETE = 10;
export const INTERNAL_FETCH_LIMIT = 10000;
export const PAGE_OFFSET = 0;

// =============================================================================
// SNIPPET
// =============================================================================

export const SNIPPET_LENGTH = 200;

// =============================================================================
// SERVER
// =============================================================================

export const SERVER_PORT = 3000;
export const MCP_DEFAULT_PORT = 3001;

// =============================================================================
// INFERENCE
// =============================================================================

export const OLLAMA_URL = 'http://127.0.0.1:11434';
export const GEN_MODEL = 'gpt-oss:20b';

// =============================================================================
// JOB PROCESSING
// =============================================================================

export const JOB_MAX_RETRIES = 3;
export const AUTO_EMBED_BATCH_SIZE = 10;
export const AUTO_EMBED_PRIORITY = 5;

// =============================================================================
// BATCH
// =============================================================================

export const BATCH_IMPORT_MAX = 100;

// =============================================================================
// API PREFIX
// =============================================================================

export const API_V1_PREFIX = '/api/v1';
