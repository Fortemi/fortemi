#!/usr/bin/env bash
# Create a run-scoped Docker bridge without consuming a predefined address pool.

set -euo pipefail

remove_docker() { bash "$(dirname "${BASH_SOURCE[0]}")/remove-run-docker.sh" "$@"; }

network_name="${1:?Usage: $0 <network-name> [numeric-seed]}"
network_seed="${2:-${GITHUB_RUN_ID:-0}}"

if [[ ! "$network_name" =~ ^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,127}$ ]]; then
    echo "ERROR: invalid Docker network name" >&2
    exit 2
fi
if [[ ! "$network_seed" =~ ^[0-9]+$ ]]; then
    echo "ERROR: Docker network seed must be numeric" >&2
    exit 2
fi
if docker network inspect "$network_name" >/dev/null 2>&1; then
    echo "ERROR: Docker network already exists: $network_name" >&2
    exit 1
fi

# 10.240.0.0/12 supplies 4096 private /24 candidates outside Docker's common
# 172.16.0.0/12 defaults. Creation is atomic, so concurrent jobs that select
# the same first candidate safely advance without a check-then-create race.
network_start=$((network_seed % 4096))
ci_docker_labels=()
if [[ -n "${CI_RESOURCE_SCOPE:-}" ]]; then
    ci_docker_labels=(--label "fortemi.ci.scope=${CI_RESOURCE_SCOPE}")
fi

for network_offset in {0..255}; do
    network_slot=$(((network_start + network_offset) % 4096))
    second_octet=$((240 + (network_slot / 256)))
    third_octet=$((network_slot % 256))
    network_subnet="10.${second_octet}.${third_octet}.0/24"

    if ! docker network create \
        "${ci_docker_labels[@]}" \
        --driver bridge \
        --subnet "$network_subnet" \
        "$network_name" >/dev/null 2>&1; then
        continue
    fi

    actual_subnet="$(docker network inspect \
        --format '{{(index .IPAM.Config 0).Subnet}}' \
        "$network_name")"
    if [[ "$actual_subnet" != "$network_subnet" ]]; then
        remove_docker network rm "$network_name" || true
        echo "ERROR: Docker network subnet verification failed" >&2
        exit 1
    fi

    printf '%s\n' "$network_subnet"
    exit 0
done

echo "ERROR: no collision-free explicit subnet was available for $network_name" >&2
exit 1
