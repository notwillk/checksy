#!/usr/bin/env bash
set -eu

printf 'interactive repair stdout before supervision event\n'
printf 'interactive repair stderr before supervision event\n' >&2
: > "$RULESY_INTERACTIVE_FIX_STARTED"
trap '' INT TERM HUP QUIT
export RULESY_HELPER_ROLE=leader
exec "$RULESY_PROCESS_HELPER" --ignored --exact interactive_process_tree_helper --nocapture
