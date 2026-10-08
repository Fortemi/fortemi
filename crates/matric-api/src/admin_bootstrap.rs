//! Idempotent hosted tenant bootstrap: registry row plus default-memory baseline.
//!
//! Hosted admission requires an `active` `tenant_registry` row for the tenant id
//! carried in the verified token claim. This module provisions that row and the
//! tenant's default memory (the tenant-scoped rows of the `public` schema that
//! hosted archive routing selects when no registered default exists) so that a
//! deployment never needs out-of-band SQL. Every query is tenant-qualified so the
//! command is correct under both a forced-RLS owner and a `BYPASSRLS` migrator.

use std::ffi::OsString;
use std::fmt;

use serde::Serialize;
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

/// Fixed UUIDv5 namespace for slug-derived tenant ids. Never change it: doing so
/// would map existing slugs to new tenant ids and break issued IdP claims.
pub const TENANT_SLUG_NAMESPACE: Uuid = Uuid::from_u128(0x6f0b_6c2e_4f1d_5a8e_9c3b_2d7e_1a4f_0c59);

const LOCAL_TENANT: Uuid = Uuid::nil();
const DEFAULT_TENANT_CLAIM: &str = "fortemi:tenant_id";
const BOOTSTRAP_LOCK: &str = "fortemi.tenant-bootstrap";
const USAGE: &str = "usage: matric-api admin bootstrap --slug <slug> [--tenant-id <uuid>] \
[--display-name <name>] [--reactivate] [--dry-run] [--json] [--skip-migrations]";

