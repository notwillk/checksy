#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

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

"$PROJECT_DIR/scripts/build.sh"
buildroot_dir=$("$PROJECT_DIR/scripts/prepare-buildroot.sh")

exec python3 "$SCRIPT_DIR/run.py" \
  --buildroot "$buildroot_dir" \
  built \
  --kernel "$PROJECT_DIR/output/images/bzImage" \
  --rootfs "$PROJECT_DIR/output/images/rootfs.ext2"
