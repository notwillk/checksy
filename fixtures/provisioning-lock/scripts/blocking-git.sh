#!/usr/bin/env bash
set -euo pipefail

: "${CHECKSY_REAL_GIT:?}"

if [[ "${1-}" == "clone" ]]; then
  : "${CHECKSY_PROVISION_GIT_READY_FIFO:?}"
  : "${CHECKSY_PROVISION_GIT_RELEASE_FIFO:?}"
  printf 'ready\n' > "$CHECKSY_PROVISION_GIT_READY_FIFO"
  IFS= read -r _ < "$CHECKSY_PROVISION_GIT_RELEASE_FIFO"
fi

exec "$CHECKSY_REAL_GIT" "$@"
