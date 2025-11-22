#!/usr/bin/env bash
set -euo pipefail

# Install into a shared bin dir so the non-root devcontainer user sees it.
GOBIN=/usr/local/bin go install github.com/goreleaser/goreleaser/v2@latest
