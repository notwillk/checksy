#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions

install_node_packages=false
if ! command -v node >/dev/null || ! command -v npm >/dev/null; then
  install_node_packages=true
elif ! node_version_is_supported "$(node --version)"; then
  install_node_packages=true
fi

if [[ $install_node_packages == true ]]; then
  if ! command -v sudo >/dev/null || ! command -v apt-get >/dev/null; then
    provision_error "Node.js installation requires sudo and apt-get"
    exit 1
  fi
  if ! sudo -n true; then
    provision_error "passwordless sudo is required for non-interactive Node.js provisioning"
    exit 1
  fi
  sudo -n env DEBIAN_FRONTEND=noninteractive apt-get update
  sudo -n env DEBIAN_FRONTEND=noninteractive \
    apt-get install -y --no-install-recommends nodejs npm
fi

if ! command -v node >/dev/null; then
  provision_error "Ubuntu packages did not install Node.js"
  exit 1
fi
node_version=$(node --version)
if ! node_version_is_supported "$node_version"; then
  provision_error "Ubuntu supplied '$node_version'; Node.js $MINIMUM_NODE_MAJOR or newer is required"
  exit 1
fi
if ! command -v npm >/dev/null; then
  provision_error "Ubuntu packages did not install npm"
  exit 1
fi

mkdir -p "$HOME/.local"
npm install --global --prefix "$HOME/.local" --no-audit --no-fund \
  "@devcontainers/cli@$DEVCONTAINER_CLI_VERSION"
