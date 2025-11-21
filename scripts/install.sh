#!/usr/bin/env sh
set -e

REPO="notwillk/checksy"
BIN_NAME="checksy"

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

if [ -z "$VERSION" ]; then
  echo "Unable to determine version" >&2
  exit 1
fi

TAG="$VERSION"              # e.g. v0.1.0
BASENAME_VERSION="${TAG#v}" # e.g. 0.1.0

TARBALL="${BIN_NAME}_${BASENAME_VERSION}_${OS}_${ARCH}.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$TARBALL"

echo "Installing $BIN_NAME $TAG for $OS/$ARCH..."
echo "Downloading: $URL"

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" -o "$TMPDIR/$TARBALL"
tar -C "$TMPDIR" -xzf "$TMPDIR/$TARBALL"

chmod +x "$TMPDIR/$BIN_NAME"
sudo mv "$TMPDIR/$BIN_NAME" /usr/local/bin/$BIN_NAME

echo "Done. Run '$BIN_NAME --help' to get started."
