#!/usr/bin/env bash
set -euo pipefail

if ! command -v sudo >/dev/null || ! command -v apt-get >/dev/null; then
  printf '%s\n' \
    'Rulesy devcontainer provisioning: RulesyOS KVM test dependency installation requires sudo and apt-get' \
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
sudo -n env DEBIAN_FRONTEND=noninteractive \
  apt-get install -y --no-install-recommends qemu-system-x86 python3-pexpect
