#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions

if ! command -v node >/dev/null; then
  provision_error "Codex CLI installation requires Node.js $MINIMUM_NODE_MAJOR or newer"
  exit 1
fi
node_version=$(node --version)
if ! node_version_is_supported "$node_version"; then
  provision_error "Codex CLI requires Node.js $MINIMUM_NODE_MAJOR or newer; found '$node_version'"
  exit 1
fi
if ! command -v npm >/dev/null; then
  provision_error "Codex CLI installation requires npm"
  exit 1
fi

codex_prefix="$HOME/.local/opt/codex-cli"
mkdir -p "$codex_prefix"
npm install --global --prefix "$codex_prefix" --no-audit --no-fund --ignore-scripts \
  "@openai/codex@$CODEX_CLI_VERSION"
