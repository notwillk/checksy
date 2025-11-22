#!/usr/bin/env bash
set -euo pipefail

V="${VERSION:-latest}"
if [ "$V" != "latest" ] && [ "$V" != "current" ]; then
  V="${V#v}"
fi

# Install checksy via upstream installer, honoring CHECKSY_VERSION
curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/install.sh | CHECKSY_VERSION="$V" bash

checksy --version || true
