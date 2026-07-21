#!/usr/bin/env bash
set -euo pipefail

[[ -f .checksy.yaml ]]
[[ "$(<Brewfile)" == "ROOT_BREWFILE_SENTINEL" ]]
[[ "$(<template.txt)" == "ROOT_TEMPLATE_SENTINEL" ]]
