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
    Error, FileProvenanceRecord, MemoryDevice, MemoryLocation, MemoryLocationResult,
    MemoryProvenance, MemoryTimeResult, Result,
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
            SELECT
                fp.id as provenance_id,
                fp.attachment_id,
                a.note_id,
                a.filename,
                ab.content_type,
                ST_Distance(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                ) as distance_m,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                nl.name as location_name,
                fp.event_type
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            JOIN attachment_blob ab ON a.blob_id = ab.id
            JOIN prov_location pl ON fp.location_id = pl.id
            LEFT JOIN named_location nl ON pl.named_location_id = nl.id
            WHERE ST_DWithin(
                pl.point,
                ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                $3
            )
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
            SELECT
                fp.id as provenance_id,
                fp.attachment_id,
                a.note_id,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                fp.event_type,
                nl.name as location_name
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            LEFT JOIN prov_location pl ON fp.location_id = pl.id
            LEFT JOIN named_location nl ON pl.named_location_id = nl.id
            WHERE fp.capture_time && tstzrange($1, $2)
            ORDER BY lower(fp.capture_time)
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
            SELECT
                fp.id as provenance_id,
                fp.attachment_id,
                a.note_id,
                a.filename,
                ab.content_type,
                ST_Distance(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                ) as distance_m,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                nl.name as location_name,
                fp.event_type
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            JOIN attachment_blob ab ON a.blob_id = ab.id
            JOIN prov_location pl ON fp.location_id = pl.id
            LEFT JOIN named_location nl ON pl.named_location_id = nl.id
            WHERE ST_DWithin(
                pl.point,
                ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                $3
            )
            AND fp.capture_time && tstzrange($4, $5)
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

    /// Get the complete provenance chain for a note's file attachments.
    ///
    /// Returns detailed provenance information including location, device,
    /// and temporal context for all file attachments linked to the note.
    ///
    /// # Arguments
    ///
    /// * `note_id` - The note ID to retrieve provenance for
    ///
    /// # Returns
    ///
    /// `Some(MemoryProvenance)` if the note has file provenance, `None` otherwise.
    pub async fn get_memory_provenance(&self, note_id: Uuid) -> Result<Option<MemoryProvenance>> {
        // Get all file provenance records for attachments of this note
        let rows = sqlx::query(
            r#"
            SELECT
                fp.id,
                fp.attachment_id,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                fp.capture_timezone,
                fp.capture_duration_seconds,
                fp.time_source,
                fp.time_confidence,
                fp.event_type,
                fp.event_title,
                fp.event_description,
                fp.user_corrected,
                fp.created_at,
                fp.location_id,
                fp.device_id
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            WHERE a.note_id = $1
            ORDER BY fp.created_at DESC
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

        for row in rows {
            let location_id: Option<Uuid> = row.get("location_id");
            let device_id: Option<Uuid> = row.get("device_id");

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

            files.push(FileProvenanceRecord {
                id: row.get("id"),
                attachment_id: row.get("attachment_id"),
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
            });
        }

        Ok(Some(MemoryProvenance { note_id, files }))
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
            SELECT
                fp.id as provenance_id,
                fp.attachment_id,
                a.note_id,
                a.filename,
                ab.content_type,
                ST_Distance(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                ) as distance_m,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                nl.name as location_name,
                fp.event_type
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            JOIN attachment_blob ab ON a.blob_id = ab.id
            JOIN prov_location pl ON fp.location_id = pl.id
            LEFT JOIN named_location nl ON pl.named_location_id = nl.id
            WHERE ST_DWithin(
                pl.point,
                ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                $3
            )
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
            SELECT
                fp.id as provenance_id,
                fp.attachment_id,
                a.note_id,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                fp.event_type,
                nl.name as location_name
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            LEFT JOIN prov_location pl ON fp.location_id = pl.id
            LEFT JOIN named_location nl ON pl.named_location_id = nl.id
            WHERE fp.capture_time && tstzrange($1, $2)
            ORDER BY lower(fp.capture_time)
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
            SELECT
                fp.id as provenance_id,
                fp.attachment_id,
                a.note_id,
                a.filename,
                ab.content_type,
                ST_Distance(
                    pl.point,
                    ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography
                ) as distance_m,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                nl.name as location_name,
                fp.event_type
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            JOIN attachment_blob ab ON a.blob_id = ab.id
            JOIN prov_location pl ON fp.location_id = pl.id
            LEFT JOIN named_location nl ON pl.named_location_id = nl.id
            WHERE ST_DWithin(
                pl.point,
                ST_SetSRID(ST_MakePoint($2, $1), 4326)::geography,
                $3
            )
            AND fp.capture_time && tstzrange($4, $5)
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
        // Get all file provenance records for attachments of this note
        let rows = sqlx::query(
            r#"
            SELECT
                fp.id,
                fp.attachment_id,
                lower(fp.capture_time) as capture_time_start,
                upper(fp.capture_time) as capture_time_end,
                fp.capture_timezone,
                fp.capture_duration_seconds,
                fp.time_source,
                fp.time_confidence,
                fp.event_type,
                fp.event_title,
                fp.event_description,
                fp.user_corrected,
                fp.created_at,
                fp.location_id,
                fp.device_id
            FROM file_provenance fp
            JOIN attachment a ON fp.attachment_id = a.id
            WHERE a.note_id = $1
            ORDER BY fp.created_at DESC
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

        for row in rows {
            let location_id: Option<Uuid> = row.get("location_id");
            let device_id: Option<Uuid> = row.get("device_id");

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

            files.push(FileProvenanceRecord {
                id: row.get("id"),
                attachment_id: row.get("attachment_id"),
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
            });
        }

        Ok(Some(MemoryProvenance { note_id, files }))
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
