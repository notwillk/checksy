#!/usr/bin/env bash
set -euo pipefail

[[ $(<Brewfile) == child-brewfile ]]
[[ $(<template.txt) == child-template ]]
printf 'child-pattern\n' >> "$RULESY_LOCAL_ORIGIN_TRACE"
