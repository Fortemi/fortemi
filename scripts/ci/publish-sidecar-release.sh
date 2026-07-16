#!/usr/bin/env bash
set -euo pipefail

MODE="${1:?usage: publish-sidecar-release.sh <immutable|rolling>}"
: "${GITEA_API:?GITEA_API is required}"
: "${REPO:?REPO is required}"
: "${GITEA_TOKEN:?GITEA_TOKEN is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"

API="${GITEA_API}/repos/${REPO}"
SHORT_SHA="${GITHUB_SHA:0:7}"
BINARIES=(
  matric-api-x86_64-unknown-linux-gnu
  matric-api-aarch64-apple-darwin
  matric-api-x86_64-apple-darwin
)

for binary in "${BINARIES[@]}"; do
  test -s "${binary}" || {
    echo "missing sidecar asset: ${binary}" >&2
    exit 1
  }
done

sha256sum "${BINARIES[@]}" > SHA256SUMS.txt

jq -n \
  --arg commit "${GITHUB_SHA}" \
  --arg repository "${REPO}" \
  --arg run_id "${GITHUB_RUN_ID:-unknown}" \
  --arg run_attempt "${GITHUB_RUN_ATTEMPT:-unknown}" \
  --arg built_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --rawfile sums SHA256SUMS.txt \
  '{
    "_type": "https://in-toto.io/Statement/v1",
    "subject": (
      $sums
      | split("\n")
      | map(select(length > 0))
      | map(capture("^(?<sha256>[0-9a-f]{64})  (?<name>.+)$"))
      | map({name: .name, digest: {sha256: .sha256}})
    ),
    "predicateType": "https://slsa.dev/provenance/v1",
    "predicate": {
      buildDefinition: {
        buildType: "https://git.integrolabs.net/Fortemi/fortemi/.gitea/workflows/publish-sidecar.yml",
        externalParameters: {repository: $repository, commit: $commit},
        internalParameters: {},
        resolvedDependencies: [
          {uri: ("git+https://git.integrolabs.net/" + $repository), digest: {gitCommit: $commit}}
        ]
      },
      runDetails: {
        builder: {id: "https://git.integrolabs.net/actions"},
        metadata: {
          invocationId: $run_id,
          startedOn: $built_at,
          finishedOn: $built_at
        },
        byproducts: [{name: "gitea-run-attempt", value: $run_attempt}]
      }
    }
  }' > sidecar-provenance.intoto.json

release_by_tag() {
  curl -fsS -H "Authorization: token ${GITEA_TOKEN}" \
    "${API}/releases/tags/$1" 2>/dev/null || true
}

release_by_id() {
  curl -fsS -H "Authorization: token ${GITEA_TOKEN}" \
    "${API}/releases/$1"
}

wait_for_release_tag() {
  local tag="$1"
  local expected_id="$2"
  local response
  for _ in {1..10}; do
    response=$(release_by_tag "${tag}")
    if [[ -n "${response}" ]] && \
      jq -e --arg tag "${tag}" --argjson id "${expected_id}" \
      '.tag_name == $tag and .id == $id' >/dev/null 2>&1 <<<"${response}"; then
      printf '%s' "${response}"
      return 0
    fi
    sleep 1
  done
  echo "release tag index did not expose ${tag} (id=${expected_id})" >&2
  return 1
}

download_asset() {
  local release_json="$1"
  local asset_name="$2"
  local output="$3"
  local url
  url=$(jq -r --arg name "${asset_name}" \
    '.assets[] | select(.name == $name) | .browser_download_url' <<<"${release_json}")
  test -n "${url}" && test "${url}" != "null" || return 1
  curl -fsSL -H "Authorization: token ${GITEA_TOKEN}" "${url}" -o "${output}"
}

verify_existing_immutable() {
  local release_json="$1"
  local tag="$2"
  local target
  local actual_tag
  actual_tag=$(jq -r '.tag_name // empty' <<<"${release_json}")
  if [[ "${actual_tag}" != "${tag}" ]]; then
    echo "immutable release tag mismatch: expected ${tag}, got ${actual_tag:-missing}" >&2
    return 1
  fi
  target=$(jq -r '.target_commitish // empty' <<<"${release_json}")
  if [[ -n "${target}" && "${target}" != "${GITHUB_SHA}" ]]; then
    echo "immutable release target mismatch: expected ${GITHUB_SHA}, got ${target:-missing}" >&2
    return 1
  fi

  local tmp
  tmp=$(mktemp -d)
  trap 'rm -rf "${tmp}"' RETURN
  local expected_assets actual_assets
  expected_assets=$(printf '%s\n' "${BINARIES[@]}" SHA256SUMS.txt \
    sidecar-provenance.intoto.json | sort)
  actual_assets=$(jq -r '.assets[].name' <<<"${release_json}" | sort)
  if [[ "${actual_assets}" != "${expected_assets}" ]]; then
    echo "immutable release asset set mismatch" >&2
    diff -u <(printf '%s\n' "${expected_assets}") \
      <(printf '%s\n' "${actual_assets}") >&2 || true
    return 1
  fi
  download_asset "${release_json}" SHA256SUMS.txt "${tmp}/SHA256SUMS.txt"
  download_asset "${release_json}" sidecar-provenance.intoto.json \
    "${tmp}/sidecar-provenance.intoto.json"
  cmp -s SHA256SUMS.txt "${tmp}/SHA256SUMS.txt" || {
    echo "immutable release checksum manifest replacement detected" >&2
    return 1
  }
  cmp -s sidecar-provenance.intoto.json "${tmp}/sidecar-provenance.intoto.json" || {
    echo "immutable release provenance replacement detected" >&2
    return 1
  }

  for binary in "${BINARIES[@]}"; do
    download_asset "${release_json}" "${binary}" "${tmp}/${binary}"
    (
      cd "${tmp}"
      sha256sum -c --ignore-missing SHA256SUMS.txt
    )
  done
  echo "immutable sidecar release already exists and matches local publication"
}

