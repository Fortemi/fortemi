//! Server-side authorization transactions binding the consent page to its POST (#943).
//!
//! The GET stores the validated request here and hands the browser only an opaque
//! transaction id, a CSRF token (in the form) and a binding secret (in an HttpOnly,
//! SameSite=Strict cookie). The POST must present all three; client, redirect, scope
//! and PKCE values never round-trip through the browser, so they cannot be tampered
//! with. Transactions are single-use, expire after ten minutes and live in process
//! memory: run one API replica, or route a browser's requests to the same replica.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(crate) const TRANSACTION_TTL: Duration = Duration::from_secs(600);
const MAX_PENDING: usize = 1024;
pub(crate) const MAX_OWNER_ATTEMPTS: u8 = 5;

/// The validated authorization request, held server-side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedRequest {
    pub(crate) client_id: String,
    pub(crate) client_name: String,
    pub(crate) redirect_uri: String,
    pub(crate) scope: String,
    pub(crate) state: Option<String>,
    pub(crate) code_challenge: Option<String>,
    pub(crate) code_challenge_method: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingAuthorization {
    pub(crate) request: ValidatedRequest,
    pub(crate) csrf_token: String,
    pub(crate) binding: String,
    pub(crate) owner_attempts: u8,
    expires_at: Instant,
}

impl PendingAuthorization {
    pub(crate) fn new(request: ValidatedRequest) -> Self {
        Self {
            request,
            csrf_token: random_token(),
            binding: random_token(),
            owner_attempts: 0,
            expires_at: Instant::now() + TRANSACTION_TTL,
        }
    }

    fn expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }

    /// Whether the presented CSRF token and cookie binding both match.
    pub(crate) fn matches(&self, csrf_token: &str, binding: Option<&str>) -> bool {
        let binding_ok = binding.is_some_and(|value| constant_time_eq(value, &self.binding));
        constant_time_eq(csrf_token, &self.csrf_token) & binding_ok
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StoreFull;

#[derive(Default)]
pub(crate) struct TransactionStore {
    pending: Mutex<HashMap<String, PendingAuthorization>>,
}

impl TransactionStore {
    /// Store a transaction and return its id.
    pub(crate) fn insert(&self, pending: PendingAuthorization) -> Result<String, StoreFull> {
        let mut map = self
            .pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let now = Instant::now();
        map.retain(|_, entry| !entry.expired(now));
        if map.len() >= MAX_PENDING {
            return Err(StoreFull);
        }
        let id = random_token();
        map.insert(id.clone(), pending);
        Ok(id)
    }

    /// Remove and return a live transaction. Every POST consumes it first.
    pub(crate) fn take(&self, id: &str) -> Option<PendingAuthorization> {
        let mut map = self
            .pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let entry = map.remove(id)?;
        (!entry.expired(Instant::now())).then_some(entry)
    }

    /// Return a transaction after a failed owner sign-in, keeping its id.
    pub(crate) fn restore(&self, id: String, pending: PendingAuthorization) {
        let mut map = self
            .pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        map.insert(id, pending);
    }

    #[cfg(test)]
    pub(crate) fn expire_all(&self) {
        let mut map = self
            .pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for entry in map.values_mut() {
            entry.expires_at = Instant::now();
        }
    }
}

/// 244 random bits, URL and cookie safe.
pub(crate) fn random_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ValidatedRequest {
        ValidatedRequest {
            client_id: "mm_client".to_string(),
            client_name: "Client".to_string(),
            redirect_uri: "https://client.example/cb".to_string(),
            scope: "read".to_string(),
            state: Some("xyz".to_string()),
            code_challenge: None,
            code_challenge_method: None,
        }
    }

    #[test]
    fn transactions_are_single_use() {
        let store = TransactionStore::default();
        let id = store.insert(PendingAuthorization::new(request())).unwrap();
        assert!(store.take(&id).is_some());
        assert!(
            store.take(&id).is_none(),
            "a replayed transaction must fail"
        );
    }

    #[test]
    fn expired_transactions_are_rejected() {
        let store = TransactionStore::default();
        let id = store.insert(PendingAuthorization::new(request())).unwrap();
        store.expire_all();
        assert!(store.take(&id).is_none());
    }

    #[test]
    fn csrf_and_binding_must_both_match() {
        let pending = PendingAuthorization::new(request());
        let (csrf, binding) = (pending.csrf_token.clone(), pending.binding.clone());
        assert!(pending.matches(&csrf, Some(&binding)));
        assert!(!pending.matches(&csrf, None));
        assert!(!pending.matches(&csrf, Some("other")));
        assert!(!pending.matches("other", Some(&binding)));
    }

    #[test]
    fn tokens_are_unique_and_url_safe() {
        let (a, b) = (random_token(), random_token());
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
