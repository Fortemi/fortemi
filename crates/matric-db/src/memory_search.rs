//! Memory search repository for temporal-spatial provenance queries.
//!
//! Provides search capabilities for file memories based on:
//! - Location (spatial queries with PostGIS)
//! - Time (temporal range queries)
//! - Combined location + time filters
//! - Full provenance chain retrieval
//!
//! Uses the W3C PROV temporal-spatial extension schema.

use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres, Row, Transaction};
use uuid::Uuid;

use matric_core::{
    CreateFileProvenanceRequest, CreateNamedLocationRequest, CreateNoteProvenanceRequest,
    CreateProvDeviceRequest, CreateProvLocationRequest, Error, MemoryDevice, MemoryLocation,
    MemoryLocationResult, MemoryProvenance, MemoryTimeResult, ProvenanceRecord, Result,
};

/// PostgreSQL memory search repository.
pub struct PgMemorySearchRepository {
    pool: Pool<Postgres>,
}

impl PgMemorySearchRepository {
    /// Create a new memory search repository.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Search for memories near a geographic location.
    ///
    /// Returns attachments captured within `radius_meters` of the given lat/lon,
    /// ordered by distance (closest first).
    ///
    /// # Arguments
    ///
    /// * `lat` - Latitude in decimal degrees (-90 to 90)
    /// * `lon` - Longitude in decimal degrees (-180 to 180)
    /// * `radius_meters` - Search radius in meters
    ///
    /// # Returns
    ///
    /// Vector of `MemoryLocationResult` ordered by distance ascending.
    pub async fn search_by_location(
        &self,
        lat: f64,
        lon: f64,
        radius_meters: f64,
    ) -> Result<Vec<MemoryLocationResult>> {
        let rows = sqlx::query(
            r#"
            SELECT provenance_id, attachment_id, note_id, filename, content_type,
                   distance_m, capture_time_start, capture_time_end, location_name, event_type
            FROM (
                -- Provenance matches (file and note)
                SELECT
                    p.id as provenance_id,
                    p.attachment_id,
                    COALESCE(p.note_id, a.note_id) as note_id,
                    COALESCE(a.filename, pn.title) as filename,
                    ab.content_type,
                    ST_Distance(
                        pl.point,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    lower(p.capture_time) as capture_time_start,
                    upper(p.capture_time) as capture_time_end,
                    nl.name as location_name,
                    p.event_type
                FROM provenance p
                LEFT JOIN attachment a ON p.attachment_id = a.id
                LEFT JOIN attachment_blob ab ON a.blob_id = ab.id
                LEFT JOIN note pn ON p.note_id = pn.id
                JOIN prov_location pl ON p.location_id = pl.id
                LEFT JOIN named_location nl ON pl.named_location_id = nl.id
                WHERE ST_DWithin(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                    $3
                )

                UNION ALL

                -- Note metadata matches (latitude/longitude in JSONB)
                SELECT
                    NULL::uuid as provenance_id,
                    NULL::uuid as attachment_id,
                    n.id as note_id,
                    n.title as filename,
                    NULL::text as content_type,
                    ST_Distance(
                        ST_SetSRID(ST_MakePoint(
                            (n.metadata->>'longitude')::float8,
                            (n.metadata->>'latitude')::float8
                        ), 4326)::geography,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    no.user_created_at as capture_time_start,
                    no.user_created_at as capture_time_end,
                    n.metadata->>'location_name' as location_name,
                    NULL::text as event_type
                FROM note n
                LEFT JOIN note_original no ON n.id = no.note_id
                WHERE n.metadata->>'latitude' IS NOT NULL
                  AND n.metadata->>'longitude' IS NOT NULL
                  AND n.deleted_at IS NULL
                  AND NOT EXISTS (SELECT 1 FROM provenance WHERE note_id = n.id)
                  AND ST_DWithin(
                      ST_SetSRID(ST_MakePoint(
                          (n.metadata->>'longitude')::float8,
                          (n.metadata->>'latitude')::float8
                      ), 4326)::geography,
                      ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                      $3
                  )
            ) combined
            ORDER BY distance_m
            LIMIT 100
            "#,
        )
        .bind(lat)
        .bind(lon)
        .bind(radius_meters)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| MemoryLocationResult {
                provenance_id: row.get("provenance_id"),
                attachment_id: row.get("attachment_id"),
                note_id: row.get("note_id"),
                filename: row.get("filename"),
                content_type: row.get("content_type"),
                distance_m: row.get("distance_m"),
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                location_name: row.get("location_name"),
                event_type: row.get("event_type"),
            })
            .collect())
    }

