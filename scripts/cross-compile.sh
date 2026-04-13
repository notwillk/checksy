#!/usr/bin/env bash
set -euo pipefail

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
cd .. && tar -czf dist/checksy_${os}_${arch}.tar.gz --transform="s|^checksy_${os}_${arch}$|checksy|" -C dist checksy_${os}_${arch}
echo "Calculating checksum: checksy_${os}_${arch}-checksum.txt"
(cd dist && sha256sum checksy_${os}_${arch}.tar.gz) > dist/checksy_${os}_${arch}-checksum.txt
echo "Done"