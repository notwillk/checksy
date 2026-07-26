#!/usr/bin/env bash
set -euo pipefail

: "${RULESY_PROVISION_TRACE:?}"
: "${RULESY_PROVISION_FIXED:?}"

printf 'fix\n' >> "$RULESY_PROVISION_TRACE"
: > "$RULESY_PROVISION_FIXED"
