#!/usr/bin/env sh
set -e

REPO="notwillk/workspace-doctor"
BIN_NAME="workspace-doctor"

OS=$(uname | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64) ARCH="amd64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

VERSION="${WORKSPACE_DOCTOR_VERSION:-latest}"

if [ "$VERSION" = "latest" ]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\(.*\)".*/\1/p')
fi

TARBALL="${BIN_NAME}_${VERSION}_${OS}_${ARCH}.tar.gz"
URL="https://github.com/$REPO/releases/download/$VERSION/$TARBALL"

echo "Installing $BIN_NAME $VERSION for $OS/$ARCH..."
echo "Downloading: $URL"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" -o "$TMPDIR/$TARBALL"
tar -C "$TMPDIR" -xzf "$TMPDIR/$TARBALL"

chmod +x "$TMPDIR/$BIN_NAME"
sudo mv "$TMPDIR/$BIN_NAME" /usr/local/bin/$BIN_NAME

echo "Done. Run '$BIN_NAME --help' to get started."
