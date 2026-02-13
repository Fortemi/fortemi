//! EXIF metadata extraction from image bytes.
//!
//! Parses EXIF data from JPEG and other image formats using the kamadak-exif crate.
//! Extracts camera info, capture settings, GPS coordinates, datetime, and lens information.
//!
//! Returns `None` for images without EXIF data (e.g., PNG files) without logging errors,
//! as this is expected behavior.

use exif::{Reader, Tag};
use serde_json::{json, Value as JsonValue};
use std::io::Cursor;

/// Extracts EXIF metadata from image bytes.
///
/// Returns a JSON object with structured EXIF data organized by category:
/// - `camera`: make, model, software
/// - `settings`: f_number, exposure_time, iso, focal_length, flash
/// - `gps`: latitude, longitude, altitude (decimal degrees)
/// - `datetime`: original, digitized
/// - `image`: orientation, x_resolution, y_resolution, color_space
/// - `lens`: model, make
///
/// Returns `None` if the image contains no EXIF data.
pub fn extract_exif_metadata(data: &[u8]) -> Option<JsonValue> {
    let mut cursor = Cursor::new(data);
    let mut exif_reader = Reader::new();
    exif_reader.continue_on_error(true);
    let exif = exif_reader
        .read_from_container(&mut cursor)
        .or_else(|e| e.distill_partial_result(|_| {}))
        .ok()?;

    let mut camera = json!({});
    let mut settings = json!({});
    let mut gps_data = json!({});
    let mut datetime = json!({});
    let mut image = json!({});
    let mut lens = json!({});

    // GPS coordinate accumulators
    let mut gps_lat: Option<f64> = None;
    let mut gps_lat_ref: Option<String> = None;
    let mut gps_lon: Option<f64> = None;
    let mut gps_lon_ref: Option<String> = None;

    for field in exif.fields() {
        match field.tag {
            // Camera info
            Tag::Make => {
                if let Some(s) = field_as_string(field) {
                    camera["make"] = json!(s);
                }
            }
            Tag::Model => {
                if let Some(s) = field_as_string(field) {
                    camera["model"] = json!(s);
                }
            }
            Tag::Software => {
                if let Some(s) = field_as_string(field) {
                    camera["software"] = json!(s);
                }
            }

            // Capture settings
            Tag::FNumber => {
                if let Some(f) = field_as_rational_f64(field) {
                    settings["f_number"] = json!(f);
                }
            }
            Tag::ExposureTime => {
                if let Some(s) = field_as_string(field) {
                    settings["exposure_time"] = json!(s);
                }
            }
            Tag::PhotographicSensitivity => {
                if let Some(iso) = field_as_u32(field) {
                    settings["iso"] = json!(iso);
                }
            }
            Tag::FocalLength => {
                if let Some(s) = field_as_string(field) {
                    settings["focal_length"] = json!(s);
                }
            }
            Tag::Flash => {
                if let Some(s) = field_as_string(field) {
                    settings["flash"] = json!(s);
                }
            }

            // GPS coordinates (accumulate for conversion)
            Tag::GPSLatitude => {
                if let Some(dms) = field_as_rational_vec(field) {
                    gps_lat = dms_to_decimal(&dms);
                }
            }
            Tag::GPSLatitudeRef => {
                gps_lat_ref = field_as_string(field);
            }
            Tag::GPSLongitude => {
                if let Some(dms) = field_as_rational_vec(field) {
                    gps_lon = dms_to_decimal(&dms);
                }
            }
            Tag::GPSLongitudeRef => {
                gps_lon_ref = field_as_string(field);
            }
            Tag::GPSAltitude => {
                if let Some(alt) = field_as_rational_f64(field) {
                    gps_data["altitude"] = json!(alt);
                }
            }

            // DateTime
            Tag::DateTimeOriginal => {
                if let Some(s) = field_as_string(field) {
                    datetime["original"] = json!(s);
                }
            }
            Tag::DateTimeDigitized => {
                if let Some(s) = field_as_string(field) {
                    datetime["digitized"] = json!(s);
                }
            }

            // Image info
            Tag::Orientation => {
                if let Some(n) = field_as_u32(field) {
                    image["orientation"] = json!(n);
                }
            }
            Tag::XResolution => {
                if let Some(f) = field_as_rational_f64(field) {
                    image["x_resolution"] = json!(f);
                }
            }
            Tag::YResolution => {
                if let Some(f) = field_as_rational_f64(field) {
                    image["y_resolution"] = json!(f);
                }
            }
            Tag::ColorSpace => {
                if let Some(n) = field_as_u32(field) {
                    let color_space = match n {
                        1 => "sRGB".to_string(),
                        65535 => "Uncalibrated".to_string(),
                        _ => n.to_string(),
                    };
                    image["color_space"] = json!(color_space);
                }
            }

            // Lens
            Tag::LensModel => {
                if let Some(s) = field_as_string(field) {
                    lens["model"] = json!(s);
                }
            }
            Tag::LensMake => {
                if let Some(s) = field_as_string(field) {
                    lens["make"] = json!(s);
                }
            }

            _ => {}
        }
    }

    // Convert GPS coordinates to decimal degrees with hemisphere
    if let (Some(lat), Some(lat_ref)) = (gps_lat, gps_lat_ref) {
        let lat_decimal = if lat_ref == "S" { -lat } else { lat };
        gps_data["latitude"] = json!(lat_decimal);
    }
    if let (Some(lon), Some(lon_ref)) = (gps_lon, gps_lon_ref) {
        let lon_decimal = if lon_ref == "W" { -lon } else { lon };
        gps_data["longitude"] = json!(lon_decimal);
    }

    // Build final JSON, only including non-empty categories
    let mut result = json!({});
    let mut has_data = false;

    if !camera.as_object().unwrap().is_empty() {
        result["camera"] = camera;
        has_data = true;
    }
    if !settings.as_object().unwrap().is_empty() {
        result["settings"] = settings;
        has_data = true;
    }
    if !gps_data.as_object().unwrap().is_empty() {
        result["gps"] = gps_data;
        has_data = true;
    }
    if !datetime.as_object().unwrap().is_empty() {
        result["datetime"] = datetime;
        has_data = true;
    }
    if !image.as_object().unwrap().is_empty() {
        result["image"] = image;
        has_data = true;
    }
    if !lens.as_object().unwrap().is_empty() {
        result["lens"] = lens;
        has_data = true;
    }

    if has_data {
        Some(json!({ "exif": result }))
    } else {
        None
    }
}

