#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=provision-lib.sh
source "$SCRIPT_DIR/provision-lib.sh"

if ! command -v sudo >/dev/null || ! command -v apt-get >/dev/null; then
  provision_error "Entr installation requires sudo and apt-get"
  exit 1
fi
if ! sudo -n true; then
  provision_error "passwordless sudo is required for non-interactive provisioning"
  exit 1
fi

sudo -n env DEBIAN_FRONTEND=noninteractive apt-get update
sudo -n env DEBIAN_FRONTEND=noninteractive \
  apt-get install -y --no-install-recommends entr
