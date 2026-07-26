#!/usr/bin/env bash
set -euo pipefail

cmds=(cargo sha256sum tar)
missing=()
for cmd in "${cmds[@]}"; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
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

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
project_root="$(CDPATH= cd -- "$script_dir/.." && pwd)"
dist_dir="$project_root/dist"
binary_name="rulesy_${os}_${arch}"
archive_name="${binary_name}.tar.gz"
checksum_name="${binary_name}-checksum.txt"

cd "$project_root"
mkdir -p "$dist_dir"

if [ "$os" = "darwin" ]; then
  if ! command -v rustup >/dev/null 2>&1; then
    echo "Missing required command for macOS builds: rustup" >&2
    exit 1
  fi
  echo "Building for macOS natively..."
  rustup target add "$target"
  cargo build --manifest-path Cargo.toml --locked --release --target "$target"
  cp "target/$target/release/rulesy" "$dist_dir/$binary_name"
else
  echo "Cross-compiling via Docker..."
  installed_cross_version=$(cross --version 2>/dev/null | sed -n '1p' || true)
  if [ "$installed_cross_version" != "cross 0.2.5" ]; then
    cargo install cross --version 0.2.5 --locked --force
  fi
  cross build --manifest-path Cargo.toml --locked --release --target "$target"
  cp "target/$target/release/rulesy" "$dist_dir/$binary_name"
fi

echo "Packaging: $archive_name"
tmp_dir="$dist_dir/tmp"
mkdir -p "$tmp_dir"
cp "$dist_dir/$binary_name" "$tmp_dir/rulesy"
tar -czf "$dist_dir/$archive_name" -C "$tmp_dir" rulesy
rm -rf "$tmp_dir"

echo "Calculating checksum: $checksum_name"
(cd "$dist_dir" && sha256sum "$archive_name") >"$dist_dir/$checksum_name"
echo "Done"
