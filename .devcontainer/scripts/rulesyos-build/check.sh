#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"

missing=()
for required_command in \
  bc bzip2 cpio file g++ gcc gnuinstall gzip make patch perl python3 rsync unzip wget; do
  command -v "$required_command" >/dev/null || missing+=("$required_command")
done
install --version 2>/dev/null | grep -Fq 'install (GNU coreutils)' || \
  missing+=(gnu-install-selection)

if ((${#missing[@]} != 0)); then
  provision_error "missing RulesyOS build dependencies: ${missing[*]}"
  exit 1
fi
