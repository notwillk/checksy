#!/usr/bin/env bash
set -euo pipefail

printf 'child-excluded\n' >> "$RULESY_LOCAL_ORIGIN_FORBIDDEN"
exit 92
