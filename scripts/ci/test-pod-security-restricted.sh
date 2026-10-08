#!/usr/bin/env bash
# Prove the all-in-one bundle starts under Pod Security "restricted" (#1173).
#
# Creates a disposable kind cluster, labels a namespace
# pod-security.kubernetes.io/enforce=restricted, deploys
# deploy/kustomize/examples/bundle-restricted with the image under test, and
# requires the pod to be admitted, become Ready (kubelet /readyz probe) and
# answer /health through a port-forward. It also asserts the running processes
# are uid 999 with no effective capabilities.
#
# Usage: scripts/ci/test-pod-security-restricted.sh --bundle-image <local-image>
#   The image must exist in the local Docker daemon (it is loaded into kind).
# Environment: KEEP_PSA_CLUSTER=1 keeps the cluster; PSA_TIMEOUT_SECONDS
# (default 600).
#
# kind and kubectl are version- and SHA-256-pinned (ci/digests.txt), as is the
# kindest/node image. On the shared CI host kube-proxy exits with "fsnotify
# watcher init: too many open files" (fs.inotify.max_user_instances is 128):
# it watches its --config file. The test re-runs kube-proxy with the same
# settings as flags, which needs no watcher, so Services and the local-path
# storage provisioner (which reaches the API server through a Service) work.
set -euo pipefail

BUNDLE_IMAGE=""
while (( $# )); do
    case "$1" in
        --bundle-image) BUNDLE_IMAGE="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -n "$BUNDLE_IMAGE" ]] || { echo "--bundle-image is required" >&2; exit 2; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KIND_VERSION=v0.33.0
KIND_SHA256=aee6151561422756b764a4ae28e7f44cda5af5a9eead3cc9985112b1de8d8e0d
KUBECTL_VERSION=v1.37.0
KUBECTL_SHA256=6129359f4e1f3848a5572ccb0b26cf28b8ca08cef38c95a765b2f64a2c961a2f
NODE_IMAGE="kindest/node:v1.37.0@sha256:a1ed56cfb0e7b93589bdf97c8cd566405a265939e3620fc4f5de89adff580ae5"
TIMEOUT="${PSA_TIMEOUT_SECONDS:-600}"
CLUSTER="fortemi-psa-$$"
NAMESPACE=fortemi-bundle
WORK="$(mktemp -d "${TMPDIR:-/tmp}/fortemi-psa.XXXXXX")"
chmod 0700 "$WORK"
export KUBECONFIG="${WORK}/kubeconfig"
PF_PID=""

log() { printf '[psa-restricted] %s\n' "$*"; }
fail() { printf '[psa-restricted] FAIL: %s\n' "$*" >&2; exit 1; }
cleanup() {
    [[ -n "$PF_PID" ]] && kill "$PF_PID" 2>/dev/null
    if [[ "${KEEP_PSA_CLUSTER:-0}" == 1 ]]; then
        log "kept cluster ${CLUSTER} (KUBECONFIG=${KUBECONFIG})"
        return
    fi
    "${WORK}/bin/kind" delete cluster --name "$CLUSTER" >/dev/null 2>&1 || true
    rm -rf "$WORK"
}
trap cleanup EXIT

fetch_tool() {
    local name="$1" url="$2" sha="$3"
    curl -fsSL --retry 3 -o "${WORK}/bin/${name}" "$url"
    echo "${sha}  ${WORK}/bin/${name}" | sha256sum -c --quiet -
    chmod 0755 "${WORK}/bin/${name}"
}
mkdir -p "${WORK}/bin"
fetch_tool kind "https://github.com/kubernetes-sigs/kind/releases/download/${KIND_VERSION}/kind-linux-amd64" "$KIND_SHA256"
fetch_tool kubectl "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/amd64/kubectl" "$KUBECTL_SHA256"
kubectl() { "${WORK}/bin/kubectl" "$@"; }

docker image inspect "$BUNDLE_IMAGE" >/dev/null || fail "${BUNDLE_IMAGE} is not in the local Docker daemon"

log "creating kind cluster ${CLUSTER} (${NODE_IMAGE%@*})"
"${WORK}/bin/kind" create cluster --name "$CLUSTER" --image "$NODE_IMAGE" \
    --kubeconfig "$KUBECONFIG" --wait 180s >"${WORK}/kind.log" 2>&1 ||
    { cat "${WORK}/kind.log"; fail "kind create cluster failed"; }
# kind's kube-proxy settings (iptables, pod CIDR 10.244.0.0/16, no conntrack
# sysctl) as flags instead of a watched --config file; see the header.
# shellcheck disable=SC2016 # $(NODE_NAME) is expanded by the kubelet
kubectl -n kube-system patch daemonset kube-proxy --type=json -p='[{"op":"replace",
  "path":"/spec/template/spec/containers/0/command","value":["/usr/local/bin/kube-proxy",
  "--kubeconfig=/var/lib/kube-proxy/kubeconfig.conf","--cluster-cidr=10.244.0.0/16",
  "--proxy-mode=iptables","--conntrack-max-per-core=0","--hostname-override=$(NODE_NAME)"]}]' >/dev/null