create_release() {
  local tag="$1"
  local name="$2"
  local body="$3"
  local prerelease="$4"
  curl -fsS -X POST \
    -H "Authorization: token ${GITEA_TOKEN}" \
    -H "Content-Type: application/json" \
    "${API}/releases" \
    -d "$(
      jq -n \
        --arg tag "${tag}" \
        --arg target "${GITHUB_SHA}" \
        --arg name "${name}" \
        --arg body "${body}" \
        --argjson prerelease "${prerelease}" \
        '{
          tag_name: $tag,
          target_commitish: $target,
          name: $name,
          body: $body,
          draft: false,
          prerelease: $prerelease
        }'
    )"
}

remove_preassociated_assets() {
  local release_json="$1"
  local release_id="$2"
  while read -r asset_id asset_name; do
    [[ -n "${asset_id}" ]] || continue
    echo "removing pre-associated release asset ${asset_name} (${asset_id})"
    curl -fsS -X DELETE \
      -H "Authorization: token ${GITEA_TOKEN}" \
      "${API}/releases/${release_id}/assets/${asset_id}"
  done < <(jq -r '.assets[]? | "\(.id) \(.name)"' <<<"${release_json}")
}

upload_assets() {
  local release_id="$1"
  for asset in "${BINARIES[@]}" SHA256SUMS.txt sidecar-provenance.intoto.json; do
    echo "uploading ${asset}"
    curl -fsS -X POST \
      -H "Authorization: token ${GITEA_TOKEN}" \
      -H "Content-Type: application/octet-stream" \
      "${API}/releases/${release_id}/assets?name=${asset}" \
      --data-binary @"${asset}" >/dev/null
  done
}

case "${MODE}" in
  immutable)
    TAG="sidecar-${GITHUB_SHA:0:12}"
    EXISTING=$(release_by_tag "${TAG}")
    if [[ -n "${EXISTING}" ]] && \
      jq -e --arg tag "${TAG}" '.id and .tag_name == $tag' \
      >/dev/null 2>&1 <<<"${EXISTING}"; then
      verify_existing_immutable "${EXISTING}" "${TAG}"
      exit 0
    fi

    BODY="Immutable native sidecar binaries built from commit ${GITHUB_SHA}.

Verify the downloaded assets with SHA256SUMS.txt and
sidecar-provenance.intoto.json. This release identity is append-only and must
never be replaced. Use sidecar-latest only to discover the current immutable
tag."
    RESPONSE=$(create_release "${TAG}" "Sidecar Binaries (${SHORT_SHA})" "${BODY}" true)
    if ! jq -e --arg tag "${TAG}" --arg target "${GITHUB_SHA}" \
      '.id and .tag_name == $tag and .target_commitish == $target' \
      >/dev/null 2>&1 <<<"${RESPONSE}"; then
      echo "immutable release creation returned an unexpected response" >&2
      jq '{id, tag_name, target_commitish, message}' <<<"${RESPONSE}" >&2 || true
      exit 1
    fi
    RELEASE_ID=$(jq -er '.id' <<<"${RESPONSE}")
    echo "created immutable release ${TAG} (id=${RELEASE_ID})"
    remove_preassociated_assets "${RESPONSE}" "${RELEASE_ID}"
    upload_assets "${RELEASE_ID}"
    verify_existing_immutable "$(release_by_id "${RELEASE_ID}")" "${TAG}"
    wait_for_release_tag "${TAG}" "${RELEASE_ID}" >/dev/null
    echo "published immutable sidecar release ${TAG}"
    ;;
  rolling)
    TAG="sidecar-latest"
    IMMUTABLE_TAG="sidecar-${GITHUB_SHA:0:12}"
    EXISTING=$(release_by_tag "${TAG}")
    if jq -e '.id' >/dev/null 2>&1 <<<"${EXISTING}"; then
      RELEASE_ID=$(jq -er '.id' <<<"${EXISTING}")
      curl -fsS -X DELETE \
        -H "Authorization: token ${GITEA_TOKEN}" \
        "${API}/releases/${RELEASE_ID}"
    fi
    curl -fsS -X DELETE \
      -H "Authorization: token ${GITEA_TOKEN}" \
      "${API}/tags/${TAG}" 2>/dev/null || true

    BODY="Rolling discovery pointer for native sidecar binaries.

Current immutable release: ${IMMUTABLE_TAG}
Commit: ${GITHUB_SHA}

Consumers must pin the immutable tag and verify SHA256SUMS.txt plus
sidecar-provenance.intoto.json. Assets under sidecar-latest may change."
    RESPONSE=$(create_release "${TAG}" "Sidecar Binaries (latest)" "${BODY}" true)
    RELEASE_ID=$(jq -er '.id' <<<"${RESPONSE}")
    remove_preassociated_assets "${RESPONSE}" "${RELEASE_ID}"
    upload_assets "${RELEASE_ID}"
    echo "updated sidecar-latest discovery pointer to ${IMMUTABLE_TAG}"
    ;;
  *)
    echo "unknown publication mode: ${MODE}" >&2
    exit 2
    ;;
esac
