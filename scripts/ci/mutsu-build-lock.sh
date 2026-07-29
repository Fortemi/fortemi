#!/usr/bin/env bash
# Serialize heavyweight CI builds that share the mutsu host.

set -euo pipefail

LOCK_ROOT="${MUTSU_BUILD_LOCK_ROOT:-/Volumes/build/.locks}"
LOCK_DIR="${LOCK_ROOT}/mutsu-heavy-build"
RECLAIM_DIR="${LOCK_ROOT}/mutsu-heavy-build-reclaim"
POLL_SECONDS="${MUTSU_BUILD_LOCK_POLL_SECONDS:-5}"
LOCK_HELD=0
LOCK_TOKEN=""

release_lock() {
  if [[ "$LOCK_HELD" != 1 || ! -d "$LOCK_DIR" ]]; then
    return
  fi
  if [[ -f "$LOCK_DIR/token" ]] &&
    [[ "$(<"$LOCK_DIR/token")" == "$LOCK_TOKEN" ]]; then
    rm -rf "$LOCK_DIR"
    echo "Released mutsu heavyweight-build lock."
  fi
  LOCK_HELD=0
}

acquire_lock() {
  local label="$1"
  local timeout_seconds="$2"
  local started now owner_pid stale

  mkdir -p "$LOCK_ROOT"
  started="$(date +%s)"
  LOCK_TOKEN="${HOSTNAME:-mutsu}:$$:${started}:${RANDOM}"

  while true; do
    if mkdir "$LOCK_DIR" 2>/dev/null; then
      printf '%s\n' "$$" >"$LOCK_DIR/pid"
      printf '%s\n' "$LOCK_TOKEN" >"$LOCK_DIR/token"
      printf '%s\n' "$label" >"$LOCK_DIR/label"
      printf '%s\n' "$started" >"$LOCK_DIR/started_at_epoch"
      LOCK_HELD=1
      trap release_lock EXIT INT TERM HUP
      echo "Acquired mutsu heavyweight-build lock for ${label}."
      return
    fi

    if mkdir "$RECLAIM_DIR" 2>/dev/null; then
      stale=0
      owner_pid=""
      [[ -f "$LOCK_DIR/pid" ]] && owner_pid="$(<"$LOCK_DIR/pid")"
      if [[ "$owner_pid" =~ ^[0-9]+$ ]]; then
        kill -0 "$owner_pid" 2>/dev/null || stale=1
      elif find "$LOCK_DIR" -prune -mmin +1 -print -quit 2>/dev/null |
        grep -q .; then
        stale=1
      fi

      if [[ "$stale" == 1 ]]; then
        echo "Reclaiming stale mutsu heavyweight-build lock (pid=${owner_pid:-missing})."
        rm -rf "$LOCK_DIR"
      fi
      rmdir "$RECLAIM_DIR" 2>/dev/null || true
    fi

    now="$(date +%s)"
    if ((now - started >= timeout_seconds)); then
      echo "Timed out waiting for mutsu heavyweight-build lock after ${timeout_seconds}s." >&2
      if [[ -f "$LOCK_DIR/label" ]]; then
        echo "Current holder: $(<"$LOCK_DIR/label")" >&2
      fi
      return 1
    fi
    sleep "$POLL_SECONDS"
  done
}

usage() {
  echo "Usage: $0 --label <description> [--timeout <seconds>] -- <command> [args...]" >&2
}

LABEL=""
TIMEOUT_SECONDS=3600
while [[ $# -gt 0 ]]; do
  case "$1" in
    --label)
      [[ $# -ge 2 ]] || {
        usage
        exit 2
      }
      LABEL="$2"
      shift 2
      ;;
    --timeout)
      [[ $# -ge 2 && "$2" =~ ^[1-9][0-9]*$ ]] || {
        usage
        exit 2
      }
      TIMEOUT_SECONDS="$2"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

[[ -n "$LABEL" && $# -gt 0 ]] || {
  usage
  exit 2
}

acquire_lock "$LABEL" "$TIMEOUT_SECONDS"
set +e
"$@"
status=$?
set -e
exit "$status"
