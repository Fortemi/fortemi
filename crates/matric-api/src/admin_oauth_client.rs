//! `matric-api admin oauth-client register`: provision a first-party OAuth client (#944).
//!
//! First-party clients (the bundle's MCP introspection client, operator tooling) are
//! created directly in the database, so they do not depend on the public dynamic
//! registration endpoint, whose policy may be `admin` or `disabled`. The command uses
//! `DATABASE_URL`, the same role the API uses for registration. The client secret is
//! printed once, on stdout only.

use matric_core::ClientRegistrationRequest;

use crate::oauth_profile::is_allowed_oauth_scope;

const USAGE: &str = "usage: matric-api admin oauth-client register --name <name> \
[--grant-types <list>] [--scope <scopes>] [--redirect-uri <uri>]... [--json]";
const ALLOWED_GRANT_TYPES: [&str; 3] =
    ["authorization_code", "client_credentials", "refresh_token"];

#[derive(Debug, PartialEq, Eq)]
struct RegisterCli {
    name: String,
    grant_types: Vec<String>,
    scope: String,
    redirect_uris: Vec<String>,
    json: bool,
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split([',', ' '])
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn take_value(
    flag: &str,
    inline: Option<String>,
    rest: &mut impl Iterator<Item = String>,
) -> anyhow::Result<String> {
    inline
        .or_else(|| rest.next())
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value\n{USAGE}"))
}

fn parse_register_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<RegisterCli> {
    let mut cli = RegisterCli {
        name: String::new(),
        grant_types: vec!["client_credentials".to_string()],
        scope: "read".to_string(),
        redirect_uris: Vec::new(),
        json: false,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        match flag.as_str() {
            "--name" => cli.name = take_value("--name", inline, &mut args)?,
            "--grant-types" => {
                cli.grant_types = split_list(&take_value("--grant-types", inline, &mut args)?)
            }
            "--scope" => cli.scope = take_value("--scope", inline, &mut args)?,
            "--redirect-uri" => {
                cli.redirect_uris
                    .push(take_value("--redirect-uri", inline, &mut args)?)
            }
            "--json" if inline.is_none() => cli.json = true,
            _ => anyhow::bail!("unknown argument\n{USAGE}"),
        }
    }
    validate(&cli)?;
    Ok(cli)
}

fn validate(cli: &RegisterCli) -> anyhow::Result<()> {
    let name = cli.name.trim();
    if name.is_empty() || name.chars().count() > 200 || name.chars().any(char::is_control) {
        anyhow::bail!("--name must be 1-200 printable characters\n{USAGE}");
    }
    if cli.grant_types.is_empty()
        || cli
            .grant_types
            .iter()
            .any(|grant| !ALLOWED_GRANT_TYPES.contains(&grant.as_str()))
    {
        anyhow::bail!("--grant-types accepts: {}", ALLOWED_GRANT_TYPES.join(", "));
    }
    let scopes = split_list(&cli.scope);
    if scopes.is_empty() || scopes.iter().any(|scope| !is_allowed_oauth_scope(scope)) {
        anyhow::bail!("--scope contains a scope this server does not issue");
    }
    let needs_redirect = cli.grant_types.iter().any(|g| g == "authorization_code");
    if needs_redirect && cli.redirect_uris.is_empty() {
        anyhow::bail!("authorization_code clients need at least one --redirect-uri");
    }
    Ok(())
}

fn request_for(cli: &RegisterCli) -> ClientRegistrationRequest {
    ClientRegistrationRequest {
        client_name: cli.name.trim().to_string(),
        redirect_uris: cli.redirect_uris.clone(),
        grant_types: cli.grant_types.clone(),
        response_types: Vec::new(),
        scope: Some(split_list(&cli.scope).join(" ")),
        token_endpoint_auth_method: Some("client_secret_basic".to_string()),
        client_uri: None,
        logo_uri: None,
        contacts: None,
        policy_uri: None,
        tos_uri: None,
        software_id: None,
        software_version: None,
        software_statement: None,
    }
}

/// Runs the command when invoked as `matric-api admin oauth-client ...`.
pub async fn run_if_requested() -> anyhow::Result<bool> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("admin")
        || args.get(1).map(String::as_str) != Some("oauth-client")
    {
        return Ok(false);
    }
    if args.get(2).map(String::as_str) != Some("register") {
        anyhow::bail!("{USAGE}");
    }
    dotenvy::dotenv().ok();
    let cli = parse_register_args(args.into_iter().skip(3))?;
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("admin oauth-client register requires DATABASE_URL"))?;
    let db = matric_db::Database::connect(&url)
        .await
        .map_err(|_| anyhow::anyhow!("could not connect with DATABASE_URL"))?;
    let mut response = db
        .oauth
        .register_client(request_for(&cli))
        .await
        .map_err(|_| anyhow::anyhow!("could not register the OAuth client"))?;
    db.pool.close().await;
    response.registration_access_token = None;
    response.registration_client_uri = None;
    if cli.json {
        println!("{}", serde_json::to_string(&response)?);
    } else {
        println!("client_id: {}", response.client_id);
        println!(
            "client_secret: {}",
            response.client_secret.unwrap_or_default()
        );
        println!("scope: {}", response.scope);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> anyhow::Result<RegisterCli> {
        parse_register_args(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn parses_the_bundle_mcp_client_invocation() {
        let cli = parse(&[
            "--name",
            "MCP Server (auto-registered)",
            "--grant-types=client_credentials",
            "--scope",
            "mcp read write",
            "--json",
        ])
        .unwrap();
        assert_eq!(cli.grant_types, ["client_credentials"]);
        assert!(cli.json);
        let request = request_for(&cli);
        assert_eq!(request.scope.as_deref(), Some("mcp read write"));
        assert_eq!(
            request.token_endpoint_auth_method.as_deref(),
            Some("client_secret_basic")
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&[]).is_err(), "name is required");
        assert!(parse(&["--name", "x", "--scope", "delete"]).is_err());
        assert!(parse(&["--name", "x", "--grant-types", "password"]).is_err());
        assert!(parse(&["--name", "x", "--grant-types", "authorization_code"]).is_err());
        assert!(parse(&["--name", "x", "--bogus"]).is_err());
        assert!(parse(&["--name"]).is_err());
    }

    #[test]
    fn authorization_code_clients_carry_redirects() {
        let cli = parse(&[
            "--name",
            "Desktop",
            "--grant-types",
            "authorization_code,refresh_token",
            "--redirect-uri",
            "http://127.0.0.1:8765/callback",
        ])
        .unwrap();
        assert_eq!(cli.redirect_uris, ["http://127.0.0.1:8765/callback"]);
    }
}
