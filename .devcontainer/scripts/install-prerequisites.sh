#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=provision-lib.sh
source "$SCRIPT_DIR/provision-lib.sh"

if ! command -v sudo >/dev/null; then
  provision_error "sudo is required to install provisioning prerequisites"
  exit 1
fi
if ! command -v apt-get >/dev/null; then
  provision_error "apt-get is required; this devcontainer provisioning supports Ubuntu images only"
  exit 1
fi
if ! sudo -n true; then
  provision_error "passwordless sudo is required for non-interactive provisioning"
  exit 1
fi

packages=()
command -v curl >/dev/null || packages+=(curl)
command -v tar >/dev/null || packages+=(tar)
command -v sha256sum >/dev/null || packages+=(coreutils)

if ((${#packages[@]} == 0)); then
  exit 0
fi

sudo -n env DEBIAN_FRONTEND=noninteractive apt-get update
sudo -n env DEBIAN_FRONTEND=noninteractive \
  apt-get install -y --no-install-recommends "${packages[@]}"
