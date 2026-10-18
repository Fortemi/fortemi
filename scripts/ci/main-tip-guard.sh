#!/usr/bin/env bash
# Sourceable. 0=current, 10=superseded (caller must skip successfully), 1=unknown.
main_tip_guard() {
  local tip attempt
  [[ "${CHAIN_SOURCE_SHA:?}" =~ ^[0-9a-f]{40}$ ]] || return 1
  for attempt in 1 2 3; do
    if tip=$(timeout 20 git ls-remote --exit-code origin refs/heads/main 2>/dev/null) &&
       [[ "$tip" =~ ^([0-9a-f]{40})[[:space:]]+refs/heads/main$ ]]; then
      if [[ "${BASH_REMATCH[1]}" == "$CHAIN_SOURCE_SHA" ]]; then
        return 0
      fi
      echo "::notice::Superseded source ${CHAIN_SOURCE_SHA}; skipping main side effect (tip ${BASH_REMATCH[1]})."
      return 10
    fi
    [[ "$attempt" == 3 ]] || sleep "${CI_TIP_RETRY_DELAY:-2}"
  done
  echo '::error::Cannot establish main freshness after three bounded reads; side effect refused.' >&2
  return 1
}
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main_tip_guard
fi
