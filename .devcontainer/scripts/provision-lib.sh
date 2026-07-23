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
  cd "$(provision_script_dir)/.." && pwd
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
    JUST_VERSION \
    JUST_X86_64_SHA256 \
    JUST_AARCH64_SHA256 \
    DEVCONTAINER_CLI_VERSION \
    MINIMUM_NODE_MAJOR; do
    if [[ -z ${!required:-} ]]; then
      provision_fail "missing $required in $versions_file"
      return 1
    fi
  done

  if ! is_release_version "$CHECKSY_VERSION" || \
    ! is_release_version "$JUST_VERSION" || \
    ! is_release_version "$DEVCONTAINER_CLI_VERSION"; then
    provision_fail "tool versions must use numeric major.minor.patch syntax"
    return 1
  fi

  if [[ ! $JUST_X86_64_SHA256 =~ ^[0-9a-f]{64}$ ]] || \
    [[ ! $JUST_AARCH64_SHA256 =~ ^[0-9a-f]{64}$ ]]; then
    provision_fail "Just checksums must be 64 lowercase hexadecimal characters"
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
  curl --fail --show-error --location --output "$destination" "$url"
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
