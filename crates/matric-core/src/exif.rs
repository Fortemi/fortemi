//! EXIF metadata extraction for image files.
//!
//! Extracts temporal and spatial metadata from images to support W3C PROV
//! provenance tracking. Supports JPEG, PNG, HEIF/HEIC, TIFF, and WebP formats.
//!
//! Key capabilities:
//! - DateTime extraction (original capture time)
//! - GPS coordinates (latitude/longitude/altitude)
//! - Camera device information (make/model)
//! - Image orientation and dimensions
//! - PostGIS-compatible geographic point conversion

use crate::{Error, Result};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use std::io::Cursor;

/// EXIF metadata extracted from an image file
#[derive(Debug, Clone, PartialEq)]
pub struct ExifMetadata {
    /// Original capture date/time from EXIF DateTimeOriginal or DateTimeDigitized
    pub datetime: Option<DateTime<Utc>>,

    /// GPS coordinates (if available)
    pub gps: Option<GpsCoordinates>,

    /// Camera device information
    pub device: Option<DeviceInfo>,

    /// Image orientation (EXIF orientation tag)
    pub orientation: Option<u32>,

    /// Image dimensions (width x height)
    pub dimensions: Option<(u32, u32)>,
}

/// GPS coordinates extracted from EXIF
#[derive(Debug, Clone, PartialEq)]
pub struct GpsCoordinates {
    /// Latitude in decimal degrees (positive = North, negative = South)
    pub latitude: f64,

    /// Longitude in decimal degrees (positive = East, negative = West)
    pub longitude: f64,

    /// Altitude in meters above sea level (if available)
    pub altitude: Option<f64>,
}

impl GpsCoordinates {
    /// Convert GPS coordinates to PostGIS-compatible WKT format
    ///
    /// Returns WKT string for use with ST_SetSRID(ST_GeomFromText(...), 4326)
    ///
    /// # Example
    /// ```
    /// use matric_core::exif::GpsCoordinates;
    ///
    /// let gps = GpsCoordinates {
    ///     latitude: 48.8584,
    ///     longitude: 2.2945,
    ///     altitude: Some(35.0),
    /// };
    ///
    /// assert_eq!(gps.to_wkt(), "POINT(2.2945 48.8584)");
    /// ```
    pub fn to_wkt(&self) -> String {
        format!("POINT({} {})", self.longitude, self.latitude)
    }

    /// Convert GPS coordinates to PostGIS geography constructor
    ///
    /// Returns SQL expression for direct use in queries
    ///
    /// # Example
    /// ```
    /// use matric_core::exif::GpsCoordinates;
    ///
    /// let gps = GpsCoordinates {
    ///     latitude: 48.8584,
    ///     longitude: 2.2945,
    ///     altitude: None,
    /// };
    ///
    /// assert_eq!(
    ///     gps.to_postgis_geography(),
    ///     "ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography"
    /// );
    /// ```
    pub fn to_postgis_geography(&self) -> String {
        format!(
            "ST_SetSRID(ST_MakePoint({}, {}), 4326)::geography",
            self.longitude, self.latitude
        )
    }
}

/// Camera/device information from EXIF
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// Camera/device manufacturer (EXIF Make tag)
    pub make: Option<String>,

    /// Camera/device model (EXIF Model tag)
    pub model: Option<String>,

    /// Software used to process the image
    pub software: Option<String>,
}

/// Extract EXIF metadata from image file bytes
///
/// Supports JPEG, PNG, HEIF/HEIC, TIFF, and WebP formats.
///
/// # Arguments
/// * `data` - Raw image file bytes
///
/// # Returns
/// * `Ok(ExifMetadata)` - Extracted metadata (fields may be None if not present)
/// * `Err(Error::InvalidInput)` - If file format is unsupported or corrupted
///
/// # Example
/// ```no_run
/// use matric_core::exif::extract_exif;
///
/// let image_data = std::fs::read("photo.jpg").unwrap();
/// let metadata = extract_exif(&image_data).unwrap();
///
/// if let Some(gps) = metadata.gps {
///     println!("Captured at: {}, {}", gps.latitude, gps.longitude);
/// }
/// ```
pub fn extract_exif(data: &[u8]) -> Result<ExifMetadata> {
    let mut reader = exif::Reader::new();
    reader.continue_on_error(true);
    let mut cursor = Cursor::new(data);

    let exif = reader
        .read_from_container(&mut cursor)
        .or_else(|e| e.distill_partial_result(|_| {}))
        .map_err(|e| Error::InvalidInput(format!("Failed to read EXIF data: {}", e)))?;

    let mut metadata = ExifMetadata {
        datetime: None,
        gps: None,
        device: None,
        orientation: None,
        dimensions: None,
    };

    // Extract datetime (prefer DateTimeOriginal > DateTimeDigitized > DateTime)
    metadata.datetime = extract_datetime(&exif);

    // Extract GPS coordinates
    metadata.gps = extract_gps(&exif);

    // Extract device info
    metadata.device = extract_device_info(&exif);

    // Extract orientation
    metadata.orientation = extract_u32_field(&exif, exif::Tag::Orientation);

    // Extract dimensions
    metadata.dimensions = extract_dimensions(&exif);

    Ok(metadata)
}

