#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions

codex_cli="$HOME/.local/opt/codex-cli/bin/codex"
if [[ ! -x $codex_cli ]]; then
  provision_error "Codex CLI $CODEX_CLI_VERSION is not installed"
  exit 1
fi

actual_version=$("$codex_cli" --version)
expected_version="codex-cli $CODEX_CLI_VERSION"
if [[ $actual_version != "$expected_version" ]]; then
  provision_error "expected '$expected_version', got '$actual_version'"
  exit 1
fi
