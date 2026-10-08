#!/usr/bin/env bash
# Keep the Kustomize base (deploy/kustomize) and the Helm chart (deploy/helm/fortemi)
# from drifting (#1153 follow-up). Renders matching pairs with `helm template` and
# `kustomize build`, reduces each to its workload set, and fails on any difference:
#   kinds and names of ServiceAccounts, Deployments, Jobs, Services, PDBs and HPAs;
#   per container: name, command, effective env var NAMES, ports, probe paths/ports;
#   Service ports; HPA targets.
# Effective env names resolve envFrom ConfigMaps and optional configMapKeyRefs
# against the rendered ConfigMaps. NetworkPolicies are deliberately not compared:
# the base denies by default with explicit allows, the chart's policy is an opt-in
# example. Also schema-validates the Kustomize renders (kubeconform -strict) and
# checks image references: names only in the base, digests in the example.
#
# Requires helm and mikefarah yq v4 on PATH (both ship in the CI job's pinned
# alpine/helm image). Installs pinned, checksum-verified kustomize and kubeconform
# when absent (ci/digests.txt).
set -euo pipefail

CHART_DIR="${1:-deploy/helm/fortemi}"
KUSTOMIZE_DIR="${2:-deploy/kustomize}"
KUBE_VERSION="${KUBE_VERSION:-1.31.0}"
KUSTOMIZE_VERSION="v5.8.1"
# sha256 of kustomize_v5.8.1_linux_amd64.tar.gz from the release checksums.txt.
KUSTOMIZE_SHA256="029a7f0f4e1932c52a0476cf02a0fd855c0bb85694b82c338fc648dcb53a819d"
KUBECONFORM_VERSION="v0.8.0"
# Same pin as scripts/ci/lint-helm-chart.sh.
KUBECONFORM_SHA256="9bc2bffbf71f261128533edaf912153948b7ff238f9a531ae6d34466ec287883"

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

fetch() { # url sha256 archive-member
  curl -fsSL -o "${work}/dl.tar.gz" "$1"
  echo "$2  ${work}/dl.tar.gz" | sha256sum -c - >/dev/null
  tar -xzf "${work}/dl.tar.gz" -C "${work}" "$3"
}
if ! command -v kustomize >/dev/null 2>&1; then
  fetch "https://github.com/kubernetes-sigs/kustomize/releases/download/kustomize%2F${KUSTOMIZE_VERSION}/kustomize_${KUSTOMIZE_VERSION}_linux_amd64.tar.gz" \
    "${KUSTOMIZE_SHA256}" kustomize
fi
if ! command -v kubeconform >/dev/null 2>&1; then
  fetch "https://github.com/yannh/kubeconform/releases/download/${KUBECONFORM_VERSION}/kubeconform-linux-amd64.tar.gz" \
    "${KUBECONFORM_SHA256}" kubeconform
fi
PATH="${work}:${PATH}"
yq --version | grep -q 'version v4\.' || { echo "mikefarah yq v4 is required" >&2; exit 1; }

