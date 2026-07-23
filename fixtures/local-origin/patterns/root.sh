#!/usr/bin/env bash
set -euo pipefail

[[ $(<Brewfile) == root-brewfile ]]
[[ $(<template.txt) == root-template ]]
printf 'root-pattern\n' >> "$CHECKSY_LOCAL_ORIGIN_TRACE"
