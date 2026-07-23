#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions
require_rust_toolchain_pin_matches

if ! command -v cc >/dev/null; then
  if ! command -v sudo >/dev/null || ! command -v apt-get >/dev/null; then
    provision_error "build-essential installation requires sudo and apt-get"
    exit 1
  fi
  if ! sudo -n true; then
    provision_error "passwordless sudo is required for non-interactive provisioning"
    exit 1
  fi
  sudo -n env DEBIAN_FRONTEND=noninteractive apt-get update
  sudo -n env DEBIAN_FRONTEND=noninteractive \
    apt-get install -y --no-install-recommends build-essential
fi
if ! command -v cc >/dev/null; then
  provision_error "build-essential did not provide a native C compiler"
  exit 1
fi

temporary_directory=""
cleanup() {
  if [[ -n $temporary_directory ]]; then
    rm -rf -- "$temporary_directory"
  fi
}
trap cleanup EXIT

rustup=""
if candidate=$(rustup_binary); then
  if rustup_output=$(rustup_version_output "$candidate" 2>/dev/null) && \
    actual_rustup_version=$(rustup_version_from_output "$rustup_output") && \
    [[ $actual_rustup_version == "$RUSTUP_VERSION" ]]; then
    rustup=$candidate
  fi
fi

if [[ -z $rustup ]]; then
  if [[ $(uname -s) != Linux ]]; then
    provision_error "Rustup provisioning supports Linux devcontainers only"
    exit 1
  fi

  target=$(rustup_target_for_arch "$(uname -m)")
  url=$(rustup_download_url "$target")
  checksum=$(rustup_checksum_for_target "$target")
  temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/checksy-rustup.XXXXXX")
  installer="$temporary_directory/rustup-init"
  download_file "$url" "$installer"
  if ! verify_sha256 "$installer" "$checksum"; then
    provision_error "SHA-256 verification failed for Rustup $RUSTUP_VERSION ($target)"
    exit 1
  fi
  chmod 0755 "$installer"
  "$installer" -y --no-modify-path --profile minimal --default-toolchain none
  rustup=$(rustup_binary) || true
  if [[ ! -x $rustup ]]; then
    provision_error "rustup installer did not create $rustup"
    exit 1
  fi
fi

rustup_output=$(rustup_version_output "$rustup")
actual_rustup_version=$(rustup_version_from_output "$rustup_output") || {
  provision_error "unable to parse rustup version output: $rustup_output"
  exit 1
}
if [[ $actual_rustup_version != "$RUSTUP_VERSION" ]]; then
  provision_error "expected rustup $RUSTUP_VERSION, got $actual_rustup_version"
  exit 1
fi

"$rustup" toolchain install "$RUST_TOOLCHAIN_VERSION" \
  --profile minimal \
  --component rustfmt \
  --component clippy \
  --no-self-update
