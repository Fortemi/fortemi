#!/usr/bin/env bash
# Disposable Redis for database-backed Rust test jobs, including SSE tokens.
set -euo pipefail

action=${1:?usage: test-redis.sh start|stop container-name}
container=${2:?container-name is required}

case "$action" in
  start)
    : "${GITHUB_ENV:?GITHUB_ENV must name the test environment file}"
    docker rm -f "$container" >/dev/null 2>&1 || true
    docker run -d --name "$container" -p 127.0.0.1::6379 \
      redis:7-alpine@sha256:6ab0b6e7381779332f97b8ca76193e45b0756f38d4c0dcda72dbb3c32061ab99 >/dev/null
    for i in {1..30}; do
      if docker exec "$container" redis-cli ping 2>/dev/null | grep -q PONG; then
        port=$(docker port "$container" 6379/tcp | head -1 | cut -d: -f2)
        printf 'REDIS_URL=redis://127.0.0.1:%s\n' "$port" >> "$GITHUB_ENV"
        exit 0
      fi
      sleep 2
    done
    docker logs "$container" >&2
    exit 1
    ;;
  stop)
    docker rm -f "$container" >/dev/null 2>&1 || true
    ;;
  *)
    echo "usage: test-redis.sh start|stop container-name" >&2
    exit 2
    ;;
esac
