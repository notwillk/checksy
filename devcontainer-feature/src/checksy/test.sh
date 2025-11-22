#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP_PROJECT="$(mktemp -d)"
trap 'rm -rf "$TMP_PROJECT"' EXIT

cp -R "$PROJECT_ROOT/src" "$TMP_PROJECT/src"
mkdir -p "$TMP_PROJECT/test/checksy"

cat >"$TMP_PROJECT/test/checksy/test.sh" <<'EOS'
#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=/dev/null
source dev-container-features-test-lib

check "checksy available" checksy --version

reportResults
EOS
chmod +x "$TMP_PROJECT/test/checksy/test.sh"

devcontainer features test \
  --project-folder "$TMP_PROJECT" \
  --features checksy \
  --base-image mcr.microsoft.com/devcontainers/base:ubuntu
