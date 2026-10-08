#!/usr/bin/env bash
# Lint, render and schema-validate deploy/helm/fortemi for the default and the
# hosted single-tenant values (#1153). Requires helm on PATH; installs a pinned,
# checksum-verified kubeconform when it is not already available.
set -euo pipefail

CHART_DIR="${1:-deploy/helm/fortemi}"
KUBE_VERSION="${KUBE_VERSION:-1.31.0}"
KUBECONFORM_VERSION="v0.8.0"
# sha256 of kubeconform-linux-amd64.tar.gz from the v0.8.0 release CHECKSUMS (ci/digests.txt).
KUBECONFORM_SHA256="9bc2bffbf71f261128533edaf912153948b7ff238f9a531ae6d34466ec287883"
CRD_SCHEMAS='https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json'

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

if ! command -v kubeconform >/dev/null 2>&1; then
  archive="${work}/kubeconform.tar.gz"
  curl -fsSL -o "${archive}" \
    "https://github.com/yannh/kubeconform/releases/download/${KUBECONFORM_VERSION}/kubeconform-linux-amd64.tar.gz"
  echo "${KUBECONFORM_SHA256}  ${archive}" | sha256sum -c -
  tar -xzf "${archive}" -C "${work}" kubeconform
  PATH="${work}:${PATH}"
fi

# Hosted values deliberately leave identity fields empty; supply placeholders
# (no credentials) so the fail-fast validation passes and every route renders.
hosted_args=(
  -f "${CHART_DIR}/values-hosted-single-tenant.yaml"
  --set env.ISSUER_URL=https://memory.example.com
  --set env.FORTEMI_AUTH_AUDIENCE=fortemi-api
  --set ingress.enabled=true
  --set httpRoute.enabled=true
  --set-json 'httpRoute.parentRefs=[{"name":"public-gateway"}]'
)

helm lint --strict "${CHART_DIR}"
helm lint --strict "${CHART_DIR}" "${hosted_args[@]}"

helm template fortemi "${CHART_DIR}" > "${work}/default.yaml"
helm template fortemi "${CHART_DIR}" "${hosted_args[@]}" > "${work}/hosted.yaml"

# Hosted mode without its required identity settings must fail to render.
if helm template fortemi "${CHART_DIR}" -f "${CHART_DIR}/values-hosted-single-tenant.yaml" >/dev/null 2>&1; then
  echo "hosted values rendered without ISSUER_URL; fail-fast validation is broken" >&2
  exit 1
fi

kubeconform -strict -summary -kubernetes-version "${KUBE_VERSION}" \
  -schema-location default -schema-location "${CRD_SCHEMAS}" \
  "${work}/default.yaml" "${work}/hosted.yaml"

# Chart version must equal the workspace CalVer (no leading zeros).
workspace_version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n1)"
chart_version="$(sed -n 's/^version: //p' "${CHART_DIR}/Chart.yaml")"
app_version="$(sed -n 's/^appVersion: "\(.*\)"$/\1/p' "${CHART_DIR}/Chart.yaml")"
if [[ "${chart_version}" != "${workspace_version}" || "${app_version}" != "${workspace_version}" ]]; then
  echo "Chart version ${chart_version}/appVersion ${app_version} must equal workspace ${workspace_version}" >&2
  exit 1
fi
echo "Helm chart OK (version ${chart_version})"
