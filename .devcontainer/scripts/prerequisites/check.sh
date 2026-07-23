#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"

missing=()
for required_command in sudo curl tar sha256sum; do
  command -v "$required_command" >/dev/null || missing+=("$required_command")
done

if ((${#missing[@]} != 0)); then
  provision_error "missing required commands: ${missing[*]}"
  exit 1
fi
