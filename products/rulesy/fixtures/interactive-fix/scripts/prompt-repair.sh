#!/usr/bin/env bash
set -eu

printf 'interactive stdin prompt: '
IFS= read -r stdin_answer
printf 'interactive tty prompt: ' >/dev/tty
IFS= read -r tty_answer </dev/tty

if [ "$stdin_answer" != "stdin-answer" ] || [ "$tty_answer" != "tty-answer" ]; then
  printf 'unexpected interactive answers: %s / %s\n' "$stdin_answer" "$tty_answer" >&2
  exit 24
fi

printf 'interactive repair complete\n'
: > .interactive-fixed
