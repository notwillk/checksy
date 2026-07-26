#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"

KVM_DEVICE=/dev/kvm
KVM_DEVICE_NUMBER=a:e8
cleanup_created_device=false

warn_and_skip() {
  provision_error "warning: $*"
  exit 0
}

cleanup() {
  local actual_device_number

  if [[ $cleanup_created_device == true && ! -L $KVM_DEVICE && -c $KVM_DEVICE ]] &&
    actual_device_number=$(stat -c '%t:%T' "$KVM_DEVICE") &&
    [[ $actual_device_number == "$KVM_DEVICE_NUMBER" ]]; then
    sudo -n rm -f -- "$KVM_DEVICE" || true
  fi
}
trap cleanup EXIT

grant_access() {
  sudo -n chown "0:$remote_group_id" "$KVM_DEVICE" &&
    sudo -n chmod 0660 "$KVM_DEVICE" &&
    bash "$SCRIPT_DIR/check.sh"
}

if ! grep -Eq '^[[:space:]]*232[[:space:]]+kvm$' /proc/misc; then
  warn_and_skip "host KVM acceleration is unavailable; leaving $KVM_DEVICE absent"
fi

for required_command in sudo mknod chown chmod stat id; do
  command -v "$required_command" >/dev/null ||
    warn_and_skip "cannot expose KVM because $required_command is unavailable"
done
sudo -n true || warn_and_skip "cannot expose KVM without passwordless sudo"
remote_group_id=$(id -g)

if [[ -e $KVM_DEVICE || -L $KVM_DEVICE ]]; then
  if [[ -L $KVM_DEVICE || ! -c $KVM_DEVICE ]]; then
    warn_and_skip \
      "$KVM_DEVICE already exists but is not a character device; leaving it unchanged"
  fi

  actual_device_number=$(stat -c '%t:%T' "$KVM_DEVICE") ||
    warn_and_skip "cannot inspect the existing $KVM_DEVICE; leaving it unchanged"
  if [[ $actual_device_number != "$KVM_DEVICE_NUMBER" ]]; then
    warn_and_skip \
      "$KVM_DEVICE has device number $actual_device_number; leaving it unchanged"
  fi
  if [[ ! -r $KVM_DEVICE || ! -w $KVM_DEVICE ]]; then
    device_mount=$(stat -c '%m' "$KVM_DEVICE") || warn_and_skip \
      "cannot identify the existing $KVM_DEVICE mount; leaving it unchanged"
    device_filesystem=$(stat -f -c '%T' "$KVM_DEVICE") || warn_and_skip \
      "cannot identify the existing $KVM_DEVICE filesystem; leaving it unchanged"
    if [[ $device_mount != /dev || $device_filesystem != tmpfs ]]; then
      warn_and_skip \
        "$KVM_DEVICE is not on the container-owned /dev tmpfs; leaving it unchanged"
    fi
    grant_access ||
      warn_and_skip "failed to repair remote-user access to the existing $KVM_DEVICE"
  fi
  exit 0
fi

sudo -n mknod -m 0660 "$KVM_DEVICE" c 10 232 ||
  warn_and_skip "failed to create $KVM_DEVICE; the devcontainer will continue without KVM"
cleanup_created_device=true

grant_access ||
  warn_and_skip "failed to grant safe remote-user access to $KVM_DEVICE"
cleanup_created_device=false
