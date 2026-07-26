#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
workspace_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
# shellcheck source=../../.devcontainer/scripts/shared/lib.sh
source "$workspace_root/.devcontainer/scripts/shared/lib.sh"
load_tool_versions "$workspace_root/.devcontainer/tool-versions.env"

: "${RUNNER_TEMP:?RUNNER_TEMP must identify the GitHub Actions temporary directory}"
: "${GITHUB_PATH:?GITHUB_PATH must identify the GitHub Actions PATH command file}"

target=$(moon_target_for_platform "$(uname -s)" "$(uname -m)")
archive=$(moon_archive_name "$target")
url=$(moon_download_url "$target")
checksum=$(moon_checksum_for_target "$target")
temporary_directory=$(mktemp -d "$RUNNER_TEMP/rulesy-moon.XXXXXX")
archive_path="$temporary_directory/$archive"
extracted_directory="$temporary_directory/moon_cli-$target"
install_directory="$RUNNER_TEMP/rulesy-moon-$MOON_VERSION-$target"

cleanup() {
  rm -rf -- "$temporary_directory"
}
trap cleanup EXIT

download_file "$url" "$archive_path"
verify_sha256 "$archive_path" "$checksum"
tar --extract --xz --file "$archive_path" --directory "$temporary_directory"

if [[ ! -f $extracted_directory/moon ]] || [[ ! -f $extracted_directory/moonx ]]; then
  provision_error "Moon release archive did not contain the expected binaries"
  exit 1
fi

mkdir -p "$install_directory"
install -m 0755 "$extracted_directory/moon" "$install_directory/moon"
install -m 0755 "$extracted_directory/moonx" "$install_directory/moonx"

test "$("$install_directory/moon" --version)" = "moon $MOON_VERSION"
test "$("$install_directory/moonx" --version)" = "moon-exec $MOON_VERSION"
printf '%s\n' "$install_directory" >>"$GITHUB_PATH"
