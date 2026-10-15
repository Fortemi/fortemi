//! Startup selection seam for enterprise policy implementations (#1201).
use std::sync::{Arc, OnceLock};

use matric_core::{AllowAllPolicy, AuthorizationPolicy, RoleBasedPolicy};

#[derive(Clone, Copy, Debug)]
pub struct PolicyMode {
    pub multi_tenant: bool,
    pub external_oidc: bool,
}

/// Receives the unchanged community default; implementations can wrap or replace
/// it. Install once during enterprise bootstrap, before selecting any policy.
pub type PolicySelector =
    dyn Fn(PolicyMode, Arc<dyn AuthorizationPolicy>) -> Arc<dyn AuthorizationPolicy> + Send + Sync;

static SELECTOR: OnceLock<Arc<PolicySelector>> = OnceLock::new();

pub fn install_policy_selector(selector: Arc<PolicySelector>) -> Result<(), Arc<PolicySelector>> {
    SELECTOR.set(selector)
}

pub fn select_policy(mode: PolicyMode) -> Arc<dyn AuthorizationPolicy> {
    select_policy_with(mode, SELECTOR.get().map(AsRef::as_ref))
}

/// Explicit selection also supports isolated routers without global test state.
pub fn select_policy_with(
    mode: PolicyMode,
    selector: Option<&PolicySelector>,
) -> Arc<dyn AuthorizationPolicy> {
    let default: Arc<dyn AuthorizationPolicy> = if mode.multi_tenant || mode.external_oidc {
        Arc::new(RoleBasedPolicy)
    } else {
        Arc::new(AllowAllPolicy)
    };
    match selector {
        Some(select) => select(mode, default),
        None => default,
    }
}
