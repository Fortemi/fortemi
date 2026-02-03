//! Test suite for temporal-spatial memory search (Issue #437).
//!
//! Tests spatial and temporal queries on file provenance data using PostGIS
//! and W3C PROV extensions.
//!
//! **IMPORTANT**: These tests require a fully migrated PostgreSQL database with:
//! - Base schema (notes, attachments, etc.)
//! - PostGIS extension
//! - W3C PROV temporal-spatial schema (migration 20260204100000)
//!
//! Run migrations first: `sqlx migrate run`

use chrono::{Duration, Utc};
use matric_db::Database;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Helper to create a test database pool.
async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://matric:matric@localhost/matric".to_string());
    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Helper to create a test note with attachment.
/// Requires database to be fully migrated.
/// Uses unique identifiers to avoid conflicts when tests run in parallel.
async fn create_test_note_with_attachment(
    pool: &PgPool,
) -> Result<(Uuid, Uuid), Box<dyn std::error::Error>> {
    // Create a test note using the notes repository
    let note_id = Uuid::new_v4();

    // Use UUID-based unique values for test isolation
    let unique_hash = format!("hash-{}", note_id);
    let unique_blob_hash = format!("blob-{}", Uuid::new_v4());

    // Insert into note table
    sqlx::query(
        r#"
        INSERT INTO note (id, format, source, created_at_utc, updated_at_utc)
        VALUES ($1, 'markdown', 'test', NOW(), NOW())
        "#,
    )
    .bind(note_id)
    .execute(pool)
    .await?;

    // Insert into note_original
    sqlx::query(
        r#"
        INSERT INTO note_original (note_id, content, hash)
        VALUES ($1, 'Test note for memory search', $2)
        "#,
    )
    .bind(note_id)
    .bind(&unique_hash)
    .execute(pool)
    .await?;

    // Insert into note_revised_current
    sqlx::query(
        r#"
        INSERT INTO note_revised_current (note_id, content)
        VALUES ($1, 'Test note for memory search')
        "#,
    )
    .bind(note_id)
    .execute(pool)
    .await?;

    // Create a test attachment blob with unique content_hash
    let blob_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO attachment_blob (id, content_hash, content_type, size_bytes, data)
        VALUES ($1, $2, 'image/jpeg', 1024, 'testdata')
        "#,
    )
    .bind(blob_id)
    .bind(&unique_blob_hash)
    .execute(pool)
    .await?;

    // Create attachment
    let attachment_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO attachment (id, note_id, blob_id, filename)
        VALUES ($1, $2, $3, 'test.jpg')
        "#,
    )
    .bind(attachment_id)
    .bind(note_id)
    .bind(blob_id)
    .execute(pool)
    .await?;

    Ok((note_id, attachment_id))
}

/// Helper to create a location in the database.
/// Uses 'gps_exif' as source which is valid per check_source constraint.
async fn create_test_location(
    pool: &PgPool,
    lat: f64,
    lon: f64,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let row = sqlx::query(
        r#"
        INSERT INTO prov_location (point, source, confidence)
        VALUES (ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography, 'gps_exif', 'high')
        RETURNING id
        "#,
    )
    .bind(lon)
    .bind(lat)
    .fetch_one(pool)
    .await?;

    Ok(row.get("id"))
}

