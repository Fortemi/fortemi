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
    let exif_reader = Reader::new();
    let exif = exif_reader.read_from_container(&mut cursor).ok()?;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!((decimal - 40.44611).abs() < 0.001, "Expected ~40.446, got {}", decimal);
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
}
