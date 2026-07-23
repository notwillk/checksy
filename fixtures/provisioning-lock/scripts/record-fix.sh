#!/usr/bin/env bash
set -euo pipefail

: "${CHECKSY_PROVISION_TRACE:?}"
: "${CHECKSY_PROVISION_FIXED:?}"

printf 'fix\n' >> "$CHECKSY_PROVISION_TRACE"
: > "$CHECKSY_PROVISION_FIXED"
