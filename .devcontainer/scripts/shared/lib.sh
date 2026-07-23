#!/usr/bin/env bash

provision_error() {
  printf 'checksy devcontainer provisioning: %s\n' "$*" >&2
}

provision_fail() {
  provision_error "$@"
  return 1
}

provision_script_dir() {
  cd "$(dirname "${BASH_SOURCE[0]}")" && pwd
}

devcontainer_config_dir() {
  cd "$(provision_script_dir)/../.." && pwd
}

workspace_root() {
  cd "$(devcontainer_config_dir)/.." && pwd
}

is_release_version() {
  [[ ${1:-} =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

load_tool_versions() {
  local versions_file=${1:-"$(devcontainer_config_dir)/tool-versions.env"}

  if [[ ! -f $versions_file ]]; then
    provision_fail "version file not found: $versions_file"
    return 1
  fi

  # This is a trusted, repository-owned file containing shell assignments only.
  # shellcheck disable=SC1090
  source "$versions_file"

  local required
  for required in \
    CHECKSY_VERSION \
    CODEX_CLI_VERSION \
    JUST_VERSION \
    JUST_X86_64_SHA256 \
    JUST_AARCH64_SHA256 \
    RUSTUP_VERSION \
    RUSTUP_X86_64_SHA256 \
    RUSTUP_AARCH64_SHA256 \
    RUST_TOOLCHAIN_VERSION \
    DEVCONTAINER_CLI_VERSION \
    MINIMUM_NODE_MAJOR; do
    if [[ -z ${!required:-} ]]; then
      provision_fail "missing $required in $versions_file"
      return 1
    fi
  done

  if ! is_release_version "$CHECKSY_VERSION" || \
    ! is_release_version "$CODEX_CLI_VERSION" || \
    ! is_release_version "$JUST_VERSION" || \
    ! is_release_version "$RUSTUP_VERSION" || \
    ! is_release_version "$RUST_TOOLCHAIN_VERSION" || \
    ! is_release_version "$DEVCONTAINER_CLI_VERSION"; then
    provision_fail "tool versions must use numeric major.minor.patch syntax"
    return 1
  fi

  if [[ ! $JUST_X86_64_SHA256 =~ ^[0-9a-f]{64}$ ]] || \
    [[ ! $JUST_AARCH64_SHA256 =~ ^[0-9a-f]{64}$ ]] || \
    [[ ! $RUSTUP_X86_64_SHA256 =~ ^[0-9a-f]{64}$ ]] || \
    [[ ! $RUSTUP_AARCH64_SHA256 =~ ^[0-9a-f]{64}$ ]]; then
    provision_fail "download checksums must be 64 lowercase hexadecimal characters"
    return 1
  fi

  if [[ ! $MINIMUM_NODE_MAJOR =~ ^[1-9][0-9]*$ ]]; then
    provision_fail "MINIMUM_NODE_MAJOR must be a positive integer"
    return 1
  fi
}

just_target_for_arch() {
  case ${1:-} in
    x86_64)
      printf 'x86_64-unknown-linux-musl\n'
      ;;
    aarch64|arm64)
      printf 'aarch64-unknown-linux-musl\n'
      ;;
    *)
      provision_fail "unsupported Linux architecture for Just: ${1:-<empty>}"
      return 1
      ;;
  esac
}

just_checksum_for_target() {
  case ${1:-} in
    x86_64-unknown-linux-musl)
      printf '%s\n' "$JUST_X86_64_SHA256"
      ;;
    aarch64-unknown-linux-musl)
      printf '%s\n' "$JUST_AARCH64_SHA256"
      ;;
    *)
      provision_fail "unsupported Just release target: ${1:-<empty>}"
      return 1
      ;;
  esac
}

just_archive_name() {
  printf 'just-%s-%s.tar.gz\n' "$JUST_VERSION" "$1"
}

just_download_url() {
  local target=$1
  local archive
  archive=$(just_archive_name "$target")
  printf 'https://github.com/casey/just/releases/download/%s/%s\n' "$JUST_VERSION" "$archive"
}

download_file() {
  local url=$1
  local destination=$2
  curl --proto '=https' --tlsv1.2 --fail --show-error --location \
    --output "$destination" "$url"
}

