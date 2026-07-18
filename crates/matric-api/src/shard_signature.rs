use std::collections::HashMap;

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SIGNATURE_ENTRY: &str = "signature.json";
pub const MAX_SIGNATURE_ENVELOPE_BYTES: usize = 64 * 1024;
pub const SIGNATURE_SCHEMA: &str =
    include_str!("../../../contracts/knowledge-shard/1.1.0/full-v1/signature.schema.json");
const SIGNING_ENVELOPE_VERSION: &str = "1";
const SIGNING_ALGORITHM: &str = "ed25519";
const MAX_TRUST_STORE_BYTES: usize = 64 * 1024;
const MAX_TRUSTED_KEYS: usize = 256;
const MAX_KEY_ID_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ShardSignaturePolicy {
    Require,
    Prefer,
    TrustedLocalOnly,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShardSigner {
    key_id: String,
    algorithm: String,
    public_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShardSignatureEnvelope {
    format_version: String,
    signer: ShardSigner,
    manifest_digest: String,
    blob_digests: Vec<String>,
    signature: String,
}

#[derive(Serialize)]
struct CanonicalSigner<'a> {
    algorithm: &'a str,
    key_id: &'a str,
    public_key: &'a str,
}

#[derive(Serialize)]
struct CanonicalPayload<'a> {
    blob_digests: Vec<&'a str>,
    format_version: &'a str,
    manifest_digest: &'a str,
    signer: CanonicalSigner<'a>,
}

#[derive(Deserialize)]
struct TrustedKeyConfig {
    key_id: String,
    public_key: String,
    #[serde(default)]
    revoked: bool,
}

struct TrustedKey {
    public_key: String,
    public_key_bytes: [u8; 32],
    revoked: bool,
}

pub struct ShardTrustStore {
    keys: HashMap<String, TrustedKey>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ShardSignatureVerdict {
    Valid,
    Unsigned,
    Malformed,
    UnknownSigner,
    Revoked,
    BadSignature,
    ContentMismatch,
}

impl ShardSignatureVerdict {
    fn reason(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Unsigned => "unsigned",
            Self::Malformed => "malformed",
            Self::UnknownSigner => "unknown-signer",
            Self::Revoked => "revoked",
            Self::BadSignature => "bad-signature",
            Self::ContentMismatch => "content-mismatch",
        }
    }
}

pub fn parse_trust_store(value: Option<&str>) -> Result<Option<ShardTrustStore>, String> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    if value.len() > MAX_TRUST_STORE_BYTES {
        return Err(
            "Knowledge shard trusted-key configuration exceeds the size limit.".to_string(),
        );
    }
    let configs = serde_json::from_str::<Vec<TrustedKeyConfig>>(value)
        .map_err(|_| "Knowledge shard trusted-key configuration is invalid.".to_string())?;
    if configs.is_empty() || configs.len() > MAX_TRUSTED_KEYS {
        return Err("Knowledge shard trusted-key configuration is invalid.".to_string());
    }

    let mut keys = HashMap::with_capacity(configs.len());
    for config in configs {
        if config.key_id.is_empty() || config.key_id.len() > MAX_KEY_ID_BYTES {
            return Err("Knowledge shard trusted-key configuration is invalid.".to_string());
        }
        let decoded = decode_base64url(&config.public_key)
            .ok_or_else(|| "Knowledge shard trusted-key configuration is invalid.".to_string())?;
        let public_key_bytes: [u8; 32] = decoded
            .try_into()
            .map_err(|_| "Knowledge shard trusted-key configuration is invalid.".to_string())?;
        if keys
            .insert(
                config.key_id,
                TrustedKey {
                    public_key: config.public_key,
                    public_key_bytes,
                    revoked: config.revoked,
                },
            )
            .is_some()
        {
            return Err("Knowledge shard trusted-key configuration is invalid.".to_string());
        }
    }
    Ok(Some(ShardTrustStore { keys }))
}

fn decode_base64url(value: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()
}

