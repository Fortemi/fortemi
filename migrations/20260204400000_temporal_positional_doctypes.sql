-- ============================================================================
-- Temporal & Positional Document Types
-- Issue: #431 - Subject-matter temporal search
-- ============================================================================
--
-- Adds document types for content with inherent temporal or spatial properties:
-- - Events, meetings, deadlines with start/end times
-- - Location notes, travel logs with GPS coordinates
-- - Combined temporal-spatial (itineraries, conference sessions)
-- ============================================================================

-- ============================================================================
-- PART 1: Add temporal_metadata and positional_metadata columns to document_type
-- ============================================================================

ALTER TABLE document_type ADD COLUMN IF NOT EXISTS temporal_metadata JSONB DEFAULT '{}';
ALTER TABLE document_type ADD COLUMN IF NOT EXISTS positional_metadata JSONB DEFAULT '{}';

COMMENT ON COLUMN document_type.temporal_metadata IS
'Schema for temporal fields: {"primary_field": "starts_at", "fields": [{"name": "starts_at", "type": "datetime", "required": true}]}';
COMMENT ON COLUMN document_type.positional_metadata IS
'Schema for positional fields: {"fields": [{"name": "latitude", "type": "float"}, {"name": "longitude", "type": "float"}]}';

-- ============================================================================
-- PART 2: Calendar & Events (4 types)
-- ============================================================================

