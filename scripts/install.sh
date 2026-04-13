#!/usr/bin/env sh
set -e

REPO="notwillk/checksy"
BIN_NAME="checksy"

OS=$(uname | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

VERSION="${CHECKSY_VERSION:-latest}"

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
BASE_URL="https://github.com/$REPO/releases/download/$TAG"

TARBALL_FILENAME="${BIN_NAME}_${OS}_${ARCH}.tar.gz"
CHECKSUM_FILENAME="checksums.txt"
CHECKSUM_SIGNATURE_FILENAME="checksums.txt.sig"

TARBALL_URL="$BASE_URL/$TARBALL_FILENAME"
CHECKSUM_URL="$BASE_URL/$CHECKSUM_FILENAME"
CHECKSUM_SIGNATURE_URL="$BASE_URL/$CHECKSUM_SIGNATURE_FILENAME"
PUBLIC_KEY_URL="https://raw.githubusercontent.com/$REPO/main/keys/signing-key.asc"

echo "Installing $BIN_NAME $TAG for $OS/$ARCH..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading: $TARBALL_URL"
curl -fsSL "$TARBALL_URL" -o "$TMPDIR/$TARBALL_FILENAME"

echo "Downloading: $CHECKSUM_URL"
curl -fsSL "$CHECKSUM_URL" -o "$TMPDIR/$CHECKSUM_FILENAME"

echo "Downloading: $CHECKSUM_SIGNATURE_URL"
curl -fsSL "$CHECKSUM_SIGNATURE_URL" -o "$TMPDIR/$CHECKSUM_SIGNATURE_FILENAME"

if [ -f "$TMPDIR/checksums.txt.sig" ]; then
  echo "Downloading: $PUBLIC_KEY_URL"
  curl -fsSL "$PUBLIC_KEY_URL" -o "$TMPDIR/signing-key.asc"
  echo "Verifying binary signature..."
  import_result=$(gpg --batch --import "$TMPDIR/signing-key.asc" 2>&1 || true)
  if echo "$import_result" | grep -q "imported" || echo "$import_result" | grep -q "unchanged"; then
    if gpg --batch --verify "$TMPDIR/checksums.txt.sig" "$TMPDIR/checksums.txt" 2>/dev/null; then
      echo "Signature verified successfully"
    else
      echo "Signature verification failed!" >&2
      exit 1
    fi
  else
    echo "Warning: Could not import signing key, skipping signature verification" >&2
  fi
fi

CHECKSUM_EXPECTED=$(grep "$TARBALL_FILENAME$" "$TMPDIR/checksums.txt" | awk '{print $1}')
if [ -n "$CHECKSUM_EXPECTED" ]; then
  CHECKSUM_ACTUAL=$(sha256sum "$TMPDIR/$TARBALL_FILENAME" | awk '{print $1}')
  if [ "$CHECKSUM_EXPECTED" != "$CHECKSUM_ACTUAL" ]; then
    echo "Checksum mismatch!" >&2
    exit 1
  fi
  echo "Checksum verified"
fi

tar -C "$TMPDIR" -xzf "$TMPDIR/$TARBALL_FILENAME"

chmod +x "$TMPDIR/$BIN_NAME"
sudo mv "$TMPDIR/$BIN_NAME" "${DEST:-/usr/local/bin}/$BIN_NAME"

echo "Done. Run '$BIN_NAME --help' to get started."