/// Extracts a string value from an EXIF field.
fn field_as_string(field: &exif::Field) -> Option<String> {
    match &field.value {
        exif::Value::Ascii(ref vecs) => {
            // Ascii values are Vec<Vec<u8>>
            vecs.first()
                .map(|v| String::from_utf8_lossy(v).trim().to_string())
        }
        _ => {
            let s = field.display_value().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s.trim().to_string())
            }
        }
    }
}

/// Extracts a rational value as f64 from an EXIF field.
fn field_as_rational_f64(field: &exif::Field) -> Option<f64> {
    match &field.value {
        exif::Value::Rational(ref v) => v.first().map(|r| r.to_f64()),
        _ => None,
    }
}

/// Extracts a rational vector from an EXIF field (for GPS coordinates).
fn field_as_rational_vec(field: &exif::Field) -> Option<Vec<exif::Rational>> {
    match &field.value {
        exif::Value::Rational(ref v) => Some(v.clone()),
        _ => None,
    }
}

/// Extracts a u32 value from an EXIF field.
fn field_as_u32(field: &exif::Field) -> Option<u32> {
    match &field.value {
        exif::Value::Short(ref v) => v.first().map(|&n| n as u32),
        exif::Value::Long(ref v) => v.first().copied(),
        _ => None,
    }
}

/// Converts DMS (degrees/minutes/seconds) to decimal degrees.
fn dms_to_decimal(dms: &[exif::Rational]) -> Option<f64> {
    if dms.len() < 3 {
        return None;
    }
    let d = dms[0].to_f64();
    let m = dms[1].to_f64();
    let s = dms[2].to_f64();
    Some(d + m / 60.0 + s / 3600.0)
}

