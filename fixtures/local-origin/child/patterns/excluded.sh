#!/usr/bin/env bash
set -euo pipefail

printf 'child-excluded\n' >> "$CHECKSY_LOCAL_ORIGIN_FORBIDDEN"
exit 92
