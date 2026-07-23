#!/usr/bin/env bash
set -euo pipefail

# Exercise version resolution without contacting GitHub or installing a binary.

FEATURE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="$FEATURE_DIR/install.sh"
REAL_BASH="${BASH:-/bin/bash}"
TEST_ROOT="$(mktemp -d)"
MOCK_BIN="$TEST_ROOT/bin"
MOCK_LOG="$TEST_ROOT/mock.log"
LATEST_TAG="v0.7.6"

trap 'rm -rf "$TEST_ROOT"' EXIT
mkdir -p "$MOCK_BIN"

cat >"$MOCK_BIN/curl" <<'EOF'
#!/bin/sh
for argument in "$@"; do
  url=$argument
done

printf 'curl-url=%s\n' "$url" >>"$MOCK_LOG"

case "$url" in
  https://api.github.com/repos/notwillk/checksy/releases/latest)
    if [ "${MOCK_LATEST_EXIT:-0}" -ne 0 ]; then
      exit "$MOCK_LATEST_EXIT"
    fi
    printf '{"tag_name":"%s"}\n' "$MOCK_LATEST_TAG"
    ;;
  https://raw.githubusercontent.com/notwillk/checksy/*/scripts/install.sh)
    printf 'mock-installer:%s\n' "$url"
    ;;
  *)
    printf 'unexpected curl URL: %s\n' "$url" >&2
    exit 1
    ;;
esac
EOF

cat >"$MOCK_BIN/bash" <<'EOF'
#!/bin/sh
installer=$(cat)
printf 'bash-version=%s\n' "${CHECKSY_VERSION:-}" >>"$MOCK_LOG"
printf 'bash-input=%s\n' "$installer" >>"$MOCK_LOG"
EOF

cat >"$MOCK_BIN/checksy" <<'EOF'
#!/bin/sh
printf 'checksy-args=%s\n' "$*" >>"$MOCK_LOG"
if [ "${MOCK_CHECKSY_EXIT:-0}" -ne 0 ]; then
  exit "$MOCK_CHECKSY_EXIT"
fi
printf '%s\n' "${MOCK_CHECKSY_VERSION:-checksy mock-version}"
EOF

chmod +x "$MOCK_BIN/curl" "$MOCK_BIN/bash" "$MOCK_BIN/checksy"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

run_case() {
  local case_name=$1
  local requested_version=$2
  local expected_tag=$3
  local resolves_latest=$4
  local raw_url="https://raw.githubusercontent.com/notwillk/checksy/$expected_tag/scripts/install.sh"
  local expected_log="$TEST_ROOT/$case_name.expected"
  local output="$TEST_ROOT/$case_name.output"

  : >"$MOCK_LOG"
  if ! PATH="$MOCK_BIN:/usr/bin:/bin" \
    VERSION="$requested_version" \
    MOCK_LOG="$MOCK_LOG" \
    MOCK_LATEST_TAG="$LATEST_TAG" \
    MOCK_CHECKSY_VERSION="checksy ${expected_tag#v}" \
    "$REAL_BASH" "$INSTALLER" >"$output" 2>&1; then
    cat "$output" >&2
    fail "$case_name installer invocation failed"
  fi

  {
    if [ "$resolves_latest" = true ]; then
      printf 'curl-url=https://api.github.com/repos/notwillk/checksy/releases/latest\n'
    fi
    printf 'curl-url=%s\n' "$raw_url"
    printf 'bash-version=%s\n' "$expected_tag"
    printf 'bash-input=mock-installer:%s\n' "$raw_url"
    printf 'checksy-args=--version\n'
  } >"$expected_log"

  if ! diff -u "$expected_log" "$MOCK_LOG"; then
    fail "$case_name used an unexpected tag or command sequence"
  fi
}

run_case bare 0.7.5 v0.7.5 false
run_case prefixed v0.7.5 v0.7.5 false
run_case latest latest "$LATEST_TAG" true
run_case current current "$LATEST_TAG" true

: >"$MOCK_LOG"
set +e
PATH="$MOCK_BIN:/usr/bin:/bin" \
  VERSION="0.7.5" \
  MOCK_LOG="$MOCK_LOG" \
  MOCK_LATEST_TAG="$LATEST_TAG" \
  MOCK_CHECKSY_EXIT=9 \
  "$REAL_BASH" "$INSTALLER" >"$TEST_ROOT/verification-failure.output" 2>&1
verification_status=$?
set -e

if [ "$verification_status" -ne 9 ]; then
  cat "$TEST_ROOT/verification-failure.output" >&2
  fail "failed checksy --version verification returned $verification_status instead of 9"
fi

: >"$MOCK_LOG"
set +e
PATH="$MOCK_BIN:/usr/bin:/bin" \
  VERSION="0.7.5" \
  MOCK_LOG="$MOCK_LOG" \
  MOCK_LATEST_TAG="$LATEST_TAG" \
  MOCK_CHECKSY_VERSION="checksy 9.9.9" \
  "$REAL_BASH" "$INSTALLER" >"$TEST_ROOT/version-mismatch.output" 2>&1
mismatch_status=$?
set -e

if [ "$mismatch_status" -ne 1 ]; then
  cat "$TEST_ROOT/version-mismatch.output" >&2
  fail "mismatched checksy version returned $mismatch_status instead of 1"
fi

if ! grep -Fx \
  "Installed checksy version mismatch: expected 'checksy 0.7.5', got 'checksy 9.9.9'" \
  "$TEST_ROOT/version-mismatch.output" >/dev/null; then
  cat "$TEST_ROOT/version-mismatch.output" >&2
  fail "mismatched checksy version did not emit the expected diagnostic"
fi

: >"$MOCK_LOG"
set +e
PATH="$MOCK_BIN:/usr/bin:/bin" \
  VERSION="latest" \
  MOCK_LOG="$MOCK_LOG" \
  MOCK_LATEST_TAG="$LATEST_TAG" \
  MOCK_LATEST_EXIT=22 \
  "$REAL_BASH" "$INSTALLER" >"$TEST_ROOT/latest-lookup-failure.output" 2>&1
lookup_status=$?
set -e

if [ "$lookup_status" -ne 1 ]; then
  cat "$TEST_ROOT/latest-lookup-failure.output" >&2
  fail "failed latest lookup returned $lookup_status instead of 1"
fi

if ! grep -Fx \
  "Unable to determine the latest checksy release tag" \
  "$TEST_ROOT/latest-lookup-failure.output" >/dev/null; then
  cat "$TEST_ROOT/latest-lookup-failure.output" >&2
  fail "failed latest lookup did not emit the expected diagnostic"
fi

if [ "$(wc -l <"$MOCK_LOG")" -ne 1 ] || \
  ! grep -Fx \
    "curl-url=https://api.github.com/repos/notwillk/checksy/releases/latest" \
    "$MOCK_LOG" >/dev/null; then
  cat "$MOCK_LOG" >&2
  fail "failed latest lookup continued past release resolution"
fi

printf 'Feature installer unit tests passed\n'
