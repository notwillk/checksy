#!/usr/bin/env bash
set -euo pipefail

[[ -f .checksy.yaml ]]
[[ "$(<Brewfile)" == "NESTED_BREWFILE_SENTINEL" ]]
[[ "$(<template.txt)" == "NESTED_TEMPLATE_SENTINEL" ]]
