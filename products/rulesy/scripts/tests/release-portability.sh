#!/usr/bin/env bash
set -euo pipefail

project_root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
workspace_root="$(CDPATH= cd -- "$project_root/../.." && pwd)"
cross_config="$project_root/Cross.toml"
cross_compile="$project_root/scripts/cross-compile.sh"
release_script="$project_root/scripts/release.sh"
rulesy_manifest="$project_root/Cargo.toml"
rulesy_lock="$project_root/Cargo.lock"
rulesy_version="$project_root/src/version.rs"
moon_project="$project_root/moon.yml"
ci_workflow="$workspace_root/.github/workflows/ci.yml"
release_workflow="$workspace_root/.github/workflows/release.yml"
moon_workspace="$workspace_root/.moon/workspace.yml"
moon_installer="$workspace_root/.github/scripts/install-moon.sh"
tool_versions="$workspace_root/.devcontainer/tool-versions.env"
devcontainer="$workspace_root/.devcontainer/devcontainer.json"
public_installer="$workspace_root/scripts/install.sh"
public_uninstaller="$workspace_root/scripts/uninstall.sh"
signing_key="$workspace_root/keys/signing-key.asc"
feature_installer="$workspace_root/devcontainer-features/src/rulesy/install.sh"
feature_manifest="$workspace_root/devcontainer-features/src/rulesy/devcontainer-feature.json"
test_root=$(mktemp -d)
trap 'rm -rf -- "$test_root"' EXIT

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
grep -F 'cross build --manifest-path Cargo.toml --locked --release --target "$target"' \
  "$cross_compile" >/dev/null ||
  fail "release build does not enforce Cargo.lock"
grep -F 'dist_dir="$project_root/dist"' "$cross_compile" >/dev/null ||
  fail "release artifacts are not rooted in the Rulesy project"
grep -F 'workspace_root="$(CDPATH= cd -- "$project_root/../.." && pwd)"' \
  "$release_script" >/dev/null ||
  fail "release script does not resolve the monorepo root from the Rulesy project"
grep -F 'manifest_file="products/rulesy/Cargo.toml"' "$release_script" >/dev/null ||
  fail "release script does not update the Rulesy manifest"
grep -F 'lock_file="products/rulesy/Cargo.lock"' "$release_script" >/dev/null ||
  fail "release script does not identify the Rulesy lockfile"
grep -F -- '--manifest-path "$manifest_file"' "$release_script" >/dev/null ||
  fail "release script does not update the lockfile through the Rulesy manifest"
grep -F -- '--package rulesy' "$release_script" >/dev/null ||
  fail "release script does not target the Rulesy package lock entry"
grep -F -- '--precise "$new_version"' "$release_script" >/dev/null ||
  fail "release script does not synchronize the precise Rulesy lock version"
grep -F 'git add -- "$manifest_file" "$lock_file"' "$release_script" >/dev/null ||
  fail "release script does not stage the Rulesy manifest and lockfile"
require_line 'pub const VERSION: &str = env!("CARGO_PKG_VERSION");' "$rulesy_version"
for version_consumer in \
  "$moon_project" \
  "$release_script" \
  "$ci_workflow" \
  "$release_workflow"; do
  if grep -F 'src/version.rs' "$version_consumer" >/dev/null; then
    fail "$version_consumer still reads the superseded handwritten version"
  fi
done
grep -F 'Cargo.toml' "$moon_project" >/dev/null ||
  fail "Moon version tasks do not read the Rulesy manifest"
grep -F 'products/rulesy/Cargo.toml' "$ci_workflow" >/dev/null ||
  fail "CI artifact verification does not read the Rulesy manifest"
grep -F 'products/rulesy/Cargo.toml' "$release_workflow" >/dev/null ||
  fail "release artifact verification does not read the Rulesy manifest"
test -f "$rulesy_manifest" || fail "Rulesy manifest is missing"
test -f "$rulesy_lock" || fail "Rulesy lockfile is missing"
grep -F 'cmds=(cargo sha256sum tar)' "$cross_compile" >/dev/null ||
  fail "Linux cross-build dependencies must not require rustup"
grep -F 'Missing required command for macOS builds: rustup' "$cross_compile" >/dev/null ||
  fail "macOS builds do not report a missing rustup command clearly"
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
  products/rulesy/scripts/tests/verify-static-linux-binary.sh \
  products/rulesy/scripts/tests/release-portability.sh; do
  grep -F "bash $test_script" "$ci_workflow" >/dev/null ||
    fail "Quality CI does not run $test_script"