/// Helper to cleanup test data
async fn cleanup_test_data(pool: &PgPool, note_id: Uuid, attachment_id: Uuid) {
    // Delete file provenance first (references attachment)
    let _ = sqlx::query("DELETE FROM file_provenance WHERE attachment_id = $1")
        .bind(attachment_id)
        .execute(pool)
        .await;

    // Get blob_id before deleting attachment
    let blob_id: Option<Uuid> = sqlx::query_scalar("SELECT blob_id FROM attachment WHERE id = $1")
        .bind(attachment_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

    // Delete attachment (references blob and note)
    let _ = sqlx::query("DELETE FROM attachment WHERE id = $1")
        .bind(attachment_id)
        .execute(pool)
        .await;

    // Delete blob if found
    if let Some(bid) = blob_id {
        let _ = sqlx::query("DELETE FROM attachment_blob WHERE id = $1")
            .bind(bid)
            .execute(pool)
            .await;
    }

    let _ = sqlx::query("DELETE FROM note_revised_current WHERE note_id = $1")
        .bind(note_id)
        .execute(pool)
        .await;

    let _ = sqlx::query("DELETE FROM note_original WHERE note_id = $1")
        .bind(note_id)
        .execute(pool)
        .await;

    let _ = sqlx::query("DELETE FROM note WHERE id = $1")
        .bind(note_id)
        .execute(pool)
        .await;
}

#[tokio::test]
#[ignore = "requires migrated database with PostGIS and W3C PROV schema"]
async fn test_search_by_location() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create a test note and attachment
    let (note_id, attachment_id) = create_test_note_with_attachment(&pool)
        .await
        .expect("Failed to create test note");

    // Create a location (Eiffel Tower: 48.8584°N, 2.2945°E)
    let location_id = create_test_location(&pool, 48.8584, 2.2945)
        .await
        .expect("Failed to create location");

    // Create file provenance linking attachment to location
    // Note: tstzrange($3, $3, '[]') creates a single-point range with inclusive bounds
    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO file_provenance (attachment_id, location_id, capture_time, event_type, time_confidence)
        VALUES ($1, $2, tstzrange($3, $3, '[]'), 'photo', 'high')
        "#,
    )
    .bind(attachment_id)
    .bind(location_id)
    .bind(now)
    .execute(&pool)
    .await
    .expect("Failed to create file provenance");

    // Test: Search within 1000m of Eiffel Tower (should find the photo)
    let results = db
        .memory_search
        .search_by_location(48.8584, 2.2945, 1000.0)
        .await
        .expect("Failed to search by location");

    // Find our specific result in the results (database may have data from other runs)
    let our_result = results
        .iter()
        .find(|r| r.attachment_id == attachment_id)
        .expect("Expected attachment_id not found in search results");

    assert_eq!(our_result.event_type, Some("photo".to_string()));
    assert!(our_result.distance_m < 1000.0);

    // Test: Search far away - our attachment should NOT be found at NYC
    let results = db
        .memory_search
        .search_by_location(40.7128, -74.0060, 1000.0) // NYC
        .await
        .expect("Failed to search by location");

    assert!(
        !results.iter().any(|r| r.attachment_id == attachment_id),
        "Attachment_id {} should NOT be found in NYC (it's at Eiffel Tower)",
        attachment_id
    );

    // Cleanup
    cleanup_test_data(&pool, note_id, attachment_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database with PostGIS and W3C PROV schema"]
async fn test_search_by_timerange() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create a test note and attachment
    let (note_id, attachment_id) = create_test_note_with_attachment(&pool)
        .await
        .expect("Failed to create test note");

    // Create file provenance with a capture time (yesterday)
    // Note: tstzrange($2, $2, '[]') creates a single-point range with inclusive bounds
    // Without '[]', tstzrange defaults to [) which creates an EMPTY range when start=end
    let yesterday = Utc::now() - Duration::days(1);
    sqlx::query(
        r#"
        INSERT INTO file_provenance (attachment_id, capture_time, event_type, time_confidence)
        VALUES ($1, tstzrange($2, $2, '[]'), 'photo', 'high')
        "#,
    )
    .bind(attachment_id)
    .bind(yesterday)
    .execute(&pool)
    .await
    .expect("Failed to create file provenance");

    // Test: Search in range including yesterday
    let start = Utc::now() - Duration::days(2);
    let end = Utc::now();
    let results = db
        .memory_search
        .search_by_timerange(start, end)
        .await
        .expect("Failed to search by timerange");

    // Verify our attachment is in the results (database may have data from other runs)
    assert!(
        results.iter().any(|r| r.attachment_id == attachment_id),
        "Expected attachment_id {} not found in results",
        attachment_id
    );

    // Test: Search in range excluding yesterday - our attachment should NOT be found
    let start = Utc::now() - Duration::days(10);
    let end = Utc::now() - Duration::days(5);
    let results = db
        .memory_search
        .search_by_timerange(start, end)
        .await
        .expect("Failed to search by timerange");

    assert!(
        !results.iter().any(|r| r.attachment_id == attachment_id),
        "Attachment_id {} should NOT be found in this time range",
        attachment_id
    );

    // Cleanup
    cleanup_test_data(&pool, note_id, attachment_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database with PostGIS and W3C PROV schema"]
async fn test_search_by_location_and_time() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create a test note and attachment
    let (note_id, attachment_id) = create_test_note_with_attachment(&pool)
        .await
        .expect("Failed to create test note");

    // Create a location and file provenance
    let location_id = create_test_location(&pool, 48.8584, 2.2945)
        .await
        .expect("Failed to create location");

    // Note: tstzrange($3, $3, '[]') creates a single-point range with inclusive bounds
    let yesterday = Utc::now() - Duration::days(1);
    sqlx::query(
        r#"
        INSERT INTO file_provenance (attachment_id, location_id, capture_time, event_type, time_confidence)
        VALUES ($1, $2, tstzrange($3, $3, '[]'), 'photo', 'high')
        "#,
    )
    .bind(attachment_id)
    .bind(location_id)
    .bind(yesterday)
    .execute(&pool)
    .await
    .expect("Failed to create file provenance");

    // Test: Search with matching location and time
    let start = Utc::now() - Duration::days(2);
    let end = Utc::now();
    let results = db
        .memory_search
        .search_by_location_and_time(48.8584, 2.2945, 1000.0, start, end)
        .await
        .expect("Failed to search by location and time");

    // Verify our attachment is in the results
    assert!(
        results.iter().any(|r| r.attachment_id == attachment_id),
        "Expected attachment_id {} not found in location+time search results",
        attachment_id
    );

    // Test: Search with matching location but wrong time - our attachment should NOT be found
    let start = Utc::now() - Duration::days(10);
    let end = Utc::now() - Duration::days(5);
    let results = db
        .memory_search
        .search_by_location_and_time(48.8584, 2.2945, 1000.0, start, end)
        .await
        .expect("Failed to search by location and time");

    assert!(
        !results.iter().any(|r| r.attachment_id == attachment_id),
        "Attachment_id {} should NOT be found with wrong time range",
        attachment_id
    );

    // Test: Search with matching time but wrong location (NYC) - our attachment should NOT be found
    let start = Utc::now() - Duration::days(2);
    let end = Utc::now();
    let results = db
        .memory_search
        .search_by_location_and_time(40.7128, -74.0060, 1000.0, start, end)
        .await
        .expect("Failed to search by location and time");

    assert!(
        !results.iter().any(|r| r.attachment_id == attachment_id),
        "Attachment_id {} should NOT be found with wrong location (NYC vs Paris)",
        attachment_id
    );

    // Cleanup
    cleanup_test_data(&pool, note_id, attachment_id).await;
}

#[tokio::test]
#[ignore = "requires migrated database with PostGIS and W3C PROV schema"]
async fn test_get_memory_provenance() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create a test note and attachment
    let (note_id, attachment_id) = create_test_note_with_attachment(&pool)
        .await
        .expect("Failed to create test note");

    // Create location, device, and file provenance
    let location_id = create_test_location(&pool, 48.8584, 2.2945)
        .await
        .expect("Failed to create location");

    let device_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO prov_agent_device (device_make, device_model)
        VALUES ('Apple', 'iPhone 15 Pro')
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to create device");

    // Note: tstzrange($4, $4, '[]') creates a single-point range with inclusive bounds
    let yesterday = Utc::now() - Duration::days(1);
    sqlx::query(
        r#"
        INSERT INTO file_provenance (
            attachment_id, location_id, device_id, capture_time,
            event_type, event_title, time_source, time_confidence
        )
        VALUES ($1, $2, $3, tstzrange($4, $4, '[]'), 'photo', 'Eiffel Tower Visit', 'exif', 'high')
        "#,
    )
    .bind(attachment_id)
    .bind(location_id)
    .bind(device_id)
    .bind(yesterday)
    .execute(&pool)
    .await
    .expect("Failed to create file provenance");

    // Test: Get full provenance chain
    let prov = db
        .memory_search
        .get_memory_provenance(note_id)
        .await
        .expect("Failed to get provenance");

    assert!(prov.is_some());
    let prov = prov.unwrap();
    assert_eq!(prov.note_id, note_id);
    assert_eq!(prov.files.len(), 1);

    let file_prov = &prov.files[0];
    assert_eq!(file_prov.attachment_id, attachment_id);
    assert_eq!(file_prov.event_type, Some("photo".to_string()));
    assert_eq!(
        file_prov.event_title,
        Some("Eiffel Tower Visit".to_string())
    );
    assert!(file_prov.location.is_some());
    assert!(file_prov.device.is_some());

    let device = file_prov.device.as_ref().unwrap();
    assert_eq!(device.device_make, Some("Apple".to_string()));
    assert_eq!(device.device_model, Some("iPhone 15 Pro".to_string()));

    // Cleanup
    cleanup_test_data(&pool, note_id, attachment_id).await;
    let _ = sqlx::query("DELETE FROM prov_agent_device WHERE id = $1")
        .bind(device_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
#[ignore = "requires migrated database with PostGIS and W3C PROV schema"]
async fn test_search_ordering_by_distance() {
    let pool = setup_test_db().await;
    let db = Database::new(pool.clone());

    // Create multiple test notes with attachments at different distances
    let mut test_data = Vec::new();
    let coords = vec![
        (48.8584, 2.2945), // Eiffel Tower (0m from center)
        (48.8606, 2.3376), // Louvre (4km from Eiffel Tower)
        (48.8530, 2.3499), // Notre-Dame (5km from Eiffel Tower)
    ];

    for (lat, lon) in coords {
        let (note_id, attachment_id) = create_test_note_with_attachment(&pool)
            .await
            .expect("Failed to create test note");

        let location_id = create_test_location(&pool, lat, lon)
            .await
            .expect("Failed to create location");

        // Note: tstzrange(NOW(), NOW(), '[]') creates a single-point range with inclusive bounds
        sqlx::query(
            r#"
            INSERT INTO file_provenance (attachment_id, location_id, capture_time, event_type, time_confidence)
            VALUES ($1, $2, tstzrange(NOW(), NOW(), '[]'), 'photo', 'high')
            "#,
        )
        .bind(attachment_id)
        .bind(location_id)
        .execute(&pool)
        .await
        .expect("Failed to create file provenance");

        test_data.push((note_id, attachment_id));
    }

    // Test: Search from Eiffel Tower - should return results ordered by distance
    let results = db
        .memory_search
        .search_by_location(48.8584, 2.2945, 10000.0)
        .await
        .expect("Failed to search by location");

    assert!(results.len() >= 3, "Should find at least 3 memories");

    // First result should be closest (Eiffel Tower itself)
    let closest = &results[0];
    assert!(
        closest.distance_m < 100.0,
        "First result should be very close to center"
    );

    // Results should be ordered by distance (ascending)
    for i in 0..results.len() - 1 {
        assert!(
            results[i].distance_m <= results[i + 1].distance_m,
            "Results should be ordered by distance"
        );
    }

    // Cleanup
    for (note_id, attachment_id) in test_data {
        cleanup_test_data(&pool, note_id, attachment_id).await;
    }
}
