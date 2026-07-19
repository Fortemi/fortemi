#!/usr/bin/env bash
# Create or repair the Docker bundle database secret without printing it.

set -euo pipefail
umask 077

env_file="${1:-.env}"
env_dir=$(dirname "$env_file")

if [[ -L "$env_file" ]]; then
    echo "ERROR: refusing to update symlinked environment file: ${env_file}" >&2
    exit 1
fi
mkdir -p "$env_dir"

normalize_secret() {
    local value="$1"
    if (( ${#value} >= 2 )) && [[ "$value" == \"*\" && "$value" == *\" ]]; then
        value="${value:1:${#value}-2}"
    elif (( ${#value} >= 2 )) && [[ "$value" == \'*\' && "$value" == *\' ]]; then
        value="${value:1:${#value}-2}"
    fi
    printf '%s' "${value,,}"
}

is_insecure_secret() {
    local normalized
    normalized=$(normalize_secret "$1")
    case "$normalized" in
        ""|matric|fortemi-local-dev|password|changeme|\
        "<postgres_password>"|"<operator_supplied_database_password>")
            return 0
            ;;
    esac
    return 1
}

validate_secret() {
    local value="$1"
    if is_insecure_secret "$value"; then
        return 1
    fi
    if [[ "$value" == *$'\n'* || "$value" == *$'\r'* ]]; then
        return 1
    fi
    return 0
}

existing_secret=""
has_assignment=false
if [[ -f "$env_file" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
        if [[ "$line" == POSTGRES_PASSWORD=* ]]; then
            existing_secret="${line#POSTGRES_PASSWORD=}"
            has_assignment=true
        fi
    done < "$env_file"
fi

if [[ "$has_assignment" == true ]] && validate_secret "$existing_secret"; then
    chmod 600 "$env_file"
    echo "Bundle database secret is already configured (value not printed)."
    exit 0
fi

secret_source="generated"
if [[ -n "${POSTGRES_PASSWORD:-}" ]]; then
    if ! validate_secret "$POSTGRES_PASSWORD"; then
        echo "ERROR: POSTGRES_PASSWORD is empty, reusable, or contains a newline." >&2
        exit 1
    fi
    secret="$POSTGRES_PASSWORD"
    secret_source="operator_supplied"
elif command -v openssl >/dev/null 2>&1; then
    secret=$(openssl rand -hex 32)
elif command -v python3 >/dev/null 2>&1; then
    secret=$(python3 -c 'import secrets; print(secrets.token_hex(32))')
else
    echo "ERROR: openssl or python3 is required to generate POSTGRES_PASSWORD." >&2
    exit 2
fi

if ! validate_secret "$secret"; then
    echo "ERROR: generated database secret failed validation." >&2
    exit 2
fi

tmp=$(mktemp "${env_dir}/.fortemi-env.XXXXXX")
trap 'rm -f "$tmp"' EXIT
assignment_written=false
if [[ -f "$env_file" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
        if [[ "$line" == POSTGRES_PASSWORD=* ]]; then
            if [[ "$assignment_written" == false ]]; then
                printf 'POSTGRES_PASSWORD=%s\n' "$secret" >> "$tmp"
                assignment_written=true
            fi
        else
            printf '%s\n' "$line" >> "$tmp"
        fi
    done < "$env_file"
fi
if [[ "$assignment_written" == false ]]; then
    if [[ -s "$tmp" ]]; then
        printf '\n' >> "$tmp"
    fi
    printf 'POSTGRES_PASSWORD=%s\n' "$secret" >> "$tmp"
fi

chmod 600 "$tmp"
mv -f "$tmp" "$env_file"
trap - EXIT
echo "Bundle database secret configured (${secret_source}; value not printed)."
