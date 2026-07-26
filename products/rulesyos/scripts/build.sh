#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

readonly EXTERNAL_DIR="$PROJECT_DIR/br2-external"
readonly OUTPUT_DIR="$PROJECT_DIR/output"
readonly DOWNLOAD_DIR="$PROJECT_DIR/.cache/buildroot-downloads"

buildroot_dir=$("$SCRIPT_DIR/prepare-buildroot.sh")
build_jobs=${RULESYOS_BUILD_JOBS:-$(nproc)}

mkdir -p "$DOWNLOAD_DIR"
make -C "$buildroot_dir" \
  O="$OUTPUT_DIR" \
  BR2_EXTERNAL="$EXTERNAL_DIR" \
  BR2_DL_DIR="$DOWNLOAD_DIR" \
  rulesyos_qemu_x86_64_defconfig
make -C "$buildroot_dir" \
  O="$OUTPUT_DIR" \
  BR2_EXTERNAL="$EXTERNAL_DIR" \
  BR2_DL_DIR="$DOWNLOAD_DIR" \
  -j"$build_jobs"

test -f "$OUTPUT_DIR/images/bzImage"
test -f "$OUTPUT_DIR/images/rootfs.ext2"