/// Extract datetime from EXIF, trying multiple fields in priority order
fn extract_datetime(exif: &exif::Exif) -> Option<DateTime<Utc>> {
    // Priority order: DateTimeOriginal > DateTimeDigitized > DateTime
    let datetime_tags = [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ];

    for tag in &datetime_tags {
        if let Some(field) = exif.get_field(*tag, exif::In::PRIMARY) {
            if let Some(dt) = parse_exif_datetime(&field.display_value().to_string()) {
                // Try to get timezone offset from OffsetTime tags
                let offset_tag = match *tag {
                    exif::Tag::DateTimeOriginal => exif::Tag::OffsetTimeOriginal,
                    exif::Tag::DateTimeDigitized => exif::Tag::OffsetTimeDigitized,
                    _ => exif::Tag::OffsetTime,
                };

                if let Some(offset_field) = exif.get_field(offset_tag, exif::In::PRIMARY) {
                    let offset_str = offset_field.display_value().to_string();
                    if let Some(offset_dt) = parse_exif_datetime_with_offset(&dt, &offset_str) {
                        return Some(offset_dt);
                    }
                }

                // No timezone info, assume UTC
                return Some(dt);
            }
        }
    }

    None
}

/// Parse EXIF datetime string (format: "YYYY:MM:DD HH:MM:SS")
fn parse_exif_datetime(s: &str) -> Option<DateTime<Utc>> {
    // EXIF datetime format: "YYYY:MM:DD HH:MM:SS"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }

    let date_parts: Vec<&str> = parts[0].split(':').collect();
    let time_parts: Vec<&str> = parts[1].split(':').collect();

    if date_parts.len() != 3 || time_parts.len() != 3 {
        return None;
    }

    let year = date_parts[0].parse::<i32>().ok()?;
    let month = date_parts[1].parse::<u32>().ok()?;
    let day = date_parts[2].parse::<u32>().ok()?;
    let hour = time_parts[0].parse::<u32>().ok()?;
    let minute = time_parts[1].parse::<u32>().ok()?;
    let second = time_parts[2].parse::<u32>().ok()?;

    let naive = NaiveDateTime::parse_from_str(
        &format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, day, hour, minute, second
        ),
        "%Y-%m-%d %H:%M:%S",
    )
    .ok()?;

    Some(Utc.from_utc_datetime(&naive))
}

/// Parse EXIF datetime with timezone offset (format: "+HH:MM" or "-HH:MM")
fn parse_exif_datetime_with_offset(dt: &DateTime<Utc>, offset_str: &str) -> Option<DateTime<Utc>> {
    // Offset format: "+HH:MM" or "-HH:MM"
    let offset_str = offset_str.trim();
    if offset_str.len() != 6 {
        return None;
    }

    let sign = match &offset_str[0..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };

    let hours = offset_str[1..3].parse::<i32>().ok()?;
    let minutes = offset_str[4..6].parse::<i32>().ok()?;
    let offset_seconds = sign * (hours * 3600 + minutes * 60);

    let offset = FixedOffset::east_opt(offset_seconds)?;
    let local_dt = offset.from_utc_datetime(&dt.naive_utc());

    Some(local_dt.with_timezone(&Utc))
}

/// Extract GPS coordinates from EXIF
fn extract_gps(exif: &exif::Exif) -> Option<GpsCoordinates> {
    let lat = extract_gps_coordinate(exif, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef)?;
    let lon = extract_gps_coordinate(exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef)?;

    let altitude = extract_gps_altitude(exif);

    Some(GpsCoordinates {
        latitude: lat,
        longitude: lon,
        altitude,
    })
}

