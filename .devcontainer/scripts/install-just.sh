#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=provision-lib.sh
source "$SCRIPT_DIR/provision-lib.sh"
load_tool_versions

if [[ $(uname -s) != Linux ]]; then
  provision_error "Just provisioning supports Linux devcontainers only"
  exit 1
fi
if ! command -v sudo >/dev/null || ! sudo -n true; then
  provision_error "passwordless sudo is required for non-interactive provisioning"
  exit 1
fi

target=$(just_target_for_arch "$(uname -m)")
archive=$(just_archive_name "$target")
url=$(just_download_url "$target")
checksum=$(just_checksum_for_target "$target")
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/checksy-just.XXXXXX")
archive_path="$temporary_directory/$archive"
staged_binary="/usr/local/bin/.checksy-just-${JUST_VERSION}-$$"

cleanup() {
  rm -rf -- "$temporary_directory"
  sudo -n rm -f -- "$staged_binary" >/dev/null 2>&1 || true
}
trap cleanup EXIT

download_file "$url" "$archive_path"
if ! verify_sha256 "$archive_path" "$checksum"; then
  provision_error "SHA-256 verification failed for $archive"
  exit 1
fi

tar --extract --gzip --file "$archive_path" --directory "$temporary_directory" just
if [[ ! -f $temporary_directory/just ]]; then
  provision_error "Just release archive did not contain the expected binary"
  exit 1
fi

sudo -n install -m 0755 -- "$temporary_directory/just" "$staged_binary"
sudo -n mv -f -- "$staged_binary" /usr/local/bin/just
