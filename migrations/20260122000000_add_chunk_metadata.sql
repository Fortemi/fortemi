-- ============================================================================
-- Add chunk_metadata column to note table
-- ============================================================================
-- This migration adds a JSONB column to store chunking metadata for notes
-- that have been split into multiple chunks during processing.
--
-- Version: 0.1.0
-- Generated: 2026-01-22
-- Ticket: #107
-- ============================================================================

-- Add chunk_metadata JSONB column to note table
ALTER TABLE note
ADD COLUMN chunk_metadata JSONB DEFAULT NULL;

-- Create GIN index for efficient JSONB queries on chunk_metadata
CREATE INDEX idx_note_chunk_metadata ON note USING gin (chunk_metadata);

-- Create index for notes that have been chunked (where chunk_metadata is not null)
CREATE INDEX idx_note_chunked ON note (id) WHERE chunk_metadata IS NOT NULL;

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON COLUMN note.chunk_metadata IS 'Metadata about note chunking: total_chunks, chunking_strategy, chunk_sequence, etc.';

-- ============================================================================
-- Expected chunk_metadata structure (for documentation):
-- {
--   "total_chunks": 5,              // Total number of chunks this note was split into
--   "chunking_strategy": "semantic", // Strategy used: "semantic", "fixed", etc.
--   "chunk_sequence": [              // Ordered list of chunk note IDs
--     "uuid-1",
--     "uuid-2",
--     "uuid-3"
--   ],
--   "parent_note_id": "uuid",        // Original note ID if this is a chunk
--   "chunk_index": 2,                // Position in sequence (0-based) if this is a chunk
--   "overlap_tokens": 50             // Token overlap between chunks
-- }
-- ============================================================================