kubectl -n kube-system rollout status daemonset/kube-proxy --timeout=120s >/dev/null
# Restart the API clients that crash-looped without Service routing instead of
# waiting out their back-off.
kubectl -n local-path-storage delete pod --all --wait=false >/dev/null
kubectl -n kube-system delete pod -l k8s-app=kube-dns --wait=false >/dev/null
kubectl -n local-path-storage rollout status deployment/local-path-provisioner --timeout=180s >/dev/null
kubectl -n kube-system rollout status deployment/coredns --timeout=180s >/dev/null
"${WORK}/bin/kind" load docker-image --name "$CLUSTER" "$BUNDLE_IMAGE" >/dev/null

# The example's own Namespace carries the Pod Security labels under test.
kubectl apply -f "${ROOT}/deploy/kustomize/examples/bundle-restricted/namespace.yaml" >/dev/null
[[ "$(kubectl get namespace "$NAMESPACE" \
    -o jsonpath='{.metadata.labels.pod-security\.kubernetes\.io/enforce}')" == restricted ]] ||
    fail "namespace ${NAMESPACE} does not enforce restricted"

# Negative control: the namespace must actually reject a root, privileged pod.
if kubectl -n "$NAMESPACE" run psa-negative-control --image="$BUNDLE_IMAGE" \
    --restart=Never --overrides='{"spec":{"securityContext":{"runAsUser":0}}}' \
    >"${WORK}/negative.log" 2>&1; then
    fail "restricted namespace admitted a root pod; PSA is not enforcing"
fi
grep -q 'violates PodSecurity "restricted' "${WORK}/negative.log" ||
    { cat "${WORK}/negative.log"; fail "negative control failed for an unexpected reason"; }
log "negative control rejected by PodSecurity restricted"

# The example references the Secret by name only; the test supplies a per-run value.
kubectl -n "$NAMESPACE" create secret generic fortemi-bundle \
    --from-literal=POSTGRES_PASSWORD="psa-$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')" >/dev/null

# Render the example with the image under test (kubectl's built-in kustomize).
mkdir -p "${WORK}/overlay"
# kustomize refuses absolute resource paths; render a copy of the example.
cp -r "${ROOT}/deploy/kustomize/examples/bundle-restricted" "${WORK}/overlay/example"
cat > "${WORK}/overlay/kustomization.yaml" <<YAML
resources:
  - example
patches:
  - target: {kind: Deployment, name: fortemi-bundle}
    patch: |-
      - op: replace
        path: /spec/template/spec/containers/0/image
        value: ${BUNDLE_IMAGE}
      - op: add
        path: /spec/template/spec/containers/0/imagePullPolicy
        value: Never
YAML
kubectl kustomize "${WORK}/overlay" > "${WORK}/rendered.yaml"
kubectl apply -f "${WORK}/rendered.yaml" > "${WORK}/apply.log" 2>&1 ||
    { cat "${WORK}/apply.log"; fail "apply failed"; }
