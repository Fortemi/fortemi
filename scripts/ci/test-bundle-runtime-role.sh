#!/usr/bin/env bash
# Prove the bundle provisions its restricted runtime login in external-OIDC mode (#1220).
#
# Starts the image under test with no database URLs, on a fresh volume and again
# after a restart, and requires readiness with the API connected as
# fortemi_runtime (NOSUPERUSER NOBYPASSRLS, bundle-marked, password rotated per
# start), the owner holding BYPASSRLS, the ownership receipt, and the
# audit_event UPDATE/DELETE and tenant_registry write revocations surviving the
# restart, and the MCP client registered once under the configured tenant and
# accepted again after the restart. It also requires:
# refusal of an unmarked existing fortemi_runtime role, community mode creating
# no runtime role, the same external-OIDC start as the image's postgres account
# (uid 999), and that neither password appears in container or PostgreSQL logs.
# No network is attached; the issuer is never contacted.
#
# Usage: scripts/ci/test-bundle-runtime-role.sh --bundle-image <local-image>
# Environment: RUNTIME_ROLE_TIMEOUT_SECONDS (default 240 per start).
set -euo pipefail

remove_docker() { bash "$(dirname "${BASH_SOURCE[0]}")/remove-run-docker.sh" "$@"; }

BUNDLE_IMAGE=""
while (( $# )); do
    case "$1" in
        --bundle-image) BUNDLE_IMAGE="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[[ -n "$BUNDLE_IMAGE" ]] || { echo "--bundle-image is required" >&2; exit 2; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TIMEOUT="${RUNTIME_ROLE_TIMEOUT_SECONDS:-240}"
NAME="fortemi-runtime-role-${CI_RESOURCE_SCOPE:-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}}-${CI_RESOURCE_ATTEMPT:-1}-$$"
POLICY="${ROOT}/deploy/identity/claim-policy.example.json"
OWNER_PASSWORD="$(python3 -c 'import secrets; print(secrets.token_hex(24))')"

cleanup() {
    remove_docker rm -fv "$NAME" || true
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; docker logs "$NAME" 2>&1 | grep -E 'ERROR|Error:' | tail -5 >&2 || true; exit 1; }

# Capture logs before matching: with pipefail, `docker logs | grep -q` fails when
# grep exits on the first match and docker logs receives SIGPIPE.
container_log_has() {
    local logs
    logs="$(docker logs --since "$STARTED_AT" "$NAME" 2>&1)" || return 1
    grep -qE -- "$1" <<<"$logs"
}

common_env=(-e POSTGRES_PASSWORD="$OWNER_PASSWORD" -e PGDATA=/var/lib/postgresql/data/pgdata
    -e MATRIC_ATTACHMENT_SCAN_MODE=disabled -e REDIS_ENABLED=false -e RENDERER_ENABLED=false
    -e WORKER_ENABLED=false -e JOB_WORKER_ENABLED=false)
external_env=(-v "${POLICY}:/etc/fortemi/claim-policy.json:ro"
    -e FORTEMI_AUTH_MODE=external-oidc -e FORTEMI_AUTH_CLAIM_POLICY_FILE=/etc/fortemi/claim-policy.json
    -e FORTEMI_AUTH_ISSUER=https://issuer.invalid/realms/ci
    -e FORTEMI_AUTH_AUDIENCES=https://fortemi.invalid,https://fortemi.invalid/mcp
    -e MCP_RESOURCE_URI=https://fortemi.invalid/mcp -e MCP_BASE_URL=https://fortemi.invalid/mcp -e ISSUER_URL=https://fortemi.invalid
    -e FORTEMI_AUTH_DEFAULT_TENANT=00000000-0000-4000-8000-000000000001)

# start <expect: ready|exit> <docker run args...>
start() {
    local expect="$1"; shift
    STARTED_AT="$(date -u +%Y-%m-%dT%H:%M:%S.%NZ)"
    if [[ "${1:-}" == --restart ]]; then
        shift
        docker restart "$NAME" >/dev/null
    else
        remove_docker rm -fv "$NAME" || true
        docker run --label "fortemi.ci.scope=${CI_RESOURCE_SCOPE:-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}}" -d --name "$NAME" --network none "$@" "$BUNDLE_IMAGE" >/dev/null
    fi
    local deadline=$((SECONDS + TIMEOUT))
    while (( SECONDS < deadline )); do
        if [ "$(docker inspect -f '{{.State.Status}}' "$NAME")" != running ]; then
            [ "$expect" = exit ] && return 0
            fail "container exited before readiness"
        fi
        if docker exec "$NAME" curl -sf http://localhost:3000/readyz >/dev/null 2>&1; then
            [ "$expect" = ready ] && return 0
            fail "container became ready but was expected to refuse startup"
        fi
        sleep 3
    done
    fail "timed out waiting for ${expect}"
}

sql() { docker exec "$NAME" psql -X -U postgres -d matric -Atc "$1"; }

# Print the "login:password" user-info part of the running API's DATABASE_URL, or
# nothing when the URL has no login with a non-empty password.
api_db_userinfo() {
    docker exec "$NAME" sh -c 'for p in /proc/[0-9]*; do
        if [ "$(cat "$p/comm" 2>/dev/null)" = matric-api ]; then
            url="$(tr "\0" "\n" < "$p/environ" | sed -n "s/^DATABASE_URL=//p")"
            # Only postgres:// with a non-empty login and password counts.
            case "$url" in postgres://*@*) ;; *) break ;; esac
            info="${url#postgres://}"; info="${info%%@*}"
            login="${info%%:*}"; password="${info#*:}"
            if [ "$info" != "$login" ] && [ -n "$login" ] && [ -n "$password" ]; then
                printf "%s\n" "$info"
            fi
            break
        fi; done'
}

api_db_user() { local info; info="$(api_db_userinfo)"; printf '%s\n' "${info%%:*}"; }

# The entrypoint validates or registers the MCP client after API readiness.
wait_for_mcp_credentials() {
    local deadline=$((SECONDS + 60))
    while (( SECONDS < deadline )); do
        container_log_has "MCP credentials (valid|registered)" && return 0
        sleep 2
    done
    fail "MCP credentials were neither validated nor registered"
}

assert_no_secret_in_logs() {
    local runtime_password
    local info
    info="$(api_db_userinfo)"
    runtime_password=""
    if [ -n "$info" ]; then runtime_password="${info#*:}"; fi
    [ -n "$runtime_password" ] || fail "could not read the API database login"
    local logs
    logs="$(docker logs "$NAME" 2>&1; docker exec "$NAME" cat /var/log/postgresql/postgresql.log)"
    if grep -qF -- "$runtime_password" <<<"$logs"; then fail "runtime password appears in logs"; fi
    if grep -qF -- "$OWNER_PASSWORD" <<<"$logs"; then fail "owner password appears in logs"; fi
}

assert_external_posture() {
    [ "$(api_db_user)" = fortemi_runtime ] || fail "API is not connected as fortemi_runtime"
    [ "$(sql "SELECT rolsuper::text || rolbypassrls::text FROM pg_roles WHERE rolname = 'fortemi_runtime'")" = falsefalse ] \
        || fail "fortemi_runtime is privileged"
    [ "$(sql "SELECT rolbypassrls FROM pg_roles WHERE rolname = 'matric'")" = t ] || fail "owner lacks BYPASSRLS"
    [ "$(sql "SELECT shobj_description(oid, 'pg_authid') LIKE 'fortemi-bundle managed%' FROM pg_roles WHERE rolname = 'fortemi_runtime'")" = t ] \
        || fail "fortemi_runtime is not bundle-marked"
    [ "$(sql "SELECT count(*) FROM public.external_oidc_ownership WHERE completed_at IS NOT NULL")" = 1 ] \
        || fail "ownership receipt missing"
    [ "$(sql "SELECT has_table_privilege('fortemi_runtime', 'public.audit_event', 'UPDATE') OR has_table_privilege('fortemi_runtime', 'public.audit_event', 'DELETE')")" = f ] \
        || fail "fortemi_runtime can update or delete audit_event"
    [ "$(sql "SELECT has_table_privilege('fortemi_runtime', 'public.note', 'INSERT')")" = t ] \
        || fail "fortemi_runtime lacks note DML"
    [ "$(sql "SELECT has_table_privilege('fortemi_runtime', 'public.tenant_registry', 'SELECT')::text || (has_table_privilege('fortemi_runtime', 'public.tenant_registry', 'INSERT') OR has_table_privilege('fortemi_runtime', 'public.tenant_registry', 'UPDATE') OR has_table_privilege('fortemi_runtime', 'public.tenant_registry', 'DELETE'))::text")" = truefalse ] \
        || fail "fortemi_runtime tenant_registry access is not read-only"
    wait_for_mcp_credentials
    [ "$(sql "SELECT count(*) FROM public.oauth_client WHERE tenant_id <> '00000000-0000-4000-8000-000000000001'")" = 0 ] \
        || fail "an OAuth client was registered outside the configured tenant"
    assert_no_secret_in_logs
}

echo ">>> external OIDC, fresh volume"
start ready -v /var/lib/postgresql/data "${common_env[@]}" "${external_env[@]}"
assert_external_posture
first_verifier="$(sql "SELECT md5(rolpassword) FROM pg_authid WHERE rolname = 'fortemi_runtime'")"

echo ">>> external OIDC, restart on the same volume"
start ready --restart
assert_external_posture
[ "$(sql "SELECT md5(rolpassword) FROM pg_authid WHERE rolname = 'fortemi_runtime'")" != "$first_verifier" ] \
    || fail "runtime password was not rotated on restart"
container_log_has "MCP credentials valid" || fail "persisted MCP credentials were not accepted after restart"
[ "$(sql "SELECT count(*) FROM public.oauth_client")" = 1 ] || fail "restart registered another OAuth client"

echo ">>> unmarked existing runtime role is refused"
sql "COMMENT ON ROLE fortemi_runtime IS NULL" >/dev/null
start exit --restart
container_log_has "is not managed by the bundle" || fail "refusal reason missing"
remove_docker rm -fv "$NAME"

echo ">>> community mode creates no runtime role"
start ready "${common_env[@]}"
[ "$(api_db_user)" = matric ] || fail "community API is not connected as the owner"
[ "$(sql "SELECT count(*) FROM pg_roles WHERE rolname = 'fortemi_runtime'")" = 0 ] || fail "community mode created fortemi_runtime"

echo ">>> external OIDC as the image's postgres account"
start ready --user 999:999 --tmpfs /var/lib/postgresql/data:uid=999,gid=999,mode=0700 \
    "${common_env[@]}" "${external_env[@]}"
assert_external_posture

echo "PASS: bundle runtime role provisioning"
