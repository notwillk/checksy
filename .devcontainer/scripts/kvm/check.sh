#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"

KVM_DEVICE=/dev/kvm
KVM_DEVICE_NUMBER=a:e8

if [[ -L $KVM_DEVICE || ! -c $KVM_DEVICE ]]; then
  provision_error "$KVM_DEVICE is not a character device"
  exit 1
fi

actual_device_number=$(stat -c '%t:%T' "$KVM_DEVICE")
if [[ $actual_device_number != "$KVM_DEVICE_NUMBER" ]]; then
  provision_error \
    "$KVM_DEVICE has device number $actual_device_number; expected $KVM_DEVICE_NUMBER"
  exit 1
fi

if [[ ! -r $KVM_DEVICE || ! -w $KVM_DEVICE ]]; then
  provision_error "$KVM_DEVICE is not readable and writable by the remote user"
  exit 1
fi