    /// Search for memories captured within a time range.
    ///
    /// Returns attachments with capture times that overlap the given range,
    /// ordered by capture time (earliest first).
    ///
    /// # Arguments
    ///
    /// * `start` - Start of time range (inclusive)
    /// * `end` - End of time range (inclusive)
    pub async fn search_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MemoryTimeResult>> {
        let rows = sqlx::query(
            r#"
            SELECT provenance_id, attachment_id, note_id,
                   capture_time_start, capture_time_end, event_type, location_name
            FROM (
                -- Provenance matches (file and note)
                SELECT
                    p.id as provenance_id,
                    p.attachment_id,
                    COALESCE(p.note_id, a.note_id) as note_id,
                    lower(p.capture_time) as capture_time_start,
                    upper(p.capture_time) as capture_time_end,
                    p.event_type,
                    nl.name as location_name
                FROM provenance p
                LEFT JOIN attachment a ON p.attachment_id = a.id
                LEFT JOIN prov_location pl ON p.location_id = pl.id
                LEFT JOIN named_location nl ON pl.named_location_id = nl.id
                WHERE p.capture_time && tstzrange($1, $2)

                UNION ALL

                -- Note metadata matches (user_created_at in time range)
                SELECT
                    NULL::uuid as provenance_id,
                    NULL::uuid as attachment_id,
                    n.id as note_id,
                    no.user_created_at as capture_time_start,
                    no.user_created_at as capture_time_end,
                    NULL::text as event_type,
                    n.metadata->>'location_name' as location_name
                FROM note n
                JOIN note_original no ON n.id = no.note_id
                WHERE n.deleted_at IS NULL
                  AND NOT EXISTS (SELECT 1 FROM provenance WHERE note_id = n.id)
                  AND NOT EXISTS (SELECT 1 FROM provenance p2 JOIN attachment att ON p2.attachment_id = att.id WHERE att.note_id = n.id)
                  AND no.user_created_at >= $1
                  AND no.user_created_at <= $2
            ) combined
            ORDER BY
                CASE WHEN provenance_id IS NOT NULL THEN 0 ELSE 1 END,
                capture_time_start
            LIMIT 100
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| MemoryTimeResult {
                provenance_id: row.get("provenance_id"),
                attachment_id: row.get("attachment_id"),
                note_id: row.get("note_id"),
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                event_type: row.get("event_type"),
                location_name: row.get("location_name"),
            })
            .collect())
    }

    /// Search for memories by both location and time.
    ///
    /// Returns attachments captured within the specified radius AND time range,
    /// ordered by distance (closest first).
    ///
    /// # Arguments
    ///
    /// * `lat` - Latitude in decimal degrees
    /// * `lon` - Longitude in decimal degrees
    /// * `radius_meters` - Search radius in meters
    /// * `start` - Start of time range (inclusive)
    /// * `end` - End of time range (inclusive)
    pub async fn search_by_location_and_time(
        &self,
        lat: f64,
        lon: f64,
        radius_meters: f64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MemoryLocationResult>> {
        let rows = sqlx::query(
            r#"
            SELECT provenance_id, attachment_id, note_id, filename, content_type,
                   distance_m, capture_time_start, capture_time_end, location_name, event_type
            FROM (
                -- Provenance matches (file and note)
                SELECT
                    p.id as provenance_id,
                    p.attachment_id,
                    COALESCE(p.note_id, a.note_id) as note_id,
                    COALESCE(a.filename, pn.title) as filename,
                    ab.content_type,
                    ST_Distance(
                        pl.point,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    lower(p.capture_time) as capture_time_start,
                    upper(p.capture_time) as capture_time_end,
                    nl.name as location_name,
                    p.event_type
                FROM provenance p
                LEFT JOIN attachment a ON p.attachment_id = a.id
                LEFT JOIN attachment_blob ab ON a.blob_id = ab.id
                LEFT JOIN note pn ON p.note_id = pn.id
                JOIN prov_location pl ON p.location_id = pl.id
                LEFT JOIN named_location nl ON pl.named_location_id = nl.id
                WHERE ST_DWithin(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                    $3
                )
                AND p.capture_time && tstzrange($4, $5)

                UNION ALL

                -- Note metadata matches (lat/lng + time)
                SELECT
                    NULL::uuid as provenance_id,
                    NULL::uuid as attachment_id,
                    n.id as note_id,
                    n.title as filename,
                    NULL::text as content_type,
                    ST_Distance(
                        ST_SetSRID(ST_MakePoint(
                            (n.metadata->>'longitude')::float8,
                            (n.metadata->>'latitude')::float8
                        ), 4326)::geography,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    no.user_created_at as capture_time_start,
                    no.user_created_at as capture_time_end,
                    n.metadata->>'location_name' as location_name,
                    NULL::text as event_type
                FROM note n
                LEFT JOIN note_original no ON n.id = no.note_id
                WHERE n.metadata->>'latitude' IS NOT NULL
                  AND n.metadata->>'longitude' IS NOT NULL
                  AND n.deleted_at IS NULL
                  AND NOT EXISTS (SELECT 1 FROM provenance WHERE note_id = n.id)
                  AND ST_DWithin(
                      ST_SetSRID(ST_MakePoint(
                          (n.metadata->>'longitude')::float8,
                          (n.metadata->>'latitude')::float8
                      ), 4326)::geography,
                      ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                      $3
                  )
                  AND no.user_created_at >= $4
                  AND no.user_created_at <= $5
            ) combined
            ORDER BY distance_m
            LIMIT 100
            "#,
        )
        .bind(lat)
        .bind(lon)
        .bind(radius_meters)
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| MemoryLocationResult {
                provenance_id: row.get("provenance_id"),
                attachment_id: row.get("attachment_id"),
                note_id: row.get("note_id"),
                filename: row.get("filename"),
                content_type: row.get("content_type"),
                distance_m: row.get("distance_m"),
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                location_name: row.get("location_name"),
                event_type: row.get("event_type"),
            })
            .collect())
    }

    /// Get the complete provenance chain for a note.
    ///
    /// Returns detailed provenance information including location, device,
    /// and temporal context for all file attachments and the note itself.
    ///
    /// # Arguments
    ///
    /// * `note_id` - The note ID to retrieve provenance for
    ///
    /// # Returns
    ///
    /// `Some(MemoryProvenance)` if the note has provenance, `None` otherwise.
    pub async fn get_memory_provenance(&self, note_id: Uuid) -> Result<Option<MemoryProvenance>> {
        // Get all provenance records (file-level and note-level) for this note
        let rows = sqlx::query(
            r#"
            SELECT
                p.id,
                p.attachment_id,
                p.note_id,
                lower(p.capture_time) as capture_time_start,
                upper(p.capture_time) as capture_time_end,
                p.capture_timezone,
                p.capture_duration_seconds,
                p.time_source,
                p.time_confidence,
                p.event_type,
                p.event_title,
                p.event_description,
                p.user_corrected,
                p.created_at,
                p.location_id,
                p.device_id
            FROM provenance p
            LEFT JOIN attachment a ON p.attachment_id = a.id
            WHERE (a.note_id = $1 OR p.note_id = $1)
            ORDER BY p.created_at DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        if rows.is_empty() {
            return Ok(None);
        }

        let mut files = Vec::new();
        let mut note_prov = None;

        for row in rows {
            let location_id: Option<Uuid> = row.get("location_id");
            let device_id: Option<Uuid> = row.get("device_id");
            let attachment_id: Option<Uuid> = row.get::<Option<Uuid>, _>("attachment_id");
            let prov_note_id: Option<Uuid> = row.get::<Option<Uuid>, _>("note_id");

            // Fetch location if present
            let location = if let Some(loc_id) = location_id {
                self.get_location(loc_id).await?
            } else {
                None
            };

            // Fetch device if present
            let device = if let Some(dev_id) = device_id {
                self.get_device(dev_id).await?
            } else {
                None
            };

            let record = ProvenanceRecord {
                id: row.get("id"),
                attachment_id,
                note_id: prov_note_id,
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                capture_timezone: row.get("capture_timezone"),
                capture_duration_seconds: row.get("capture_duration_seconds"),
                time_source: row.get("time_source"),
                time_confidence: row.get("time_confidence"),
                location,
                device,
                event_type: row.get("event_type"),
                event_title: row.get("event_title"),
                event_description: row.get("event_description"),
                user_corrected: row.get("user_corrected"),
                created_at: row.get("created_at"),
            };

            // Separate file-level and note-level provenance
            if attachment_id.is_some() {
                files.push(record);
            } else if prov_note_id.is_some() {
                note_prov = Some(record);
            }
        }

        Ok(Some(MemoryProvenance {
            note_id,
            files,
            note: note_prov,
        }))
    }

    /// Internal helper to fetch location details.
    async fn get_location(&self, location_id: Uuid) -> Result<Option<MemoryLocation>> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                ST_Y(point::geometry) as latitude,
                ST_X(point::geometry) as longitude,
                horizontal_accuracy_m,
                altitude_m,
                vertical_accuracy_m,
                heading_degrees,
                speed_mps,
                named_location_id,
                source,
                confidence
            FROM prov_location
            WHERE id = $1
            "#,
        )
        .bind(location_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        if let Some(row) = row {
            let named_location_id: Option<Uuid> = row.get("named_location_id");
            let named_location_name = if let Some(nl_id) = named_location_id {
                self.get_named_location_name(nl_id).await?
            } else {
                None
            };

            Ok(Some(MemoryLocation {
                id: row.get("id"),
                latitude: row.get("latitude"),
                longitude: row.get("longitude"),
                horizontal_accuracy_m: row.get("horizontal_accuracy_m"),
                altitude_m: row.get("altitude_m"),
                vertical_accuracy_m: row.get("vertical_accuracy_m"),
                heading_degrees: row.get("heading_degrees"),
                speed_mps: row.get("speed_mps"),
                named_location_id,
                named_location_name,
                source: row.get("source"),
                confidence: row.get("confidence"),
            }))
        } else {
            Ok(None)
        }
    }

    /// Internal helper to fetch named location name.
    async fn get_named_location_name(&self, named_location_id: Uuid) -> Result<Option<String>> {
        let row = sqlx::query("SELECT name FROM named_location WHERE id = $1")
            .bind(named_location_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(row.map(|r| r.get("name")))
    }

    /// Internal helper to fetch device details.
    async fn get_device(&self, device_id: Uuid) -> Result<Option<MemoryDevice>> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                device_make,
                device_model,
                device_os,
                device_os_version,
                software,
                software_version,
                device_name
            FROM prov_agent_device
            WHERE id = $1
            "#,
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.map(|r| MemoryDevice {
            id: r.get("id"),
            device_make: r.get("device_make"),
            device_model: r.get("device_model"),
            device_os: r.get("device_os"),
            device_os_version: r.get("device_os_version"),
            software: r.get("software"),
            software_version: r.get("software_version"),
            device_name: r.get("device_name"),
        }))
    }
}

