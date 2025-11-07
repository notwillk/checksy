#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_ALIASES_FILE="${SCRIPT_DIR}/aliases.yaml"

usage() {
  cat <<'USAGE'
Generate shell aliases from an aliases.yaml file.

Usage:
  aliases.sh [--file path]
  aliases.sh [path]
  aliases.sh --help

Options:
  -f, --file  Path to aliases YAML file (defaults to aliases.yaml next to this script)
  -h, --help  Show this message
USAGE
}

die() {
  echo "aliases.sh: $*" >&2
  exit 1
}

ensure_dependency() {
  command -v "$1" >/dev/null 2>&1 || die "missing required dependency: $1"
}

escape_single_quotes() {
  local value="$1"
  printf "%s" "${value//\'/\'\\\'\'}"
}

emit_alias() {
  local name="$1"
  local command="$2"
  local escaped
  escaped="$(escape_single_quotes "$command")"
  printf "alias %s='%s'\n" "$name" "$escaped"
}

alias_stream() {
  local file="$1"
  yq -r 'to_entries[] | [.key, (.value|tostring)] | @tsv' "$file"
}

parse_args() {
  ALIASES_FILE="$DEFAULT_ALIASES_FILE"
  while (($#)); do
    case "$1" in
      -h|--help)
        usage
        exit 0
        ;;
      -f|--file)
        shift || die "missing argument for $1"
        ALIASES_FILE="$1"
        ;;
      --)
        shift
        break
        ;;
      -*)
        die "unknown option: $1"
        ;;
      *)
        ALIASES_FILE="$1"
        shift
        break
        ;;
    esac
    shift
  done

  if (($#)); then
    die "unexpected arguments: $*"
  fi
}

main() {
  parse_args "$@"
  ensure_dependency yq

  [[ -f "$ALIASES_FILE" ]] || die "aliases file not found: $ALIASES_FILE"

  alias_stream "$ALIASES_FILE" | while IFS=$'\t' read -r name command; do
    emit_alias "$name" "$command"
  done
}

main "$@"