fn canonical_payload_bytes(envelope: &ShardSignatureEnvelope) -> Result<Vec<u8>, ()> {
    let mut blob_digests = envelope
        .blob_digests
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    blob_digests.sort_unstable();
    serde_json::to_vec(&CanonicalPayload {
        blob_digests,
        format_version: &envelope.format_version,
        manifest_digest: &envelope.manifest_digest,
        signer: CanonicalSigner {
            algorithm: &envelope.signer.algorithm,
            key_id: &envelope.signer.key_id,
            public_key: &envelope.signer.public_key,
        },
    })
    .map_err(|_| ())
}

fn parse_signature_envelope(data: &[u8]) -> Result<ShardSignatureEnvelope, ()> {
    use std::sync::LazyLock;

    static VALIDATOR: LazyLock<Result<jsonschema::Validator, ()>> = LazyLock::new(|| {
        let schema = serde_json::from_str::<serde_json::Value>(SIGNATURE_SCHEMA).map_err(|_| ())?;
        jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema)
            .map_err(|_| ())
    });
    let value = serde_json::from_slice::<serde_json::Value>(data).map_err(|_| ())?;
    let validator = VALIDATOR.as_ref().map_err(|_| ())?;
    if !validator.is_valid(&value) {
        return Err(());
    }
    serde_json::from_value(value).map_err(|_| ())
}

#[cfg(test)]
pub(super) fn create_test_signature_envelope(
    manifest: &[u8],
    blob_digests: &[&str],
) -> (Vec<u8>, String, String) {
    use ring::signature::KeyPair;

    let key_pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7_u8; 32]).unwrap();
    let public_key =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key_pair.public_key().as_ref());
    let manifest_digest = hex::encode(Sha256::digest(manifest));
    let mut sorted_digests = blob_digests.to_vec();
    sorted_digests.sort_unstable();
    let canonical = serde_json::to_vec(&CanonicalPayload {
        blob_digests: sorted_digests,
        format_version: SIGNING_ENVELOPE_VERSION,
        manifest_digest: &manifest_digest,
        signer: CanonicalSigner {
            algorithm: SIGNING_ALGORITHM,
            key_id: "fortemi-fixture-1",
            public_key: &public_key,
        },
    })
    .unwrap();
    let signed_digest = hex::encode(Sha256::digest(canonical));
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(key_pair.sign(signed_digest.as_bytes()).as_ref());
    let envelope = serde_json::json!({
        "format_version": SIGNING_ENVELOPE_VERSION,
        "signer": {
            "key_id": "fortemi-fixture-1",
            "algorithm": SIGNING_ALGORITHM,
            "public_key": public_key,
        },
        "manifest_digest": manifest_digest,
        "blob_digests": blob_digests,
        "signature": signature,
    });
    (
        serde_json::to_vec_pretty(&envelope).unwrap(),
        public_key,
        signature,
    )
}