verify_sha256() {
  local file=$1
  local expected=$2

  if [[ ! $expected =~ ^[0-9a-f]{64}$ ]]; then
    provision_fail "invalid expected SHA-256 value"
    return 1
  fi

  printf '%s  %s\n' "$expected" "$file" | sha256sum --check --status -
}

node_major_from_version() {
  local version=${1:-}

  if [[ $version =~ ^v?([0-9]+)\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi

  return 1
}

node_version_is_supported() {
  local major

  if ! major=$(node_major_from_version "${1:-}"); then
    return 1
  fi

  ((major >= MINIMUM_NODE_MAJOR))
}

rust_toolchain_from_file() {
  local toolchain_file=$1
  local -a channels

  if [[ ! -f $toolchain_file ]]; then
    provision_fail "Rust toolchain file not found: $toolchain_file"
    return 1
  fi

  mapfile -t channels < <(
    sed -nE \
      's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"([^"]+)"[[:space:]]*$/\1/p' \
      "$toolchain_file"
  )
  if ((${#channels[@]} != 1)) || ! is_release_version "${channels[0]:-}"; then
    provision_fail "Rust toolchain file must contain one numeric channel"
    return 1
  fi

  printf '%s\n' "${channels[0]}"
}

require_rust_toolchain_pin_matches() {
  local root_pin
  root_pin=$(rust_toolchain_from_file "$(workspace_root)/rust-toolchain.toml") || return 1
  if [[ $root_pin != "$RUST_TOOLCHAIN_VERSION" ]]; then
    provision_fail \
      "Rust toolchain pin mismatch: tool-versions.env has $RUST_TOOLCHAIN_VERSION, rust-toolchain.toml has $root_pin"
    return 1
  fi
}

rustc_version_from_output() {
  local output=${1:-}

  if [[ $output =~ ^rustc[[:space:]]+([0-9]+\.[0-9]+\.[0-9]+)([[:space:]]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi

  return 1
}

rustup_target_for_arch() {
  case ${1:-} in
    x86_64)
      printf 'x86_64-unknown-linux-gnu\n'
      ;;
    aarch64|arm64)
      printf 'aarch64-unknown-linux-gnu\n'
      ;;
    *)
      provision_fail "unsupported Linux architecture for Rustup: ${1:-<empty>}"
      return 1
      ;;
  esac
}

rustup_checksum_for_target() {
  case ${1:-} in
    x86_64-unknown-linux-gnu)
      printf '%s\n' "$RUSTUP_X86_64_SHA256"
      ;;
    aarch64-unknown-linux-gnu)
      printf '%s\n' "$RUSTUP_AARCH64_SHA256"
      ;;
    *)
      provision_fail "unsupported Rustup release target: ${1:-<empty>}"
      return 1
      ;;
  esac
}

rustup_download_url() {
  local target=$1
  printf 'https://static.rust-lang.org/rustup/archive/%s/%s/rustup-init\n' \
    "$RUSTUP_VERSION" \
    "$target"
}

rustup_version_from_output() {
  local output=${1:-}

  if [[ $output =~ ^rustup[[:space:]]+([0-9]+\.[0-9]+\.[0-9]+)([[:space:]]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi

  return 1
}

rustup_version_output() {
  local rustup=$1

  # Avoid allowing a repository rust-toolchain.toml to trigger toolchain
  # acquisition before the pinned Rustup manager itself has been verified.
  (cd / && "$rustup" --version)
}

rust_components_present() {
  local components=${1:-}
  grep -Eq '^rustfmt(-|$)' <<<"$components" && \
    grep -Eq '^clippy(-|$)' <<<"$components"
}

rust_toolchain_commands_usable() {
  local rustup=$1
  local toolchain=$2
  local command

  for command in cargo rustfmt clippy-driver; do
    "$rustup" run "$toolchain" "$command" --version >/dev/null || return 1
  done
}

rustup_binary() {
  local cargo_home=${CARGO_HOME:-"${HOME}/.cargo"}
  local rustup="$cargo_home/bin/rustup"

  [[ -x $rustup ]] || return 1
  printf '%s\n' "$rustup"
}
