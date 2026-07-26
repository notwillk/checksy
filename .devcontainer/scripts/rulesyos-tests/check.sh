#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"

missing=()
command -v qemu-system-x86_64 >/dev/null || missing+=(qemu-system-x86_64)
command -v python3 >/dev/null || missing+=(python3)
python3 -c 'import pexpect' >/dev/null 2>&1 || missing+=(python3-pexpect)

if ((${#missing[@]} != 0)); then
  provision_error "missing RulesyOS KVM test dependencies: ${missing[*]}"
  exit 1
fi