/// Transaction-aware variants for archive-scoped operations.
impl PgMemorySearchRepository {
    /// Transaction-aware variant of search_by_location.
    pub async fn search_by_location_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        lat: f64,
        lon: f64,
        radius_meters: f64,
    ) -> Result<Vec<MemoryLocationResult>> {
        let rows = sqlx::query(
            r#"
            SELECT provenance_id, attachment_id, note_id, filename, content_type,
                   distance_m, capture_time_start, capture_time_end, location_name, event_type
            FROM (
                -- Provenance matches (file and note)
                SELECT
                    p.id as provenance_id,
                    p.attachment_id,
                    COALESCE(p.note_id, a.note_id) as note_id,
                    COALESCE(a.filename, pn.title) as filename,
                    ab.content_type,
                    ST_Distance(
                        pl.point,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    lower(p.capture_time) as capture_time_start,
                    upper(p.capture_time) as capture_time_end,
                    nl.name as location_name,
                    p.event_type
                FROM provenance p
                LEFT JOIN attachment a ON p.attachment_id = a.id
                LEFT JOIN attachment_blob ab ON a.blob_id = ab.id
                LEFT JOIN note pn ON p.note_id = pn.id
                JOIN prov_location pl ON p.location_id = pl.id
                LEFT JOIN named_location nl ON pl.named_location_id = nl.id
                WHERE ST_DWithin(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                    $3
                )

                UNION ALL

                -- Note metadata matches (latitude/longitude in JSONB)
                SELECT
                    NULL::uuid as provenance_id,
                    NULL::uuid as attachment_id,
                    n.id as note_id,
                    n.title as filename,
                    NULL::text as content_type,
                    ST_Distance(
                        ST_SetSRID(ST_MakePoint(
                            (n.metadata->>'longitude')::float8,
                            (n.metadata->>'latitude')::float8
                        ), 4326)::geography,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    no.user_created_at as capture_time_start,
                    no.user_created_at as capture_time_end,
                    n.metadata->>'location_name' as location_name,
                    NULL::text as event_type
                FROM note n
                LEFT JOIN note_original no ON n.id = no.note_id
                WHERE n.metadata->>'latitude' IS NOT NULL
                  AND n.metadata->>'longitude' IS NOT NULL
                  AND n.deleted_at IS NULL
                  AND NOT EXISTS (SELECT 1 FROM provenance WHERE note_id = n.id)
                  AND ST_DWithin(
                      ST_SetSRID(ST_MakePoint(
                          (n.metadata->>'longitude')::float8,
                          (n.metadata->>'latitude')::float8
                      ), 4326)::geography,
                      ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                      $3
                  )
            ) combined
            ORDER BY distance_m
            LIMIT 100
            "#,
        )
        .bind(lat)
        .bind(lon)
        .bind(radius_meters)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| MemoryLocationResult {
                provenance_id: row.get("provenance_id"),
                attachment_id: row.get("attachment_id"),
                note_id: row.get("note_id"),
                filename: row.get("filename"),
                content_type: row.get("content_type"),
                distance_m: row.get("distance_m"),
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                location_name: row.get("location_name"),
                event_type: row.get("event_type"),
            })
            .collect())
    }

    /// Transaction-aware variant of search_by_timerange.
    pub async fn search_by_timerange_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MemoryTimeResult>> {
        let rows = sqlx::query(
            r#"
            SELECT provenance_id, attachment_id, note_id,
                   capture_time_start, capture_time_end, event_type, location_name
            FROM (
                -- Provenance matches (file and note)
                SELECT
                    p.id as provenance_id,
                    p.attachment_id,
                    COALESCE(p.note_id, a.note_id) as note_id,
                    lower(p.capture_time) as capture_time_start,
                    upper(p.capture_time) as capture_time_end,
                    p.event_type,
                    nl.name as location_name
                FROM provenance p
                LEFT JOIN attachment a ON p.attachment_id = a.id
                LEFT JOIN prov_location pl ON p.location_id = pl.id
                LEFT JOIN named_location nl ON pl.named_location_id = nl.id
                WHERE p.capture_time && tstzrange($1, $2)

                UNION ALL

                -- Note metadata matches (user_created_at in time range)
                SELECT
                    NULL::uuid as provenance_id,
                    NULL::uuid as attachment_id,
                    n.id as note_id,
                    no.user_created_at as capture_time_start,
                    no.user_created_at as capture_time_end,
                    NULL::text as event_type,
                    n.metadata->>'location_name' as location_name
                FROM note n
                JOIN note_original no ON n.id = no.note_id
                WHERE n.deleted_at IS NULL
                  AND NOT EXISTS (SELECT 1 FROM provenance WHERE note_id = n.id)
                  AND NOT EXISTS (SELECT 1 FROM provenance p2 JOIN attachment att ON p2.attachment_id = att.id WHERE att.note_id = n.id)
                  AND no.user_created_at >= $1
                  AND no.user_created_at <= $2
            ) combined
            ORDER BY
                CASE WHEN provenance_id IS NOT NULL THEN 0 ELSE 1 END,
                capture_time_start
            LIMIT 100
            "#,
        )
        .bind(start)
        .bind(end)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| MemoryTimeResult {
                provenance_id: row.get("provenance_id"),
                attachment_id: row.get("attachment_id"),
                note_id: row.get("note_id"),
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                event_type: row.get("event_type"),
                location_name: row.get("location_name"),
            })
            .collect())
    }

    /// Transaction-aware variant of search_by_location_and_time.
    pub async fn search_by_location_and_time_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        lat: f64,
        lon: f64,
        radius_meters: f64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MemoryLocationResult>> {
        let rows = sqlx::query(
            r#"
            SELECT provenance_id, attachment_id, note_id, filename, content_type,
                   distance_m, capture_time_start, capture_time_end, location_name, event_type
            FROM (
                -- Provenance matches (file and note)
                SELECT
                    p.id as provenance_id,
                    p.attachment_id,
                    COALESCE(p.note_id, a.note_id) as note_id,
                    COALESCE(a.filename, pn.title) as filename,
                    ab.content_type,
                    ST_Distance(
                        pl.point,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    lower(p.capture_time) as capture_time_start,
                    upper(p.capture_time) as capture_time_end,
                    nl.name as location_name,
                    p.event_type
                FROM provenance p
                LEFT JOIN attachment a ON p.attachment_id = a.id
                LEFT JOIN attachment_blob ab ON a.blob_id = ab.id
                LEFT JOIN note pn ON p.note_id = pn.id
                JOIN prov_location pl ON p.location_id = pl.id
                LEFT JOIN named_location nl ON pl.named_location_id = nl.id
                WHERE ST_DWithin(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                    $3
                )
                AND p.capture_time && tstzrange($4, $5)

                UNION ALL

                -- Note metadata matches (lat/lng + time)
                SELECT
                    NULL::uuid as provenance_id,
                    NULL::uuid as attachment_id,
                    n.id as note_id,
                    n.title as filename,
                    NULL::text as content_type,
                    ST_Distance(
                        ST_SetSRID(ST_MakePoint(
                            (n.metadata->>'longitude')::float8,
                            (n.metadata->>'latitude')::float8
                        ), 4326)::geography,
                        ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                    ) as distance_m,
                    no.user_created_at as capture_time_start,
                    no.user_created_at as capture_time_end,
                    n.metadata->>'location_name' as location_name,
                    NULL::text as event_type
                FROM note n
                LEFT JOIN note_original no ON n.id = no.note_id
                WHERE n.metadata->>'latitude' IS NOT NULL
                  AND n.metadata->>'longitude' IS NOT NULL
                  AND n.deleted_at IS NULL
                  AND NOT EXISTS (SELECT 1 FROM provenance WHERE note_id = n.id)
                  AND ST_DWithin(
                      ST_SetSRID(ST_MakePoint(
                          (n.metadata->>'longitude')::float8,
                          (n.metadata->>'latitude')::float8
                      ), 4326)::geography,
                      ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                      $3
                  )
                  AND no.user_created_at >= $4
                  AND no.user_created_at <= $5
            ) combined
            ORDER BY distance_m
            LIMIT 100
            "#,
        )
        .bind(lat)
        .bind(lon)
        .bind(radius_meters)
        .bind(start)
        .bind(end)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(rows
            .into_iter()
            .map(|row| MemoryLocationResult {
                provenance_id: row.get("provenance_id"),
                attachment_id: row.get("attachment_id"),
                note_id: row.get("note_id"),
                filename: row.get("filename"),
                content_type: row.get("content_type"),
                distance_m: row.get("distance_m"),
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                location_name: row.get("location_name"),
                event_type: row.get("event_type"),
            })
            .collect())
    }

    /// Transaction-aware variant of get_memory_provenance.
    pub async fn get_memory_provenance_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Option<MemoryProvenance>> {
        // Get all provenance records (file-level and note-level) for this note
        let rows = sqlx::query(
            r#"
            SELECT
                p.id,
                p.attachment_id,
                p.note_id,
                lower(p.capture_time) as capture_time_start,
                upper(p.capture_time) as capture_time_end,
                p.capture_timezone,
                p.capture_duration_seconds,
                p.time_source,
                p.time_confidence,
                p.event_type,
                p.event_title,
                p.event_description,
                p.user_corrected,
                p.created_at,
                p.location_id,
                p.device_id
            FROM provenance p
            LEFT JOIN attachment a ON p.attachment_id = a.id
            WHERE (a.note_id = $1 OR p.note_id = $1)
            ORDER BY p.created_at DESC
            "#,
        )
        .bind(note_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(Error::Database)?;

        if rows.is_empty() {
            return Ok(None);
        }

        let mut files = Vec::new();
        let mut note_prov = None;

        for row in rows {
            let location_id: Option<Uuid> = row.get("location_id");
            let device_id: Option<Uuid> = row.get("device_id");
            let attachment_id: Option<Uuid> = row.get::<Option<Uuid>, _>("attachment_id");
            let prov_note_id: Option<Uuid> = row.get::<Option<Uuid>, _>("note_id");

            // Inline location query to use transaction
            let location = if let Some(loc_id) = location_id {
                let loc_row = sqlx::query(
                    r#"
                    SELECT
                        id,
                        ST_Y(point::geometry) as latitude,
                        ST_X(point::geometry) as longitude,
                        horizontal_accuracy_m,
                        altitude_m,
                        vertical_accuracy_m,
                        heading_degrees,
                        speed_mps,
                        named_location_id,
                        source,
                        confidence
                    FROM prov_location
                    WHERE id = $1
                    "#,
                )
                .bind(loc_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(Error::Database)?;

                if let Some(loc_row) = loc_row {
                    let named_location_id: Option<Uuid> = loc_row.get("named_location_id");
                    let named_location_name = if let Some(nl_id) = named_location_id {
                        let nl_row = sqlx::query("SELECT name FROM named_location WHERE id = $1")
                            .bind(nl_id)
                            .fetch_optional(&mut **tx)
                            .await
                            .map_err(Error::Database)?;
                        nl_row.map(|r| r.get("name"))
                    } else {
                        None
                    };

                    Some(MemoryLocation {
                        id: loc_row.get("id"),
                        latitude: loc_row.get("latitude"),
                        longitude: loc_row.get("longitude"),
                        horizontal_accuracy_m: loc_row.get("horizontal_accuracy_m"),
                        altitude_m: loc_row.get("altitude_m"),
                        vertical_accuracy_m: loc_row.get("vertical_accuracy_m"),
                        heading_degrees: loc_row.get("heading_degrees"),
                        speed_mps: loc_row.get("speed_mps"),
                        named_location_id,
                        named_location_name,
                        source: loc_row.get("source"),
                        confidence: loc_row.get("confidence"),
                    })
                } else {
                    None
                }
            } else {
                None
            };

            // Inline device query to use transaction
            let device = if let Some(dev_id) = device_id {
                let dev_row = sqlx::query(
                    r#"
                    SELECT
                        id,
                        device_make,
                        device_model,
                        device_os,
                        device_os_version,
                        software,
                        software_version,
                        device_name
                    FROM prov_agent_device
                    WHERE id = $1
                    "#,
                )
                .bind(dev_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(Error::Database)?;

                dev_row.map(|r| MemoryDevice {
                    id: r.get("id"),
                    device_make: r.get("device_make"),
                    device_model: r.get("device_model"),
                    device_os: r.get("device_os"),
                    device_os_version: r.get("device_os_version"),
                    software: r.get("software"),
                    software_version: r.get("software_version"),
                    device_name: r.get("device_name"),
                })
            } else {
                None
            };

            let record = ProvenanceRecord {
                id: row.get("id"),
                attachment_id,
                note_id: prov_note_id,
                capture_time_start: row.get("capture_time_start"),
                capture_time_end: row.get("capture_time_end"),
                capture_timezone: row.get("capture_timezone"),
                capture_duration_seconds: row.get("capture_duration_seconds"),
                time_source: row.get("time_source"),
                time_confidence: row.get("time_confidence"),
                location,
                device,
                event_type: row.get("event_type"),
                event_title: row.get("event_title"),
                event_description: row.get("event_description"),
                user_corrected: row.get("user_corrected"),
                created_at: row.get("created_at"),
            };

            // Separate file-level and note-level provenance
            if attachment_id.is_some() {
                files.push(record);
            } else if prov_note_id.is_some() {
                note_prov = Some(record);
            }
        }

        Ok(Some(MemoryProvenance {
            note_id,
            files,
            note: note_prov,
        }))
    }
}

/// Provenance record creation methods.
impl PgMemorySearchRepository {
    /// Create a provenance location record.
    ///
    /// # Arguments
    ///
    /// * `req` - Location creation request with coordinates and metadata
    ///
    /// # Returns
    ///
    /// The UUID of the created location record.
    pub async fn create_prov_location(&self, req: &CreateProvLocationRequest) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO prov_location (
                point, horizontal_accuracy_m, altitude_m, vertical_accuracy_m,
                heading_degrees, speed_mps, named_location_id, source, confidence
            )
            VALUES (
                ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                $3, $4, $5, $6, $7, $8, $9, $10
            )
            RETURNING id
            "#,
        )
        .bind(req.latitude)
        .bind(req.longitude)
        .bind(req.horizontal_accuracy_m)
        .bind(req.altitude_m)
        .bind(req.vertical_accuracy_m)
        .bind(req.heading_degrees)
        .bind(req.speed_mps)
        .bind(req.named_location_id)
        .bind(&req.source)
        .bind(&req.confidence)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Transaction-aware variant of create_prov_location.
    pub async fn create_prov_location_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: &CreateProvLocationRequest,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO prov_location (
                point, horizontal_accuracy_m, altitude_m, vertical_accuracy_m,
                heading_degrees, speed_mps, named_location_id, source, confidence
            )
            VALUES (
                ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                $3, $4, $5, $6, $7, $8, $9, $10
            )
            RETURNING id
            "#,
        )
        .bind(req.latitude)
        .bind(req.longitude)
        .bind(req.horizontal_accuracy_m)
        .bind(req.altitude_m)
        .bind(req.vertical_accuracy_m)
        .bind(req.heading_degrees)
        .bind(req.speed_mps)
        .bind(req.named_location_id)
        .bind(&req.source)
        .bind(&req.confidence)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Create a named location record (landmark, address, etc.).
    ///
    /// Automatically generates a slug from the name.
    ///
    /// # Arguments
    ///
    /// * `req` - Named location creation request
    ///
    /// # Returns
    ///
    /// JSON representation of the created named location.
    pub async fn create_named_location(
        &self,
        req: &CreateNamedLocationRequest,
    ) -> Result<serde_json::Value> {
        // Generate slug from name
        let slug = req
            .name
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        let row = sqlx::query(
            r#"
            INSERT INTO named_location (
                name, slug, location_type, point, radius_m, address_line,
                locality, admin_area, country, country_code, postal_code,
                timezone, altitude_m, is_private, metadata
            )
            VALUES (
                $1, $2, $3, ST_SetSRID(ST_MakePoint($5, $4), 4326)::geography,
                $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16
            )
            RETURNING
                id, name, slug, display_name, location_type,
                ST_Y(point::geometry) as latitude,
                ST_X(point::geometry) as longitude,
                created_at
            "#,
        )
        .bind(&req.name)
        .bind(&slug)
        .bind(&req.location_type)
        .bind(req.latitude)
        .bind(req.longitude)
        .bind(req.radius_m)
        .bind(&req.address_line)
        .bind(&req.locality)
        .bind(&req.admin_area)
        .bind(&req.country)
        .bind(&req.country_code)
        .bind(&req.postal_code)
        .bind(&req.timezone)
        .bind(req.altitude_m)
        .bind(req.is_private)
        .bind(&req.metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(serde_json::json!({
            "id": row.get::<Uuid, _>("id"),
            "name": row.get::<String, _>("name"),
            "slug": row.get::<String, _>("slug"),
            "display_name": row.get::<Option<String>, _>("display_name"),
            "location_type": row.get::<String, _>("location_type"),
            "latitude": row.get::<f64, _>("latitude"),
            "longitude": row.get::<f64, _>("longitude"),
            "created_at": row.get::<DateTime<Utc>, _>("created_at"),
        }))
    }

    /// Transaction-aware variant of create_named_location.
    pub async fn create_named_location_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: &CreateNamedLocationRequest,
    ) -> Result<serde_json::Value> {
        // Generate slug from name
        let slug = req
            .name
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        let row = sqlx::query(
            r#"
            INSERT INTO named_location (
                name, slug, location_type, point, radius_m, address_line,
                locality, admin_area, country, country_code, postal_code,
                timezone, altitude_m, is_private, metadata
            )
            VALUES (
                $1, $2, $3, ST_SetSRID(ST_MakePoint($5, $4), 4326)::geography,
                $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16
            )
            RETURNING
                id, name, slug, display_name, location_type,
                ST_Y(point::geometry) as latitude,
                ST_X(point::geometry) as longitude,
                created_at
            "#,
        )
        .bind(&req.name)
        .bind(&slug)
        .bind(&req.location_type)
        .bind(req.latitude)
        .bind(req.longitude)
        .bind(req.radius_m)
        .bind(&req.address_line)
        .bind(&req.locality)
        .bind(&req.admin_area)
        .bind(&req.country)
        .bind(&req.country_code)
        .bind(&req.postal_code)
        .bind(&req.timezone)
        .bind(req.altitude_m)
        .bind(req.is_private)
        .bind(&req.metadata)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(serde_json::json!({
            "id": row.get::<Uuid, _>("id"),
            "name": row.get::<String, _>("name"),
            "slug": row.get::<String, _>("slug"),
            "display_name": row.get::<Option<String>, _>("display_name"),
            "location_type": row.get::<String, _>("location_type"),
            "latitude": row.get::<f64, _>("latitude"),
            "longitude": row.get::<f64, _>("longitude"),
            "created_at": row.get::<DateTime<Utc>, _>("created_at"),
        }))
    }

    /// Create a provenance agent device record.
    ///
    /// Uses ON CONFLICT to deduplicate devices by (device_make, device_model, owner_id).
    ///
    /// # Arguments
    ///
    /// * `req` - Device creation request
    ///
    /// # Returns
    ///
    /// The created or existing device record.
    pub async fn create_prov_agent_device(
        &self,
        req: &CreateProvDeviceRequest,
    ) -> Result<MemoryDevice> {
        let row = sqlx::query(
            r#"
            INSERT INTO prov_agent_device (
                device_make, device_model, device_os, device_os_version,
                software, software_version, has_gps, has_accelerometer,
                sensor_metadata, device_name
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (device_make, device_model, owner_id)
            DO UPDATE SET device_make = EXCLUDED.device_make
            RETURNING
                id, device_make, device_model, device_os, device_os_version,
                software, software_version, device_name
            "#,
        )
        .bind(&req.device_make)
        .bind(&req.device_model)
        .bind(&req.device_os)
        .bind(&req.device_os_version)
        .bind(&req.software)
        .bind(&req.software_version)
        .bind(req.has_gps)
        .bind(req.has_accelerometer)
        .bind(&req.sensor_metadata)
        .bind(&req.device_name)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(MemoryDevice {
            id: row.get("id"),
            device_make: row.get("device_make"),
            device_model: row.get("device_model"),
            device_os: row.get("device_os"),
            device_os_version: row.get("device_os_version"),
            software: row.get("software"),
            software_version: row.get("software_version"),
            device_name: row.get("device_name"),
        })
    }

    /// Transaction-aware variant of create_prov_agent_device.
    pub async fn create_prov_agent_device_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: &CreateProvDeviceRequest,
    ) -> Result<MemoryDevice> {
        let row = sqlx::query(
            r#"
            INSERT INTO prov_agent_device (
                device_make, device_model, device_os, device_os_version,
                software, software_version, has_gps, has_accelerometer,
                sensor_metadata, device_name
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (device_make, device_model, owner_id)
            DO UPDATE SET device_make = EXCLUDED.device_make
            RETURNING
                id, device_make, device_model, device_os, device_os_version,
                software, software_version, device_name
            "#,
        )
        .bind(&req.device_make)
        .bind(&req.device_model)
        .bind(&req.device_os)
        .bind(&req.device_os_version)
        .bind(&req.software)
        .bind(&req.software_version)
        .bind(req.has_gps)
        .bind(req.has_accelerometer)
        .bind(&req.sensor_metadata)
        .bind(&req.device_name)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(MemoryDevice {
            id: row.get("id"),
            device_make: row.get("device_make"),
            device_model: row.get("device_model"),
            device_os: row.get("device_os"),
            device_os_version: row.get("device_os_version"),
            software: row.get("software"),
            software_version: row.get("software_version"),
            device_name: row.get("device_name"),
        })
    }

    /// Create a file provenance record linking an attachment to spatial-temporal context.
    ///
    /// # Arguments
    ///
    /// * `req` - File provenance creation request
    ///
    /// # Returns
    ///
    /// The UUID of the created file provenance record.
    pub async fn create_file_provenance(&self, req: &CreateFileProvenanceRequest) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO provenance (
                attachment_id, note_id, capture_time, capture_timezone, capture_duration_seconds,
                time_source, time_confidence, location_id, device_id, event_type,
                event_title, event_description, raw_metadata
            )
            VALUES (
                $1, $2,
                CASE WHEN $3::timestamptz IS NOT NULL
                    THEN tstzrange($3::timestamptz, $4::timestamptz, '[]')
                    ELSE NULL
                END,
                $5, $6, $7, COALESCE($8, 'unknown'), $9, $10, $11, $12, $13, $14
            )
            RETURNING id
            "#,
        )
        .bind(req.attachment_id)
        .bind(req.note_id)
        .bind(req.capture_time_start)
        .bind(req.capture_time_end)
        .bind(&req.capture_timezone)
        .bind(req.capture_duration_seconds)
        .bind(&req.time_source)
        .bind(&req.time_confidence)
        .bind(req.location_id)
        .bind(req.device_id)
        .bind(&req.event_type)
        .bind(&req.event_title)
        .bind(&req.event_description)
        .bind(&req.raw_metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Transaction-aware variant of create_file_provenance.
    pub async fn create_file_provenance_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: &CreateFileProvenanceRequest,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO provenance (
                attachment_id, note_id, capture_time, capture_timezone, capture_duration_seconds,
                time_source, time_confidence, location_id, device_id, event_type,
                event_title, event_description, raw_metadata
            )
            VALUES (
                $1, $2,
                CASE WHEN $3::timestamptz IS NOT NULL
                    THEN tstzrange($3::timestamptz, $4::timestamptz, '[]')
                    ELSE NULL
                END,
                $5, $6, $7, COALESCE($8, 'unknown'), $9, $10, $11, $12, $13, $14
            )
            RETURNING id
            "#,
        )
        .bind(req.attachment_id)
        .bind(req.note_id)
        .bind(req.capture_time_start)
        .bind(req.capture_time_end)
        .bind(&req.capture_timezone)
        .bind(req.capture_duration_seconds)
        .bind(&req.time_source)
        .bind(&req.time_confidence)
        .bind(req.location_id)
        .bind(req.device_id)
        .bind(&req.event_type)
        .bind(&req.event_title)
        .bind(&req.event_description)
        .bind(&req.raw_metadata)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Create a note provenance record linking a note to spatial-temporal context.
    ///
    /// # Arguments
    ///
    /// * `req` - Note provenance creation request
    ///
    /// # Returns
    ///
    /// The UUID of the created note provenance record.
    pub async fn create_note_provenance(&self, req: &CreateNoteProvenanceRequest) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO provenance (
                note_id, capture_time, capture_timezone,
                time_source, time_confidence, location_id, device_id, event_type,
                event_title, event_description
            )
            VALUES (
                $1,
                CASE WHEN $2::timestamptz IS NOT NULL
                    THEN tstzrange($2::timestamptz, $3::timestamptz, '[]')
                    ELSE NULL
                END,
                $4, $5, COALESCE($6, 'unknown'), $7, $8, $9, $10, $11
            )
            RETURNING id
            "#,
        )
        .bind(req.note_id)
        .bind(req.capture_time_start)
        .bind(req.capture_time_end)
        .bind(&req.capture_timezone)
        .bind(&req.time_source)
        .bind(&req.time_confidence)
        .bind(req.location_id)
        .bind(req.device_id)
        .bind(&req.event_type)
        .bind(&req.event_title)
        .bind(&req.event_description)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Transaction-aware variant of create_note_provenance.
    pub async fn create_note_provenance_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        req: &CreateNoteProvenanceRequest,
    ) -> Result<Uuid> {
        let row = sqlx::query(
            r#"
            INSERT INTO provenance (
                note_id, capture_time, capture_timezone,
                time_source, time_confidence, location_id, device_id, event_type,
                event_title, event_description
            )
            VALUES (
                $1,
                CASE WHEN $2::timestamptz IS NOT NULL
                    THEN tstzrange($2::timestamptz, $3::timestamptz, '[]')
                    ELSE NULL
                END,
                $4, $5, COALESCE($6, 'unknown'), $7, $8, $9, $10, $11
            )
            RETURNING id
            "#,
        )
        .bind(req.note_id)
        .bind(req.capture_time_start)
        .bind(req.capture_time_end)
        .bind(&req.capture_timezone)
        .bind(&req.time_source)
        .bind(&req.time_confidence)
        .bind(req.location_id)
        .bind(req.device_id)
        .bind(&req.event_type)
        .bind(&req.event_title)
        .bind(&req.event_description)
        .fetch_one(&mut **tx)
        .await
        .map_err(Error::Database)?;

        Ok(row.get("id"))
    }

    /// Get note-level provenance for a specific note.
    ///
    /// # Arguments
    ///
    /// * `note_id` - The note ID to retrieve provenance for
    ///
    /// # Returns
    ///
    /// `Some(ProvenanceRecord)` if the note has note-level provenance, `None` otherwise.
    pub async fn get_note_provenance(&self, note_id: Uuid) -> Result<Option<ProvenanceRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                p.id,
                p.note_id,
                lower(p.capture_time) as capture_time_start,
                upper(p.capture_time) as capture_time_end,
                p.capture_timezone,
                p.capture_duration_seconds,
                p.time_source,
                p.time_confidence,
                p.event_type,
                p.event_title,
                p.event_description,
                p.user_corrected,
                p.created_at,
                p.location_id,
                p.device_id
            FROM provenance p
            WHERE p.note_id = $1
            "#,
        )
        .bind(note_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        match row {
            None => Ok(None),
            Some(row) => {
                let location_id: Option<Uuid> = row.get("location_id");
                let device_id: Option<Uuid> = row.get("device_id");
                let location = if let Some(loc_id) = location_id {
                    self.get_location(loc_id).await?
                } else {
                    None
                };
                let device = if let Some(dev_id) = device_id {
                    self.get_device(dev_id).await?
                } else {
                    None
                };
                Ok(Some(ProvenanceRecord {
                    id: row.get("id"),
                    attachment_id: None,
                    note_id: row.get("note_id"),
                    capture_time_start: row.get("capture_time_start"),
                    capture_time_end: row.get("capture_time_end"),
                    capture_timezone: row.get("capture_timezone"),
                    capture_duration_seconds: row.get("capture_duration_seconds"),
                    time_source: row.get("time_source"),
                    time_confidence: row.get("time_confidence"),
                    location,
                    device,
                    event_type: row.get("event_type"),
                    event_title: row.get("event_title"),
                    event_description: row.get("event_description"),
                    user_corrected: row.get("user_corrected"),
                    created_at: row.get("created_at"),
                }))
            }
        }
    }

    /// Transaction-aware variant of get_note_provenance.
    pub async fn get_note_provenance_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        note_id: Uuid,
    ) -> Result<Option<ProvenanceRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                p.id,
                p.note_id,
                lower(p.capture_time) as capture_time_start,
                upper(p.capture_time) as capture_time_end,
                p.capture_timezone,
                p.capture_duration_seconds,
                p.time_source,
                p.time_confidence,
                p.event_type,
                p.event_title,
                p.event_description,
                p.user_corrected,
                p.created_at,
                p.location_id,
                p.device_id
            FROM provenance p
            WHERE p.note_id = $1
            "#,
        )
        .bind(note_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(Error::Database)?;

        match row {
            None => Ok(None),
            Some(row) => {
                let location_id: Option<Uuid> = row.get("location_id");
                let device_id: Option<Uuid> = row.get("device_id");
                let location = if let Some(loc_id) = location_id {
                    let loc_row = sqlx::query(
                        r#"SELECT id, ST_Y(point::geometry) as latitude, ST_X(point::geometry) as longitude,
                           horizontal_accuracy_m, altitude_m, vertical_accuracy_m, heading_degrees,
                           speed_mps, named_location_id, source, confidence
                           FROM prov_location WHERE id = $1"#,
                    )
                    .bind(loc_id)
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(Error::Database)?;
                    if let Some(loc_row) = loc_row {
                        let named_location_id: Option<Uuid> = loc_row.get("named_location_id");
                        let named_location_name = if let Some(nl_id) = named_location_id {
                            let nl_row =
                                sqlx::query("SELECT name FROM named_location WHERE id = $1")
                                    .bind(nl_id)
                                    .fetch_optional(&mut **tx)
                                    .await
                                    .map_err(Error::Database)?;
                            nl_row.map(|r| r.get("name"))
                        } else {
                            None
                        };
                        Some(MemoryLocation {
                            id: loc_row.get("id"),
                            latitude: loc_row.get("latitude"),
                            longitude: loc_row.get("longitude"),
                            horizontal_accuracy_m: loc_row.get("horizontal_accuracy_m"),
                            altitude_m: loc_row.get("altitude_m"),
                            vertical_accuracy_m: loc_row.get("vertical_accuracy_m"),
                            heading_degrees: loc_row.get("heading_degrees"),
                            speed_mps: loc_row.get("speed_mps"),
                            named_location_id,
                            named_location_name,
                            source: loc_row.get("source"),
                            confidence: loc_row.get("confidence"),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };
                let device = if let Some(dev_id) = device_id {
                    let dev_row = sqlx::query(
                        r#"SELECT id, device_make, device_model, device_os, device_os_version,
                           software, software_version, device_name
                           FROM prov_agent_device WHERE id = $1"#,
                    )
                    .bind(dev_id)
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(Error::Database)?;
                    dev_row.map(|r| MemoryDevice {
                        id: r.get("id"),
                        device_make: r.get("device_make"),
                        device_model: r.get("device_model"),
                        device_os: r.get("device_os"),
                        device_os_version: r.get("device_os_version"),
                        software: r.get("software"),
                        software_version: r.get("software_version"),
                        device_name: r.get("device_name"),
                    })
                } else {
                    None
                };
                Ok(Some(ProvenanceRecord {
                    id: row.get("id"),
                    attachment_id: None,
                    note_id: row.get("note_id"),
                    capture_time_start: row.get("capture_time_start"),
                    capture_time_end: row.get("capture_time_end"),
                    capture_timezone: row.get("capture_timezone"),
                    capture_duration_seconds: row.get("capture_duration_seconds"),
                    time_source: row.get("time_source"),
                    time_confidence: row.get("time_confidence"),
                    location,
                    device,
                    event_type: row.get("event_type"),
                    event_title: row.get("event_title"),
                    event_description: row.get("event_description"),
                    user_corrected: row.get("user_corrected"),
                    created_at: row.get("created_at"),
                }))
            }
        }
    }
}

/// Repository trait for memory search operations.
pub trait MemorySearchRepository {
    /// Search for memories near a geographic location.
    fn search_by_location(
        &self,
        lat: f64,
        lon: f64,
        radius_meters: f64,
    ) -> impl std::future::Future<Output = Result<Vec<MemoryLocationResult>>> + Send;

    /// Search for memories captured within a time range.
    fn search_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> impl std::future::Future<Output = Result<Vec<MemoryTimeResult>>> + Send;

    /// Search for memories by both location and time.
    fn search_by_location_and_time(
        &self,
        lat: f64,
        lon: f64,
        radius_meters: f64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> impl std::future::Future<Output = Result<Vec<MemoryLocationResult>>> + Send;

    /// Get the complete provenance chain for a note.
    fn get_memory_provenance(
        &self,
        note_id: Uuid,
    ) -> impl std::future::Future<Output = Result<Option<MemoryProvenance>>> + Send;
}

impl MemorySearchRepository for PgMemorySearchRepository {
    fn search_by_location(
        &self,
        lat: f64,
        lon: f64,
        radius_meters: f64,
    ) -> impl std::future::Future<Output = Result<Vec<MemoryLocationResult>>> + Send {
        self.search_by_location(lat, lon, radius_meters)
    }

    fn search_by_timerange(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> impl std::future::Future<Output = Result<Vec<MemoryTimeResult>>> + Send {
        self.search_by_timerange(start, end)
    }

    fn search_by_location_and_time(
        &self,
        lat: f64,
        lon: f64,
        radius_meters: f64,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> impl std::future::Future<Output = Result<Vec<MemoryLocationResult>>> + Send {
        self.search_by_location_and_time(lat, lon, radius_meters, start, end)
    }

    fn get_memory_provenance(
        &self,
        note_id: Uuid,
    ) -> impl std::future::Future<Output = Result<Option<MemoryProvenance>>> + Send {
        self.get_memory_provenance(note_id)
    }
}
