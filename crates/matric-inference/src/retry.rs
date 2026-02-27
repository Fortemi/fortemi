//! Retry with exponential backoff for sidecar HTTP clients.
//!
//! Provides a generic retry wrapper that handles transient failures common
//! when communicating with sidecar containers (whisper, pyannote, gliner).
//! Retries on connection refused, timeout, and 503 responses. Does NOT
//! retry on 4xx errors (client bugs).

use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not counting the initial attempt).
    pub max_retries: u32,
    /// Initial backoff duration before first retry.
    pub initial_backoff: Duration,
    /// Maximum backoff duration (caps exponential growth).
    pub max_backoff: Duration,
    /// Backoff multiplier per retry (typically 2.0 for exponential).
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(8),
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Calculate backoff duration for a given attempt (0-indexed).
    fn backoff_for(&self, attempt: u32) -> Duration {
        let backoff_ms =
            self.initial_backoff.as_millis() as f64 * self.multiplier.powi(attempt as i32);
        let capped = Duration::from_millis(backoff_ms as u64).min(self.max_backoff);
        capped
    }
}

/// Classifies whether an HTTP error is retryable.
pub fn is_retryable_reqwest_error(err: &reqwest::Error) -> bool {
    // Connection refused, reset, timeout — all transient
    if err.is_connect() || err.is_timeout() {
        return true;
    }
    // 503 Service Unavailable is retryable (sidecar restarting)
    if let Some(status) = err.status() {
        return status == reqwest::StatusCode::SERVICE_UNAVAILABLE;
    }
    // Request builder errors, decode errors — not retryable
    false
}

/// Classifies whether an HTTP response status is retryable.
pub fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        || status == reqwest::StatusCode::BAD_GATEWAY
        || status == reqwest::StatusCode::GATEWAY_TIMEOUT
}

/// Result of a retry operation.
pub enum RetryOutcome<T> {
    /// The operation succeeded.
    Success(T),
    /// The operation failed with a non-retryable error.
    Failed(matric_core::Error),
    /// The operation failed after exhausting all retries.
    Exhausted {
        last_error: matric_core::Error,
        attempts: u32,
    },
}

/// Execute an async operation with retry and exponential backoff.
///
/// The `operation` closure is called repeatedly until it succeeds or retries
/// are exhausted. The closure returns `Result<reqwest::Response>` — the retry
/// logic inspects both the error and the response status to decide whether
/// to retry.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `service_name` - Human-readable name for logging (e.g., "whisper", "pyannote")
/// * `operation` - Async closure that performs the HTTP request
pub async fn with_retry<F, Fut>(
    config: &RetryConfig,
    service_name: &str,
    operation: F,
) -> matric_core::Result<reqwest::Response>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    let total_attempts = config.max_retries + 1;

    for attempt in 0..total_attempts {
        match operation().await {
            Ok(response) => {
                if response.status().is_success() || response.status().is_client_error() {
                    // Success or 4xx (client error — don't retry)
                    return Ok(response);
                }

                if is_retryable_status(response.status()) && attempt < config.max_retries {
                    let backoff = config.backoff_for(attempt);
                    warn!(
                        service = service_name,
                        status = %response.status(),
                        attempt = attempt + 1,
                        max_attempts = total_attempts,
                        backoff_ms = backoff.as_millis() as u64,
                        "Retryable HTTP status, backing off"
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                // Non-retryable status or retries exhausted — return as-is
                return Ok(response);
            }
            Err(err) => {
                if is_retryable_reqwest_error(&err) && attempt < config.max_retries {
                    let backoff = config.backoff_for(attempt);
                    warn!(
                        service = service_name,
                        error = %err,
                        attempt = attempt + 1,
                        max_attempts = total_attempts,
                        backoff_ms = backoff.as_millis() as u64,
                        "Retryable error, backing off"
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                // Non-retryable or retries exhausted
                if attempt >= config.max_retries {
                    debug!(
                        service = service_name,
                        attempts = total_attempts,
                        "All retry attempts exhausted"
                    );
                }
                return Err(matric_core::Error::Internal(format!(
                    "{} request failed after {} attempt(s): {}",
                    service_name,
                    attempt + 1,
                    err
                )));
            }
        }
    }

    // Should not reach here, but just in case
    Err(matric_core::Error::Internal(format!(
        "{} request failed: retry loop completed without result",
        service_name
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_backoff, Duration::from_secs(1));
        assert_eq!(config.max_backoff, Duration::from_secs(8));
        assert_eq!(config.multiplier, 2.0);
    }

    #[test]
    fn test_backoff_calculation() {
        let config = RetryConfig {
            max_retries: 5,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(16),
            multiplier: 2.0,
        };

        // 1s, 2s, 4s, 8s, 16s
        assert_eq!(config.backoff_for(0), Duration::from_secs(1));
        assert_eq!(config.backoff_for(1), Duration::from_secs(2));
        assert_eq!(config.backoff_for(2), Duration::from_secs(4));
        assert_eq!(config.backoff_for(3), Duration::from_secs(8));
        assert_eq!(config.backoff_for(4), Duration::from_secs(16));
    }

    #[test]
    fn test_backoff_capped_at_max() {
        let config = RetryConfig {
            max_retries: 5,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(4),
            multiplier: 2.0,
        };

        // Should cap at 4s
        assert_eq!(config.backoff_for(0), Duration::from_secs(1));
        assert_eq!(config.backoff_for(1), Duration::from_secs(2));
        assert_eq!(config.backoff_for(2), Duration::from_secs(4));
        assert_eq!(config.backoff_for(3), Duration::from_secs(4)); // capped
        assert_eq!(config.backoff_for(4), Duration::from_secs(4)); // capped
    }

    #[test]
    fn test_retryable_status_codes() {
        assert!(is_retryable_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(is_retryable_status(reqwest::StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(reqwest::StatusCode::GATEWAY_TIMEOUT));
        assert!(!is_retryable_status(reqwest::StatusCode::OK));
        assert!(!is_retryable_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(reqwest::StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
    }

    #[tokio::test]
    async fn test_with_retry_immediate_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/test")
            .with_status(200)
            .with_body("ok")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/test", server.url());
        let config = RetryConfig::default();

        let resp = with_retry(&config, "test", || {
            let client = client.clone();
            let url = url.clone();
            async move { client.get(&url).send().await }
        })
        .await
        .unwrap();

        assert_eq!(resp.status(), 200);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_with_retry_4xx_not_retried() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/test")
            .with_status(400)
            .with_body("bad request")
            .expect(1)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/test", server.url());
        let config = RetryConfig::default();

        let resp = with_retry(&config, "test", || {
            let client = client.clone();
            let url = url.clone();
            async move { client.get(&url).send().await }
        })
        .await
        .unwrap();

        assert_eq!(resp.status(), 400);
        mock.assert_async().await;
    }
}
