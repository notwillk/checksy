#!/bin/sh
set -eu

board_dir="$BR2_EXTERNAL_RULESYOS_PATH/board/rulesyos/qemu_x86_64"
state_image="$BINARIES_DIR/state.ext4"

rm -f -- "$state_image"
truncate -s 64M "$state_image"
"$HOST_DIR/sbin/mkfs.ext4" \
	-q \
	-F \
	-L RULESYOS_STATE \
	-U 22222222-2222-4222-8222-222222222222 \
	"$state_image"

support/scripts/genimage.sh -c "$board_dir/genimage.cfg"

test -f "$BINARIES_DIR/rulesyos.img"
