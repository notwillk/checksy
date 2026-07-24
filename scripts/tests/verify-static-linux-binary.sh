#!/usr/bin/env bash
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
verifier="$repo_root/scripts/verify-static-linux-binary.sh"
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT

mock_bin="$test_dir/mock-bin"
mkdir -p "$mock_bin"

cat >"$mock_bin/readelf" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  -h)
    printf '%s\n' "${MOCK_ELF_HEADER:-  Type:                              EXEC (Executable file)}"
    exit "${MOCK_ELF_HEADER_STATUS:-0}"
    ;;
  -lW)
    printf '%s\n' "${MOCK_PROGRAM_HEADERS:-Program Headers:}"
    ;;
  -dW)
    printf '%s\n' "${MOCK_DYNAMIC_SECTION:-There is no dynamic section in this file.}"
    ;;
  --version-info)
    printf '%s\n' "${MOCK_VERSION_INFO:-No version information found in this file.}"
    ;;
  *)
    echo "unexpected readelf arguments: $*" >&2
    exit 2
    ;;
esac
EOF
chmod +x "$mock_bin/readelf"

fake_binary="$test_dir/checksy"
cat >"$fake_binary" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" != "version" ]; then
  exit 2
fi
printf '%s\n' "${MOCK_BINARY_VERSION:-checksy 0.7.7}"
exit "${MOCK_BINARY_STATUS:-0}"
EOF
chmod +x "$fake_binary"

run_verifier() {
  env PATH="$mock_bin:$PATH" "$verifier" "$fake_binary" 0.7.7
}

assert_fails_with() {
  expected="$1"
  shift

  if output=$("$@" 2>&1); then
    echo "Expected command to fail: $*" >&2
    exit 1
  fi

  case "$output" in
    *"$expected"*)
      ;;
    *)
      echo "Expected failure output to contain '$expected', got:" >&2
      echo "$output" >&2
      exit 1
      ;;
  esac
}

run_verifier >/dev/null

env "MOCK_ELF_HEADER=  Type:                              DYN (Position-Independent Executable file)" \
  PATH="$mock_bin:$PATH" "$verifier" "$fake_binary" 0.7.7 >/dev/null

assert_fails_with "is not an ELF file" \
  env MOCK_ELF_HEADER_STATUS=1 PATH="$mock_bin:$PATH" \
  "$verifier" "$fake_binary" 0.7.7

assert_fails_with "is not an ELF executable" \
  env "MOCK_ELF_HEADER=  Type:                              DYN (Shared object file)" \
  PATH="$mock_bin:$PATH" "$verifier" "$fake_binary" 0.7.7

assert_fails_with "has a program interpreter" \
  env "MOCK_PROGRAM_HEADERS=  INTERP         0x0000000000000350" \
  PATH="$mock_bin:$PATH" "$verifier" "$fake_binary" 0.7.7

assert_fails_with "has dynamic NEEDED entries" \
  env "MOCK_DYNAMIC_SECTION= 0x0000000000000001 (NEEDED) Shared library: [libc.so.6]" \
  PATH="$mock_bin:$PATH" "$verifier" "$fake_binary" 0.7.7

assert_fails_with "has GLIBC version requirements" \
  env "MOCK_VERSION_INFO=  Name: GLIBC_2.39  Flags: none  Version: 2" \
  PATH="$mock_bin:$PATH" "$verifier" "$fake_binary" 0.7.7

assert_fails_with "reported 'checksy 9.9.9'" \
  env MOCK_BINARY_VERSION="checksy 9.9.9" PATH="$mock_bin:$PATH" \
  "$verifier" "$fake_binary" 0.7.7

echo "verify-static-linux-binary tests passed"
