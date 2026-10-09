#!/usr/bin/env bash
# Generate SBOMs, sign, attest (SPDX + SLSA v1) and verify published image digests.
#
# Usage: sign-container-images.sh --subjects <file> --out <dir>
#
# Subjects file: one "<family> <repository@sha256:digest>" per line. The CI
# workflow derives it from container-release-evidence receipts, so a new image
# family (api/worker/mcp split images, bundle, ...) is covered as soon as its
# receipt exists. Tags are never signed; only digests.
#
# Environment:
#   COSIGN_KEY_REF     hashivault://<transit-key> (preferred), env://COSIGN_PRIVATE_KEY,
#                      or a key file path (local dry-runs only).
#   COSIGN_PUBLIC_KEY  published public key used for verification (docs/security/cosign.pub).
#   SIGNING_REQUIRED   1 = missing key material fails (release tags); 0 = SBOM only, skip signing.
#   SOURCE_REVISION SOURCE_URI SOURCE_REF WORKFLOW_PATH EVENT_NAME BUILDER_ID INVOCATION_ID
#   hashivault: VAULT_ADDR, VAULT_CACERT, TRANSIT_SECRET_ENGINE_PATH and either VAULT_TOKEN
#   or VAULT_CI_ROLE_ID/VAULT_CI_SECRET_ID (AppRole login; token revoked on exit).
set -euo pipefail

SUBJECTS=""
OUT_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --subjects) SUBJECTS="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    *) echo "sign-container-images: unknown argument $1" >&2; exit 2 ;;
  esac
done
[[ -r "$SUBJECTS" && -n "$OUT_DIR" ]] || { echo "usage: $0 --subjects <file> --out <dir>" >&2; exit 2; }

KEY_REF="${COSIGN_KEY_REF:-}"
PUBLIC_KEY="${COSIGN_PUBLIC_KEY:-docs/security/cosign.pub}"
REQUIRED="${SIGNING_REQUIRED:-1}"
: "${SOURCE_REVISION:?SOURCE_REVISION is required}"
: "${SOURCE_URI:?SOURCE_URI is required}"
: "${SOURCE_REF:?SOURCE_REF is required}"
WORKFLOW_PATH="${WORKFLOW_PATH:-.gitea/workflows/ci-builder.yaml}"
EVENT_NAME="${EVENT_NAME:-push}"
BUILDER_ID="${BUILDER_ID:-${SOURCE_URI}/actions/runners/matric-builder}"
INVOCATION_ID="${INVOCATION_ID:-local}"
STARTED_ON="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIGEST_RE='^[^@[:space:]]+@sha256:[0-9a-f]{64}$'

mkdir -p "$OUT_DIR/sbom" "$OUT_DIR/provenance" "$OUT_DIR/receipts"
work="$(mktemp -d)"
vault_token_owned=0
cleanup() {
  if [[ "$vault_token_owned" == 1 && -n "${VAULT_TOKEN:-}" ]]; then
    curl -fsS --cacert "${VAULT_CACERT}" --max-time 10 -H "X-Vault-Token: ${VAULT_TOKEN}" \
      -X POST "${VAULT_ADDR}/v1/auth/token/revoke-self" >/dev/null 2>&1 ||
      echo "sign-container-images: warning: could not revoke the signing token" >&2
  fi
  rm -rf "$work"
}
trap cleanup EXIT

missing=()
[[ -n "$KEY_REF" ]] || missing+=("COSIGN_KEY_REF variable (e.g. hashivault://fortemi-release-cosign)")
[[ -s "$PUBLIC_KEY" ]] || missing+=("published public key at ${PUBLIC_KEY}")
case "$KEY_REF" in
  hashivault://*)
    [[ -n "${VAULT_ADDR:-}" ]] || missing+=("VAULT_ADDR")
    [[ -r "${VAULT_CACERT:-}" ]] || missing+=("readable VAULT_CACERT")
    if [[ -z "${VAULT_TOKEN:-}" && ( -z "${VAULT_CI_ROLE_ID:-}" || -z "${VAULT_CI_SECRET_ID:-}" ) ]]; then
      missing+=("VAULT_CI_ROLE_ID/VAULT_CI_SECRET_ID secrets for the Transit signing AppRole")
    fi ;;
  env://*)
    key_env="${KEY_REF#env://}"
    [[ -n "${!key_env:-}" ]] || missing+=("${key_env} secret")
    [[ -v COSIGN_PASSWORD ]] || missing+=("COSIGN_PASSWORD secret") ;;
esac