/// Derive the stable tenant id for a slug (UUIDv5 under [`TENANT_SLUG_NAMESPACE`]).
pub fn derive_tenant_id(slug: &str) -> Uuid {
    Uuid::new_v5(&TENANT_SLUG_NAMESPACE, slug.as_bytes())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapRequest {
    pub tenant_id: Uuid,
    pub slug: String,
    pub display_name: String,
    /// Allow returning a `suspended` tenant to `active`.
    pub reactivate: bool,
}

impl BootstrapRequest {
    /// Validate inputs; the tenant id is derived from the slug when absent.
    pub fn new(
        slug: &str,
        tenant_id: Option<Uuid>,
        display_name: Option<&str>,
        reactivate: bool,
    ) -> Result<Self, BootstrapError> {
        validate_slug(slug)?;
        let tenant_id = tenant_id.unwrap_or_else(|| derive_tenant_id(slug));
        if tenant_id == LOCAL_TENANT {
            return Err(BootstrapError::Invalid(
                "the nil tenant id is reserved for the local personal tenant".into(),
            ));
        }
        let display_name = display_name.map(str::trim).unwrap_or(slug).to_string();
        if display_name.is_empty()
            || display_name.chars().count() > 200
            || display_name.chars().any(char::is_control)
        {
            return Err(BootstrapError::Invalid(
                "display name must be 1-200 printable characters".into(),
            ));
        }
        Ok(Self {
            tenant_id,
            slug: slug.to_string(),
            display_name,
            reactivate,
        })
    }
}

fn validate_slug(slug: &str) -> Result<(), BootstrapError> {
    let bytes = slug.as_bytes();
    let valid_chars = bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
    let valid_edges =
        bytes.first().is_some_and(|b| *b != b'-') && bytes.last().is_some_and(|b| *b != b'-');
    if !(1..=63).contains(&bytes.len()) || !valid_chars || !valid_edges {
        return Err(BootstrapError::Invalid(
            "slug must be 1-63 characters of a-z, 0-9 or '-', not starting or ending with '-'"
                .into(),
        ));
    }
    if slug == "local" {
        return Err(BootstrapError::Invalid(
            "slug 'local' is reserved for the local personal tenant".into(),
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub enum BootstrapError {
    Invalid(String),
    Conflict(String),
    Database(sqlx::Error),
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid bootstrap request: {message}"),
            Self::Conflict(message) => write!(f, "bootstrap refused: {message}"),
            // Only the SQLSTATE and server message: never connection strings.
            Self::Database(sqlx::Error::Database(error)) => write!(
                f,
                "database error (SQLSTATE {}): {}",
                error.code().as_deref().unwrap_or("unknown"),
                error.message()
            ),
            Self::Database(_) => write!(f, "database unavailable or protocol error"),
        }
    }
}

impl std::error::Error for BootstrapError {}

impl From<sqlx::Error> for BootstrapError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAction {
    Create,
    Update,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BootstrapStep {
    pub resource: &'static str,
    pub action: StepAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TenantClaim {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BootstrapReport {
    pub tenant_id: Uuid,
    pub slug: String,
    pub display_name: String,
    pub status: &'static str,
    pub default_memory: &'static str,
    pub dry_run: bool,
    /// False when every step was already satisfied (a no-op run).
    pub changed: bool,
    pub steps: Vec<BootstrapStep>,
    pub tenant_claim: TenantClaim,
}

/// Create or update the tenant and its default-memory baseline in one
/// transaction. With `dry_run` the transaction is rolled back, so the report
/// describes exactly what an apply run would do without persisting it.
pub async fn bootstrap_tenant(
    pool: &PgPool,
    request: &BootstrapRequest,
    dry_run: bool,
    tenant_claim_name: &str,
) -> Result<BootstrapReport, BootstrapError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(BOOTSTRAP_LOCK)
        .execute(&mut *tx)
        .await?;

    let mut steps = vec![BootstrapStep {
        resource: "tenant_registry",
        action: upsert_tenant(&mut tx, request).await?,
    }];
    steps.push(BootstrapStep {
        resource: "embedding_config:default",
        action: ensure_default_embedding_config(&mut tx, request.tenant_id).await?,
    });
    // The set and scheme are created under the target tenant's scope.
    set_tenant(&mut tx, request.tenant_id).await?;
    steps.push(BootstrapStep {
        resource: "embedding_set:default",
        action: ensure_default_embedding_set(&mut tx, request.tenant_id).await?,
    });
    steps.push(BootstrapStep {
        resource: "skos_concept_scheme:default",
        action: ensure_default_scheme(&mut tx, request.tenant_id).await?,
    });

    if dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }

    Ok(BootstrapReport {
        tenant_id: request.tenant_id,
        slug: request.slug.clone(),
        display_name: request.display_name.clone(),
        status: "active",
        default_memory: "public",
        dry_run,
        changed: steps.iter().any(|step| step.action != StepAction::None),
        steps,
        tenant_claim: TenantClaim {
            name: tenant_claim_name.to_string(),
            value: request.tenant_id.to_string(),
        },
    })
}

async fn set_tenant(conn: &mut PgConnection, tenant_id: Uuid) -> Result<(), BootstrapError> {
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant_id.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

async fn upsert_tenant(
    conn: &mut PgConnection,
    request: &BootstrapRequest,
) -> Result<StepAction, BootstrapError> {
    let existing = sqlx::query(
        "SELECT slug, display_name, status FROM tenant_registry WHERE id = $1 FOR UPDATE",
    )
    .bind(request.tenant_id)
    .fetch_optional(&mut *conn)
    .await?;

    let Some(row) = existing else {
        let slug_owner: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM tenant_registry WHERE slug = $1")
                .bind(&request.slug)
                .fetch_optional(&mut *conn)
                .await?;
        if slug_owner.is_some() {
            return Err(BootstrapError::Conflict(
                "the slug is already registered to a different tenant id".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO tenant_registry (id, slug, display_name, status) \
             VALUES ($1, $2, $3, 'active')",
        )
        .bind(request.tenant_id)
        .bind(&request.slug)
        .bind(&request.display_name)
        .execute(&mut *conn)
        .await?;
        return Ok(StepAction::Create);
    };

    let slug: String = row.get("slug");
    let display_name: String = row.get("display_name");
    let status: String = row.get("status");
    if slug != request.slug {
        return Err(BootstrapError::Conflict(
            "the tenant id is registered under a different slug; slugs are not renamed".into(),
        ));
    }
    let reactivate = match status.as_str() {
        "active" => false,
        "suspended" if request.reactivate => true,
        "suspended" => {
            return Err(BootstrapError::Conflict(
                "the tenant is suspended; pass --reactivate to restore it".into(),
            ))
        }
        _ => {
            return Err(BootstrapError::Conflict(
                "the tenant is soft-deleted and cannot be bootstrapped".into(),
            ))
        }
    };
    if !reactivate && display_name == request.display_name {
        return Ok(StepAction::None);
    }
    sqlx::query(
        "UPDATE tenant_registry SET display_name = $2, status = 'active', \
         suspended_at = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(request.tenant_id)
    .bind(&request.display_name)
    .execute(&mut *conn)
    .await?;
    Ok(StepAction::Update)
}

/// Copy the deployment's local-tenant default configuration when present so the
/// tenant embeds with the same model as the deployment; otherwise use the
/// migration seed values.
async fn ensure_default_embedding_config(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> Result<StepAction, BootstrapError> {
    set_tenant(&mut *conn, tenant_id).await?;
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM embedding_config WHERE tenant_id = $1 AND is_default LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    if existing.is_some() {
        return Ok(StepAction::None);
    }
    let named: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM embedding_config WHERE tenant_id = $1 AND name = 'default'",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    if named.is_some() {
        return Err(BootstrapError::Conflict(
            "the tenant has an embedding configuration named 'default' that is not marked \
             default; resolve it before bootstrapping"
                .into(),
        ));
    }

    set_tenant(&mut *conn, LOCAL_TENANT).await?;
    let template: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM embedding_config c \
         WHERE c.tenant_id = $1 AND c.is_default ORDER BY c.created_at LIMIT 1",
    )
    .bind(LOCAL_TENANT)
    .fetch_optional(&mut *conn)
    .await?;
    set_tenant(&mut *conn, tenant_id).await?;

    let id = Uuid::now_v7();
    match template {
        Some(template) => {
            sqlx::query(
                "INSERT INTO embedding_config SELECT * FROM jsonb_populate_record(\
                 NULL::embedding_config, $1::jsonb || jsonb_build_object(\
                 'id', $2::uuid, 'tenant_id', $3::uuid, 'name', 'default', 'is_default', true, \
                 'created_at', now(), 'updated_at', now()))",
            )
            .bind(template)
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *conn)
            .await?;
        }
        None => {
            sqlx::query(
                "INSERT INTO embedding_config \
                 (id, tenant_id, name, description, model, dimension, chunk_size, chunk_overlap, is_default) \
                 VALUES ($1, $2, 'default', \
                 'Default embedding configuration using nomic-embed-text (768 dimensions)', \
                 'nomic-embed-text', 768, 1500, 200, TRUE)",
            )
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(StepAction::Create)
}

/// Mirrors the archive initializer's default set, including bootstrap custody.
async fn ensure_default_embedding_set(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> Result<StepAction, BootstrapError> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM embedding_set WHERE tenant_id = $1 AND slug = 'default'",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    if existing.is_some() {
        return Ok(StepAction::None);
    }
    let config_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_config WHERE tenant_id = $1 AND is_default \
         ORDER BY created_at LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_one(&mut *conn)
    .await?;
    let set_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO embedding_set (
            id, tenant_id, name, slug, description, purpose, usage_hints, keywords,
            mode, criteria, embedding_config_id, is_system, is_active, index_status
        ) VALUES (
            $1, $2, 'Default', 'default',
            'Primary embedding set containing all notes. Used for general semantic search.',
            'Provides semantic search across the entire knowledge base.',
            'Use this set for general queries when you want to search all content. This is the default set used when no specific set is specified.',
            ARRAY['all', 'general', 'default', 'everything', 'global'],
            'auto', $3, $4, TRUE, TRUE, 'ready'
        )
        "#,
    )
    .bind(set_id)
    .bind(tenant_id)
    .bind(serde_json::json!({"include_all": true, "exclude_archived": true}))
    .bind(config_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO shard_embedding_set_bootstrap (tenant_id, set_id) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(set_id)
        .execute(&mut *conn)
        .await?;
    Ok(StepAction::Create)
}

/// The scheme URI is globally unique, so tenant copies leave it unset.
async fn ensure_default_scheme(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> Result<StepAction, BootstrapError> {
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM skos_concept_scheme WHERE tenant_id = $1 AND notation = 'default'",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?;
    if existing.is_some() {
        return Ok(StepAction::None);
    }
    let scheme_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO skos_concept_scheme (id, tenant_id, notation, title, description, is_system) \
         VALUES ($1, $2, 'default', 'Default Tags', \
         'Default concept scheme for general-purpose tagging', TRUE)",
    )
    .bind(scheme_id)
    .bind(tenant_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("INSERT INTO shard_skos_scheme_bootstrap (tenant_id, scheme_id) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(scheme_id)
        .execute(&mut *conn)
        .await?;
    Ok(StepAction::Create)
}

/// Parsed `admin bootstrap` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapCli {
    pub request: BootstrapRequest,
    pub dry_run: bool,
    pub json: bool,
    pub skip_migrations: bool,
}

/// Parse arguments following `admin bootstrap`. Flags override the
/// `FORTEMI_BOOTSTRAP_TENANT_{SLUG,ID,DISPLAY_NAME}` environment fallbacks.
pub fn parse_bootstrap_args<I, F>(args: I, env: F) -> Result<BootstrapCli, BootstrapError>
where
    I: IntoIterator<Item = String>,
    F: Fn(&str) -> Option<String>,
{
    let mut slug = env("FORTEMI_BOOTSTRAP_TENANT_SLUG");
    let mut tenant_id = env("FORTEMI_BOOTSTRAP_TENANT_ID");
    let mut display_name = env("FORTEMI_BOOTSTRAP_TENANT_DISPLAY_NAME");
    let (mut reactivate, mut dry_run, mut json, mut skip_migrations) = (false, false, false, false);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value.into())),
            _ => (arg, None),
        };
        let mut value = |name: &str| {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| BootstrapError::Invalid(format!("{name} requires a value; {USAGE}")))
        };
        match flag.as_str() {
            "--slug" | "--tenant-slug" => slug = Some(value("--slug")?),
            "--tenant-id" => tenant_id = Some(value("--tenant-id")?),
            "--display-name" => display_name = Some(value("--display-name")?),
            "--reactivate" => reactivate = true,
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--skip-migrations" => skip_migrations = true,
            _ => {
                return Err(BootstrapError::Invalid(format!(
                    "unrecognized argument; {USAGE}"
                )))
            }
        }
    }
    let slug = slug
        .filter(|slug| !slug.is_empty())
        .ok_or_else(|| BootstrapError::Invalid(format!("--slug is required; {USAGE}")))?;
    let tenant_id = tenant_id
        .filter(|id| !id.is_empty())
        .map(|id| {
            Uuid::parse_str(&id)
                .map_err(|_| BootstrapError::Invalid("--tenant-id must be a UUID".into()))
        })
        .transpose()?;
    Ok(BootstrapCli {
        request: BootstrapRequest::new(&slug, tenant_id, display_name.as_deref(), reactivate)?,
        dry_run,
        json,
        skip_migrations,
    })
}

/// Human-readable report. Contains identifiers only, never credentials.
pub fn render_text(report: &BootstrapReport) -> String {
    let mut out = format!(
        "Fortemi tenant bootstrap{}\n  tenant id:      {}\n  slug:           {}\n  display name:   {}\n  status:         {}\n  default memory: {}\n",
        if report.dry_run { " (dry run, nothing written)" } else { "" },
        report.tenant_id,
        report.slug,
        report.display_name,
        report.status,
        report.default_memory,
    );
    for step in &report.steps {
        let action = match step.action {
            StepAction::Create => "create",
            StepAction::Update => "update",
            StepAction::None => "unchanged",
        };
        out.push_str(&format!("  {:<28} {action}\n", step.resource));
    }
    out.push_str(if report.changed {
        "Result: changed\n"
    } else {
        "Result: unchanged (no-op)\n"
    });
    out.push_str(&format!(
        "Configure the IdP to emit claim {} = {}\n",
        report.tenant_claim.name, report.tenant_claim.value
    ));
    out
}

/// Dispatch `matric-api admin bootstrap ...`. Returns `Ok(false)` when the
/// process arguments are not an admin command, so normal startup continues.
pub async fn run_if_requested() -> anyhow::Result<bool> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().map(|a| a.as_os_str()) != Some("admin".as_ref()) {
        return Ok(false);
    }
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| anyhow::anyhow!("arguments must be valid UTF-8"))
        })
        .collect::<anyhow::Result<_>>()?;
    if args.get(1).map(String::as_str) != Some("bootstrap") {
        anyhow::bail!("{USAGE}");
    }
    dotenvy::dotenv().ok();
    let cli = parse_bootstrap_args(args.into_iter().skip(2), |name| std::env::var(name).ok())?;
    let url = std::env::var("MIGRATION_DATABASE_URL").map_err(|_| {
        anyhow::anyhow!("admin bootstrap requires MIGRATION_DATABASE_URL (the migration role)")
    })?;
    let db = matric_db::Database::connect(&url)
        .await
        .map_err(|_| anyhow::anyhow!("could not connect with MIGRATION_DATABASE_URL"))?;
    if !cli.dry_run && !cli.skip_migrations {
        db.migrate()
            .await
            .map_err(|error| anyhow::anyhow!("migrations failed: {error}"))?;
    }
    let claim = std::env::var("FORTEMI_AUTH_TENANT_CLAIM")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_TENANT_CLAIM.to_string());
    let report = bootstrap_tenant(&db.pool, &cli.request, cli.dry_run, &claim).await?;
    db.pool.close().await;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_text(&report));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<BootstrapCli, BootstrapError> {
        parse_bootstrap_args(args.iter().map(|a| a.to_string()), |_| None)
    }

    #[test]
    fn slug_derivation_is_stable_and_distinct() {
        assert_eq!(derive_tenant_id("acme"), derive_tenant_id("acme"));
        assert_ne!(derive_tenant_id("acme"), derive_tenant_id("acme-2"));
        assert_eq!(derive_tenant_id("acme").get_version_num(), 5);
    }

    #[test]
    fn parses_flags_and_derives_id() {
        let cli = parse(&[
            "--slug",
            "acme",
            "--display-name=Acme Inc",
            "--dry-run",
            "--json",
        ])
        .unwrap();
        assert_eq!(cli.request.tenant_id, derive_tenant_id("acme"));
        assert_eq!(cli.request.display_name, "Acme Inc");
        assert!(cli.dry_run && cli.json && !cli.skip_migrations && !cli.request.reactivate);
    }

    #[test]
    fn explicit_tenant_id_overrides_derivation() {
        let id = Uuid::new_v4();
        let cli = parse(&["--slug", "acme", "--tenant-id", &id.to_string()]).unwrap();
        assert_eq!(cli.request.tenant_id, id);
        assert_eq!(cli.request.display_name, "acme");
    }

    #[test]
    fn env_fallbacks_apply_and_flags_win() {
        let env = |name: &str| match name {
            "FORTEMI_BOOTSTRAP_TENANT_SLUG" => Some("from-env".to_string()),
            "FORTEMI_BOOTSTRAP_TENANT_DISPLAY_NAME" => Some("Env Name".to_string()),
            _ => None,
        };
        let cli = parse_bootstrap_args(Vec::<String>::new(), env).unwrap();
        assert_eq!(cli.request.slug, "from-env");
        let cli = parse_bootstrap_args(vec!["--slug".into(), "flag".into()], env).unwrap();
        assert_eq!(cli.request.slug, "flag");
        assert_eq!(cli.request.display_name, "Env Name");
    }

    #[test]
    fn rejects_invalid_input() {
        for args in [
            vec![],
            vec!["--slug", "Bad_Slug"],
            vec!["--slug", "-edge"],
            vec!["--slug", "local"],
            vec!["--slug", "ok", "--tenant-id", "not-a-uuid"],
            vec![
                "--slug",
                "ok",
                "--tenant-id",
                "00000000-0000-0000-0000-000000000000",
            ],
            vec!["--slug", "ok", "--display-name", "   "],
            vec!["--slug", "ok", "--unknown"],
            vec!["--slug"],
        ] {
            assert!(parse(&args).is_err(), "{args:?} must be rejected");
        }
    }

    #[test]
    fn errors_and_reports_never_leak_connection_strings() {
        let error = BootstrapError::Database(sqlx::Error::Configuration(
            "postgres://user:secret@db.internal/fortemi".into(),
        ));
        let message = error.to_string();
        assert!(!message.contains("secret") && !message.contains("postgres://"));

        let report = BootstrapReport {
            tenant_id: derive_tenant_id("acme"),
            slug: "acme".into(),
            display_name: "Acme".into(),
            status: "active",
            default_memory: "public",
            dry_run: true,
            changed: false,
            steps: vec![BootstrapStep {
                resource: "tenant_registry",
                action: StepAction::None,
            }],
            tenant_claim: TenantClaim {
                name: DEFAULT_TENANT_CLAIM.into(),
                value: derive_tenant_id("acme").to_string(),
            },
        };
        let text = render_text(&report);
        assert!(text.contains("dry run") && text.contains("no-op"));
        assert!(text.contains(&report.tenant_claim.value));
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["steps"][0]["action"], "none");
        assert_eq!(json["changed"], false);
    }
}