pub fn verify_shard_signature<'a>(
    files: &HashMap<String, Vec<u8>>,
    sidecar_checksums: impl Iterator<Item = &'a String>,
    trust_store: &ShardTrustStore,
) -> ShardSignatureVerdict {
    let Some(signature_bytes) = files.get(SIGNATURE_ENTRY) else {
        return ShardSignatureVerdict::Unsigned;
    };
    if signature_bytes.len() > MAX_SIGNATURE_ENVELOPE_BYTES {
        return ShardSignatureVerdict::Malformed;
    }
    let Ok(envelope) = parse_signature_envelope(signature_bytes) else {
        return ShardSignatureVerdict::Malformed;
    };
    if envelope.format_version != SIGNING_ENVELOPE_VERSION
        || envelope.signer.algorithm != SIGNING_ALGORITHM
    {
        return ShardSignatureVerdict::Malformed;
    }
    let Some(trusted) = trust_store.keys.get(&envelope.signer.key_id) else {
        return ShardSignatureVerdict::UnknownSigner;
    };
    if trusted.revoked {
        return ShardSignatureVerdict::Revoked;
    }
    if trusted.public_key != envelope.signer.public_key {
        return ShardSignatureVerdict::BadSignature;
    }

    let Some(signature) = decode_base64url(&envelope.signature) else {
        return ShardSignatureVerdict::Malformed;
    };
    if signature.len() != 64 {
        return ShardSignatureVerdict::Malformed;
    }
    let Ok(payload) = canonical_payload_bytes(&envelope) else {
        return ShardSignatureVerdict::Malformed;
    };
    let signed_digest = hex::encode(Sha256::digest(payload));
    let public_key = ring::signature::UnparsedPublicKey::new(
        &ring::signature::ED25519,
        trusted.public_key_bytes,
    );
    if public_key
        .verify(signed_digest.as_bytes(), &signature)
        .is_err()
    {
        return ShardSignatureVerdict::BadSignature;
    }

    let Some(manifest) = files.get("manifest.json") else {
        return ShardSignatureVerdict::ContentMismatch;
    };
    if hex::encode(Sha256::digest(manifest)) != envelope.manifest_digest {
        return ShardSignatureVerdict::ContentMismatch;
    }
    let mut archive_digests = sidecar_checksums
        .filter_map(|checksum| checksum.strip_prefix("blake3:"))
        .collect::<Vec<_>>();
    archive_digests.sort_unstable();
    let mut signed_digests = envelope
        .blob_digests
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    signed_digests.sort_unstable();
    if archive_digests != signed_digests {
        return ShardSignatureVerdict::ContentMismatch;
    }

    ShardSignatureVerdict::Valid
}

