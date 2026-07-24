#!/usr/bin/env bash
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
cross_config="$repo_root/src/Cross.toml"
cross_compile="$repo_root/scripts/cross-compile.sh"
ci_workflow="$repo_root/.github/workflows/ci.yml"
release_workflow="$repo_root/.github/workflows/release.yml"
devcontainer="$repo_root/.devcontainer/devcontainer.json"

fail() {
  printf 'release portability contract: %s\n' "$*" >&2
  exit 1
}

require_line() {
  local expected=$1
  local file=$2
  grep -Fqx "$expected" "$file" ||
    fail "$file is missing: $expected"
}

require_line \
  'image = "ghcr.io/cross-rs/x86_64-unknown-linux-musl@sha256:77db671d8356a64ae72a3e1415e63f547f26d374fbe3c4762c1cd36c7eac7b99"' \
  "$cross_config"
require_line \
  'image = "ghcr.io/cross-rs/aarch64-unknown-linux-musl@sha256:702154f52b2d8091671aa2c84d5582d849f949977228c735ff8462f93cc0e1e4"' \
  "$cross_config"

grep -F 'cargo install cross --version 0.2.5 --locked --force' "$cross_compile" >/dev/null ||
  fail "Cross 0.2.5 installation is not pinned"
grep -F 'cross build --locked --release --target "$target"' "$cross_compile" >/dev/null ||
  fail "release build does not enforce Cargo.lock"
if grep -F 'cargo install cross --git' "$cross_compile" >/dev/null; then
  fail "Cross must not be installed from a mutable Git branch"
fi

for target in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
  grep -F "target: $target" "$ci_workflow" >/dev/null ||
    fail "CI does not build $target"
  grep -F "target: $target" "$release_workflow" >/dev/null ||
    fail "release automation does not build $target"
done
if grep -E 'target: (x86_64|aarch64)-unknown-linux-gnu' \
  "$ci_workflow" "$release_workflow" >/dev/null; then
  fail "official Linux workflow targets must not use glibc"
fi

grep -F 'runner: ubuntu-24.04-arm' "$ci_workflow" >/dev/null ||
  fail "CI does not execute the aarch64 artifact natively"
grep -F 'runner: ubuntu-24.04-arm' "$release_workflow" >/dev/null ||
  fail "release automation does not execute the aarch64 artifact natively"
for test_script in \
  scripts/tests/verify-static-linux-binary.sh \
  scripts/tests/release-portability.sh; do
  grep -F "bash $test_script" "$ci_workflow" >/dev/null ||
    fail "Quality CI does not run $test_script"
done
grep -F '"image": "mcr.microsoft.com/devcontainers/base:ubuntu-24.04"' \
  "$devcontainer" >/dev/null ||
  fail "devcontainer base is not pinned to the Ubuntu 24.04 line"

printf 'Release portability contract passed\n'
