#!/usr/bin/env bash
# Mirror the configured native release evidence from Gitea to GitHub.

set -euo pipefail

GH_TOKEN="${GH_TOKEN:-${GH_PUBLISH_TOKEN:-}}"
RELEASE_TAG="${RELEASE_TAG:-${GITHUB_REF_NAME:-}}"
: "${GITEA_TOKEN:?GITEA_TOKEN is required}"
: "${GH_TOKEN:?GH_TOKEN or GH_PUBLISH_TOKEN is required}"
: "${RELEASE_TAG:?RELEASE_TAG or GITHUB_REF_NAME is required}"

GITEA_API="${GITEA_API:-https://git.integrolabs.net/api/v1}"
GITEA_REPO="${GITEA_REPO:-Fortemi/fortemi}"
GITHUB_API="${GITHUB_API:-https://api.github.com}"
GITHUB_REPO="${GITHUB_REPO:-Fortemi/fortemi}"
ASSETS=(
  matric-api-x86_64-unknown-linux-gnu
  matric-api-aarch64-apple-darwin
  matric-api-x86_64-apple-darwin
  SHA256SUMS.txt
  sidecar-provenance.intoto.json
)

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

gitea_release=$(curl -fsS \
  -H "Authorization: token ${GITEA_TOKEN}" \
  "${GITEA_API}/repos/${GITEA_REPO}/releases/tags/${RELEASE_TAG}")
github_release=$(curl -fsS \
  -H "Authorization: Bearer ${GH_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  "${GITHUB_API}/repos/${GITHUB_REPO}/releases/tags/${RELEASE_TAG}")
github_release_id=$(jq -er '.id' <<<"${github_release}")

for asset in "${ASSETS[@]}"; do
  url=$(jq -er --arg name "${asset}" \
    '.assets[] | select(.name == $name) | .browser_download_url' \
    <<<"${gitea_release}")
  curl -fsSL -H "Authorization: token ${GITEA_TOKEN}" \
    "${url}" -o "${tmp}/${asset}"
done

(
  cd "${tmp}"
  sha256sum -c SHA256SUMS.txt
  while read -r digest name; do
    jq -e --arg name "${name}" --arg digest "${digest}" \
      '.subject[] | select(.name == $name and .digest.sha256 == $digest)' \
      sidecar-provenance.intoto.json >/dev/null
  done < SHA256SUMS.txt
)

for asset in "${ASSETS[@]}"; do
  existing_url=$(jq -r --arg name "${asset}" \
    '.assets[] | select(.name == $name) | .url' \
    <<<"${github_release}")
  if [[ -n "${existing_url}" ]]; then
    curl -fsSL \
      -H "Authorization: Bearer ${GH_TOKEN}" \
      -H "Accept: application/octet-stream" \
      "${existing_url}" -o "${tmp}/github-${asset}"
    cmp -s "${tmp}/${asset}" "${tmp}/github-${asset}" || {
      echo "GitHub asset differs from Gitea source: ${asset}" >&2
      exit 1
    }
    echo "GitHub asset already matches: ${asset}"
    continue
  fi

  encoded_name=$(jq -rn --arg value "${asset}" '$value|@uri')
  curl -fsS -X POST \
    -H "Authorization: Bearer ${GH_TOKEN}" \
    -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/octet-stream" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "https://uploads.github.com/repos/${GITHUB_REPO}/releases/${github_release_id}/assets?name=${encoded_name}" \
    --data-binary @"${tmp}/${asset}" >/dev/null
  echo "Mirrored release asset: ${asset}"
done
