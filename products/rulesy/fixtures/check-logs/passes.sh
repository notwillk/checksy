#!/usr/bin/env bash
set -euo pipefail

echo "[fixtures/check-logs/passes] something written to stderr" >&2
echo "[fixtures/check-logs/passes] something written to stdout"

exit 0
