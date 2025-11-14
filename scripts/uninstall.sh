#!/usr/bin/env sh
set -e

BIN_NAME="workspace-doctor"
TARGET="${WORKSPACE_DOCTOR_BIN_PATH:-/usr/local/bin/$BIN_NAME}"

echo "Uninstalling $BIN_NAME from $TARGET..."

if [ ! -e "$TARGET" ]; then
  echo "No installation found at $TARGET."
  exit 0
fi

if rm "$TARGET" 2>/dev/null; then
  echo "Removed $BIN_NAME."
  exit 0
fi

if command -v sudo >/dev/null 2>&1; then
  echo "Retrying removal with sudo..."
  sudo rm "$TARGET"
  echo "Removed $BIN_NAME."
else
  echo "Failed to remove $TARGET. Try running with elevated privileges." >&2
  exit 1
fi
