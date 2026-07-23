#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEVCONTAINER_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=provision-lib.sh
source "$SCRIPT_DIR/provision-lib.sh"
load_tool_versions "$DEVCONTAINER_DIR/tool-versions.env"

TEST_ROOT=$(mktemp -d)
trap 'rm -rf -- "$TEST_ROOT"' EXIT

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

assert_equal() {
  local expected=$1
  local actual=$2
  local description=$3
  if [[ $actual != "$expected" ]]; then
    fail "$description: expected '$expected', got '$actual'"
  fi
}

assert_fails() {
  local description=$1
  shift
  if "$@" >/dev/null 2>&1; then
    fail "$description unexpectedly succeeded"
  fi
}

assert_equal 0.7.6 "$CHECKSY_VERSION" "Checksy version pin"
assert_equal 1.57.0 "$JUST_VERSION" "Just version pin"
assert_equal 0.88.0 "$DEVCONTAINER_CLI_VERSION" "Dev Container CLI version pin"
is_release_version 1.57.0 || fail "valid release version was rejected"
assert_fails "two-component release version" is_release_version 1.57
assert_fails "prefixed release version" is_release_version v1.57.0

mapfile -t checksy_feature_refs < <(
  grep -oE 'ghcr\.io/notwillk/checksy/checksy[^"[:space:]]*' \
    "$DEVCONTAINER_DIR/devcontainer.json" || true
)
if ((${#checksy_feature_refs[@]} != 1)); then
  fail "devcontainer.json must contain exactly one canonical Checksy Feature reference"
fi
checksy_feature_ref=${checksy_feature_refs[0]}
if [[ ! $checksy_feature_ref =~ ^ghcr\.io/notwillk/checksy/checksy@sha256:[0-9a-f]{64}$ ]]; then
  fail "Checksy Feature must use the canonical package name and a lowercase SHA-256 digest"
fi
feature_checksy_version=$(
  awk -v feature="\"$checksy_feature_ref\"" '
    index($0, feature) { in_feature = 1; next }
    in_feature && /"version"[[:space:]]*:/ {
      line = $0
      sub(/^[^:]*:[[:space:]]*"/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
    in_feature && /^[[:space:]]*}/ { exit }
  ' "$DEVCONTAINER_DIR/devcontainer.json"
)
assert_equal "$CHECKSY_VERSION" "$feature_checksy_version" "Checksy Feature version option"

x86_target=$(just_target_for_arch x86_64)
arm_target=$(just_target_for_arch aarch64)
assert_equal x86_64-unknown-linux-musl "$x86_target" "x86_64 target mapping"
assert_equal aarch64-unknown-linux-musl "$arm_target" "aarch64 target mapping"
assert_equal "$arm_target" "$(just_target_for_arch arm64)" "arm64 target alias"
assert_fails "unsupported architecture" just_target_for_arch riscv64

x86_url=$(just_download_url "$x86_target")
arm_url=$(just_download_url "$arm_target")
assert_equal \
  "https://github.com/casey/just/releases/download/1.57.0/just-1.57.0-x86_64-unknown-linux-musl.tar.gz" \
  "$x86_url" \
  "x86_64 Just download"
assert_equal \
  "https://github.com/casey/just/releases/download/1.57.0/just-1.57.0-aarch64-unknown-linux-musl.tar.gz" \
  "$arm_url" \
  "aarch64 Just download"
assert_equal \
  45b548094283cb9739af8f13273b8cddeee869f5b4ef2bb631b1f311cb566155 \
  "$(just_checksum_for_target "$x86_target")" \
  "x86_64 Just checksum"
assert_equal \
  f225044a81adea6e0b3a8b9370aaf374e6af76c8735ae263ac993df55fd137ec \
  "$(just_checksum_for_target "$arm_target")" \
  "aarch64 Just checksum"

MOCK_CURL_URL=""
curl() {
  if [[ $1 != --fail || $2 != --show-error || $3 != --location || $4 != --output ]]; then
    return 90
  fi
  MOCK_CURL_URL=$6
  printf 'downloaded fixture\n' >"$5"
}
download_file "$x86_url" "$TEST_ROOT/download"
assert_equal "$x86_url" "$MOCK_CURL_URL" "curl download URL"
assert_equal "downloaded fixture" "$(<"$TEST_ROOT/download")" "downloaded fixture contents"
unset -f curl

printf 'checksum fixture\n' >"$TEST_ROOT/checksum"
actual_checksum=$(sha256sum "$TEST_ROOT/checksum" | awk '{print $1}')
verify_sha256 "$TEST_ROOT/checksum" "$actual_checksum" || fail "valid checksum was rejected"
assert_fails \
  "mismatched checksum" \
  verify_sha256 "$TEST_ROOT/checksum" \
  0000000000000000000000000000000000000000000000000000000000000000

assert_equal 20 "$(node_major_from_version v20.11.1)" "prefixed Node major"
assert_equal 22 "$(node_major_from_version 22.0.0)" "unprefixed Node major"
node_version_is_supported v20.0.0 || fail "minimum Node version was rejected"
node_version_is_supported v22.1.0 || fail "newer Node version was rejected"
assert_fails "old Node version" node_version_is_supported v19.9.0
assert_fails "malformed Node version" node_version_is_supported 20.x

expected_helpers=(
  install-prerequisites.sh
  install-entr.sh
  check-just.sh
  install-just.sh
  check-devcontainer-cli.sh
  install-devcontainer-cli.sh
)
for helper in "${expected_helpers[@]}"; do
  grep -F "exec bash ./scripts/$helper" "$DEVCONTAINER_DIR/checksy.yaml" >/dev/null || \
    fail "checksy.yaml does not reference ./scripts/$helper"
  [[ -f $SCRIPT_DIR/$helper ]] || fail "missing helper relative to checksy.yaml: $helper"
done
if grep -F '.devcontainer/scripts/' "$DEVCONTAINER_DIR/checksy.yaml" >/dev/null; then
  fail "checksy.yaml helper paths must be relative to its .devcontainer working directory"
fi

printf 'Devcontainer provisioning helper tests passed\n'