if grep -qi 'would violate PodSecurity' "${WORK}/apply.log"; then
    cat "${WORK}/apply.log"
    fail "the bundle pod template violates PodSecurity restricted"
fi

log "waiting for the bundle pod to become Ready under restricted"
if ! kubectl -n "$NAMESPACE" wait --for=condition=Available deployment/fortemi-bundle \
    --timeout="${TIMEOUT}s" >/dev/null; then
    kubectl -n "$NAMESPACE" get events --sort-by=.lastTimestamp | tail -20
    kubectl -n "$NAMESPACE" logs deployment/fortemi-bundle --tail=80 || true
    fail "bundle did not become Available within ${TIMEOUT}s"
fi
POD="$(kubectl -n "$NAMESPACE" get pod -l app.kubernetes.io/name=fortemi-bundle \
    -o jsonpath='{.items[0].metadata.name}')"
kubectl -n "$NAMESPACE" get pod "$POD" -o json > "${WORK}/pod.json"
python3 - "${WORK}/pod.json" <<'PY'
import json, sys
pod = json.load(open(sys.argv[1]))
spec = pod["spec"]
c = spec["containers"][0]
sc = c.get("securityContext", {})
psc = spec.get("securityContext", {})
assert psc.get("runAsNonRoot") is True, psc
assert psc.get("runAsUser") == 999, psc
assert sc.get("allowPrivilegeEscalation") is False, sc
assert sc.get("capabilities", {}).get("drop") == ["ALL"], sc
assert not sc.get("capabilities", {}).get("add"), sc
assert (sc.get("seccompProfile") or psc.get("seccompProfile", {})).get("type") == "RuntimeDefault"
print("pod security context: runAsUser 999, no added capabilities, RuntimeDefault seccomp")
PY

# Every process in the container runs as uid 999 with an empty effective set.
# shellcheck disable=SC2016 # expanded inside the container
kubectl -n "$NAMESPACE" exec "$POD" -- sh -c '
  for d in /proc/[0-9]*; do
    [ -r "$d/status" ] || continue
    uid=$(awk "/^Uid:/{print \$3}" "$d/status")
    eff=$(awk "/^CapEff:/{print \$2}" "$d/status")
    echo "$(cat "$d/comm") uid=$uid capeff=$eff"
  done' > "${WORK}/procs.txt"
cat "${WORK}/procs.txt"
grep -q '^postgres ' "${WORK}/procs.txt" || fail "embedded PostgreSQL is not running"
grep -q '^matric-api ' "${WORK}/procs.txt" || fail "matric-api is not running"
if grep -vE ' uid=999 capeff=0000000000000000$' "${WORK}/procs.txt" | grep -q .; then
    fail "a process runs with another uid or with effective capabilities"
fi

# Persistent state lives on the PVC, in directories the non-root entrypoint and
# API created themselves (no chown).
# shellcheck disable=SC2016 # expanded inside the container
kubectl -n "$NAMESPACE" exec "$POD" -- sh -c \
    'stat -c "%n %u:%g %a" /var/lib/fortemi/pgdata /var/lib/fortemi/files /var/lib/fortemi/backups &&
     test -s /var/lib/fortemi/pgdata/PG_VERSION && test "$(stat -c %u /var/lib/fortemi/pgdata)" = 999' ||
    fail "PGDATA, files or backups are not on the PVC as uid 999"

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
kubectl -n "$NAMESPACE" port-forward "pod/${POD}" "${PORT}:3000" >"${WORK}/pf.log" 2>&1 &
PF_PID=$!
for _ in $(seq 1 30); do
    curl -fsS -o "${WORK}/health.json" "http://127.0.0.1:${PORT}/health" 2>/dev/null && break
    sleep 1
done
jq -e '.status == "healthy"' "${WORK}/health.json" >/dev/null || fail "/health did not report healthy"
curl -fsS -o /dev/null "http://127.0.0.1:${PORT}/readyz" || fail "/readyz is not ready"
log "PASS: bundle runs under PodSecurity restricted as uid 999 with no capabilities"
