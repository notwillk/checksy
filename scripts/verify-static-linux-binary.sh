#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 <binary> <expected-version>" >&2
}

fail() {
  echo "Static Linux binary verification failed: $*" >&2
  exit 1
}

if [ "$#" -ne 2 ]; then
  usage
  exit 2
fi

binary="$1"
expected_version="$2"

if [ ! -f "$binary" ]; then
  fail "'$binary' is not a regular file"
fi

if [ ! -x "$binary" ]; then
  fail "'$binary' is not executable"
fi

if [ -z "$expected_version" ]; then
  fail "expected version must not be empty"
fi

if ! command -v readelf >/dev/null 2>&1; then
  fail "required command 'readelf' is unavailable"
fi

if ! elf_header=$(LC_ALL=C readelf -h "$binary" 2>&1); then
  fail "'$binary' is not an ELF file: $elf_header"
fi

elf_type=$(
  printf '%s\n' "$elf_header" |
    sed -n 's/^[[:space:]]*Type:[[:space:]]*//p'
)
case "$elf_type" in
  EXEC\ \(Executable\ file\) | DYN\ \(Position-Independent\ Executable\ file\))
    ;;
  *)
    fail "'$binary' is not an ELF executable (ELF type: ${elf_type:-unknown})"
    ;;
esac

program_headers=$(LC_ALL=C readelf -lW "$binary" 2>&1) ||
  fail "unable to inspect ELF program headers for '$binary': $program_headers"
if printf '%s\n' "$program_headers" | grep -Eq '(^|[[:space:]])INTERP([[:space:]]|$)|Requesting program interpreter'; then
  fail "'$binary' has a program interpreter"
fi

dynamic_section=$(LC_ALL=C readelf -dW "$binary" 2>&1) ||
  fail "unable to inspect ELF dynamic section for '$binary': $dynamic_section"
if printf '%s\n' "$dynamic_section" | grep -Eq '\(NEEDED\)'; then
  fail "'$binary' has dynamic NEEDED entries"
fi

version_info=$(LC_ALL=C readelf --version-info "$binary" 2>&1) ||
  fail "unable to inspect ELF version requirements for '$binary': $version_info"
if printf '%s\n' "$version_info" | grep -Eq 'GLIBC_'; then
  fail "'$binary' has GLIBC version requirements"
fi

if ! reported_version=$("$binary" version); then
  fail "'$binary version' exited unsuccessfully"
fi
if [ "$reported_version" != "checksy $expected_version" ]; then
  fail "'$binary version' reported '$reported_version', expected 'checksy $expected_version'"
fi

echo "Verified static Linux binary: $binary (checksy $expected_version)"
