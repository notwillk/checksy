#!/usr/bin/env bash
set -euo pipefail

TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="$(cd "$TESTS_DIR/.." && pwd)"
DEVCONTAINER_DIR="$(cd "$SCRIPTS_DIR/.." && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPTS_DIR/shared/lib.sh"
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

assert_equal 0.8.1 "$RULESY_VERSION" "Rulesy version pin"
assert_equal 0.145.0 "$CODEX_CLI_VERSION" "Codex CLI version pin"
assert_equal 2.4.5 "$MOON_VERSION" "Moon version pin"
assert_equal 1.29.0 "$RUSTUP_VERSION" "Rustup version pin"
assert_equal 1.94.1 "$RUST_TOOLCHAIN_VERSION" "Rust toolchain version pin"
assert_equal 0.88.0 "$DEVCONTAINER_CLI_VERSION" "Dev Container CLI version pin"
is_release_version 1.2.3 || fail "valid release version was rejected"
assert_fails "two-component release version" is_release_version 1.2
assert_fails "prefixed release version" is_release_version v1.2.3

mapfile -t rulesy_feature_refs < <(
  grep -oE 'ghcr\.io/notwillk/rulesy/rulesy[^"[:space:]]*' \
    "$DEVCONTAINER_DIR/devcontainer.json" || true
)
if ((${#rulesy_feature_refs[@]} != 1)); then
  fail "devcontainer.json must contain exactly one canonical Rulesy Feature reference"
fi
rulesy_feature_ref=${rulesy_feature_refs[0]}
if [[ ! $rulesy_feature_ref =~ ^ghcr\.io/notwillk/rulesy/rulesy@sha256:[0-9a-f]{64}$ ]]; then
  fail "Rulesy Feature must use the canonical package name and a lowercase SHA-256 digest"
fi
feature_rulesy_version=$(
  awk -v feature="\"$rulesy_feature_ref\"" '
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
assert_equal "$RULESY_VERSION" "$feature_rulesy_version" "Rulesy Feature version option"
devcontainer_base=$(
  sed -nE 's/^[[:space:]]*"image":[[:space:]]*"([^"]+)",?$/\1/p' \
    "$DEVCONTAINER_DIR/devcontainer.json"
)
assert_equal \
  "mcr.microsoft.com/devcontainers/base:ubuntu26.04" \
  "$devcontainer_base" \
  "devcontainer base image"
if grep -F 'devcontainer-features/rustup' "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null; then
  fail "Rustup must be provisioned by Rulesy rather than a devcontainer Feature"
fi
grep -F '"/home/vscode/.local/opt/codex-cli/bin:/home/vscode/.local/bin:/home/vscode/.cargo/bin:${containerEnv:PATH}"' \
  "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null || \
  fail "devcontainer remote PATH must expose user-installed Rulesy provisioning tools"
rulesy_lifecycle_command='/usr/local/bin/rulesy --config=.devcontainer/rulesy.yaml check --fix --non-interactive'
grep -F "\"postCreateCommand\": \"$rulesy_lifecycle_command\"" \
  "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null || \
  fail "devcontainer creation must converge through the pinned Rulesy binary"
grep -F '"--device-cgroup-rule"' "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null || \
  fail "devcontainer must grant access only to an explicitly identified device"
grep -F '"c 10:232 rwm"' "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null || \
  fail "devcontainer must grant KVM device access by its exact device number"
grep -F "\"postStartCommand\": \"$rulesy_lifecycle_command\"" \
  "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null || \
  fail "devcontainer start must converge through the pinned Rulesy binary"
if grep -F '"--privileged"' "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null; then
  fail "devcontainer must not use privileged mode for KVM"
fi
if grep -E '"--device(=|")' "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null; then
  fail "devcontainer must not bind a host device that may not exist"
fi

if grep -E 'sudo[^#]*npm[[:space:]]+install' \
  "$SCRIPTS_DIR/devcontainer-cli/install.sh" >/dev/null; then
  fail "Dev Container CLI npm lifecycle scripts must not run as root"
fi
grep -F -- '--prefix "$HOME/.local"' \
  "$SCRIPTS_DIR/devcontainer-cli/install.sh" >/dev/null || \
  fail "Dev Container CLI must install into the remote user's local prefix"
ci_workflow="$(workspace_root)/.github/workflows/ci.yml"
grep -F \
  'bash -o pipefail -c "find .devcontainer/scripts .github/scripts scripts products/rulesy/scripts devcontainer-features' \
  "$ci_workflow" >/dev/null || \
  fail "CI shell syntax discovery must cover workspace, product, and Feature scripts"
grep -F "xargs -0 -r -n1 bash -n" "$ci_workflow" >/dev/null || \
  fail "CI shell syntax discovery must not invoke Bash without a script"
grep -F 'sh -n scripts/install.sh' "$ci_workflow" >/dev/null || \
  fail "CI must preserve the public root installer path"
grep -F 'sh -n scripts/uninstall.sh' "$ci_workflow" >/dev/null || \
  fail "CI must preserve the public root uninstaller path"
for task in build format lint test; do
  grep -F "moon run rulesy:$task" "$ci_workflow" >/dev/null || \
    fail "CI must run the Rulesy $task task through Moon"
done
for task in lint test; do
  grep -F "moon run rulesy-devcontainer-feature:$task" "$ci_workflow" >/dev/null || \
    fail "CI must run the Feature $task task through Moon"
done
grep -F 'test "$(moon --version)" = "moon 2.4.5"' "$ci_workflow" >/dev/null || \
  fail "CI must assert the exact Moon version"
grep -F 'test "$(moonx --version)" = "moon-exec 2.4.5"' "$ci_workflow" >/dev/null || \
  fail "CI must assert the exact Moon executor version"
grep -F 'test "${GITHUB_ACTIONS:-}" = true' "$ci_workflow" >/dev/null || \
  fail "CI must prove the GitHub Actions marker reaches the devcontainer"
grep -F 'test ! -e "$HOME/.local/opt/codex-cli/bin/codex"' "$ci_workflow" >/dev/null || \
  fail "CI must prove the local-only Codex CLI was not installed"
grep -F '"GITHUB_ACTIONS": "${localEnv:GITHUB_ACTIONS}"' \
  "$DEVCONTAINER_DIR/devcontainer.json" >/dev/null || \
  fail "devcontainer must propagate the GitHub Actions marker"
grep -F 'skip-if: '\''[ "${GITHUB_ACTIONS:-}" = "true" ]'\''' \
  "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
  fail "Codex CLI rule must skip only in GitHub Actions"
GITHUB_ACTIONS=true bash -c '[ "${GITHUB_ACTIONS:-}" = "true" ]' || \
  fail "Codex CLI predicate did not skip GitHub Actions"
if env -u GITHUB_ACTIONS bash -c '[ "${GITHUB_ACTIONS:-}" = "true" ]'; then
  fail "Codex CLI predicate skipped local development"
fi

mock_home="$TEST_ROOT/codex-home"
mock_codex_bin="$mock_home/.local/opt/codex-cli/bin"
mkdir -p "$mock_codex_bin"
cat >"$mock_codex_bin/codex" <<'EOF'
#!/usr/bin/env bash
printf 'codex-cli 0.145.0\n'
EOF
chmod 0755 "$mock_codex_bin/codex"
HOME="$mock_home" PATH="$mock_codex_bin:$PATH" bash "$SCRIPTS_DIR/codex-cli/check.sh" || \
  fail "Codex CLI check rejected the pinned version"
foreign_bin="$TEST_ROOT/codex-foreign-bin"
mkdir -p "$foreign_bin"
cat >"$foreign_bin/codex" <<'EOF'
#!/usr/bin/env bash
printf 'codex-cli 99.0.0\n'
EOF
chmod 0755 "$foreign_bin/codex"
HOME="$mock_home" PATH="$foreign_bin:$mock_codex_bin:$PATH" \
  bash "$SCRIPTS_DIR/codex-cli/check.sh" || \
  fail "ambient Codex CLI shadowing changed managed-install compliance"
cat >"$mock_codex_bin/codex" <<'EOF'
#!/usr/bin/env bash
printf 'codex-cli 0.144.0\n'
EOF
chmod 0755 "$mock_codex_bin/codex"
assert_fails \
  "mismatched Codex CLI version" \
  env HOME="$mock_home" PATH="$mock_codex_bin:$PATH" bash "$SCRIPTS_DIR/codex-cli/check.sh"

mock_install_home="$TEST_ROOT/codex-install-home"
mock_bin="$TEST_ROOT/codex-mock-bin"
mock_npm_log="$TEST_ROOT/codex-npm-args"
foreign_codex="$mock_install_home/.local/bin/codex"
mkdir -p "$mock_install_home/.local/bin" "$mock_bin"
printf 'foreign codex sentinel\n' >"$foreign_codex"
cat >"$mock_bin/node" <<'EOF'
#!/usr/bin/env bash
printf 'v22.0.0\n'
EOF
cat >"$mock_bin/npm" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" >"$MOCK_NPM_LOG"
EOF
chmod 0755 "$mock_bin/node" "$mock_bin/npm"
HOME="$mock_install_home" \
  MOCK_NPM_LOG="$mock_npm_log" \
  PATH="$mock_bin:/usr/bin:/bin" \
  bash "$SCRIPTS_DIR/codex-cli/install.sh"
expected_npm_args=$'install\n--global\n--prefix\n'"$mock_install_home"$'/.local/opt/codex-cli\n--no-audit\n--no-fund\n--ignore-scripts\n@openai/codex@0.145.0'
assert_equal "$expected_npm_args" "$(<"$mock_npm_log")" "Codex CLI npm invocation"
assert_equal "foreign codex sentinel" "$(<"$foreign_codex")" "foreign Codex CLI preservation"
if grep -E 'sudo[^#]*npm[[:space:]]+install' \
  "$SCRIPTS_DIR/codex-cli/install.sh" >/dev/null; then
  fail "Codex CLI npm lifecycle scripts must not run as root"
fi

root_toolchain_file="$DEVCONTAINER_DIR/../rust-toolchain.toml"
root_rust_version=$(rust_toolchain_from_file "$root_toolchain_file")
assert_equal "$RUST_TOOLCHAIN_VERSION" "$root_rust_version" "root Rust toolchain pin"
require_rust_toolchain_pin_matches || fail "Rust toolchain pins do not match"
printf '[toolchain]\nchannel = "stable"\n' >"$TEST_ROOT/invalid-rust-toolchain.toml"
assert_fails \
  "non-numeric Rust channel" \
  rust_toolchain_from_file "$TEST_ROOT/invalid-rust-toolchain.toml"
assert_equal \
  1.94.1 \
  "$(rustc_version_from_output 'rustc 1.94.1 (e408947bf 2026-03-25)')" \
  "rustc version parsing"
assert_fails "malformed rustc version" rustc_version_from_output "rustc stable"
assert_equal \
  1.29.0 \
  "$(rustup_version_from_output 'rustup 1.29.0 (28d1352db 2026-03-05)')" \
  "rustup version parsing"
assert_fails "malformed rustup version" rustup_version_from_output "rustup stable"
rust_components_present $'cargo-x86_64-unknown-linux-gnu\nclippy-x86_64-unknown-linux-gnu\nrustfmt-x86_64-unknown-linux-gnu' || \
  fail "installed rustfmt and clippy components were not recognized"
assert_fails \
  "missing clippy component" \
  rust_components_present $'cargo-x86_64-unknown-linux-gnu\nrustfmt-x86_64-unknown-linux-gnu'
mock_rustup="$TEST_ROOT/mock-rustup"
cat >"$mock_rustup" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ $1 == run ]]
[[ $2 == 1.94.1 ]]
[[ $4 == --version ]]
[[ ${MOCK_RUSTUP_FAIL:-} != "$3" ]]
EOF
chmod 0755 "$mock_rustup"
rust_toolchain_commands_usable "$mock_rustup" "$RUST_TOOLCHAIN_VERSION" || \
  fail "usable Rust toolchain commands were rejected"
export MOCK_RUSTUP_FAIL=clippy-driver
assert_fails \
  "unusable Rust toolchain command" \
  rust_toolchain_commands_usable "$mock_rustup" "$RUST_TOOLCHAIN_VERSION"
unset MOCK_RUSTUP_FAIL
grep -F 'build-essential' "$SCRIPTS_DIR/rustup/install.sh" >/dev/null || \
  fail "Rustup installer must provision the native build prerequisite"
grep -F 'xz' "$SCRIPTS_DIR/prerequisites/check.sh" >/dev/null || \
  fail "provisioning prerequisites must require xz archive support"
grep -F 'xz-utils' "$SCRIPTS_DIR/prerequisites/install.sh" >/dev/null || \
  fail "provisioning prerequisites must install xz archive support"
if grep -F 'sh.rustup.rs' "$SCRIPTS_DIR/rustup/install.sh" >/dev/null; then
  fail "Rustup installer must not execute the mutable shell bootstrap"
fi
grep -F -- '--profile minimal' "$SCRIPTS_DIR/rustup/install.sh" >/dev/null || \
  fail "Rustup installer must use the minimal profile"
grep -F -- '--component rustfmt' "$SCRIPTS_DIR/rustup/install.sh" >/dev/null || \
  fail "Rustup installer must install rustfmt"
grep -F -- '--component clippy' "$SCRIPTS_DIR/rustup/install.sh" >/dev/null || \
  fail "Rustup installer must install clippy"
if grep -E 'sudo[^#]*(rustup|cargo|rustc)' "$SCRIPTS_DIR/rustup/install.sh" >/dev/null; then
  fail "Rustup and Rust toolchain commands must run as the remote user"
fi
grep -F 'rust_toolchain_commands_usable' "$SCRIPTS_DIR/rustup/check.sh" >/dev/null || \
  fail "Rust check must exercise the selected toolchain commands"
grep -F 'https://cli.github.com/packages stable main' \
  "$SCRIPTS_DIR/github-cli/install.sh" >/dev/null || \
  fail "GitHub CLI installer must use GitHub's maintained Debian repository"
grep -F 'apt-get install -y --no-install-recommends gh' \
  "$SCRIPTS_DIR/github-cli/install.sh" >/dev/null || \
  fail "GitHub CLI installer must install gh"
grep -F 'name: GitHub CLI is authenticated' "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
  fail "rulesy.yaml must check GitHub CLI authentication"
grep -F 'check: gh auth status --active --hostname github.com' \
  "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
  fail "GitHub CLI authentication check must target the active github.com account"
grep -F 'severity: warn' "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
  fail "GitHub CLI authentication must be warning-only"
grep -F 'name: Host KVM acceleration is available' \
  "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
  fail "rulesy.yaml must warn when host KVM is unavailable"
kvm_rules=$(
  sed -n \
    '/name: Host KVM acceleration is available/,/name: Entr is installed/p' \
    "$DEVCONTAINER_DIR/rulesy.yaml"
)
assert_equal \
  2 \
  "$(grep -Fc 'severity: warn' <<<"$kvm_rules")" \
  "warning-only KVM rule count"
grep -F 'skip-if: |' <<<"$kvm_rules" >/dev/null || \
  fail "KVM exposure rule must skip when host KVM is unavailable"
grep -F "grep -Eq '^[[:space:]]*232[[:space:]]+kvm$' /proc/misc" \
  <<<"$kvm_rules" >/dev/null || \
  fail "rulesy.yaml must identify host KVM by its exact misc device number"
grep -F 'name: KVM is exposed to the devcontainer' \
  "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
  fail "rulesy.yaml must manage optional devcontainer KVM exposure"

if grep -Eq '^[[:space:]]*232[[:space:]]+kvm$' /proc/misc; then
  bash "$SCRIPTS_DIR/kvm/check.sh" || \
    fail "KVM check rejected the provisioned device on a KVM host"
else
  assert_fails \
    "KVM device on a non-KVM host" \
    bash "$SCRIPTS_DIR/kvm/check.sh"
  kvm_path_before=$(stat -c '%F:%t:%T:%u:%g:%a' /dev/kvm 2>/dev/null || printf 'absent')
  kvm_install_output=$(bash "$SCRIPTS_DIR/kvm/install.sh" 2>&1) || \
    fail "KVM installation must not fail when host KVM is unavailable"
  grep -F 'host KVM acceleration is unavailable' <<<"$kvm_install_output" >/dev/null || \
    fail "KVM installation must warn when host KVM is unavailable"
  kvm_path_after=$(stat -c '%F:%t:%T:%u:%g:%a' /dev/kvm 2>/dev/null || printf 'absent')
  assert_equal "$kvm_path_before" "$kvm_path_after" "non-KVM host device state"
fi
grep -F 'mknod -m 0660 "$KVM_DEVICE" c 10 232' \
  "$SCRIPTS_DIR/kvm/install.sh" >/dev/null || \
  fail "KVM installer must create only character device 10:232"
grep -F '[[ $device_mount != /dev || $device_filesystem != tmpfs ]]' \
  "$SCRIPTS_DIR/kvm/install.sh" >/dev/null || \
  fail "KVM installer must not repair access outside the container-owned /dev"
grep -F 'failed to repair remote-user access to the existing $KVM_DEVICE' \
  "$SCRIPTS_DIR/kvm/install.sh" >/dev/null || \
  fail "KVM installer must repair access to a verified container-owned KVM device"
grep -F 'leaving it unchanged' "$SCRIPTS_DIR/kvm/install.sh" >/dev/null || \
  fail "KVM installer must preserve unexpected pre-existing paths"
grep -F 'name: RulesyOS KVM test dependencies are installed' \
  "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
  fail "rulesy.yaml must provision RulesyOS KVM test dependencies"
grep -F 'command -v qemu-system-x86_64' \
  "$SCRIPTS_DIR/rulesyos-tests/check.sh" >/dev/null || \
  fail "QEMU check must require the x86-64 system emulator"
grep -F "python3 -c 'import pexpect'" \
  "$SCRIPTS_DIR/rulesyos-tests/check.sh" >/dev/null || \
  fail "RulesyOS test dependency check must require Python pexpect"
grep -F 'qemu-system-x86 python3-pexpect' \
  "$SCRIPTS_DIR/rulesyos-tests/install.sh" >/dev/null || \
  fail "RulesyOS test dependency installer must install QEMU and Python pexpect"
if grep -F 'qemu-utils' "$SCRIPTS_DIR/rulesyos-tests/install.sh" >/dev/null; then
  fail "RulesyOS bootstrap tests do not require qemu-utils"
fi

moon_x86_target=$(moon_target_for_arch x86_64)
moon_arm_target=$(moon_target_for_arch aarch64)
moon_x86_darwin_target=$(moon_target_for_platform Darwin x86_64)
moon_arm_darwin_target=$(moon_target_for_platform Darwin arm64)
assert_equal x86_64-unknown-linux-musl "$moon_x86_target" "x86_64 Moon target mapping"
assert_equal aarch64-unknown-linux-musl "$moon_arm_target" "aarch64 Moon target mapping"
assert_equal "$moon_arm_target" "$(moon_target_for_arch arm64)" "Moon arm64 target alias"
assert_fails "unsupported Moon architecture" moon_target_for_arch riscv64
assert_equal x86_64-apple-darwin "$moon_x86_darwin_target" "x86_64 macOS Moon target"
assert_equal aarch64-apple-darwin "$moon_arm_darwin_target" "ARM64 macOS Moon target"
assert_fails "unsupported Moon platform" moon_target_for_platform FreeBSD x86_64

rustup_x86_target=$(rustup_target_for_arch x86_64)
rustup_arm_target=$(rustup_target_for_arch aarch64)
assert_equal x86_64-unknown-linux-gnu "$rustup_x86_target" "x86_64 Rustup target mapping"
assert_equal aarch64-unknown-linux-gnu "$rustup_arm_target" "aarch64 Rustup target mapping"
assert_equal "$rustup_arm_target" "$(rustup_target_for_arch arm64)" "Rustup arm64 target alias"
assert_fails "unsupported Rustup architecture" rustup_target_for_arch riscv64
assert_equal \
  "https://static.rust-lang.org/rustup/archive/1.29.0/x86_64-unknown-linux-gnu/rustup-init" \
  "$(rustup_download_url "$rustup_x86_target")" \
  "x86_64 Rustup download"
assert_equal \
  "https://static.rust-lang.org/rustup/archive/1.29.0/aarch64-unknown-linux-gnu/rustup-init" \
  "$(rustup_download_url "$rustup_arm_target")" \
  "aarch64 Rustup download"
assert_equal \
  4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10 \
  "$(rustup_checksum_for_target "$rustup_x86_target")" \
  "x86_64 Rustup checksum"
assert_equal \
  9732d6c5e2a098d3521fca8145d826ae0aaa067ef2385ead08e6feac88fa5792 \
  "$(rustup_checksum_for_target "$rustup_arm_target")" \
  "aarch64 Rustup checksum"

moon_x86_url=$(moon_download_url "$moon_x86_target")
moon_arm_url=$(moon_download_url "$moon_arm_target")
moon_x86_darwin_url=$(moon_download_url "$moon_x86_darwin_target")
moon_arm_darwin_url=$(moon_download_url "$moon_arm_darwin_target")
assert_equal \
  "https://github.com/moonrepo/moon/releases/download/v2.4.5/moon_cli-x86_64-unknown-linux-musl.tar.xz" \
  "$moon_x86_url" \
  "x86_64 Moon download"
assert_equal \
  "https://github.com/moonrepo/moon/releases/download/v2.4.5/moon_cli-aarch64-unknown-linux-musl.tar.xz" \
  "$moon_arm_url" \
  "aarch64 Moon download"
assert_equal \
  627f99ec29e7f52829daef9c48dfb70840313e01980d297d09e58fd9dbe1a6e9 \
  "$(moon_checksum_for_target "$moon_x86_target")" \
  "x86_64 Moon checksum"
assert_equal \
  41cca0fcca0a63de1f7c4d94d275f55c2b26ef559bf19cf7d5bbf29c2ae5df53 \
  "$(moon_checksum_for_target "$moon_arm_target")" \
  "aarch64 Moon checksum"
assert_equal \
  "https://github.com/moonrepo/moon/releases/download/v2.4.5/moon_cli-x86_64-apple-darwin.tar.xz" \
  "$moon_x86_darwin_url" \
  "x86_64 macOS Moon download"
assert_equal \
  "https://github.com/moonrepo/moon/releases/download/v2.4.5/moon_cli-aarch64-apple-darwin.tar.xz" \
  "$moon_arm_darwin_url" \
  "ARM64 macOS Moon download"
assert_equal \
  5ac82abc98495b2322e01c22219320a71c45903278cbdd40f5e9d874d6cc0b65 \
  "$(moon_checksum_for_target "$moon_x86_darwin_target")" \
  "x86_64 macOS Moon checksum"
assert_equal \
  c7bd4aee9fc5b9f76fb17f081e7aba2d7a1b34eba8f89535349cb3266b7af0ad \
  "$(moon_checksum_for_target "$moon_arm_darwin_target")" \
  "ARM64 macOS Moon checksum"

MOCK_CURL_URL=""
curl() {
  if [[ $1 != --proto || $2 != '=https' || $3 != --tlsv1.2 || \
    $4 != --fail || $5 != --show-error || $6 != --location || \
    $7 != --output ]]; then
    return 90
  fi
  MOCK_CURL_URL=$9
  printf 'downloaded fixture\n' >"$8"
}
download_file "$moon_x86_url" "$TEST_ROOT/download"
assert_equal "$moon_x86_url" "$MOCK_CURL_URL" "curl download URL"
assert_equal "downloaded fixture" "$(<"$TEST_ROOT/download")" "downloaded fixture contents"
unset -f curl

printf 'checksum fixture\n' >"$TEST_ROOT/checksum"
actual_checksum=$(sha256sum "$TEST_ROOT/checksum" | awk '{print $1}')
verify_sha256 "$TEST_ROOT/checksum" "$actual_checksum" || fail "valid checksum was rejected"
assert_fails \
  "mismatched checksum" \
  verify_sha256 "$TEST_ROOT/checksum" \
  0000000000000000000000000000000000000000000000000000000000000000

mock_moon_bin="$TEST_ROOT/moon-bin"
mkdir -p "$mock_moon_bin"
cat >"$mock_moon_bin/moon" <<'EOF'
#!/usr/bin/env bash
printf 'moon 2.4.5\n'
EOF
cat >"$mock_moon_bin/moonx" <<'EOF'
#!/usr/bin/env bash
printf 'moon-exec 2.4.5\n'
EOF
chmod 0755 "$mock_moon_bin/moon" "$mock_moon_bin/moonx"
PATH="$mock_moon_bin:/usr/bin:/bin" bash "$SCRIPTS_DIR/moon/check.sh" || \
  fail "Moon check rejected the pinned binaries"
cat >"$mock_moon_bin/moonx" <<'EOF'
#!/usr/bin/env bash
printf 'moon-exec 2.4.4\n'
EOF
chmod 0755 "$mock_moon_bin/moonx"
assert_fails \
  "mismatched Moon executor version" \
  env PATH="$mock_moon_bin:/usr/bin:/bin" bash "$SCRIPTS_DIR/moon/check.sh"
grep -F 'tar --extract --xz' "$SCRIPTS_DIR/moon/install.sh" >/dev/null || \
  fail "Moon installer must extract the pinned tar.xz archive"
grep -F '"$extracted_directory/moon"' "$SCRIPTS_DIR/moon/install.sh" >/dev/null || \
  fail "Moon installer must stage the moon binary from the verified archive"
grep -F '"$extracted_directory/moonx"' "$SCRIPTS_DIR/moon/install.sh" >/dev/null || \
  fail "Moon installer must stage the moonx binary from the verified archive"

assert_equal 20 "$(node_major_from_version v20.11.1)" "prefixed Node major"
assert_equal 22 "$(node_major_from_version 22.0.0)" "unprefixed Node major"
node_version_is_supported v20.0.0 || fail "minimum Node version was rejected"
node_version_is_supported v22.1.0 || fail "newer Node version was rejected"
assert_fails "old Node version" node_version_is_supported v19.9.0
assert_fails "malformed Node version" node_version_is_supported 20.x

expected_files=(
  codex-cli/check.sh
  codex-cli/install.sh
  devcontainer-cli/check.sh
  devcontainer-cli/install.sh
  entr/check.sh
  entr/install.sh
  github-cli/check.sh
  github-cli/install.sh
  kvm/check.sh
  kvm/install.sh
  moon/check.sh
  moon/install.sh
  prerequisites/check.sh
  prerequisites/install.sh
  rulesyos-tests/check.sh
  rulesyos-tests/install.sh
  rustup/check.sh
  rustup/install.sh
  shared/lib.sh
  tests/run.sh
)
mapfile -t actual_files < <(
  find "$SCRIPTS_DIR" -type f -printf '%P\n' | sort
)
if ((${#actual_files[@]} != ${#expected_files[@]})); then
  printf 'Expected script files:\n%s\n' "${expected_files[*]}" >&2
  printf 'Actual script files:\n%s\n' "${actual_files[*]}" >&2
  fail "provisioning script layout is not closed"
fi
for index in "${!expected_files[@]}"; do
  assert_equal "${expected_files[$index]}" "${actual_files[$index]}" "script layout entry $index"
done

expected_tools=(prerequisites kvm rulesyos-tests entr moon rustup devcontainer-cli github-cli codex-cli)
for tool in "${expected_tools[@]}"; do
  grep -F "check: exec bash ./scripts/$tool/check.sh" "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
    fail "rulesy.yaml does not reference ./scripts/$tool/check.sh"
  grep -F "fix: exec bash ./scripts/$tool/install.sh" "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null || \
    fail "rulesy.yaml does not reference ./scripts/$tool/install.sh"
  [[ -f $SCRIPTS_DIR/$tool/check.sh ]] || fail "missing $tool/check.sh"
  [[ -f $SCRIPTS_DIR/$tool/install.sh ]] || fail "missing $tool/install.sh"
  grep -F 'source "$SCRIPT_DIR/../shared/lib.sh"' "$SCRIPTS_DIR/$tool/check.sh" >/dev/null || \
    fail "$tool/check.sh does not source shared/lib.sh relative to itself"
  grep -F 'source "$SCRIPT_DIR/../shared/lib.sh"' "$SCRIPTS_DIR/$tool/install.sh" >/dev/null || \
    fail "$tool/install.sh does not source shared/lib.sh relative to itself"
done
if grep -F '.devcontainer/scripts/' "$DEVCONTAINER_DIR/rulesy.yaml" >/dev/null; then
  fail "rulesy.yaml helper paths must be relative to its .devcontainer working directory"
fi

printf 'Devcontainer provisioning helper tests passed\n'