INSERT INTO document_type (
    name, display_name, category, description,
    chunking_strategy, chunk_size_default, chunk_overlap_default, preserve_boundaries,
    temporal_metadata, is_system, is_active
)
VALUES
-- Event
(
    'event',
    'Event',
    'personal',
    'Calendar event with start/end time, location, and attendees',
    'whole',
    4096,
    0,
    true,
    '{
        "primary_field": "starts_at",
        "supports_duration": true,
        "supports_recurrence": true,
        "fields": [
            {"name": "starts_at", "type": "datetime", "required": true, "indexed": true},
            {"name": "ends_at", "type": "datetime", "required": false},
            {"name": "all_day", "type": "boolean", "required": false},
            {"name": "recurrence", "type": "rrule", "required": false},
            {"name": "location", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Meeting
(
    'meeting',
    'Meeting Record',
    'communication',
    'Scheduled meeting with attendees, agenda, and outcomes',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "starts_at",
        "supports_duration": true,
        "fields": [
            {"name": "starts_at", "type": "datetime", "required": true, "indexed": true},
            {"name": "ends_at", "type": "datetime", "required": false},
            {"name": "attendees", "type": "string[]", "required": false},
            {"name": "location", "type": "string", "required": false},
            {"name": "meeting_type", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Deadline
(
    'deadline',
    'Deadline',
    'personal',
    'Task or project deadline with due date and reminders',
    'whole',
    2048,
    0,
    true,
    '{
        "primary_field": "due_at",
        "fields": [
            {"name": "due_at", "type": "datetime", "required": true, "indexed": true},
            {"name": "reminder_at", "type": "datetime[]", "required": false},
            {"name": "priority", "type": "string", "required": false},
            {"name": "status", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Milestone
(
    'milestone',
    'Project Milestone',
    'research',
    'Project milestone with target and completion dates',
    'whole',
    2048,
    0,
    true,
    '{
        "primary_field": "target_date",
        "fields": [
            {"name": "target_date", "type": "date", "required": true, "indexed": true},
            {"name": "completed_at", "type": "datetime", "required": false},
            {"name": "project_id", "type": "string", "required": false},
            {"name": "status", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
)
ON CONFLICT (name) DO UPDATE SET
    temporal_metadata = EXCLUDED.temporal_metadata,
    description = EXCLUDED.description,
    updated_at = NOW();

-- ============================================================================
-- PART 3: Time-Bound Records (6 types)
-- ============================================================================

INSERT INTO document_type (
    name, display_name, category, description,
    chunking_strategy, chunk_size_default, chunk_overlap_default, preserve_boundaries,
    temporal_metadata, is_system, is_active
)
VALUES
-- Sprint Record
(
    'sprint-record',
    'Sprint Record',
    'research',
    'Agile sprint with start/end dates and iteration number',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "start_date",
        "supports_duration": true,
        "fields": [
            {"name": "start_date", "type": "date", "required": true, "indexed": true},
            {"name": "end_date", "type": "date", "required": true},
            {"name": "iteration_number", "type": "integer", "required": false},
            {"name": "team", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Weekly Review
(
    'weekly-review',
    'Weekly Review',
    'personal',
    'Weekly reflection and planning document',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "week_start",
        "supports_duration": true,
        "fields": [
            {"name": "week_start", "type": "date", "required": true, "indexed": true},
            {"name": "week_end", "type": "date", "required": false},
            {"name": "year", "type": "integer", "required": false},
            {"name": "week_number", "type": "integer", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Incident Report
(
    'incident-report',
    'Incident Report',
    'observability',
    'Production incident with timeline and severity',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "occurred_at",
        "fields": [
            {"name": "occurred_at", "type": "datetime", "required": true, "indexed": true},
            {"name": "detected_at", "type": "datetime", "required": false},
            {"name": "resolved_at", "type": "datetime", "required": false},
            {"name": "severity", "type": "string", "required": false},
            {"name": "incident_id", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Changelog Entry
(
    'changelog-entry',
    'Changelog Entry',
    'docs',
    'Software release changelog with version and date',
    'whole',
    2048,
    0,
    true,
    '{
        "primary_field": "release_date",
        "fields": [
            {"name": "release_date", "type": "date", "required": true, "indexed": true},
            {"name": "version", "type": "string", "required": true},
            {"name": "breaking", "type": "boolean", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Status Update
(
    'status-update',
    'Status Update',
    'communication',
    'Periodic status report covering a time period',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "period_start",
        "supports_duration": true,
        "fields": [
            {"name": "period_start", "type": "date", "required": true, "indexed": true},
            {"name": "period_end", "type": "date", "required": false},
            {"name": "report_type", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Retrospective
(
    'retrospective',
    'Retrospective',
    'communication',
    'Team retrospective for a sprint or project phase',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "review_date",
        "fields": [
            {"name": "sprint_end", "type": "date", "required": false},
            {"name": "review_date", "type": "date", "required": true, "indexed": true},
            {"name": "team", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
)
ON CONFLICT (name) DO UPDATE SET
    temporal_metadata = EXCLUDED.temporal_metadata,
    description = EXCLUDED.description,
    updated_at = NOW();

-- ============================================================================
-- PART 4: Positionally Bound (4 types)
-- ============================================================================

INSERT INTO document_type (
    name, display_name, category, description,
    chunking_strategy, chunk_size_default, chunk_overlap_default, preserve_boundaries,
    positional_metadata, is_system, is_active
)
VALUES
-- Location Note
(
    'location-note',
    'Location Note',
    'personal',
    'Note tied to a specific geographic location',
    'semantic',
    512,
    50,
    true,
    '{
        "fields": [
            {"name": "latitude", "type": "float", "required": true},
            {"name": "longitude", "type": "float", "required": true},
            {"name": "place_name", "type": "string", "required": false},
            {"name": "address", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Travel Log
(
    'travel-log',
    'Travel Log',
    'personal',
    'Travel journey with origin, destination, and waypoints',
    'semantic',
    512,
    50,
    true,
    '{
        "fields": [
            {"name": "origin", "type": "location", "required": false},
            {"name": "destination", "type": "location", "required": false},
            {"name": "waypoints", "type": "location[]", "required": false},
            {"name": "distance_km", "type": "float", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Site Survey
(
    'site-survey',
    'Site Survey',
    'research',
    'Physical site assessment with location and measurements',
    'semantic',
    512,
    50,
    true,
    '{
        "fields": [
            {"name": "latitude", "type": "float", "required": true},
            {"name": "longitude", "type": "float", "required": true},
            {"name": "area_sqm", "type": "float", "required": false},
            {"name": "site_id", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Field Note
(
    'field-note',
    'Field Note',
    'research',
    'Research field observation at a specific location',
    'semantic',
    512,
    50,
    true,
    '{
        "fields": [
            {"name": "latitude", "type": "float", "required": true},
            {"name": "longitude", "type": "float", "required": true},
            {"name": "elevation_m", "type": "float", "required": false},
            {"name": "site_id", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
)
ON CONFLICT (name) DO UPDATE SET
    positional_metadata = EXCLUDED.positional_metadata,
    description = EXCLUDED.description,
    updated_at = NOW();

-- ============================================================================
-- PART 5: Combined Temporal + Positional (4 types)
-- ============================================================================

INSERT INTO document_type (
    name, display_name, category, description,
    chunking_strategy, chunk_size_default, chunk_overlap_default, preserve_boundaries,
    temporal_metadata, positional_metadata, is_system, is_active
)
VALUES
-- Itinerary
(
    'itinerary',
    'Travel Itinerary',
    'personal',
    'Multi-segment travel plan with times and locations',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "depart_at",
        "supports_duration": true,
        "fields": [
            {"name": "depart_at", "type": "datetime", "required": true, "indexed": true},
            {"name": "arrive_at", "type": "datetime", "required": false}
        ]
    }'::jsonb,
    '{
        "fields": [
            {"name": "origin", "type": "location", "required": false},
            {"name": "destination", "type": "location", "required": false},
            {"name": "segments", "type": "segment[]", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Conference Session
(
    'conference-session',
    'Conference Session',
    'communication',
    'Conference talk or session with time and venue',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "starts_at",
        "supports_duration": true,
        "fields": [
            {"name": "starts_at", "type": "datetime", "required": true, "indexed": true},
            {"name": "ends_at", "type": "datetime", "required": false},
            {"name": "track", "type": "string", "required": false}
        ]
    }'::jsonb,
    '{
        "fields": [
            {"name": "venue", "type": "string", "required": false},
            {"name": "room", "type": "string", "required": false},
            {"name": "latitude", "type": "float", "required": false},
            {"name": "longitude", "type": "float", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Trip Entry
(
    'trip-entry',
    'Trip Entry',
    'personal',
    'Single entry in a travel journal with time and location',
    'semantic',
    512,
    50,
    true,
    '{
        "primary_field": "timestamp",
        "fields": [
            {"name": "timestamp", "type": "datetime", "required": true, "indexed": true},
            {"name": "activity", "type": "string", "required": false}
        ]
    }'::jsonb,
    '{
        "fields": [
            {"name": "latitude", "type": "float", "required": true},
            {"name": "longitude", "type": "float", "required": true},
            {"name": "place_name", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
),
-- Availability Window
(
    'availability',
    'Availability Window',
    'personal',
    'Time window when available, with optional location',
    'whole',
    1024,
    0,
    true,
    '{
        "primary_field": "available_from",
        "supports_duration": true,
        "fields": [
            {"name": "available_from", "type": "datetime", "required": true, "indexed": true},
            {"name": "available_until", "type": "datetime", "required": true},
            {"name": "timezone", "type": "string", "required": false}
        ]
    }'::jsonb,
    '{
        "fields": [
            {"name": "location", "type": "string", "required": false}
        ]
    }'::jsonb,
    true,
    true
)
ON CONFLICT (name) DO UPDATE SET
    temporal_metadata = EXCLUDED.temporal_metadata,
    positional_metadata = EXCLUDED.positional_metadata,
    description = EXCLUDED.description,
    updated_at = NOW();

-- ============================================================================
-- PART 6: Indexes for temporal queries on note.metadata
-- ============================================================================

-- NOTE: Temporal fields are stored as ISO 8601 text (e.g., "2026-01-15T09:00:00Z").
-- ISO 8601 strings sort lexicographically in chronological order, so we can use
-- text-based BTREE indexes for range queries. This avoids IMMUTABLE function issues
-- with timezone-dependent casts.

-- Index for temporal.starts_at queries (text comparison for ISO 8601)
CREATE INDEX IF NOT EXISTS idx_note_temporal_starts ON note
    USING BTREE ((metadata->'temporal'->>'starts_at'))
    WHERE metadata->'temporal'->>'starts_at' IS NOT NULL;

-- Index for temporal.ends_at queries
CREATE INDEX IF NOT EXISTS idx_note_temporal_ends ON note
    USING BTREE ((metadata->'temporal'->>'ends_at'))
    WHERE metadata->'temporal'->>'ends_at' IS NOT NULL;

-- Index for temporal.due_at (deadlines)
CREATE INDEX IF NOT EXISTS idx_note_temporal_due ON note
    USING BTREE ((metadata->'temporal'->>'due_at'))
    WHERE metadata->'temporal'->>'due_at' IS NOT NULL;

-- GIN index for complex temporal/positional queries
CREATE INDEX IF NOT EXISTS idx_note_temporal_gin ON note
    USING GIN ((metadata->'temporal'))
    WHERE metadata->'temporal' IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_note_positional_gin ON note
    USING GIN ((metadata->'positional'))
    WHERE metadata->'positional' IS NOT NULL;

COMMENT ON INDEX idx_note_temporal_starts IS 'Index for filtering notes by subject-matter start time (ISO 8601 text sort)';
COMMENT ON INDEX idx_note_temporal_ends IS 'Index for filtering notes by subject-matter end time (ISO 8601 text sort)';
COMMENT ON INDEX idx_note_temporal_due IS 'Index for filtering notes by subject-matter due date (ISO 8601 text sort)';
COMMENT ON INDEX idx_note_temporal_gin IS 'GIN index for complex temporal metadata queries';
COMMENT ON INDEX idx_note_positional_gin IS 'GIN index for complex positional metadata queries';
