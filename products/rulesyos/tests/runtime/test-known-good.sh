#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

readonly CIRROS_VERSION=0.6.3
readonly CIRROS_BASE_URL="https://download.cirros-cloud.net/$CIRROS_VERSION"
readonly CACHE_DIR="$PROJECT_DIR/.cache/cirros"

kvm_is_ready() {
  [[ $(uname -m) == x86_64 ]] &&
    grep -Eq '^[[:space:]]*232[[:space:]]+kvm$' /proc/misc &&
    [[ -c /dev/kvm ]] &&
    [[ $(stat -c '%t:%T' /dev/kvm) == a:e8 ]] &&
    [[ -r /dev/kvm && -w /dev/kvm ]]
}

if ! kvm_is_ready; then
  printf 'WARNING: KVM acceleration is unavailable; skipping RulesyOS VM test.\n' >&2
  exit 0
fi

download_artifact() {
  local filename=$1
  local expected_sha256=$2
  local destination="$CACHE_DIR/$filename"
  local temporary_file
  local actual_sha256

  if [[ ! -f $destination ]]; then
    temporary_file=$(mktemp "$CACHE_DIR/.${filename}.XXXXXX")
    curl --proto '=https' --tlsv1.2 --fail --show-error --location \
      --output "$temporary_file" "$CIRROS_BASE_URL/$filename"
    actual_sha256=$(sha256sum "$temporary_file" | awk '{print $1}')
    if [[ $actual_sha256 != "$expected_sha256" ]]; then
      rm -f -- "$temporary_file"
      printf 'SHA-256 mismatch for downloaded %s.\n' "$filename" >&2
      exit 1
    fi
    mv "$temporary_file" "$destination"
  fi

  actual_sha256=$(sha256sum "$destination" | awk '{print $1}')
  if [[ $actual_sha256 != "$expected_sha256" ]]; then
    printf 'SHA-256 mismatch for cached %s; remove it and retry.\n' \
      "$destination" >&2
    exit 1
  fi

  printf '%s\n' "$destination"
}

mkdir -p "$CACHE_DIR"
buildroot_dir=$("$PROJECT_DIR/scripts/prepare-buildroot.sh")
disk=$(download_artifact \
  "cirros-$CIRROS_VERSION-x86_64-disk.img" \
  7d6355852aeb6dbcd191bcda7cd74f1536cfe5cbf8a10495a7283a8396e4b75b)
kernel=$(download_artifact \
  "cirros-$CIRROS_VERSION-x86_64-kernel" \
  a491ac80495772be3d6d11d7eeb5987e9a7e9150aefa19894a49e1682fec727c)
initramfs=$(download_artifact \
  "cirros-$CIRROS_VERSION-x86_64-initramfs" \
  e01b0c4bbf969784b8d91c0054bc8794ba8811b0db5b86986c6c6990a3f8a4b9)

exec python3 "$SCRIPT_DIR/run.py" \
  --buildroot "$buildroot_dir" \
  known-good \
  --disk "$disk" \
  --kernel "$kernel" \
  --initramfs "$initramfs"
