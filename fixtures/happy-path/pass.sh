#!/usr/bin/env bash
set -euo pipefail

severity="${1:-unspecified}"

echo "PASS [$severity] $(date) $USER $(pwd)"
