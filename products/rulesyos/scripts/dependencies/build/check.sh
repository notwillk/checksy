#!/usr/bin/env bash
set -euo pipefail

missing=()
for required_command in \
  bc bzip2 cpio file g++ gcc gnuinstall gzip make patch perl python3 rsync unzip wget; do
  command -v "$required_command" >/dev/null || missing+=("$required_command")
done
install --version 2>/dev/null | grep -Fq 'install (GNU coreutils)' || \
  missing+=(gnu-install-selection)

if ((${#missing[@]} != 0)); then
  printf 'Rulesy devcontainer provisioning: missing RulesyOS build dependencies: %s\n' \
    "${missing[*]}" >&2
  exit 1
fi
