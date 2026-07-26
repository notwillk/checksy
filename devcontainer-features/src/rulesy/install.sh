#!/usr/bin/env bash
set -euo pipefail

REPO="notwillk/rulesy"
REQUESTED_VERSION="${VERSION:-latest}"

case "$REQUESTED_VERSION" in
  latest|current)
    if LATEST_RELEASE="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")"; then
      TAG="$(printf '%s\n' "$LATEST_RELEASE" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')"
    else
      TAG=""
    fi
    ;;
  v*)
    TAG="$REQUESTED_VERSION"
    ;;
  *)
    TAG="v$REQUESTED_VERSION"
    ;;
esac

if [ -z "$TAG" ]; then
  echo "Unable to determine the latest Rulesy release tag" >&2
  exit 1
fi

INSTALLER_URL="https://raw.githubusercontent.com/$REPO/$TAG/scripts/install.sh"
curl -fsSL "$INSTALLER_URL" | RULESY_VERSION="$TAG" bash

EXPECTED_VERSION="rulesy ${TAG#v}"
if INSTALLED_VERSION="$(rulesy --version)"; then
  if [ "$INSTALLED_VERSION" != "$EXPECTED_VERSION" ]; then
    echo "Installed Rulesy version mismatch: expected '$EXPECTED_VERSION', got '$INSTALLED_VERSION'" >&2
    exit 1
  fi
else
  verification_status=$?
  echo "Unable to verify the installed Rulesy version" >&2
  exit "$verification_status"
fi
