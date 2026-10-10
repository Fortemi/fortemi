//! Per-set HNSW `ef_search` resolution (#1181 R1).
//!
//! Semantic search runs `SET LOCAL hnsw.ef_search` inside the search
//! transaction, taking the target set's stored value when present and falling
//! back to the tuning default otherwise. The default constant mirrors
//! `HnswTuningConfig`'s Balanced `base_ef` in matric-search; a unit test there
//! pins the two together.

use sqlx::PgConnection;
use uuid::Uuid;

use matric_core::{defaults, Error, Result};

/// Resolve the effective `ef_search` for a search.
///
/// Out-of-range stored values (possible on databases predating the `ef_search`
/// range check) fall back to the tuning default instead of failing the query.
pub fn resolve_ef_search(set_value: Option<i32>) -> i32 {
    match set_value {
        Some(value) if (defaults::EF_SEARCH_MIN..=defaults::EF_SEARCH_MAX).contains(&value) => {
            value
        }
        _ => defaults::SEMANTIC_EF_SEARCH_DEFAULT,
    }
}

/// Read the stored per-set `ef_search` override, if any.
pub async fn stored_ef_search(
    connection: &mut PgConnection,
    embedding_set_id: Uuid,
) -> Result<Option<i32>> {
    let stored: Option<Option<i32>> =
        sqlx::query_scalar("SELECT ef_search FROM embedding_set WHERE id = $1")
            .bind(embedding_set_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(Error::Database)?;
    Ok(stored.flatten())
}

/// Apply the effective `ef_search` for a set inside the caller's transaction.
///
/// `set_id` is `None` for unscoped searches with no default set. Returns the
/// applied value. Must run inside a transaction: `SET LOCAL` is scoped to it.
pub async fn apply_ef_search(
    connection: &mut PgConnection,
    embedding_set_id: Option<Uuid>,
) -> Result<i32> {
    let stored = match embedding_set_id {
        Some(set_id) => stored_ef_search(connection, set_id).await?,
        None => None,
    };
    let ef_search = resolve_ef_search(stored);
    sqlx::query(&format!("SET LOCAL hnsw.ef_search = {ef_search}"))
        .execute(&mut *connection)
        .await
        .map_err(Error::Database)?;
    Ok(ef_search)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_value_wins_when_in_range() {
        assert_eq!(resolve_ef_search(Some(100)), 100);
        assert_eq!(resolve_ef_search(Some(10)), 10);
        assert_eq!(resolve_ef_search(Some(1000)), 1000);
    }

    #[test]
    fn missing_or_out_of_range_falls_back_to_tuning_default() {
        assert_eq!(
            resolve_ef_search(None),
            defaults::SEMANTIC_EF_SEARCH_DEFAULT
        );
        assert_eq!(
            resolve_ef_search(Some(9)),
            defaults::SEMANTIC_EF_SEARCH_DEFAULT
        );
        assert_eq!(
            resolve_ef_search(Some(1001)),
            defaults::SEMANTIC_EF_SEARCH_DEFAULT
        );
    }
}
