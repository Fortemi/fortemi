//! Authenticated resource-owner consent for `/oauth/authorize` (#943).
//!
//! The endpoint stays outside bearer middleware (a browser cannot send a bearer token on
//! a navigation), but it no longer approves on behalf of nobody: the owner must be
//! authenticated by one of the methods in [`config`], the request is bound to a
//! server-side transaction, and every response is frame- and cache-protected.

pub(crate) mod config;
mod handlers;
mod owner;
#[cfg(test)]
mod owner_tests;
mod page;
mod request;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod transaction;

pub(crate) use handlers::{authorize_get, authorize_post};

/// Owner-authentication policy plus the pending-transaction store.
pub(crate) struct AuthorizeRuntime {
    pub(crate) config: config::AuthorizeConfig,
    pub(crate) store: transaction::TransactionStore,
}

impl AuthorizeRuntime {
    pub(crate) fn new(config: config::AuthorizeConfig) -> Self {
        Self {
            config,
            store: transaction::TransactionStore::default(),
        }
    }
}
