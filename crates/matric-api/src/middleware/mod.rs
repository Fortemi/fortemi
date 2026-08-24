//! Middleware modules for the matric-api.

pub mod archive_routing;
// The coordinator is staged for #728; route layering and handler migration remain gated.
#[allow(dead_code)]
pub mod tenant_scope;
