use std::collections::BTreeSet;
use std::net::IpAddr;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use matric_core::{new_v7, Error, Result};

use crate::app_users::AppUser;

type HmacSha256 = Hmac<Sha256>;

const PAT_PREFIX: &str = "mm_pat_";
const PAT_HKDF_SALT: &[u8] = b"fortemi-pat-hmac-salt-v1";
const PAT_HKDF_INFO: &[u8] = b"fortemi-personal-access-token-hmac-v1";

#[derive(Clone)]
pub struct PersonalAccessTokenCrypto {
    hmac_key: [u8; 32],
}

impl PersonalAccessTokenCrypto {
    pub fn derive_from_pepper(pepper: &[u8]) -> Result<Self> {
        if pepper.len() < 32 {
            return Err(Error::Config(
                "FORTEMI_PAT_PEPPER must provide at least 32 bytes of secret material".to_string(),
            ));
        }
        let hkdf = Hkdf::<Sha256>::new(Some(PAT_HKDF_SALT), pepper);
        let mut hmac_key = [0u8; 32];
        hkdf.expand(PAT_HKDF_INFO, &mut hmac_key)
            .map_err(|_| Error::Config("PAT HKDF derivation failed".to_string()))?;
        Ok(Self { hmac_key })
    }

    pub fn derive_from_env_value(value: &str) -> Result<Self> {
        let trimmed = value.trim();
        if let Some(encoded) = trimmed.strip_prefix("base64:") {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| Error::Config("FORTEMI_PAT_PEPPER base64 value is invalid".into()))?;
            Self::derive_from_pepper(&bytes)
        } else {
            Self::derive_from_pepper(trimmed.as_bytes())
        }
    }

    pub fn for_tests() -> Self {
        Self::derive_from_pepper(b"test-only-personal-access-token-pepper-32-bytes-minimum")
            .expect("test PAT pepper must derive")
    }

    fn token_hmac_hex(&self, token: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(&self.hmac_key).expect("HMAC-SHA-256 accepts 32-byte key");
        mac.update(token.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    fn verify_token_hmac_hex(&self, token: &str, expected_hex: &str) -> bool {
        let computed = self.token_hmac_hex(token);
        computed.as_bytes().ct_eq(expected_hex.as_bytes()).into()
    }
}

#[derive(Clone, Debug)]
pub struct PersonalAccessToken {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub token_prefix: String,
    pub token_last4: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_used_ip: Option<String>,
    pub use_count: i64,
}

#[derive(Clone, Debug)]
pub struct CreatedPersonalAccessToken {
    pub token: String,
    pub record: PersonalAccessToken,
}

#[derive(Clone, Debug)]
pub struct ValidatedPersonalAccessToken {
    pub token: PersonalAccessToken,
    pub user: AppUser,
    pub effective_scopes: Vec<String>,
}

#[derive(Clone)]
pub struct PgPersonalAccessTokenRepository {
    pool: PgPool,
    crypto: PersonalAccessTokenCrypto,
}

impl PgPersonalAccessTokenRepository {
    pub fn new(pool: PgPool, crypto: PersonalAccessTokenCrypto) -> Self {
        Self { pool, crypto }
    }

    pub fn generate_token() -> String {
        let mut random = [0u8; 32];
        OsRng.fill_bytes(&mut random);
        let payload = URL_SAFE_NO_PAD.encode(random);
        let checksum = checksum_suffix(&payload);
        format!("{PAT_PREFIX}{payload}_{checksum}")
    }

    pub fn token_format_valid(token: &str) -> bool {
        let Some(rest) = token.strip_prefix(PAT_PREFIX) else {
            return false;
        };
        let Some((payload, checksum)) = rest.rsplit_once('_') else {
            return false;
        };
        if payload.len() < 43 || checksum.len() != 8 {
            return false;
        }
        if !payload
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return false;
        }
        checksum
            .as_bytes()
            .ct_eq(checksum_suffix(payload).as_bytes())
            .into()
    }

    pub async fn create(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        name: &str,
        scopes: &[String],
        expires_at: DateTime<Utc>,
    ) -> Result<CreatedPersonalAccessToken> {
        validate_name(name)?;
        validate_scopes(scopes)?;
        if expires_at <= Utc::now() {
            return Err(Error::InvalidInput(
                "PAT expiry must be in the future".to_string(),
            ));
        }
        let token = Self::generate_token();
        let token_hash = self.crypto.token_hmac_hex(&token);
        let token_prefix = token.chars().take(16).collect::<String>();
        let token_last4 = token
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let id = new_v7();
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let row = sqlx::query(
            "INSERT INTO public.personal_access_token (
                 id, tenant_id, user_id, token_hash, token_prefix, token_last4, name, scopes,
                 status, expires_at, created_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'active', $9, now())
             RETURNING id, tenant_id, user_id, token_prefix, token_last4, name, scopes, status,
                       expires_at, created_at, revoked_at, revoked_reason, last_used_at,
                       last_used_ip, use_count",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(&token_hash)
        .bind(&token_prefix)
        .bind(&token_last4)
        .bind(name)
        .bind(scopes)
        .bind(expires_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(CreatedPersonalAccessToken {
            token,
            record: row_to_pat(row)?,
        })
    }

    pub async fn list_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<PersonalAccessToken>> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let rows = sqlx::query(
            "SELECT id, tenant_id, user_id, token_prefix, token_last4, name, scopes, status,
                    expires_at, created_at, revoked_at, revoked_reason, last_used_at,
                    last_used_ip, use_count
             FROM public.personal_access_token
             WHERE tenant_id = $1 AND user_id = $2
             ORDER BY created_at DESC, id DESC",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        rows.into_iter().map(row_to_pat).collect()
    }

    pub async fn list_for_tenant(&self, tenant_id: Uuid) -> Result<Vec<PersonalAccessToken>> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let rows = sqlx::query(
            "SELECT id, tenant_id, user_id, token_prefix, token_last4, name, scopes, status,
                    expires_at, created_at, revoked_at, revoked_reason, last_used_at,
                    last_used_ip, use_count
             FROM public.personal_access_token
             WHERE tenant_id = $1
             ORDER BY created_at DESC, id DESC",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        rows.into_iter().map(row_to_pat).collect()
    }

    pub async fn revoke_for_user(&self, tenant_id: Uuid, user_id: Uuid, id: Uuid) -> Result<bool> {
        self.revoke_scoped(
            tenant_id,
            id,
            Some(user_id),
            "user_revoked",
            Some("revoked"),
        )
        .await
    }

    pub async fn revoke_for_admin(&self, tenant_id: Uuid, id: Uuid) -> Result<bool> {
        self.revoke_scoped(tenant_id, id, None, "admin_revoked", Some("revoked"))
            .await
    }

    pub async fn revoke_for_disabled_user(&self, tenant_id: Uuid, user_id: Uuid) -> Result<u64> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let result = sqlx::query(
            "UPDATE public.personal_access_token
             SET status = 'revoked', revoked_at = COALESCE(revoked_at, now()),
                 revoked_reason = 'user_disabled'
             WHERE tenant_id = $1 AND user_id = $2 AND status IN ('active', 'suspended')",
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result.rows_affected())
    }

    pub async fn unsuspend_for_user(&self, tenant_id: Uuid, user_id: Uuid) -> Result<u64> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let result = sqlx::query(
            "UPDATE public.personal_access_token
             SET status = 'active'
             WHERE tenant_id = $1 AND user_id = $2 AND status = 'suspended'
               AND expires_at > now()",
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result.rows_affected())
    }

    pub async fn validate(
        &self,
        token: &str,
        revalidate_days: i64,
        client_ip: Option<IpAddr>,
    ) -> Result<Option<ValidatedPersonalAccessToken>> {
        self.validate_inner(token, revalidate_days, client_ip, None)
            .await
    }

    pub async fn validate_for_tenant(
        &self,
        tenant_id: Uuid,
        token: &str,
        revalidate_days: i64,
        client_ip: Option<IpAddr>,
    ) -> Result<Option<ValidatedPersonalAccessToken>> {
        self.validate_inner(token, revalidate_days, client_ip, Some(tenant_id))
            .await
    }

    async fn validate_inner(
        &self,
        token: &str,
        revalidate_days: i64,
        client_ip: Option<IpAddr>,
        tenant_hint: Option<Uuid>,
    ) -> Result<Option<ValidatedPersonalAccessToken>> {
        if !Self::token_format_valid(token) {
            return Ok(None);
        }
        let token_hash = self.crypto.token_hmac_hex(token);
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        if let Some(tenant_id) = tenant_hint {
            bind_tenant(&mut tx, tenant_id).await?;
        }
        let row = sqlx::query(
            "SELECT
                 pat.id AS pat_id, pat.tenant_id, pat.user_id, pat.token_hash, pat.token_prefix,
                 pat.token_last4, pat.name, pat.scopes, pat.status AS pat_status,
                 pat.expires_at, pat.created_at, pat.revoked_at, pat.revoked_reason,
                 pat.last_used_at, pat.last_used_ip, pat.use_count,
                 au.id AS app_user_id, au.iss, au.sub, au.email, au.email_verified,
                 au.display_name, au.groups, au.current_scopes, au.kind, au.azp,
                 au.status AS user_status, au.first_seen_at, au.last_seen_at, au.last_oidc_at
             FROM public.personal_access_token pat
             JOIN public.app_user au ON au.tenant_id = pat.tenant_id AND au.id = pat.user_id
             WHERE pat.token_hash = $1",
        )
        .bind(&token_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(Error::Database)?;
        let Some(row) = row else {
            tx.rollback().await.map_err(Error::Database)?;
            return Ok(None);
        };
        let stored_hash: String = row.try_get("token_hash").map_err(Error::Database)?;
        if !self.crypto.verify_token_hmac_hex(token, &stored_hash) {
            tx.rollback().await.map_err(Error::Database)?;
            return Ok(None);
        }
        let pat_status: String = row.try_get("pat_status").map_err(Error::Database)?;
        let user_status: String = row.try_get("user_status").map_err(Error::Database)?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at").map_err(Error::Database)?;
        let last_oidc_at: DateTime<Utc> = row.try_get("last_oidc_at").map_err(Error::Database)?;
        let now = Utc::now();
        let tenant_id: Uuid = row.try_get("tenant_id").map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        if user_status != "active" || pat_status != "active" || expires_at <= now {
            tx.rollback().await.map_err(Error::Database)?;
            return Ok(None);
        }
        if last_oidc_at < now - Duration::days(revalidate_days) {
            let pat_id: Uuid = row.try_get("pat_id").map_err(Error::Database)?;
            sqlx::query(
                "UPDATE public.personal_access_token
                 SET status = 'suspended', revoked_reason = 'oidc_revalidation_required'
                 WHERE tenant_id = $1 AND id = $2 AND status = 'active'",
            )
            .bind(tenant_id)
            .bind(pat_id)
            .execute(&mut *tx)
            .await
            .map_err(Error::Database)?;
            tx.commit().await.map_err(Error::Database)?;
            return Ok(None);
        }
        let scopes: Vec<String> = row.try_get("scopes").map_err(Error::Database)?;
        let current_scopes: Vec<String> = row.try_get("current_scopes").map_err(Error::Database)?;
        let effective_scopes = intersect_scopes(&scopes, &current_scopes);
        if effective_scopes.is_empty() {
            tx.rollback().await.map_err(Error::Database)?;
            return Ok(None);
        }
        let pat_id: Uuid = row.try_get("pat_id").map_err(Error::Database)?;
        sqlx::query(
            "UPDATE public.personal_access_token
             SET last_used_at = now(), last_used_ip = $3, use_count = use_count + 1
             WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(pat_id)
        .bind(client_ip.map(|ip| ip.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;

        Ok(Some(ValidatedPersonalAccessToken {
            token: row_to_pat_alias(&row)?,
            user: row_to_user_alias(row)?,
            effective_scopes,
        }))
    }

    async fn revoke_scoped(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        user_id: Option<Uuid>,
        reason: &str,
        next_status: Option<&str>,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await.map_err(Error::Database)?;
        bind_tenant(&mut tx, tenant_id).await?;
        let result = sqlx::query(
            "UPDATE public.personal_access_token
             SET status = COALESCE($5, status), revoked_at = COALESCE(revoked_at, now()),
                 revoked_reason = $4
             WHERE tenant_id = $1 AND id = $2 AND ($3::uuid IS NULL OR user_id = $3)
               AND status IN ('active', 'suspended')",
        )
        .bind(tenant_id)
        .bind(id)
        .bind(user_id)
        .bind(reason)
        .bind(next_status)
        .execute(&mut *tx)
        .await
        .map_err(Error::Database)?;
        tx.commit().await.map_err(Error::Database)?;
        Ok(result.rows_affected() > 0)
    }
}

fn checksum_suffix(payload: &str) -> String {
    let digest = Sha256::digest(payload.as_bytes());
    hex::encode(&digest[..4])
}

fn intersect_scopes(token_scopes: &[String], user_scopes: &[String]) -> Vec<String> {
    let user: BTreeSet<&str> = user_scopes.iter().map(String::as_str).collect();
    token_scopes
        .iter()
        .filter(|scope| user.contains(scope.as_str()))
        .cloned()
        .collect()
}

fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(Error::InvalidInput("invalid PAT name".to_string()));
    }
    Ok(())
}

fn validate_scopes(scopes: &[String]) -> Result<()> {
    if scopes.is_empty()
        || scopes.iter().any(|scope| {
            scope.trim() != scope || scope.is_empty() || scope.chars().any(char::is_control)
        })
    {
        return Err(Error::InvalidInput("invalid PAT scopes".to_string()));
    }
    Ok(())
}

async fn bind_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
) -> Result<()> {
    sqlx::query("SELECT set_config('app.current_tenant', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(Error::Database)?;
    Ok(())
}

fn row_to_pat(row: sqlx::postgres::PgRow) -> Result<PersonalAccessToken> {
    pat_from_row(&row, "")
}

fn row_to_pat_alias(row: &sqlx::postgres::PgRow) -> Result<PersonalAccessToken> {
    pat_from_row(row, "pat_")
}

fn pat_from_row(row: &sqlx::postgres::PgRow, prefix: &str) -> Result<PersonalAccessToken> {
    Ok(PersonalAccessToken {
        id: row
            .try_get(format!("{prefix}id").as_str())
            .map_err(Error::Database)?,
        tenant_id: row.try_get("tenant_id").map_err(Error::Database)?,
        user_id: row.try_get("user_id").map_err(Error::Database)?,
        token_prefix: row.try_get("token_prefix").map_err(Error::Database)?,
        token_last4: row.try_get("token_last4").map_err(Error::Database)?,
        name: row.try_get("name").map_err(Error::Database)?,
        scopes: row.try_get("scopes").map_err(Error::Database)?,
        status: row
            .try_get(format!("{prefix}status").as_str())
            .map_err(Error::Database)?,
        expires_at: row.try_get("expires_at").map_err(Error::Database)?,
        created_at: row.try_get("created_at").map_err(Error::Database)?,
        revoked_at: row.try_get("revoked_at").map_err(Error::Database)?,
        revoked_reason: row.try_get("revoked_reason").map_err(Error::Database)?,
        last_used_at: row.try_get("last_used_at").map_err(Error::Database)?,
        last_used_ip: row.try_get("last_used_ip").map_err(Error::Database)?,
        use_count: row.try_get("use_count").map_err(Error::Database)?,
    })
}

fn row_to_user_alias(row: sqlx::postgres::PgRow) -> Result<AppUser> {
    Ok(AppUser {
        id: row.try_get("app_user_id").map_err(Error::Database)?,
        tenant_id: row.try_get("tenant_id").map_err(Error::Database)?,
        iss: row.try_get("iss").map_err(Error::Database)?,
        sub: row.try_get("sub").map_err(Error::Database)?,
        email: row.try_get("email").map_err(Error::Database)?,
        email_verified: row.try_get("email_verified").map_err(Error::Database)?,
        display_name: row.try_get("display_name").map_err(Error::Database)?,
        groups: row.try_get("groups").map_err(Error::Database)?,
        current_scopes: row.try_get("current_scopes").map_err(Error::Database)?,
        kind: row.try_get("kind").map_err(Error::Database)?,
        azp: row.try_get("azp").map_err(Error::Database)?,
        status: row.try_get("user_status").map_err(Error::Database)?,
        first_seen_at: row.try_get("first_seen_at").map_err(Error::Database)?,
        last_seen_at: row.try_get("last_seen_at").map_err(Error::Database)?,
        last_oidc_at: row.try_get("last_oidc_at").map_err(Error::Database)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_have_scanner_friendly_checksum() {
        let token = PgPersonalAccessTokenRepository::generate_token();
        assert!(token.starts_with(PAT_PREFIX));
        assert!(PgPersonalAccessTokenRepository::token_format_valid(&token));
        let mut tampered = token.clone();
        let replacement = if tampered.ends_with('0') { '1' } else { '0' };
        tampered.pop();
        tampered.push(replacement);
        assert!(!PgPersonalAccessTokenRepository::token_format_valid(
            &tampered
        ));
    }

    #[test]
    fn hmac_hash_rejects_plain_sha256_rows() {
        let crypto = PersonalAccessTokenCrypto::for_tests();
        let token = "mm_pat_abcdefghijklmnopqrstuvwxyzABCDEFGHI_01234567";
        let plain = hex::encode(Sha256::digest(token.as_bytes()));
        assert!(!crypto.verify_token_hmac_hex(token, &plain));
    }
}