signing=1
if (( ${#missing[@]} )); then
  if [[ "$REQUIRED" == 1 ]]; then
    echo "sign-container-images: signing is required for this run but key material is missing:" >&2
    printf '  - %s\n' "${missing[@]}" >&2
    echo "See docs/deployment/image-promotion.md#operator-key-provisioning." >&2
    exit 1
  fi
  signing=0
  echo "SKIP SIGNING: not a release run and signing key material is absent:"
  printf '  - %s\n' "${missing[@]}"
  echo "SBOMs are still generated; NO signatures or attestations are produced by this run."
fi

if [[ "$signing" == 1 && "$KEY_REF" == hashivault://* ]]; then
  # cosign's Vault client trusts system roots; add the OpenBao CA without
  # dropping public roots (GHCR and other registries still need them).
  system_roots=/etc/ssl/certs/ca-certificates.crt
  [[ -r "$system_roots" ]] || { echo "sign-container-images: ${system_roots} is required" >&2; exit 1; }
  cat "$system_roots" "$VAULT_CACERT" >"$work/ca-bundle.pem"
  export SSL_CERT_FILE="$work/ca-bundle.pem"
fi

# AppRole signing tokens are short-lived (5 minutes on the CI role) and
# signing every image takes longer, so an owned token is refreshed before
# each subject once it is older than TOKEN_REFRESH_SECONDS.
TOKEN_REFRESH_SECONDS="${SIGN_TOKEN_REFRESH_SECONDS:-120}"
vault_token_issued_at=0
vault_login() {
  local fresh
  fresh="$(
    jq -n --arg role_id "$VAULT_CI_ROLE_ID" --arg secret_id "$VAULT_CI_SECRET_ID" \
      '{role_id:$role_id, secret_id:$secret_id}' |
    curl -fsS --cacert "$VAULT_CACERT" --max-time 20 -X POST --data @- \
      "$VAULT_ADDR/v1/auth/approle/login" | jq -er '.auth.client_token'
  )"
  if [[ -n "${CI:-}${GITHUB_ACTIONS:-}" ]]; then printf '::add-mask::%s\n' "$fresh"; fi
  if [[ "$vault_token_owned" == 1 && -n "${VAULT_TOKEN:-}" ]]; then
    curl -fsS --cacert "$VAULT_CACERT" --max-time 10 -H "X-Vault-Token: ${VAULT_TOKEN}" \
      -X POST "${VAULT_ADDR}/v1/auth/token/revoke-self" >/dev/null 2>&1 || true
  fi
  VAULT_TOKEN="$fresh"
  export VAULT_TOKEN
  vault_token_owned=1
  vault_token_issued_at="$(date +%s)"
}
refresh_vault_token() {
  [[ "$vault_token_owned" == 1 ]] || return 0
  if (( $(date +%s) - vault_token_issued_at >= TOKEN_REFRESH_SECONDS )); then
    vault_login
    echo "OpenBao signing token refreshed"
  fi
}

if [[ "$signing" == 1 && "$KEY_REF" == hashivault://* && -z "${VAULT_TOKEN:-}" ]]; then
  vault_login
  echo "OpenBao AppRole login succeeded for Transit signing"
fi

pem_body() { grep -v '^-----' "$1" | tr -d '\n\r '; }
if [[ "$signing" == 1 ]]; then
  cosign public-key --key "$KEY_REF" >"$work/derived.pub"
  if [[ "$(pem_body "$work/derived.pub")" != "$(pem_body "$PUBLIC_KEY")" ]]; then
    echo "sign-container-images: ${KEY_REF} does not match the published key ${PUBLIC_KEY}" >&2
    exit 1
  fi
  # Private signing: no public Rekor upload (internal registry names stay private)
  # and no TSA. Verification therefore uses --insecure-ignore-tlog=true with the key.
  cosign signing-config create --out "$work/signing-config.json"
fi
PUBLIC_KEY_SHA256="$(sha256sum "$PUBLIC_KEY" 2>/dev/null | awk '{print $1}' || true)"

sign_args=(--yes --key "$KEY_REF" --signing-config "$work/signing-config.json")
verify_args=(--key "$PUBLIC_KEY" --insecure-ignore-tlog=true)

attest() { # <type> <predicate> <reference>
  cosign attest "${sign_args[@]}" --type "$1" --predicate "$2" "$3" >/dev/null
}

verified_attestation() { # <type> <reference> -> decoded statements, one JSON per line
  cosign verify-attestation "${verify_args[@]}" --type "$1" "$2" 2>/dev/null |
    jq -c '.payload | @base64d | fromjson'
}

subject_count=0
while read -r family ref extra; do
  [[ -z "${family:-}" || "$family" == \#* ]] && continue
  [[ "$signing" == 1 ]] && refresh_vault_token
  [[ -z "${extra:-}" && "$ref" =~ $DIGEST_RE ]] || {
    echo "sign-container-images: invalid subject line: $family $ref $extra" >&2; exit 1; }
  repo="${ref%@*}"; digest="${ref#*@}"; registry="${repo%%/*}"
  slug="${family}-$(printf '%s' "$registry" | tr -c 'A-Za-z0-9.-' '_')-${digest:7:12}"
  echo "== ${family}: ${ref}"

  docker buildx imagetools inspect --raw "$ref" >"$work/manifest.json"
  observed="sha256:$(sha256sum "$work/manifest.json" | awk '{print $1}')"
  [[ "$observed" == "$digest" ]] || {
    echo "sign-container-images: registry manifest for ${ref} hashes to ${observed}" >&2; exit 1; }
  jq -r '.manifests[]? | select((.platform.os // "unknown") != "unknown")
    | "\(.digest) \(.platform.os)/\(.platform.architecture)\(if .platform.variant then "/" + .platform.variant else "" end)"' \
    "$work/manifest.json" >"$work/targets"
  is_index=1
  if [[ ! -s "$work/targets" ]]; then
    is_index=0
    printf '%s single-manifest\n' "$digest" >"$work/targets"
  fi

  sboms_json="[]"
  while read -r tdigest platform; do
    sbom="$OUT_DIR/sbom/${slug}-$(printf '%s' "$platform" | tr '/' '_')-${tdigest:7:12}.spdx.json"
    syft scan -q "registry:${repo}@${tdigest}" -o "spdx-json=${sbom}"
    sboms_json="$(jq -c --arg p "$platform" --arg d "$tdigest" --arg f "${sbom##*/}" \
      --arg s "$(sha256sum "$sbom" | awk '{print $1}')" \
      --argjson n "$(jq '.packages | length' "$sbom")" \
      '. + [{platform:$p, manifest_digest:$d, file:$f, sha256:$s, packages:$n}]' <<<"$sboms_json")"
    if [[ "$signing" == 1 ]]; then
      attest spdxjson "$sbom" "${repo}@${tdigest}"
      if [[ "$is_index" == 1 ]]; then attest spdxjson "$sbom" "$ref"; fi
    fi
  done <"$work/targets"

  status="sbom-only"
  if [[ "$signing" == 1 ]]; then
    prov="$OUT_DIR/provenance/${slug}.slsa1.json"
    python3 "$SCRIPT_DIR/write-slsa-provenance.py" --family "$family" --subject "$ref" \
      --source-uri "$SOURCE_URI" --source-revision "$SOURCE_REVISION" --source-ref "$SOURCE_REF" \
      --workflow-path "$WORKFLOW_PATH" --event-name "$EVENT_NAME" --builder-id "$BUILDER_ID" \
      --invocation-id "$INVOCATION_ID" --started-on "$STARTED_ON" \
      --finished-on "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --output "$prov"

    cosign sign "${sign_args[@]}" --recursive "$ref" >/dev/null
    attest slsaprovenance1 "$prov" "$ref"

    cosign verify "${verify_args[@]}" "$ref" >/dev/null
    while read -r tdigest _; do
      cosign verify "${verify_args[@]}" "${repo}@${tdigest}" >/dev/null
    done <"$work/targets"
    verified_attestation spdxjson "$ref" >"$work/spdx.jsonl"
    verified_attestation slsaprovenance1 "$ref" >"$work/slsa.jsonl"
    want_sboms="$(wc -l <"$work/targets")"
    [[ "$(jq -s 'map(select(.predicateType == "https://spdx.dev/Document")) | length' "$work/spdx.jsonl")" -ge "$want_sboms" ]] || {
      echo "sign-container-images: verified SPDX attestations missing for ${ref}" >&2; exit 1; }
    jq -se --arg rev "$SOURCE_REVISION" --arg d "${digest#sha256:}" \
      'any(.[]; .predicateType == "https://slsa.dev/provenance/v1"
        and .subject[0].digest.sha256 == $d
        and .predicate.buildDefinition.resolvedDependencies[0].digest.gitCommit == $rev)' \
      "$work/slsa.jsonl" >/dev/null || {
      echo "sign-container-images: verified SLSA provenance does not bind ${ref} to ${SOURCE_REVISION}" >&2; exit 1; }
    status="signed-attested-verified"
  fi

  jq -n --arg family "$family" --arg ref "$ref" --arg digest "$digest" --arg status "$status" \
    --arg key_ref_kind "${KEY_REF%%://*}" --arg pub "$PUBLIC_KEY" --arg pub_sha "$PUBLIC_KEY_SHA256" \
    --arg rev "$SOURCE_REVISION" --argjson index "$is_index" --argjson sboms "$sboms_json" \
    '{_type:"fortemi.container-supply-chain-evidence.v1", family:$family,
      subject:{immutable_reference:$ref, digest:$digest, index:($index == 1)},
      source_revision:$rev, status:$status, sboms:$sboms,
      signature:{format:"cosign-sigstore-bundle", transparency_log:false,
        key_reference_kind:$key_ref_kind, public_key:$pub, public_key_sha256:$pub_sha},
      attestations:(if $status == "sbom-only" then [] else ["spdxjson","slsaprovenance1"] end)}' \
    >"$OUT_DIR/receipts/${slug}.json"
  echo "   ${status}"
  subject_count=$((subject_count + 1))
done <"$SUBJECTS"

(( subject_count > 0 )) || { echo "sign-container-images: no subjects to process" >&2; exit 1; }
echo "processed ${subject_count} image digest(s); evidence in ${OUT_DIR}"