pub fn enforce_signature_policy<'a>(
    files: &HashMap<String, Vec<u8>>,
    sidecar_checksums: impl Iterator<Item = &'a String>,
    requested_policy: Option<ShardSignaturePolicy>,
    trust_store: Option<&ShardTrustStore>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let policy = requested_policy.or_else(|| trust_store.map(|_| ShardSignaturePolicy::Require));
    let Some(policy) = policy else {
        return Ok(());
    };
    if matches!(policy, ShardSignaturePolicy::TrustedLocalOnly) {
        warnings.push(
            "Signature verification skipped under trusted-local-only policy; publisher provenance was not authenticated."
                .to_string(),
        );
        return Ok(());
    }
    let trust_store = trust_store.ok_or_else(|| {
        "Knowledge shard signature verification requires a configured trusted-key allowlist."
            .to_string()
    })?;
    let verdict = verify_shard_signature(files, sidecar_checksums, trust_store);
    match verdict {
        ShardSignatureVerdict::Valid => Ok(()),
        ShardSignatureVerdict::Unsigned if matches!(policy, ShardSignaturePolicy::Prefer) => {
            warnings.push(
                "Knowledge shard is unsigned; publisher provenance was not authenticated."
                    .to_string(),
            );
            Ok(())
        }
        ShardSignatureVerdict::Unsigned => {
            Err("Knowledge shard is unsigned and signature verification is required.".to_string())
        }
        _ => Err(format!(
            "Knowledge shard signature verification failed: {}.",
            verdict.reason()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;

    fn signed_fixture(
        manifest: &[u8],
        blob_digests: &[&str],
    ) -> (HashMap<String, Vec<u8>>, Vec<String>, ShardTrustStore) {
        let key_pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7_u8; 32]).unwrap();
        let public_key =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key_pair.public_key().as_ref());
        let manifest_digest = hex::encode(Sha256::digest(manifest));
        let mut sorted_digests = blob_digests.to_vec();
        sorted_digests.sort_unstable();
        let canonical = serde_json::to_vec(&CanonicalPayload {
            blob_digests: sorted_digests.clone(),
            format_version: SIGNING_ENVELOPE_VERSION,
            manifest_digest: &manifest_digest,
            signer: CanonicalSigner {
                algorithm: SIGNING_ALGORITHM,
                key_id: "publisher-1",
                public_key: &public_key,
            },
        })
        .unwrap();
        assert_eq!(
            String::from_utf8(canonical.clone()).unwrap(),
            format!(
                "{{\"blob_digests\":[{}],\"format_version\":\"1\",\"manifest_digest\":\"{manifest_digest}\",\"signer\":{{\"algorithm\":\"ed25519\",\"key_id\":\"publisher-1\",\"public_key\":\"{public_key}\"}}}}",
                sorted_digests
                    .iter()
                    .map(|digest| format!("\"{digest}\""))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        );
        let signed_digest = hex::encode(Sha256::digest(canonical));
        let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(key_pair.sign(signed_digest.as_bytes()).as_ref());
        let envelope = serde_json::json!({
            "format_version": SIGNING_ENVELOPE_VERSION,
            "signer": {
                "key_id": "publisher-1",
                "algorithm": SIGNING_ALGORITHM,
                "public_key": public_key,
            },
            "manifest_digest": manifest_digest,
            "blob_digests": blob_digests,
            "signature": signature,
        });
        let files = HashMap::from([
            ("manifest.json".to_string(), manifest.to_vec()),
            (
                SIGNATURE_ENTRY.to_string(),
                serde_json::to_vec_pretty(&envelope).unwrap(),
            ),
        ]);
        let sidecars = blob_digests
            .iter()
            .map(|digest| format!("blake3:{digest}"))
            .collect::<Vec<_>>();
        let trust_store = parse_trust_store(Some(
            &serde_json::json!([{
                "key_id": "publisher-1",
                "public_key": public_key,
            }])
            .to_string(),
        ))
        .unwrap()
        .unwrap();
        (files, sidecars, trust_store)
    }

    #[test]
    fn integrated_candidate_signature_values_are_stable() {
        let manifest = br#"{"candidate":"manifest"}"#;
        let digest = "1098b345e8aacd29e640d3bf724368680c1bfd401b5a9105cb2dc924740c27ad";
        let (envelope, public_key, signature) = create_test_signature_envelope(manifest, &[digest]);
        parse_signature_envelope(&envelope).unwrap();
        assert_eq!(public_key, "6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw");
        assert_eq!(
            signature,
            "6pbNpeneu73cxa5VTRXMwB4iA7jxVtfoHispHwWS5ToqkJcin4q2QLBlbx3kQK30zCW0jPOyQYF-u_9KTA7KCA"
        );
    }

    #[test]
    fn strict_signature_schema_rejects_unknown_fields_and_digest_drift() {
        let digest = "a".repeat(64);
        let (files, sidecars, trust_store) = signed_fixture(br#"{"version":"1.1.0"}"#, &[&digest]);
        let envelope: serde_json::Value = serde_json::from_slice(&files[SIGNATURE_ENTRY]).unwrap();

        for invalid in [
            {
                let mut value = envelope.clone();
                value["undeclared"] = serde_json::json!(true);
                value
            },
            {
                let mut value = envelope.clone();
                value["manifest_digest"] = serde_json::json!("A".repeat(64));
                value
            },
            {
                let mut value = envelope.clone();
                value["blob_digests"] = serde_json::json!([digest.clone(), digest]);
                value
            },
        ] {
            let mut invalid_files = files.clone();
            invalid_files.insert(
                SIGNATURE_ENTRY.to_string(),
                serde_json::to_vec(&invalid).unwrap(),
            );
            assert_eq!(
                verify_shard_signature(&invalid_files, sidecars.iter(), &trust_store),
                ShardSignatureVerdict::Malformed
            );
        }
    }

    #[test]
    fn verifies_react_compatible_canonical_signature_and_content_commitments() {
        let first_digest = "a".repeat(64);
        let second_digest = "b".repeat(64);
        let (mut files, sidecars, trust_store) =
            signed_fixture(br#"{"version":"1.1.0"}"#, &[&second_digest, &first_digest]);
        assert_eq!(
            verify_shard_signature(&files, sidecars.iter(), &trust_store),
            ShardSignatureVerdict::Valid
        );

        files.insert(
            "manifest.json".to_string(),
            br#"{"version":"1.1.1"}"#.to_vec(),
        );
        assert_eq!(
            verify_shard_signature(&files, sidecars.iter(), &trust_store),
            ShardSignatureVerdict::ContentMismatch
        );
    }

    #[test]
    fn rejects_unknown_revoked_substituted_and_corrupt_signatures() {
        let (mut files, sidecars, trust_store) = signed_fixture(br#"{"version":"1.1.0"}"#, &[]);
        let unknown = parse_trust_store(Some(
            r#"[{"key_id":"other","public_key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}]"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            verify_shard_signature(&files, sidecars.iter(), &unknown),
            ShardSignatureVerdict::UnknownSigner
        );

        let envelope: serde_json::Value = serde_json::from_slice(&files[SIGNATURE_ENTRY]).unwrap();
        let public_key = envelope["signer"]["public_key"].as_str().unwrap();
        let revoked = parse_trust_store(Some(
            &serde_json::json!([{
                "key_id": "publisher-1",
                "public_key": public_key,
                "revoked": true,
            }])
            .to_string(),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            verify_shard_signature(&files, sidecars.iter(), &revoked),
            ShardSignatureVerdict::Revoked
        );

        let substituted = parse_trust_store(Some(
            r#"[{"key_id":"publisher-1","public_key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}]"#,
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            verify_shard_signature(&files, sidecars.iter(), &substituted),
            ShardSignatureVerdict::BadSignature
        );

        let mut envelope: serde_json::Value =
            serde_json::from_slice(&files[SIGNATURE_ENTRY]).unwrap();
        let mut signature = decode_base64url(envelope["signature"].as_str().unwrap()).unwrap();
        signature[0] ^= 0xff;
        envelope["signature"] = serde_json::Value::String(
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature),
        );
        files.insert(
            SIGNATURE_ENTRY.to_string(),
            serde_json::to_vec(&envelope).unwrap(),
        );
        assert_eq!(
            verify_shard_signature(&files, sidecars.iter(), &revoked),
            ShardSignatureVerdict::Revoked
        );
        assert_eq!(
            verify_shard_signature(&files, sidecars.iter(), &trust_store),
            ShardSignatureVerdict::BadSignature
        );
    }

    #[test]
    fn policy_matrix_fails_closed_for_present_invalid_signatures() {
        let (files, sidecars, trust_store) = signed_fixture(br#"{"version":"1.1.0"}"#, &[]);
        let unsigned = HashMap::from([(
            "manifest.json".to_string(),
            br#"{"version":"1.1.0"}"#.to_vec(),
        )]);
        let mut warnings = Vec::new();
        assert!(enforce_signature_policy(
            &unsigned,
            sidecars.iter(),
            Some(ShardSignaturePolicy::Prefer),
            Some(&trust_store),
            &mut warnings,
        )
        .is_ok());
        assert_eq!(warnings.len(), 1);
        assert!(enforce_signature_policy(
            &unsigned,
            sidecars.iter(),
            Some(ShardSignaturePolicy::Require),
            Some(&trust_store),
            &mut Vec::new(),
        )
        .is_err());

        let mut invalid = files;
        invalid.insert(SIGNATURE_ENTRY.to_string(), b"{}".to_vec());
        assert!(enforce_signature_policy(
            &invalid,
            sidecars.iter(),
            Some(ShardSignaturePolicy::Prefer),
            Some(&trust_store),
            &mut Vec::new(),
        )
        .is_err());
    }

    #[test]
    fn trust_store_rejects_duplicate_keys_and_invalid_public_keys() {
        let duplicate = serde_json::json!([
            {
                "key_id": "publisher-1",
                "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            },
            {
                "key_id": "publisher-1",
                "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            }
        ])
        .to_string();
        assert!(parse_trust_store(Some(&duplicate)).is_err());
        assert!(parse_trust_store(Some(
            r#"[{"key_id":"publisher-1","public_key":"too-short"}]"#
        ))
        .is_err());
        assert!(parse_trust_store(Some("[]")).is_err());
    }
}
