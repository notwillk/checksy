#!/usr/bin/env bash
set -euo pipefail

severity="${1:-unspecified}"

user="${USER:-$(id -un 2>/dev/null || echo unknown)}"

echo "PASS [$severity] $(date) $user $(pwd)"
