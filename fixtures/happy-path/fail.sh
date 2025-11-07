#!/usr/bin/env bash
set -euo pipefail

severity="${1:-unspecified}"

echo "FAIL [$severity] $(date) $USER $(pwd)"
exit 1
