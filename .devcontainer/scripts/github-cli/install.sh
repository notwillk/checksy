#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../shared/lib.sh
source "$SCRIPT_DIR/../shared/lib.sh"

for required_command in sudo apt-get curl sha256sum dpkg install; do
  if ! command -v "$required_command" >/dev/null; then
    provision_error "GitHub CLI installation requires $required_command"
    exit 1
  fi
done
if ! sudo -n true; then
  provision_error "passwordless sudo is required for non-interactive provisioning"
  exit 1
fi

temporary_directory=$(mktemp -d)
trap 'rm -rf -- "$temporary_directory"' EXIT

keyring="$temporary_directory/githubcli-archive-keyring.gpg"
source_list="$temporary_directory/github-cli.list"
download_file \
  "https://cli.github.com/packages/githubcli-archive-keyring.gpg" \
  "$keyring"
verify_sha256 \
  "$keyring" \
  "6084d5d7bd8e288441e0e94fc6275570895da18e6751f70f057485dc2d1a811b"

architecture=$(dpkg --print-architecture)
printf \
  'deb [arch=%s signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main\n' \
  "$architecture" >"$source_list"

sudo -n install -d -m 0755 /etc/apt/keyrings /etc/apt/sources.list.d
sudo -n install -m 0644 "$keyring" /etc/apt/keyrings/githubcli-archive-keyring.gpg
sudo -n install -m 0644 "$source_list" /etc/apt/sources.list.d/github-cli.list
sudo -n env DEBIAN_FRONTEND=noninteractive apt-get update
sudo -n env DEBIAN_FRONTEND=noninteractive \
  apt-get install -y --no-install-recommends gh
