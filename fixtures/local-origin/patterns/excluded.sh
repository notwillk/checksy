#!/usr/bin/env bash
set -euo pipefail

printf 'root-excluded\n' >> "$CHECKSY_LOCAL_ORIGIN_FORBIDDEN"
exit 91
