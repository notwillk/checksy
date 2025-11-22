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

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$repo_root"

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

version_file="src/internal/version/version.go"
current_version=$(grep -Eo 'Version = "[^"]+"' "$version_file" | sed -E 's/Version = "([^"]+)"/\1/')
if [ -z "$current_version" ]; then
  echo "Unable to read current version from $version_file" >&2
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
sed -E "s/(Version = \").*?(\")/\1$new_version\2/" "$version_file" > "$tmp_file"
mv "$tmp_file" "$version_file"

git add "$version_file"
git commit -m "Release v$new_version"

tag="v$new_version"
git tag -a "$tag" -m "checksy $tag"

git push origin main
git push origin "$tag"

echo "Released $tag"
