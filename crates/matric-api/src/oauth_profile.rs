use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const PROFILE_CONTRACT_JSON: &str = include_str!("../oauth-profiles.json");
const REQUIRED_PROFILES: [&str; 4] = [
    "local_dev",
    "self_hosted_operator",
    "hosted_strict",
    "compatibility",
];

#[derive(Debug, Deserialize)]
pub struct OAuthProfileContract {
    version: u32,
    active_runtime_profile: String,
    profiles: BTreeMap<String, OAuthCapabilities>,
    non_advertised_legacy_seed_scopes: BTreeMap<String, LegacySeedScope>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCapabilities {
    pub availability: String,
    pub registration_mode: String,
    pub advertise_registration_endpoint: bool,
    pub registration_management_supported: bool,
    pub client_id_metadata_document_supported: bool,
    pub token_endpoint_auth_methods: Vec<String>,
    pub pkce_methods: Vec<String>,
    pub scopes: Vec<String>,
    pub allow_local_issuer: bool,
    pub owner_issues: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct LegacySeedScope {
    owner_issue: String,
    reason: String,
}

impl OAuthProfileContract {
    fn load() -> Result<Self, String> {
        let contract: Self = serde_json::from_str(PROFILE_CONTRACT_JSON)
            .map_err(|error| format!("invalid OAuth profile contract JSON: {error}"))?;
        contract.validate()?;
        Ok(contract)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported OAuth profile contract version: {}",
                self.version
            ));
        }

        let actual_profiles = self
            .profiles
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let required_profiles = REQUIRED_PROFILES.into_iter().collect::<BTreeSet<_>>();
        if actual_profiles != required_profiles {
            return Err(format!(
                "OAuth profile contract must define exactly: {}",
                REQUIRED_PROFILES.join(", ")
            ));
        }

        let active = self.profile(&self.active_runtime_profile)?;
        if active.availability != "current" {
            return Err("active OAuth runtime profile must be current".to_string());
        }

        for (name, profile) in &self.profiles {
            validate_profile(name, profile)?;
        }

        let hosted = self.profile("hosted_strict")?;
        if hosted.availability != "dependency_gated"
            || hosted.registration_mode != "unavailable"
            || hosted.advertise_registration_endpoint
            || hosted.registration_management_supported
            || hosted.client_id_metadata_document_supported
            || hosted.allow_local_issuer
            || !hosted.token_endpoint_auth_methods.is_empty()
        {
            return Err(
                "hosted_strict must remain unavailable and fail closed until its owners land"
                    .to_string(),
            );
        }

        let delete = self
            .non_advertised_legacy_seed_scopes
            .get("delete")
            .ok_or_else(|| {
                "legacy delete seed scope must remain explicitly classified".to_string()
            })?;
        if delete.owner_issue != "#924" || delete.reason.trim().len() < 24 {
            return Err(
                "legacy delete seed scope requires an actionable #924 rationale".to_string(),
            );
        }
        if self
            .profiles
            .values()
            .any(|profile| profile.scopes.iter().any(|scope| scope == "delete"))
        {
            return Err("delete must not be advertised before #924 resolves it".to_string());
        }

        Ok(())
    }

    fn profile(&self, name: &str) -> Result<&OAuthCapabilities, String> {
        self.profiles
            .get(name)
            .ok_or_else(|| format!("unknown OAuth deployment profile: {name}"))
    }

    fn active(&self) -> &OAuthCapabilities {
        self.profile(&self.active_runtime_profile)
            .expect("validated active OAuth deployment profile")
    }
}

fn validate_profile(name: &str, profile: &OAuthCapabilities) -> Result<(), String> {
    if !matches!(
        profile.availability.as_str(),
        "current" | "dependency_gated"
    ) {
        return Err(format!("{name} has an unknown availability"));
    }
    if !matches!(
        profile.registration_mode.as_str(),
        "open_dcr_compat" | "unavailable"
    ) {
        return Err(format!("{name} has an unknown registration mode"));
    }
    if profile.advertise_registration_endpoint != (profile.registration_mode == "open_dcr_compat") {
        return Err(format!(
            "{name} registration endpoint does not match its registration mode"
        ));
    }
    if profile.registration_management_supported
        || profile.client_id_metadata_document_supported
        || profile.pkce_methods.iter().any(|method| method == "plain")
    {
        return Err(format!(
            "{name} advertises an unimplemented OAuth capability"
        ));
    }
    for (field, values) in [
        (
            "token endpoint auth methods",
            &profile.token_endpoint_auth_methods,
        ),
        ("PKCE methods", &profile.pkce_methods),
        ("scopes", &profile.scopes),
        ("owner issues", &profile.owner_issues),
    ] {
        if values.len() != values.iter().collect::<BTreeSet<_>>().len() {
            return Err(format!("{name} contains duplicate {field}"));
        }
    }
    if profile.pkce_methods != ["S256"] {
        return Err(format!("{name} must advertise S256-only PKCE"));
    }
    if profile.scopes.is_empty()
        || profile
            .owner_issues
            .iter()
            .any(|issue| !issue.starts_with('#'))
    {
        return Err(format!("{name} has an incomplete capability contract"));
    }
    Ok(())
}

fn contract() -> &'static OAuthProfileContract {
    static CONTRACT: OnceLock<OAuthProfileContract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        OAuthProfileContract::load().expect("embedded OAuth profile contract must be valid")
    })
}

pub fn active_oauth_capabilities() -> &'static OAuthCapabilities {
    contract().active()
}

pub fn is_allowed_oauth_scope(scope: &str) -> bool {
    active_oauth_capabilities()
        .scopes
        .iter()
        .any(|allowed| allowed == scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_contract_is_valid_and_unknown_profiles_fail_closed() {
        let contract = OAuthProfileContract::load().expect("profile contract");

        assert!(contract.profile("unknown").is_err());
        assert_eq!(contract.active_runtime_profile, "self_hosted_operator");
    }

    #[test]
    fn hosted_strict_is_dependency_gated_without_capability_widening() {
        let contract = OAuthProfileContract::load().expect("profile contract");
        let hosted = contract.profile("hosted_strict").expect("hosted profile");

        assert_eq!(hosted.availability, "dependency_gated");
        assert!(!hosted.advertise_registration_endpoint);
        assert!(!hosted.registration_management_supported);
        assert!(!hosted.client_id_metadata_document_supported);
        assert!(hosted.token_endpoint_auth_methods.is_empty());
        assert_eq!(hosted.pkce_methods, ["S256"]);
        assert!(!hosted.scopes.iter().any(|scope| scope == "delete"));
        assert!(!hosted.allow_local_issuer);
    }

    #[test]
    fn active_profile_is_the_scope_source_of_truth() {
        let active = active_oauth_capabilities();

        assert!(is_allowed_oauth_scope("read"));
        assert!(is_allowed_oauth_scope("mcp"));
        assert!(!is_allowed_oauth_scope("delete"));
        assert_eq!(active.scopes, ["read", "write", "admin", "mcp"]);
    }
}
