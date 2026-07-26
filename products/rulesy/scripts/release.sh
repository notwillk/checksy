#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 [patch|minor|major]" >&2
  exit 1
}

if [ $# -ne 1 ]; then
  usage
fi

bump="$1"
case "$bump" in
  patch|minor|major) ;;
  *) usage ;;
esac

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
project_root="$(CDPATH= cd -- "$script_dir/.." && pwd)"
workspace_root="$(CDPATH= cd -- "$project_root/../.." && pwd)"
cd "$workspace_root"

current_branch=$(git rev-parse --abbrev-ref HEAD)
if [ "$current_branch" != "main" ]; then
  echo "Releases must be cut from main (current: $current_branch)" >&2
  exit 1
fi

git fetch origin main

local_head=$(git rev-parse HEAD)
remote_head=$(git rev-parse origin/main)
if [ "$local_head" != "$remote_head" ]; then
  echo "Local main is not up to date with origin/main" >&2
  exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "Working tree is dirty; commit or stash changes first" >&2
  exit 1
fi

manifest_file="products/rulesy/Cargo.toml"
lock_file="products/rulesy/Cargo.lock"
current_version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$manifest_file")
if [ -z "$current_version" ]; then
  echo "Unable to read current version from $manifest_file" >&2
  exit 1
fi

IFS='.' read -r major minor patch <<< "$current_version"
case "$bump" in
  major)
    major=$((major + 1))
    minor=0
    patch=0
    ;;
  minor)
    minor=$((minor + 1))
    patch=0
    ;;
  patch)
    patch=$((patch + 1))
    ;;
 esac

new_version="$major.$minor.$patch"

tmp_file=$(mktemp)
awk -v new_version="$new_version" '
  !updated && /^version = "[^"]+"$/ {
    print "version = \"" new_version "\""
    updated = 1
    next
  }
  { print }
  END {
    if (!updated) {
      exit 1
    }
  }
' "$manifest_file" > "$tmp_file"
mv "$tmp_file" "$manifest_file"

cargo update \
  --manifest-path "$manifest_file" \
  --package rulesy \
  --precise "$new_version"

git add -- "$manifest_file" "$lock_file"
git commit -m "Release v$new_version"

tag="v$new_version"
git tag -a "$tag" -m "Rulesy $tag"

git push origin main
git push origin "$tag"

echo "Released $tag"
