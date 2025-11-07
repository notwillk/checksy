#!/usr/bin/env bash
set -euo pipefail

label=${1:-fixture}

echo "[fixtures/fix-behavior] failing check for ${label}" >&2
exit 1
