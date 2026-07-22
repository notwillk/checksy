#!/usr/bin/env bash
if IFS= read -r unexpected; then
  printf 'unexpected pattern stdin: %s\n' "$unexpected" >&2
  exit 93
fi
: > .pattern-eof
