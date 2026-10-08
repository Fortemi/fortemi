//! Authenticated resource-owner consent for `/oauth/authorize` (#943).
//!
//! The endpoint stays outside bearer middleware (a browser cannot send a bearer token on
//! a navigation). Owner authentication is selected in [`config`]; the community default
//! `none` keeps approve-only consent. In every mode the request is validated before any
//! redirect, bound to a server-side transaction, and every response is frame- and
//! cache-protected.

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