# One line per workload object (or container). Env names are emitted as tokens and
# resolved against the rendered ConfigMaps: `@cm` (envFrom: every key of cm) and
# `?cm/key=NAME` (optional configMapKeyRef: NAME only when cm has key).
summarize() {
  yq -N 'select(.kind == "ConfigMap") | .metadata.name as $n | (.data // {}) | keys | .[] | $n + " " + .' \
    "$1" > "${work}/cm.txt"
  {
    yq -N 'select(.kind == "ServiceAccount" or .kind == "PodDisruptionBudget") | .kind + "/" + .metadata.name' "$1"
    yq -N '
      select(.kind == "Deployment" or .kind == "Job")
      | .kind as $k | .metadata.name as $n
      | .spec.template.spec.containers[]
      | $k + "/" + $n + " container=" + .name
        + " command=" + ((.command // []) | join("|"))
        + " env=" + (([(.env // [])[]
              | (select(.valueFrom.configMapKeyRef.optional == true)
                  | "?" + .valueFrom.configMapKeyRef.name + "/" + .valueFrom.configMapKeyRef.key + "=" + .name)
                // .name]
            + [(.envFrom // [])[] | "@" + .configMapRef.name]) | join(","))
        + " ports=" + ([(.ports // [])[] | .name + ":" + (.containerPort | tostring)] | join(","))
        + " startup=" + (.startupProbe.httpGet.path // "-") + "@" + ((.startupProbe.httpGet.port // "-") | tostring)
        + " liveness=" + (.livenessProbe.httpGet.path // "-") + "@" + ((.livenessProbe.httpGet.port // "-") | tostring)
        + " readiness=" + (.readinessProbe.httpGet.path // "-") + "@" + ((.readinessProbe.httpGet.port // "-") | tostring)
    ' "$1"
    yq -N 'select(.kind == "Service") | "Service/" + .metadata.name + " ports="
      + ([.spec.ports[] | .name + ":" + (.port | tostring) + "->" + (.targetPort | tostring)] | join(","))' "$1"
    yq -N 'select(.kind == "HorizontalPodAutoscaler")
      | "HorizontalPodAutoscaler/" + .metadata.name + " target=" + .spec.scaleTargetRef.name' "$1"
  } | awk -v cmfile="${work}/cm.txt" '
    BEGIN { while ((getline l < cmfile) > 0) { split(l, a, " "); keys[a[1]] = keys[a[1]] " " a[2]; has[a[1] "/" a[2]] = 1 } }
    {
      for (i = 1; i <= NF; i++) if ($i ~ /^env=/) {
        n = split(substr($i, 5), toks, ","); out = ""
        for (j = 1; j <= n; j++) {
          t = toks[j]
          if (t ~ /^@/) { m = split(keys[substr(t, 2)], ks, " "); for (k = 1; k <= m; k++) out = out " " ks[k] }
          else if (t ~ /^\?/) { split(substr(t, 2), p, "="); if (p[1] in has) out = out " " p[2] }
          else if (t != "") out = out " " t
        }
        m = split(out, names, " "); cmd = "printf \"%s\\n\" " out " | sort | paste -sd, -"
        if (m > 0) { cmd | getline joined; close(cmd) } else joined = ""
        $i = "env=" joined
      }
      print
    }'
}

compare() { # label helm-render kustomize-render
  summarize "$2" | sort > "${work}/helm.txt"
  summarize "$3" | sort > "${work}/kustomize.txt"
  if ! diff -u --label "helm (${1})" --label "kustomize (${1})" "${work}/helm.txt" "${work}/kustomize.txt"; then
    echo "Kustomize and Helm workload sets differ for ${1}" >&2
    exit 1
  fi
  echo "parity OK: ${1} ($(wc -l < "${work}/helm.txt") workload lines)"
}

kbuild() { kustomize build "$1" > "$2"; kubeconform -strict -summary -kubernetes-version "${KUBE_VERSION}" "$2"; }

repo_root="$(pwd)"
# 1. Base vs chart defaults.
helm template fortemi "${CHART_DIR}" > "${work}/helm-default.yaml"
kbuild "${KUSTOMIZE_DIR}/base" "${work}/k-base.yaml"
compare "defaults" "${work}/helm-default.yaml" "${work}/k-base.yaml"

# Temp overlay of the base plus components. kustomize refuses absolute paths, so
# reach the repository from the temp dir by climbing to / first.
overlay() { # dir component...
  local dir="${work}/$1" up c; shift
  mkdir -p "${dir}"
  up="$(cd "${dir}" && pwd | sed -e 's|[^/][^/]*|..|g' -e 's|^/||')${repo_root}/${KUSTOMIZE_DIR}"
  {
    printf 'apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\n'
    printf 'resources: ["%s/base"]\ncomponents:\n' "${up}"
    for c in "$@"; do printf '  - "%s/components/%s"\n' "${up}" "${c}"; done
  } > "${dir}/kustomization.yaml"
  echo "${dir}"
}

# 2. Base without MCP vs mcp.enabled=false.
helm template fortemi "${CHART_DIR}" --set mcp.enabled=false > "${work}/helm-nomcp.yaml"
kbuild "$(overlay without-mcp without-mcp)" "${work}/k-nomcp.yaml"
compare "mcp disabled" "${work}/helm-nomcp.yaml" "${work}/k-nomcp.yaml"

# Every component renders and validates on the base (each kustomization in the tree
# is built at least once, as data-edge's cluster-verify does).
for c in "${KUSTOMIZE_DIR}"/components/*/; do
  c="$(basename "${c}")"
  kbuild "$(overlay "component-${c}" "${c}")" "${work}/k-component-${c}.yaml"
done

# 3. Example overlay vs the hosted single-tenant values (same placeholders).
helm template fortemi "${CHART_DIR}" -f "${CHART_DIR}/values-hosted-single-tenant.yaml" \
  --set env.ISSUER_URL=https://memory.example.com \
  --set env.FORTEMI_AUTH_AUDIENCE=fortemi-api \
  --set mcp.baseUrl=https://memory.example.com/mcp > "${work}/helm-hosted.yaml"
kbuild "${KUSTOMIZE_DIR}/examples/single-tenant" "${work}/k-example.yaml"
compare "hosted single-tenant example" "${work}/helm-hosted.yaml" "${work}/k-example.yaml"

# Image references: the base names images only; the example pins every one by digest.
bad="$(yq ea -N '.. | select(has("image")) | .image | select(test(":pinned$") | not)' "${work}/k-base.yaml")"
[[ -z "${bad}" ]] || { echo "base images must be name-only (<name>:pinned): ${bad}" >&2; exit 1; }
bad="$(yq ea -N '.. | select(has("image")) | .image | select(test("@sha256:") | not)' "${work}/k-example.yaml")"
[[ -z "${bad}" ]] || { echo "example images must be pinned by digest: ${bad}" >&2; exit 1; }
# 4. The restricted all-in-one bundle example (#1173) renders, validates and
# pins its image by digest. scripts/ci/test-pod-security-restricted.sh runs it.
kbuild "${KUSTOMIZE_DIR}/examples/bundle-restricted" "${work}/k-bundle.yaml"
bad="$(yq ea -N '.. | select(has("image")) | .image | select(test("@sha256:") | not)' "${work}/k-bundle.yaml")"
[[ -z "${bad}" ]] || { echo "bundle example image must be pinned by digest: ${bad}" >&2; exit 1; }
echo "Kustomize base OK"
