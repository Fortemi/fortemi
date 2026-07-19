#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: check-runner-capacity.sh [--path PATH] [--min-free-kib N] [--min-free-inodes N] [--df-bin PATH]

Checks the filesystem backing a runner workspace and exits non-zero when its
free bytes or inodes are below the configured floor.
EOF
}

workspace_path="${GITHUB_WORKSPACE:-.}"
min_free_kib="${RUNNER_MIN_FREE_KIB:-41943040}"
min_free_inodes="${RUNNER_MIN_FREE_INODES:-1000000}"
df_bin="${DF_BIN:-df}"

while (($#)); do
  case "$1" in
    --path)
      workspace_path="${2:?--path requires a value}"
      shift 2
      ;;
    --min-free-kib)
      min_free_kib="${2:?--min-free-kib requires a value}"
      shift 2
      ;;
    --min-free-inodes)
      min_free_inodes="${2:?--min-free-inodes requires a value}"
      shift 2
      ;;
    --df-bin)
      df_bin="${2:?--df-bin requires a value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

for value_name in min_free_kib min_free_inodes; do
  value="${!value_name}"
  if [[ ! "$value" =~ ^[1-9][0-9]{0,14}$ ]]; then
    echo "ERROR: ${value_name} must be a bounded positive integer" >&2
    exit 2
  fi
done

if [[ ! -e "$workspace_path" ]]; then
  echo "ERROR: runner capacity path does not exist: $workspace_path" >&2
  exit 2
fi

free_kib="$("$df_bin" -Pk "$workspace_path" | awk 'NR == 2 { print $4 }')"
free_inodes="$("$df_bin" -Pi "$workspace_path" | awk 'NR == 2 { print $4 }')"

if [[ ! "$free_kib" =~ ^[0-9]+$ || ! "$free_inodes" =~ ^[0-9]+$ ]]; then
  echo "ERROR: unable to parse runner filesystem capacity" >&2
  exit 2
fi

printf 'runner_capacity path=%q free_kib=%s required_kib=%s free_inodes=%s required_inodes=%s\n' \
  "$workspace_path" "$free_kib" "$min_free_kib" "$free_inodes" "$min_free_inodes"

failed=0
if ((free_kib < min_free_kib)); then
  echo "ERROR: runner free space is below the build safety floor" >&2
  failed=1
fi
if ((free_inodes < min_free_inodes)); then
  echo "ERROR: runner free inodes are below the build safety floor" >&2
  failed=1
fi

if ((failed)); then
  echo "Runner infrastructure capacity is insufficient; drain the runner and follow build/RUNNER_SETUP.md before retrying." >&2
  exit 1
fi

echo "runner capacity preflight passed"
