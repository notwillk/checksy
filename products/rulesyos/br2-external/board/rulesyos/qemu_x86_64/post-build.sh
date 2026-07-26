#!/bin/sh
set -eu

install -d -m 0700 \
	"$TARGET_DIR/state" \
	"$TARGET_DIR/var/lib/rulesy"
