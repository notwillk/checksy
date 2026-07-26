#!/usr/bin/env bash
set -euo pipefail

echo "[fixtures/check-logs/fails] something written to stderr" >&2
echo "[fixtures/check-logs/fails] something written to stdout"

exit 3
