#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

readonly BUILDROOT_VERSION=2025.02.16
readonly BUILDROOT_DIRECTORY="buildroot-$BUILDROOT_VERSION"
readonly BUILDROOT_ARCHIVE="$BUILDROOT_DIRECTORY.tar.xz"
readonly BUILDROOT_SHA256=15305e3d366eeaf4a5ecaf2ed42f685fd6af7fe5dbf1f62e1de5f46ee83225e2
readonly BUILDROOT_URL="https://buildroot.org/downloads/$BUILDROOT_ARCHIVE"
readonly CACHE_DIR="$PROJECT_DIR/.cache"
readonly DOWNLOAD_DIR="$CACHE_DIR/downloads"
readonly SOURCE_ROOT="$CACHE_DIR/sources"
readonly ARCHIVE_PATH="$DOWNLOAD_DIR/$BUILDROOT_ARCHIVE"
readonly SOURCE_DIR="$SOURCE_ROOT/$BUILDROOT_DIRECTORY"

verify_archive() {
  local actual_sha256
  actual_sha256=$(sha256sum "$ARCHIVE_PATH" | awk '{print $1}')
  if [[ $actual_sha256 != "$BUILDROOT_SHA256" ]]; then
    printf 'SHA-256 mismatch for %s.\n' "$ARCHIVE_PATH" >&2
    return 1
  fi
}

mkdir -p "$DOWNLOAD_DIR" "$SOURCE_ROOT"

if [[ ! -f $ARCHIVE_PATH ]]; then
  temporary_archive=$(mktemp "$DOWNLOAD_DIR/.${BUILDROOT_ARCHIVE}.XXXXXX")
  trap 'rm -f -- "$temporary_archive"' EXIT
  curl --proto '=https' --tlsv1.2 --fail --show-error --location \
    --output "$temporary_archive" "$BUILDROOT_URL"
  actual_sha256=$(sha256sum "$temporary_archive" | awk '{print $1}')
  if [[ $actual_sha256 != "$BUILDROOT_SHA256" ]]; then
    printf 'SHA-256 mismatch for downloaded %s.\n' "$BUILDROOT_ARCHIVE" >&2
    exit 1
  fi
  mv "$temporary_archive" "$ARCHIVE_PATH"
  trap - EXIT
fi

verify_archive

if [[ ! -d $SOURCE_DIR ]]; then
  temporary_source=$(mktemp -d "$SOURCE_ROOT/.${BUILDROOT_DIRECTORY}.XXXXXX")
  trap 'rm -rf -- "$temporary_source"' EXIT
  tar --extract --xz --file "$ARCHIVE_PATH" --directory "$temporary_source"
  mv "$temporary_source/$BUILDROOT_DIRECTORY" "$SOURCE_DIR"
  rmdir "$temporary_source"
  trap - EXIT
fi

printf '%s\n' "$SOURCE_DIR"