done
grep -F 'moon run rulesy:format' "$ci_workflow" >/dev/null ||
  fail "Quality CI does not run formatting through Moon"
grep -F 'moon run rulesy:lint' "$ci_workflow" >/dev/null ||
  fail "Quality CI does not run linting through Moon"
grep -F 'moon run rulesy:build' "$ci_workflow" >/dev/null ||
  fail "CI does not build Rulesy through Moon"
grep -F 'moon run rulesy:test' "$ci_workflow" >/dev/null ||
  fail "CI does not run project tests through Moon"
grep -F 'moon run rulesy:cross-compile -- "$RULESY_TARGET"' \
  "$ci_workflow" >/dev/null ||
  fail "portable CI does not forward its release target through Moon"
grep -F 'path: products/rulesy/dist/rulesy_linux_${{ matrix.arch }}.tar.gz' \
  "$ci_workflow" >/dev/null ||
  fail "CI does not upload the package-local portable archive"
grep -F 'path: products/rulesy/dist' "$ci_workflow" >/dev/null ||
  fail "CI does not download portable archives into the Rulesy project"
grep -F 'products/rulesy/dist/*.tar.gz' "$release_workflow" >/dev/null ||
  fail "release automation does not upload package-local archives"
grep -F 'path: products/rulesy/dist/artifacts' "$release_workflow" >/dev/null ||
  fail "release automation does not download artifacts into the Rulesy project"
grep -F 'base-path-to-features: "./devcontainer-features/src"' \
  "$release_workflow" >/dev/null ||
  fail "release automation does not publish the plural Feature collection path"
grep -F '      - "v*.*.*"' "$release_workflow" >/dev/null ||
  fail "release automation no longer uses the vX.Y.Z tag trigger"
grep -F 'moon run rulesy:ensure-tag-matches-version -- "$RELEASE_TAG"' \
  "$release_workflow" >/dev/null ||
  fail "release automation does not validate the tag through Moon passthrough"
grep -F 'moon run rulesy:cross-compile -- "$RULESY_TARGET"' \
  "$release_workflow" >/dev/null ||
  fail "release automation does not forward the build target through Moon"
for workflow in "$ci_workflow" "$release_workflow"; do
  grep -F 'run: bash .github/scripts/install-moon.sh' "$workflow" >/dev/null ||
    fail "$workflow does not install Moon"
  grep -F '"moon 2.4.5"' "$workflow" >/dev/null ||
    fail "$workflow does not verify the pinned Moon version"
  grep -F '"moon-exec 2.4.5"' "$workflow" >/dev/null ||
    fail "$workflow does not verify the pinned Moon executor version"
done
grep -F -- '- macos-latest' "$ci_workflow" >/dev/null ||
  fail "CI does not install and run Moon on macOS"
grep -F 'os: macos-latest' "$release_workflow" >/dev/null ||
  fail "release automation does not install and run Moon on macOS"
if grep -Eiq '(^|[^[:alnum:]_])just([^[:alnum:]_]|$)' \
  "$ci_workflow" "$release_workflow"; then
  fail "workflow still installs or invokes Just"
fi

require_line 'versionConstraint: "=2.4.5"' "$moon_workspace"
test -f "$moon_installer" ||
  fail "CI Moon installer is missing"
grep -F 'MOON_VERSION=2.4.5' "$tool_versions" >/dev/null ||
  fail "Moon version is not pinned"
grep -F \
  'MOON_X86_64_SHA256=627f99ec29e7f52829daef9c48dfb70840313e01980d297d09e58fd9dbe1a6e9' \
  "$tool_versions" >/dev/null ||
  fail "x86-64 Linux Moon checksum is not pinned"
grep -F \
  'MOON_AARCH64_SHA256=41cca0fcca0a63de1f7c4d94d275f55c2b26ef559bf19cf7d5bbf29c2ae5df53' \
  "$tool_versions" >/dev/null ||
  fail "ARM64 Linux Moon checksum is not pinned"
grep -F \
  'MOON_X86_64_DARWIN_SHA256=5ac82abc98495b2322e01c22219320a71c45903278cbdd40f5e9d874d6cc0b65' \
  "$tool_versions" >/dev/null ||
  fail "x86-64 macOS Moon checksum is not pinned"
