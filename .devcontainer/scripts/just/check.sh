#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions

if ! command -v just >/dev/null; then
  provision_error "Just $JUST_VERSION is not installed"
  exit 1
fi

actual_version=$(just --version)
expected_version="just $JUST_VERSION"
if [[ $actual_version != "$expected_version" ]]; then
  provision_error "expected '$expected_version', got '$actual_version'"
  exit 1
fi
