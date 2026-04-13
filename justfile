build:
    cd src && cargo build --release

compile:
    mkdir -p dist
    cd src && cargo build --release --bin checksy
    cp src/target/release/checksy dist/checksy
    echo "Rebuilt"

dev:
    echo "Watching src for changes. Press Ctrl+C to stop."
    find src -type f \( -name "*.rs" -o -name "Cargo.toml" \) | entr -r sh -c 'just compile'

cross-compile target:
    #!/bin/bash
    set -e
    ./scripts/cross-compile.sh "{{target}}"

release version:
    ./scripts/release.sh {{version}}

test:
    cd src && cargo test

get-version:
    #!/bin/bash
    VERSION="$(grep -Eo 'VERSION: &str = "[^"]+"' src/version.rs | sed -E 's/VERSION: &str = "([^"]+)"/\1/')"
    if [ -z "$VERSION" ]; then
        echo "Unable to determine version from version.rs" >&2
        exit 1
    fi
    echo "$VERSION"

ensure-tag-matches-version tag:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION="v$(just get-version)"
    TAG="{{tag}}"
    if [ "$TAG" != "$VERSION" ]; then
        echo "Tag '$TAG' does not match version '$VERSION'" >&2
        exit 1
    fi
