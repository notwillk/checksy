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

cat >"$TMP_PROJECT/test/checksy/scenarios.json" <<'EOS'
{
  "bare-version": {
    "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
    "features": {
      "checksy": {
        "version": "0.7.5"
      }
    }
  }
}
EOS

cat >"$TMP_PROJECT/test/checksy/bare-version.sh" <<'EOS'
#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=/dev/null
source dev-container-features-test-lib

check "bare version installs exact release" \
  bash -c 'test "$(checksy --version)" = "checksy 0.7.5"'

reportResults
EOS
chmod +x "$TMP_PROJECT/test/checksy/bare-version.sh"

devcontainer features test \
  --project-folder "$TMP_PROJECT" \
  --features checksy \
  --base-image mcr.microsoft.com/devcontainers/base:ubuntu
