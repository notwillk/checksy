#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"
load_tool_versions

if [[ $(uname -s) != Linux ]]; then
  provision_error "Moon provisioning supports Linux devcontainers only"
  exit 1
fi
if ! command -v sudo >/dev/null || ! sudo -n true; then
  provision_error "passwordless sudo is required for non-interactive provisioning"
  exit 1
fi

target=$(moon_target_for_arch "$(uname -m)")
archive=$(moon_archive_name "$target")
url=$(moon_download_url "$target")
checksum=$(moon_checksum_for_target "$target")
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/rulesy-moon.XXXXXX")
archive_path="$temporary_directory/$archive"
extracted_directory="$temporary_directory/moon_cli-$target"
staged_moon="/usr/local/bin/.rulesy-moon-${MOON_VERSION}-$$"
staged_moonx="/usr/local/bin/.rulesy-moonx-${MOON_VERSION}-$$"

cleanup() {
  rm -rf -- "$temporary_directory"
  sudo -n rm -f -- "$staged_moon" "$staged_moonx" >/dev/null 2>&1 || true
}
trap cleanup EXIT

download_file "$url" "$archive_path"
if ! verify_sha256 "$archive_path" "$checksum"; then
  provision_error "SHA-256 verification failed for $archive"
  exit 1
fi

tar --extract --xz --file "$archive_path" --directory "$temporary_directory"
if [[ ! -f $extracted_directory/moon ]] || [[ ! -f $extracted_directory/moonx ]]; then
  provision_error "Moon release archive did not contain the expected binaries"
  exit 1
fi

sudo -n install -m 0755 -- "$extracted_directory/moon" "$staged_moon"
sudo -n install -m 0755 -- "$extracted_directory/moonx" "$staged_moonx"
sudo -n mv -f -- "$staged_moonx" /usr/local/bin/moonx
sudo -n mv -f -- "$staged_moon" /usr/local/bin/moon
