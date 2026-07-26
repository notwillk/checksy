#!/usr/bin/env bash
set -euo pipefail

if ! command -v sudo >/dev/null || ! command -v apt-get >/dev/null; then
  printf '%s\n' \
    'Rulesy devcontainer provisioning: RulesyOS build dependency installation requires sudo and apt-get' \
    >&2
  exit 1
fi
if ! sudo -n true; then
  printf '%s\n' \
    'Rulesy devcontainer provisioning: passwordless sudo is required for non-interactive provisioning' \
    >&2
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
