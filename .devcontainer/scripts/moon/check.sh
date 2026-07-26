#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions

if ! command -v moon >/dev/null; then
  provision_error "Moon $MOON_VERSION is not installed"
  exit 1
fi
if ! command -v moonx >/dev/null; then
  provision_error "Moon executor $MOON_VERSION is not installed"
  exit 1
fi

actual_moon_version=$(moon --version)
expected_moon_version="moon $MOON_VERSION"
if [[ $actual_moon_version != "$expected_moon_version" ]]; then
  provision_error "expected '$expected_moon_version', got '$actual_moon_version'"
  exit 1
fi

actual_moonx_version=$(moonx --version)
expected_moonx_version="moon-exec $MOON_VERSION"
if [[ $actual_moonx_version != "$expected_moonx_version" ]]; then
  provision_error "expected '$expected_moonx_version', got '$actual_moonx_version'"
  exit 1
fi