/// Extract a GPS coordinate (latitude or longitude) from EXIF
fn extract_gps_coordinate(
    exif: &exif::Exif,
    coord_tag: exif::Tag,
    ref_tag: exif::Tag,
) -> Option<f64> {
    let coord_field = exif.get_field(coord_tag, exif::In::PRIMARY)?;
    let ref_field = exif.get_field(ref_tag, exif::In::PRIMARY)?;

    // Get the value as Rational slice
    let rationals = match &coord_field.value {
        exif::Value::Rational(r) => r,
        _ => return None,
    };

    // GPS coordinates are stored as [degrees, minutes, seconds]
    if rationals.len() < 3 {
        return None;
    }

    let degrees = rationals[0].to_f64();
    let minutes = rationals[1].to_f64();
    let seconds = rationals[2].to_f64();

    let mut decimal = degrees + minutes / 60.0 + seconds / 3600.0;

    // Apply reference (N/S for latitude, E/W for longitude)
    let ref_str = ref_field.display_value().to_string();
    if ref_str == "S" || ref_str == "W" {
        decimal = -decimal;
    }

    Some(decimal)
}

/// Extract GPS altitude from EXIF
fn extract_gps_altitude(exif: &exif::Exif) -> Option<f64> {
    let alt_field = exif.get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY)?;

    let mut altitude = match &alt_field.value {
        exif::Value::Rational(r) if !r.is_empty() => r[0].to_f64(),
        _ => return None,
    };

    // Check altitude reference (0 = above sea level, 1 = below sea level)
    if let Some(ref_field) = exif.get_field(exif::Tag::GPSAltitudeRef, exif::In::PRIMARY) {
        if let Some(ref_val) = ref_field.value.get_uint(0) {
            if ref_val == 1 {
                altitude = -altitude;
            }
        }
    }

    Some(altitude)
}

/// Extract device information from EXIF
fn extract_device_info(exif: &exif::Exif) -> Option<DeviceInfo> {
    let make = extract_string_field(exif, exif::Tag::Make);
    let model = extract_string_field(exif, exif::Tag::Model);
    let software = extract_string_field(exif, exif::Tag::Software);

    if make.is_none() && model.is_none() && software.is_none() {
        return None;
    }

    Some(DeviceInfo {
        make,
        model,
        software,
    })
}

/// Extract dimensions (width x height) from EXIF
fn extract_dimensions(exif: &exif::Exif) -> Option<(u32, u32)> {
    let width = extract_u32_field(exif, exif::Tag::PixelXDimension)
        .or_else(|| extract_u32_field(exif, exif::Tag::ImageWidth))?;

    let height = extract_u32_field(exif, exif::Tag::PixelYDimension)
        .or_else(|| extract_u32_field(exif, exif::Tag::ImageLength))?;

    Some((width, height))
}

/// Extract a string field from EXIF
fn extract_string_field(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    let field = exif.get_field(tag, exif::In::PRIMARY)?;
    let value = field.display_value().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value.trim().to_string())
    }
}

