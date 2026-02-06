-- Seed: Temporal and positional document types (22 types)
-- Related migration: 20260204400000_temporal_positional_doctypes.sql

-- ============================================================================
-- PART 1: Calendar & Events (4 types)
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
-- PART 2: Time-Bound Records (6 types)
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
-- PART 3: Positionally Bound (4 types)
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
-- PART 4: Combined Temporal + Positional (4 types)
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
