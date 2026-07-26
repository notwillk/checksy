#!/usr/bin/env bash
set -euo pipefail

: "${RULESY_REAL_GIT:?}"

if [[ "${1-}" == "clone" ]]; then
  : "${RULESY_PROVISION_GIT_READY_FIFO:?}"
  : "${RULESY_PROVISION_GIT_RELEASE_FIFO:?}"
  printf 'ready\n' > "$RULESY_PROVISION_GIT_READY_FIFO"
  IFS= read -r _ < "$RULESY_PROVISION_GIT_RELEASE_FIFO"
fi

exec "$RULESY_REAL_GIT" "$@"
