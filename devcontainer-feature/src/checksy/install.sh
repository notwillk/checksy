#!/usr/bin/env bash
set -euo pipefail

REPO="notwillk/checksy"
REQUESTED_VERSION="${VERSION:-latest}"

case "$REQUESTED_VERSION" in
  latest|current)
    TAG="$(
      curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p'
    )"
    ;;
  v*)
    TAG="$REQUESTED_VERSION"
    ;;
  *)
    TAG="v$REQUESTED_VERSION"
    ;;
esac

if [ -z "$TAG" ]; then
  echo "Unable to determine the latest checksy release tag" >&2
  exit 1
fi

INSTALLER_URL="https://raw.githubusercontent.com/$REPO/$TAG/scripts/install.sh"
curl -fsSL "$INSTALLER_URL" | CHECKSY_VERSION="$TAG" bash

EXPECTED_VERSION="checksy ${TAG#v}"
if INSTALLED_VERSION="$(checksy --version)"; then
  if [ "$INSTALLED_VERSION" != "$EXPECTED_VERSION" ]; then
    echo "Installed checksy version mismatch: expected '$EXPECTED_VERSION', got '$INSTALLED_VERSION'" >&2
    exit 1
  fi
else
  verification_status=$?
  echo "Unable to verify the installed checksy version" >&2
  exit "$verification_status"
fi
