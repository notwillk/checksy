#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=provision-lib.sh
source "$SCRIPT_DIR/provision-lib.sh"
load_tool_versions

if ! command -v node >/dev/null; then
  provision_error "Node.js $MINIMUM_NODE_MAJOR or newer is not installed"
  exit 1
fi
node_version=$(node --version)
if ! node_version_is_supported "$node_version"; then
  provision_error "Node.js $MINIMUM_NODE_MAJOR or newer is required; found '$node_version'"
  exit 1
fi
if ! command -v npm >/dev/null; then
  provision_error "npm is not installed"
  exit 1
fi
if ! command -v devcontainer >/dev/null; then
  provision_error "Dev Container CLI $DEVCONTAINER_CLI_VERSION is not installed"
  exit 1
fi

actual_version=$(devcontainer --version)
if [[ $actual_version != "$DEVCONTAINER_CLI_VERSION" ]]; then
  provision_error "expected Dev Container CLI '$DEVCONTAINER_CLI_VERSION', got '$actual_version'"
  exit 1
fi
