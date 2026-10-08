//! Library components for matric-api

pub mod admin_bootstrap;
#[cfg(feature = "hosted-auth")]
pub mod hosted_auth;
#[cfg(feature = "hosted-auth")]
pub mod hosted_claim_policy;
pub mod hosted_jobs;
pub mod query_types;
pub mod realtime;
pub mod services;
