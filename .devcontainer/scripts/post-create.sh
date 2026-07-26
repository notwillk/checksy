#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=shared/lib.sh
source "$SCRIPT_DIR/shared/lib.sh"

rulesy_bin=/usr/local/bin/rulesy
if [[ ! -x $rulesy_bin ]]; then
  provision_error "digest-pinned Rulesy bootstrap not found at $rulesy_bin"
  exit 1
fi

prepend_user_tool_paths
exec "$rulesy_bin" --config=.devcontainer/rulesy.yaml check --fix --non-interactive
