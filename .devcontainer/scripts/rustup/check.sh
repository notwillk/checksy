#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions
require_rust_toolchain_pin_matches

if ! command -v cc >/dev/null; then
  provision_error "a native C compiler is not installed"
  exit 1
fi

if ! rustup=$(rustup_binary); then
  provision_error "rustup is not installed for the current user"
  exit 1
fi

if ! rustup_output=$(rustup_version_output "$rustup"); then
  provision_error "rustup is not usable"
  exit 1
fi
if ! actual_rustup_version=$(rustup_version_from_output "$rustup_output"); then
  provision_error "unable to parse rustup version output: $rustup_output"
  exit 1
fi
if [[ $actual_rustup_version != "$RUSTUP_VERSION" ]]; then
  provision_error "expected rustup $RUSTUP_VERSION, got $actual_rustup_version"
  exit 1
fi

if ! rustc_output=$("$rustup" run "$RUST_TOOLCHAIN_VERSION" rustc --version); then
  provision_error "Rust toolchain $RUST_TOOLCHAIN_VERSION is not installed"
  exit 1
fi
if ! actual_version=$(rustc_version_from_output "$rustc_output"); then
  provision_error "unable to parse rustc version output: $rustc_output"
  exit 1
fi
if [[ $actual_version != "$RUST_TOOLCHAIN_VERSION" ]]; then
  provision_error "expected rustc $RUST_TOOLCHAIN_VERSION, got $actual_version"
  exit 1
fi

components=$("$rustup" component list --toolchain "$RUST_TOOLCHAIN_VERSION" --installed)
if ! rust_components_present "$components"; then
  provision_error "Rust toolchain $RUST_TOOLCHAIN_VERSION requires rustfmt and clippy"
  exit 1
fi

if ! rust_toolchain_commands_usable "$rustup" "$RUST_TOOLCHAIN_VERSION"; then
  provision_error "cargo, rustfmt, or clippy is not usable with Rust toolchain $RUST_TOOLCHAIN_VERSION"
  exit 1
fi

if ! ambient_rustc_output=$(rustc --version); then
  provision_error "rustc is not available on PATH"
  exit 1
fi
if ! ambient_rustc_version=$(rustc_version_from_output "$ambient_rustc_output"); then
  provision_error "unable to parse rustc version on PATH: $ambient_rustc_output"
  exit 1
fi
if [[ $ambient_rustc_version != "$RUST_TOOLCHAIN_VERSION" ]]; then
  provision_error "PATH resolves rustc $ambient_rustc_version; expected $RUST_TOOLCHAIN_VERSION"
  exit 1
fi
for command in cargo rustfmt cargo-clippy; do
  if ! command -v "$command" >/dev/null; then
    provision_error "$command is not available on PATH"
    exit 1
  fi
done
if ! cargo --version >/dev/null || \
  ! rustfmt --version >/dev/null || \
  ! cargo clippy --version >/dev/null; then
  provision_error "the Rust development commands on PATH are not usable"
  exit 1
fi