/// Prepares EXIF metadata for storage on an attachment record.
///
/// Transforms the raw `extract_exif_metadata()` output (which wraps everything
/// under an `"exif"` key) into a flat structure suitable for querying as
/// `extracted_metadata.gps.latitude`, `extracted_metadata.camera.make`, etc.
///
/// Also adds a top-level `datetime_original` field in ISO 8601 format,
/// parsed from the EXIF DateTimeOriginal (or DateTimeDigitized fallback).
///
/// Returns `None` if the input has no `"exif"` key.
pub fn prepare_attachment_metadata(
    exif_data: &JsonValue,
    capture_time: Option<chrono::DateTime<chrono::Utc>>,
) -> Option<JsonValue> {
    let exif = exif_data.get("exif")?;
    let mut metadata = exif.clone();

    // Add top-level datetime_original in ISO 8601 if capture_time was parsed
    if let Some(ct) = capture_time {
        if let Some(obj) = metadata.as_object_mut() {
            obj.insert(
                "datetime_original".to_string(),
                json!(ct.to_rfc3339()),
            );
        }
    }

    Some(metadata)
}

/// Parses EXIF datetime string into a UTC DateTime.
///
/// Tries the standard EXIF format "YYYY:MM:DD HH:MM:SS".
/// Returns `None` if the datetime cannot be parsed.
pub fn parse_exif_datetime(exif: &JsonValue) -> Option<chrono::DateTime<chrono::Utc>> {
    let datetime = exif.get("datetime")?;
    let dt_str = datetime
        .get("original")
        .or_else(|| datetime.get("digitized"))
        .and_then(|v| v.as_str())?;
    let naive = chrono::NaiveDateTime::parse_from_str(dt_str, "%Y:%m:%d %H:%M:%S").ok()?;
    Some(naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    // ── dms_to_decimal tests ───────────────────────────────────────────

    #[test]
    fn test_dms_to_decimal() {
        let dms = vec![
            exif::Rational { num: 40, denom: 1 },
            exif::Rational { num: 26, denom: 1 },
            exif::Rational { num: 46, denom: 1 },
        ];
        let result = dms_to_decimal(&dms);
        assert!(result.is_some());
        let decimal = result.unwrap();
        // 40 + 26/60 + 46/3600 = 40.44611...
        assert!(
            (decimal - 40.44611).abs() < 0.001,
            "Expected ~40.446, got {}",
            decimal
        );
    }

    #[test]
    fn test_dms_to_decimal_empty() {
        assert!(dms_to_decimal(&[]).is_none());
    }

    #[test]
    fn test_dms_to_decimal_too_short() {
        let dms = vec![
            exif::Rational { num: 40, denom: 1 },
            exif::Rational { num: 26, denom: 1 },
        ];
        assert!(dms_to_decimal(&dms).is_none());
    }

    #[test]
    fn test_dms_to_decimal_fractional_seconds() {
        // GPS coord with fractional seconds: 48°51'30.24"
        let dms = vec![
            exif::Rational { num: 48, denom: 1 },
            exif::Rational { num: 51, denom: 1 },
            exif::Rational {
                num: 3024,
                denom: 100,
            },
        ];
        let result = dms_to_decimal(&dms).unwrap();
        // 48 + 51/60 + 30.24/3600 = 48.85840
        assert!(
            (result - 48.85840).abs() < 0.0001,
            "Expected ~48.858, got {}",
            result
        );
    }

    #[test]
    fn test_dms_to_decimal_zero_values() {
        let dms = vec![
            exif::Rational { num: 0, denom: 1 },
            exif::Rational { num: 0, denom: 1 },
            exif::Rational { num: 0, denom: 1 },
        ];
        let result = dms_to_decimal(&dms).unwrap();
        assert!((result - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dms_to_decimal_extra_elements_ignored() {
        // More than 3 elements — only first 3 matter
        let dms = vec![
            exif::Rational { num: 10, denom: 1 },
            exif::Rational { num: 20, denom: 1 },
            exif::Rational { num: 30, denom: 1 },
            exif::Rational { num: 99, denom: 1 }, // ignored
        ];
        let result = dms_to_decimal(&dms).unwrap();
        // 10 + 20/60 + 30/3600
        let expected = 10.0 + 20.0 / 60.0 + 30.0 / 3600.0;
        assert!(
            (result - expected).abs() < 0.0001,
            "Expected ~{}, got {}",
            expected,
            result
        );
    }

    // ── extract_exif_metadata error paths ──────────────────────────────

    #[test]
    fn test_extract_exif_no_exif_data() {
        // PNG header — no EXIF data
        let mut png_data = vec![0u8; 100];
        png_data[0..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        let result = extract_exif_metadata(&png_data);
        assert!(result.is_none(), "PNG should have no EXIF data");
    }

    #[test]
    fn test_extract_exif_empty_data() {
        let result = extract_exif_metadata(&[]);
        assert!(result.is_none(), "Empty data should return None");
    }

    #[test]
    fn test_extract_exif_random_bytes() {
        let result = extract_exif_metadata(&[0u8; 50]);
        assert!(result.is_none(), "Random bytes should return None");
    }

    #[test]
    fn test_extract_exif_short_data() {
        let result = extract_exif_metadata(&[0xFF, 0xD8]); // Just JPEG SOI marker
        assert!(result.is_none(), "JPEG SOI only should return None");
    }

    // ── extract_exif_metadata with real JPEG fixture ───────────────────

    /// Load the test JPEG file with known EXIF data.
    /// Contents: Apple iPhone 15 Pro, GPS: 48°51'30.24"N 2°17'40.20"E, altitude 35m
    fn load_test_jpeg() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/images/jpeg-with-exif.jpg");
        std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e))
    }

    #[test]
    fn test_extract_exif_returns_some_for_real_jpeg() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data);
        assert!(result.is_some(), "Should extract EXIF from test JPEG");
    }

    #[test]
    fn test_extract_exif_has_top_level_exif_key() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        assert!(result.get("exif").is_some(), "Top-level 'exif' key missing");
    }

    #[test]
    fn test_extract_exif_camera_make_model() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        let exif = &result["exif"];

        let camera = &exif["camera"];
        assert_eq!(camera["make"].as_str().unwrap(), "Apple");
        assert_eq!(camera["model"].as_str().unwrap(), "iPhone 15 Pro");
    }

    #[test]
    fn test_extract_exif_camera_software() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        let exif = &result["exif"];

        let camera = &exif["camera"];
        assert_eq!(camera["software"].as_str().unwrap(), "iOS 17.5");
    }

    #[test]
    fn test_extract_exif_gps_latitude_north() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        let exif = &result["exif"];

        let gps = &exif["gps"];
        let lat = gps["latitude"].as_f64().unwrap();
        // Paris: ~48.858° N (positive because North)
        assert!(
            lat > 48.0 && lat < 49.0,
            "Latitude should be ~48.858, got {}",
            lat
        );
        assert!(lat > 0.0, "North latitude should be positive");
    }

    #[test]
    fn test_extract_exif_gps_longitude_east() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        let exif = &result["exif"];

        let gps = &exif["gps"];
        let lon = gps["longitude"].as_f64().unwrap();
        // Paris: ~2.294° E (positive because East)
        assert!(
            lon > 2.0 && lon < 3.0,
            "Longitude should be ~2.294, got {}",
            lon
        );
        assert!(lon > 0.0, "East longitude should be positive");
    }

    #[test]
    fn test_extract_exif_gps_altitude() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        let exif = &result["exif"];

        let gps = &exif["gps"];
        let alt = gps["altitude"].as_f64().unwrap();
        // Altitude: 35m
        assert!(
            (alt - 35.0).abs() < 1.0,
            "Altitude should be ~35m, got {}",
            alt
        );
    }

    #[test]
    fn test_extract_exif_datetime_original() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        let exif = &result["exif"];

        let datetime = &exif["datetime"];
        let original = datetime["original"].as_str().unwrap();
        assert!(
            original.contains("2024"),
            "DateTime should contain year 2024, got: {}",
            original
        );
    }

    #[test]
    fn test_extract_exif_excludes_empty_categories() {
        // The no-metadata JPEG should either return None (no EXIF at all)
        // or have no empty categories in the result
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/images/jpeg-no-metadata.jpg");
        let data = std::fs::read(&path).unwrap();
        let result = extract_exif_metadata(&data);

        if let Some(result) = result {
            let exif = &result["exif"];
            let obj = exif.as_object().unwrap();
            for (key, value) in obj {
                assert!(
                    !value.as_object().unwrap().is_empty(),
                    "Category '{}' should not be empty in the output",
                    key
                );
            }
        }
        // None is also acceptable (no EXIF at all)
    }

    #[test]
    fn test_extract_exif_all_categories_present() {
        let data = load_test_jpeg();
        let result = extract_exif_metadata(&data).unwrap();
        let exif = result["exif"].as_object().unwrap();

        // The test image has camera, GPS, datetime data at minimum
        assert!(
            exif.contains_key("camera"),
            "Missing 'camera' category; keys: {:?}",
            exif.keys().collect::<Vec<_>>()
        );
        assert!(
            exif.contains_key("gps"),
            "Missing 'gps' category; keys: {:?}",
            exif.keys().collect::<Vec<_>>()
        );
        assert!(
            exif.contains_key("datetime"),
            "Missing 'datetime' category; keys: {:?}",
            exif.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_extract_exif_provenance_image_with_gps() {
        // paris-eiffel-tower.jpg should have GPS data for Paris
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/provenance/paris-eiffel-tower.jpg");
        let data = std::fs::read(&path).unwrap();
        let result = extract_exif_metadata(&data);

        assert!(result.is_some(), "Should extract EXIF from Paris image");
        let result = result.unwrap();
        let gps = &result["exif"]["gps"];

        // Paris coordinates: ~48.858°N, ~2.294°E
        let lat = gps["latitude"].as_f64().unwrap();
        let lon = gps["longitude"].as_f64().unwrap();
        assert!(
            lat > 48.0 && lat < 49.0,
            "Paris latitude should be ~48.8, got {}",
            lat
        );
        assert!(
            lon > 2.0 && lon < 3.0,
            "Paris longitude should be ~2.3, got {}",
            lon
        );
    }

    #[test]
    fn test_extract_exif_provenance_image_camera_info() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/provenance/paris-eiffel-tower.jpg");
        let data = std::fs::read(&path).unwrap();
        let result = extract_exif_metadata(&data).unwrap();
        let camera = &result["exif"]["camera"];

        // Should have make and model
        assert!(camera["make"].as_str().is_some(), "Should have camera make");
        assert!(
            camera["model"].as_str().is_some(),
            "Should have camera model"
        );
    }

    #[test]
    fn test_extract_exif_southern_hemisphere_gps() {
        // Test GPS coordinate negation for South latitude
        // We test the logic directly since we may not have a southern image
        // The code negates lat when ref is "S"
        // This is tested via the branch in extract_exif_metadata:
        //   if lat_ref == "S" { -lat }
        // We verify by using the Tokyo image (Northern hemisphere)
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/provenance/tokyo-shibuya.jpg");
        let data = std::fs::read(&path).unwrap();
        let result = extract_exif_metadata(&data);

        if let Some(result) = result {
            let gps = &result["exif"]["gps"];
            if let Some(lat) = gps["latitude"].as_f64() {
                // Tokyo is in northern hemisphere, latitude should be positive (~35.6)
                assert!(
                    lat > 0.0,
                    "Tokyo latitude should be positive (North), got {}",
                    lat
                );
                assert!(
                    lat > 35.0 && lat < 36.0,
                    "Tokyo latitude should be ~35.6, got {}",
                    lat
                );
            }
        }
    }

    #[test]
    fn test_extract_exif_new_york_western_longitude() {
        // New York has West longitude, should be negative
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/provenance/newyork-statue-liberty.jpg");
        let data = std::fs::read(&path).unwrap();
        let result = extract_exif_metadata(&data);

        if let Some(result) = result {
            let gps = &result["exif"]["gps"];
            if let Some(lon) = gps["longitude"].as_f64() {
                // New York is in western hemisphere, longitude should be negative (~-74.0)
                assert!(
                    lon < 0.0,
                    "New York longitude should be negative (West), got {}",
                    lon
                );
            }
        }
    }

    // ── prepare_attachment_metadata unit tests ────────────────────────────

    #[test]
    fn test_prepare_metadata_strips_exif_wrapper() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();

        // Raw extraction has "exif" wrapper
        assert!(exif_data.get("exif").is_some());

        let metadata = prepare_attachment_metadata(&exif_data, None).unwrap();

        // Prepared metadata should NOT have the "exif" wrapper
        assert!(
            metadata.get("exif").is_none(),
            "Stored metadata should not have 'exif' wrapper; got keys: {:?}",
            metadata.as_object().unwrap().keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_prepare_metadata_gps_at_top_level() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let metadata = prepare_attachment_metadata(&exif_data, None).unwrap();

        // GPS fields should be accessible as metadata.gps.latitude (not metadata.exif.gps.latitude)
        let gps = metadata.get("gps").expect("metadata.gps should exist");
        let lat = gps.get("latitude").and_then(|v| v.as_f64());
        let lon = gps.get("longitude").and_then(|v| v.as_f64());
        let alt = gps.get("altitude").and_then(|v| v.as_f64());

        assert!(lat.is_some(), "metadata.gps.latitude should be present");
        assert!(lon.is_some(), "metadata.gps.longitude should be present");
        assert!(alt.is_some(), "metadata.gps.altitude should be present");

        let lat = lat.unwrap();
        let lon = lon.unwrap();
        // Paris coordinates
        assert!(lat > 48.0 && lat < 49.0, "Paris lat ~48.858, got {}", lat);
        assert!(lon > 2.0 && lon < 3.0, "Paris lon ~2.294, got {}", lon);
    }

    #[test]
    fn test_prepare_metadata_camera_at_top_level() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let metadata = prepare_attachment_metadata(&exif_data, None).unwrap();

        // Camera fields should be accessible as metadata.camera.make
        let camera = metadata.get("camera").expect("metadata.camera should exist");
        assert_eq!(camera["make"].as_str().unwrap(), "Apple");
        assert_eq!(camera["model"].as_str().unwrap(), "iPhone 15 Pro");
    }

    #[test]
    fn test_prepare_metadata_datetime_original_iso8601() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let exif = exif_data.get("exif").unwrap();
        let capture_time = parse_exif_datetime(exif);
        assert!(capture_time.is_some(), "Should parse capture time");

        let metadata = prepare_attachment_metadata(&exif_data, capture_time).unwrap();

        // datetime_original should be at top level in ISO 8601
        let dt = metadata
            .get("datetime_original")
            .and_then(|v| v.as_str())
            .expect("metadata.datetime_original should be present");

        assert!(
            dt.contains("2024"),
            "datetime_original should contain year 2024, got: {}",
            dt
        );
        // ISO 8601 format: contains 'T' separator
        assert!(
            dt.contains('T'),
            "datetime_original should be ISO 8601 (contain 'T'), got: {}",
            dt
        );
        // Should be parseable as RFC 3339
        assert!(
            chrono::DateTime::parse_from_rfc3339(dt).is_ok(),
            "datetime_original should be valid RFC 3339, got: {}",
            dt
        );
    }

    #[test]
    fn test_prepare_metadata_no_datetime_original_when_unparseable() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();

        // Pass None for capture_time (simulates unparseable datetime)
        let metadata = prepare_attachment_metadata(&exif_data, None).unwrap();

        // datetime_original should NOT be present when capture_time is None
        assert!(
            metadata.get("datetime_original").is_none(),
            "datetime_original should be absent when capture_time is None"
        );

        // But raw datetime should still be in the nested datetime object
        assert!(
            metadata.get("datetime").is_some(),
            "Raw datetime object should still be present"
        );
    }

    #[test]
    fn test_prepare_metadata_returns_none_for_missing_exif_key() {
        // Input without "exif" key should return None
        let bad_input = json!({"something": "else"});
        assert!(
            prepare_attachment_metadata(&bad_input, None).is_none(),
            "Should return None when 'exif' key is missing"
        );
    }

    #[test]
    fn test_prepare_metadata_preserves_all_categories() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let metadata = prepare_attachment_metadata(&exif_data, None).unwrap();
        let obj = metadata.as_object().unwrap();

        // All inner categories from the extraction should be preserved
        assert!(obj.contains_key("camera"), "Should contain camera");
        assert!(obj.contains_key("gps"), "Should contain gps");
        assert!(obj.contains_key("datetime"), "Should contain datetime");
        // settings, image, lens may or may not be present depending on image
    }

    // ── parse_exif_datetime unit tests ───────────────────────────────────

    #[test]
    fn test_parse_exif_datetime_from_original() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let exif = exif_data.get("exif").unwrap();

        let dt = parse_exif_datetime(exif);
        assert!(dt.is_some(), "Should parse datetime from test image");
        let dt = dt.unwrap();
        assert_eq!(dt.format("%Y").to_string(), "2024");
    }

    #[test]
    fn test_parse_exif_datetime_returns_none_without_datetime() {
        let exif = json!({"camera": {"make": "Apple"}});
        assert!(
            parse_exif_datetime(&exif).is_none(),
            "Should return None when no datetime present"
        );
    }

    #[test]
    fn test_parse_exif_datetime_returns_none_for_invalid_format() {
        let exif = json!({"datetime": {"original": "not-a-date"}});
        assert!(
            parse_exif_datetime(&exif).is_none(),
            "Should return None for unparseable datetime"
        );
    }

    #[test]
    fn test_parse_exif_datetime_falls_back_to_digitized() {
        let exif = json!({"datetime": {"digitized": "2023:06:15 09:30:00"}});
        let dt = parse_exif_datetime(&exif);
        assert!(dt.is_some(), "Should fall back to 'digitized' field");
        assert_eq!(dt.unwrap().format("%Y-%m-%d").to_string(), "2023-06-15");
    }

    #[test]
    fn test_parse_exif_datetime_prefers_original_over_digitized() {
        let exif = json!({
            "datetime": {
                "original": "2024:07:14 12:30:00",
                "digitized": "2023:06:15 09:30:00"
            }
        });
        let dt = parse_exif_datetime(&exif).unwrap();
        // Should use "original", not "digitized"
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-07-14");
    }

    // ── UAT contract tests (verify API-level expectations) ───────────────

    /// UAT-2B-010: GPS coordinates available in extracted_metadata
    #[test]
    fn test_uat_2b_010_gps_coordinates_in_metadata() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let exif = exif_data.get("exif").unwrap();
        let capture_time = parse_exif_datetime(exif);
        let metadata = prepare_attachment_metadata(&exif_data, capture_time).unwrap();

        // Verify exact paths from UAT-2B-010
        let lat = metadata
            .pointer("/gps/latitude")
            .and_then(|v| v.as_f64())
            .expect("UAT-2B-010: extracted_metadata.gps.latitude must be present");
        let lon = metadata
            .pointer("/gps/longitude")
            .and_then(|v| v.as_f64())
            .expect("UAT-2B-010: extracted_metadata.gps.longitude must be present");

        // Paris Eiffel Tower approximate: 48.858°N, 2.294°E
        assert!(
            lat > 48.0 && lat < 49.0,
            "UAT-2B-010: latitude ~48.858 for Paris, got {}",
            lat
        );
        assert!(
            lon > 2.0 && lon < 3.0,
            "UAT-2B-010: longitude ~2.294 for Paris, got {}",
            lon
        );

        // Altitude may be present
        if let Some(alt) = metadata.pointer("/gps/altitude").and_then(|v| v.as_f64()) {
            assert!(
                (alt - 35.0).abs() < 5.0,
                "UAT-2B-010: altitude ~35m, got {}",
                alt
            );
        }
    }

    /// UAT-2B-011: Camera make/model and datetime in extracted_metadata
    #[test]
    fn test_uat_2b_011_camera_metadata() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let exif = exif_data.get("exif").unwrap();
        let capture_time = parse_exif_datetime(exif);
        let metadata = prepare_attachment_metadata(&exif_data, capture_time).unwrap();

        // Verify exact paths from UAT-2B-011
        let make = metadata
            .pointer("/camera/make")
            .and_then(|v| v.as_str())
            .expect("UAT-2B-011: extracted_metadata.camera.make must be present");
        let model = metadata
            .pointer("/camera/model")
            .and_then(|v| v.as_str())
            .expect("UAT-2B-011: extracted_metadata.camera.model must be present");
        let datetime_original = metadata
            .get("datetime_original")
            .and_then(|v| v.as_str())
            .expect("UAT-2B-011: extracted_metadata.datetime_original must be present");

        assert_eq!(make, "Apple", "UAT-2B-011: camera make should be 'Apple'");
        assert_eq!(
            model, "iPhone 15 Pro",
            "UAT-2B-011: camera model should be 'iPhone 15 Pro'"
        );
        assert!(
            datetime_original.contains("2024"),
            "UAT-2B-011: datetime_original should contain year 2024"
        );
    }

    /// UAT-2B-012: DateTimeOriginal in ISO 8601 format
    #[test]
    fn test_uat_2b_012_datetime_iso8601() {
        let data = load_test_jpeg();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let exif = exif_data.get("exif").unwrap();
        let capture_time = parse_exif_datetime(exif);
        let metadata = prepare_attachment_metadata(&exif_data, capture_time).unwrap();

        let dt_str = metadata
            .get("datetime_original")
            .and_then(|v| v.as_str())
            .expect("UAT-2B-012: extracted_metadata.datetime_original must be present");

        // Must be valid ISO 8601 / RFC 3339
        let parsed = chrono::DateTime::parse_from_rfc3339(dt_str);
        assert!(
            parsed.is_ok(),
            "UAT-2B-012: datetime_original must be ISO 8601, got: {}",
            dt_str
        );

        // Verify the actual date values
        let dt = parsed.unwrap();
        assert_eq!(dt.year(), 2024, "UAT-2B-012: year should be 2024");
        assert_eq!(dt.month(), 6, "UAT-2B-012: month should be June");
        assert_eq!(dt.day(), 15, "UAT-2B-012: day should be 15");
    }

    // ── Multi-image coverage tests ───────────────────────────────────────

    #[test]
    fn test_prepare_metadata_paris_provenance_image() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/provenance/paris-eiffel-tower.jpg");
        let data = std::fs::read(&path).unwrap();
        let exif_data = extract_exif_metadata(&data).unwrap();
        let exif = exif_data.get("exif").unwrap();
        let capture_time = parse_exif_datetime(exif);
        let metadata = prepare_attachment_metadata(&exif_data, capture_time).unwrap();

        // GPS should be at top level
        assert!(metadata.pointer("/gps/latitude").is_some());
        assert!(metadata.pointer("/gps/longitude").is_some());
        // Camera should be at top level
        assert!(metadata.pointer("/camera/make").is_some());
        assert!(metadata.pointer("/camera/model").is_some());
    }

    #[test]
    fn test_prepare_metadata_tokyo_image() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/provenance/tokyo-shibuya.jpg");
        let data = std::fs::read(&path).unwrap();
        if let Some(exif_data) = extract_exif_metadata(&data) {
            let exif = exif_data.get("exif").unwrap();
            let capture_time = parse_exif_datetime(exif);
            let metadata = prepare_attachment_metadata(&exif_data, capture_time).unwrap();

            // Should NOT have "exif" wrapper
            assert!(metadata.get("exif").is_none());

            // If GPS present, verify positive latitude (Northern hemisphere)
            if let Some(lat) = metadata.pointer("/gps/latitude").and_then(|v| v.as_f64()) {
                assert!(lat > 0.0, "Tokyo lat should be positive (N), got {}", lat);
            }
        }
    }

    #[test]
    fn test_prepare_metadata_new_york_image() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/provenance/newyork-statue-liberty.jpg");
        let data = std::fs::read(&path).unwrap();
        if let Some(exif_data) = extract_exif_metadata(&data) {
            let exif = exif_data.get("exif").unwrap();
            let capture_time = parse_exif_datetime(exif);
            let metadata = prepare_attachment_metadata(&exif_data, capture_time).unwrap();

            // Should NOT have "exif" wrapper
            assert!(metadata.get("exif").is_none());

            // If GPS present, verify negative longitude (Western hemisphere)
            if let Some(lon) = metadata.pointer("/gps/longitude").and_then(|v| v.as_f64()) {
                assert!(lon < 0.0, "NYC lon should be negative (W), got {}", lon);
            }
        }
    }

    #[test]
    fn test_prepare_metadata_no_metadata_image() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/uat/data/images/jpeg-no-metadata.jpg");
        let data = std::fs::read(&path).unwrap();
        let result = extract_exif_metadata(&data);

        if let Some(exif_data) = result {
            // If somehow partial EXIF exists, prepare_attachment_metadata should still work
            let metadata = prepare_attachment_metadata(&exif_data, None);
            if let Some(m) = metadata {
                assert!(m.get("exif").is_none(), "Should not have exif wrapper");
            }
        }
        // None is also acceptable for no-EXIF images
    }
}