grep -F \
  'MOON_AARCH64_DARWIN_SHA256=c7bd4aee9fc5b9f76fb17f081e7aba2d7a1b34eba8f89535349cb3266b7af0ad' \
  "$tool_versions" >/dev/null ||
  fail "ARM64 macOS Moon checksum is not pinned"

grep -F '"image": "mcr.microsoft.com/devcontainers/base:ubuntu26.04"' \
  "$devcontainer" >/dev/null ||
  fail "devcontainer base is not pinned to the Ubuntu 26.04 line"
test -f "$public_installer" ||
  fail "public installer moved from scripts/install.sh"
test -f "$public_uninstaller" ||
  fail "public uninstaller moved from scripts/uninstall.sh"
test -f "$signing_key" ||
  fail "public signing key moved from keys/signing-key.asc"
grep -F \
  'PUBLIC_KEY_URL="https://raw.githubusercontent.com/$REPO/main/keys/signing-key.asc"' \
  "$public_installer" >/dev/null ||
  fail "public signing-key URL changed"
grep -F \
  'INSTALLER_URL="https://raw.githubusercontent.com/$REPO/$TAG/scripts/install.sh"' \
  "$feature_installer" >/dev/null ||
  fail "Feature no longer consumes the public root installer URL"
grep -F '"id": "rulesy"' "$feature_manifest" >/dev/null ||
  fail "Feature OCI identity changed"

mock_workspace="$test_root/workspace"
mock_project="$mock_workspace/products/rulesy"
mock_bin="$test_root/bin"
mock_cross_log="$test_root/cross.log"
mkdir -p "$mock_project/scripts" "$mock_project/src" "$mock_bin"
cp "$cross_compile" "$mock_project/scripts/cross-compile.sh"

cat >"$mock_bin/cargo" <<'EOF'
#!/usr/bin/env bash
echo "cargo must not be invoked when pinned Cross is already available" >&2
exit 97
EOF
cat >"$mock_bin/cross" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == --version ]]; then
  printf 'cross 0.2.5\n'
  exit 0
fi

[[ ${1:-} == build ]]
printf 'pwd=%s\nargs=%s\n' "$PWD" "$*" >"$MOCK_CROSS_LOG"
target=
while (($#)); do
  if [[ $1 == --target ]]; then
    target=$2
    break
  fi
  shift
done
[[ -n $target ]]
mkdir -p "target/$target/release"
printf 'mock rulesy\n' >"target/$target/release/rulesy"
EOF
chmod 0755 "$mock_bin/cargo" "$mock_bin/cross"

restricted_path="$mock_bin:/usr/bin:/bin"
env PATH="$restricted_path" MOCK_CROSS_LOG="$mock_cross_log" \
  "$mock_project/scripts/cross-compile.sh" x86_64-unknown-linux-musl >/dev/null ||
  fail "Linux cross-build rejected an environment without rustup"
grep -Fqx "pwd=$mock_project" "$mock_cross_log" ||
  fail "Cross did not run from the Rulesy project root"
grep -Fqx \
  'args=build --manifest-path Cargo.toml --locked --release --target x86_64-unknown-linux-musl' \
  "$mock_cross_log" ||
  fail "Cross received an unexpected project manifest or target"
test -f "$mock_project/dist/rulesy_linux_x86_64.tar.gz" ||
  fail "Linux archive was not written to the Rulesy project dist directory"
test -f "$mock_project/dist/rulesy_linux_x86_64-checksum.txt" ||
  fail "Linux checksum was not written to the Rulesy project dist directory"
test ! -e "$mock_workspace/dist" ||
  fail "release artifacts leaked into the workspace root"
tar -tzf "$mock_project/dist/rulesy_linux_x86_64.tar.gz" |
  grep -Fqx rulesy ||
  fail "Linux archive no longer contains the canonical rulesy filename"
grep -Eq \
  '^[0-9a-f]{64}  rulesy_linux_x86_64\.tar\.gz$' \
  "$mock_project/dist/rulesy_linux_x86_64-checksum.txt" ||
  fail "Linux archive checksum filename or format changed"

macos_stderr="$test_root/macos.stderr"
if env PATH="$restricted_path" \
  "$mock_project/scripts/cross-compile.sh" aarch64-apple-darwin \
  >"$test_root/macos.stdout" 2>"$macos_stderr"; then
  fail "macOS build unexpectedly succeeded without rustup"
fi
grep -Fqx 'Missing required command for macOS builds: rustup' "$macos_stderr" ||
  fail "macOS build did not report its missing rustup dependency"

printf 'Release portability contract passed\n'