/// Extract a u32 field from EXIF
fn extract_u32_field(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    let field = exif.get_field(tag, exif::In::PRIMARY)?;
    field.value.get_uint(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_extract_exif_invalid_data() {
        let result = extract_exif(b"not an image");
        assert!(result.is_err());
        match result {
            Err(Error::InvalidInput(msg)) => {
                assert!(msg.contains("Failed to read EXIF data"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_extract_exif_empty_data() {
        let result = extract_exif(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_gps_to_wkt() {
        let gps = GpsCoordinates {
            latitude: 48.8584,
            longitude: 2.2945,
            altitude: Some(35.0),
        };

        assert_eq!(gps.to_wkt(), "POINT(2.2945 48.8584)");
    }

    #[test]
    fn test_gps_to_wkt_negative_coords() {
        let gps = GpsCoordinates {
            latitude: -33.8688,
            longitude: 151.2093,
            altitude: None,
        };

        assert_eq!(gps.to_wkt(), "POINT(151.2093 -33.8688)");
    }

    #[test]
    fn test_gps_to_postgis_geography() {
        let gps = GpsCoordinates {
            latitude: 48.8584,
            longitude: 2.2945,
            altitude: None,
        };

        assert_eq!(
            gps.to_postgis_geography(),
            "ST_SetSRID(ST_MakePoint(2.2945, 48.8584), 4326)::geography"
        );
    }

    #[test]
    fn test_parse_exif_datetime_valid() {
        let dt = parse_exif_datetime("2024:01:15 14:30:45");
        assert!(dt.is_some());

        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 45);
    }

    #[test]
    fn test_parse_exif_datetime_invalid_format() {
        assert!(parse_exif_datetime("2024-01-15 14:30:45").is_none());
        assert!(parse_exif_datetime("invalid").is_none());
        assert!(parse_exif_datetime("").is_none());
    }

    #[test]
    fn test_parse_exif_datetime_invalid_values() {
        assert!(parse_exif_datetime("2024:13:15 14:30:45").is_none()); // Invalid month
        assert!(parse_exif_datetime("2024:01:32 14:30:45").is_none()); // Invalid day
        assert!(parse_exif_datetime("2024:01:15 25:30:45").is_none()); // Invalid hour
    }

    #[test]
    fn test_gps_coordinates_equality() {
        let gps1 = GpsCoordinates {
            latitude: 48.8584,
            longitude: 2.2945,
            altitude: Some(35.0),
        };

        let gps2 = GpsCoordinates {
            latitude: 48.8584,
            longitude: 2.2945,
            altitude: Some(35.0),
        };

        assert_eq!(gps1, gps2);
    }

    #[test]
    fn test_device_info_equality() {
        let dev1 = DeviceInfo {
            make: Some("Apple".to_string()),
            model: Some("iPhone 15 Pro".to_string()),
            software: None,
        };

        let dev2 = DeviceInfo {
            make: Some("Apple".to_string()),
            model: Some("iPhone 15 Pro".to_string()),
            software: None,
        };

        assert_eq!(dev1, dev2);
    }

    #[test]
    fn test_exif_metadata_debug() {
        let metadata = ExifMetadata {
            datetime: None,
            gps: None,
            device: None,
            orientation: None,
            dimensions: None,
        };

        let debug_str = format!("{:?}", metadata);
        assert!(debug_str.contains("ExifMetadata"));
    }

    #[test]
    fn test_gps_coordinates_clone() {
        let gps = GpsCoordinates {
            latitude: 48.8584,
            longitude: 2.2945,
            altitude: Some(35.0),
        };

        let cloned = gps.clone();
        assert_eq!(gps, cloned);
    }

    #[test]
    fn test_gps_to_wkt_with_high_precision() {
        let gps = GpsCoordinates {
            latitude: 48.858370,
            longitude: 2.294481,
            altitude: Some(35.123456),
        };

        let wkt = gps.to_wkt();
        assert!(wkt.contains("2.294481"));
        assert!(wkt.contains("48.85837"));
    }

    #[test]
    fn test_gps_no_altitude() {
        let gps = GpsCoordinates {
            latitude: 40.7128,
            longitude: -74.0060,
            altitude: None,
        };

        // Should still work without altitude
        assert_eq!(gps.to_wkt(), "POINT(-74.006 40.7128)");
    }

    #[test]
    fn test_device_info_partial_data() {
        let dev = DeviceInfo {
            make: Some("Canon".to_string()),
            model: None,
            software: None,
        };

        assert!(dev.make.is_some());
        assert!(dev.model.is_none());
    }

    #[test]
    fn test_exif_metadata_with_dimensions() {
        let metadata = ExifMetadata {
            datetime: None,
            gps: None,
            device: None,
            orientation: Some(6),
            dimensions: Some((4032, 3024)),
        };

        assert_eq!(metadata.orientation, Some(6));
        assert_eq!(metadata.dimensions, Some((4032, 3024)));
    }

    #[test]
    fn test_parse_exif_datetime_with_offset_positive() {
        let base_dt = Utc::now();
        let result = parse_exif_datetime_with_offset(&base_dt, "+05:30");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_exif_datetime_with_offset_negative() {
        let base_dt = Utc::now();
        let result = parse_exif_datetime_with_offset(&base_dt, "-08:00");
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_exif_datetime_with_offset_invalid() {
        let base_dt = Utc::now();
        assert!(parse_exif_datetime_with_offset(&base_dt, "invalid").is_none());
        assert!(parse_exif_datetime_with_offset(&base_dt, "+5:30").is_none());
        assert!(parse_exif_datetime_with_offset(&base_dt, "05:30").is_none());
    }

    #[test]
    fn test_gps_coordinates_extreme_values() {
        // North pole
        let north = GpsCoordinates {
            latitude: 90.0,
            longitude: 0.0,
            altitude: None,
        };
        assert_eq!(north.to_wkt(), "POINT(0 90)");

        // South pole
        let south = GpsCoordinates {
            latitude: -90.0,
            longitude: 0.0,
            altitude: None,
        };
        assert_eq!(south.to_wkt(), "POINT(0 -90)");

        // International date line
        let idl_east = GpsCoordinates {
            latitude: 0.0,
            longitude: 180.0,
            altitude: None,
        };
        assert_eq!(idl_east.to_wkt(), "POINT(180 0)");

        let idl_west = GpsCoordinates {
            latitude: 0.0,
            longitude: -180.0,
            altitude: None,
        };
        assert_eq!(idl_west.to_wkt(), "POINT(-180 0)");
    }
}
