#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"

if ! command -v sudo >/dev/null || ! command -v apt-get >/dev/null; then
  provision_error "RulesyOS build dependency installation requires sudo and apt-get"
  exit 1
fi
if ! sudo -n true; then
  provision_error "passwordless sudo is required for non-interactive provisioning"
  exit 1
fi

sudo -n env DEBIAN_FRONTEND=noninteractive apt-get update
sudo -n env DEBIAN_FRONTEND=noninteractive apt-get install -y \
  --no-install-recommends \
  bc build-essential bzip2 cpio file gnu-coreutils gzip patch perl python3 \
  rsync unzip wget

if ! install --version 2>/dev/null | grep -Fq 'install (GNU coreutils)'; then
  sudo -n update-alternatives \
    --install /usr/bin/install install /usr/bin/gnuinstall 100
fi
