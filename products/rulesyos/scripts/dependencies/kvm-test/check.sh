#!/usr/bin/env bash
set -euo pipefail

missing=()
command -v qemu-system-x86_64 >/dev/null || missing+=(qemu-system-x86_64)
command -v python3 >/dev/null || missing+=(python3)
python3 -c 'import pexpect' >/dev/null 2>&1 || missing+=(python3-pexpect)

if ((${#missing[@]} != 0)); then
  printf 'Rulesy devcontainer provisioning: missing RulesyOS KVM test dependencies: %s\n' \
    "${missing[*]}" >&2
  exit 1
fi
