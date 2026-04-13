#!/usr/bin/env bash
set -euo pipefail

cmds=(cargo gpg sha256sum)
missing=()
for cmd in "${cmds[@]}"; do
  if ! which "$cmd" >/dev/null 2>&1; then
    missing+=("$cmd")
  fi
done
if [ ${#missing[@]} -eq 0 ]; then
  echo "All required commands available"
else
  echo "Missing required commands: ${missing[*]}" >&2
  exit 1
fi

if [ $# -ne 1 ]; then
  echo "Usage: $0 <target>" >&2
  exit 1
fi

target="$1"
os=$(echo "$target" | cut -d'-' -f3)
arch=$(echo "$target" | cut -d'-' -f1)

echo "Target: $target"
echo "Architecture: $arch"
echo "OS: $os"

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo_root"
mkdir -p dist

if [ "$os" = "darwin" ]; then
  echo "Building for macOS natively..."
  rustup target add "$target"
  cd src && cargo build --release --target "$target"
  echo "Cross-compiling: checksy_${os}_${arch}"
  cp target/"$target"/release/checksy ../dist/checksy_${os}_${arch}
else
  echo "Cross-compiling via Docker..."
  cargo install cross --git https://github.com/cross-rs/cross
  cd src && cross build --release --target "$target"
  echo "Cross-compiling: checksy_${os}_${arch}"
  cp target/"$target"/release/checksy ../dist/checksy_${os}_${arch}
fi

echo "Packaging: checksy_${os}_${arch}.tar.gz"
cp target/"$target"/release/checksy ../dist/checksy
cd .. && tar -czf dist/checksy -C dist checksy_${os}_${arch}
echo "Calculating checksum: checksy_${os}_${arch}-checksum.txt"
(cd dist && sha256sum checksy_${os}_${arch}.tar.gz) > dist/checksy_${os}_${arch}-checksum.txt
echo "Done"