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
    target="{{target}}"
    os=$(echo "$target" | cut -d'-' -f3)
    arch=$(echo "$target" | cut -d'-' -f1)
    echo "Target: $target"
    echo "Architecture: $arch"
    echo "OS: $os"
    mkdir -p dist

    if [ "$os" = "darwin" ]; then
        echo "Building for macOS natively..."
        rustup target add "$target"
        cargo build --manifest-path src/Cargo.toml --release --target "$target"
        mv "src/target/$target/release/checksy" "dist/checksy_${os}_${arch}"
    else
        echo "Cross-compiling via cross..."
        cargo install cross --git https://github.com/cross-rs/cross
        cross build --manifest-path src/Cargo.toml --release --target "$target"
        mv "src/target/$target/release/checksy" "dist/checksy_${os}_${arch}"
    fi

    echo "Packaging: checksy_${os}_${arch}.tar.gz"
    mkdir -p dist/tmp
    cp "dist/checksy_${os}_${arch}" "dist/tmp/checksy"
    tar -czf "dist/checksy_${os}_${arch}.tar.gz" -C dist/tmp checksy
    rm -rf dist/tmp

    echo "Calculating checksum: checksy_${os}_${arch}-checksum.txt"
    (cd dist && sha256sum checksy_${os}_${arch}.tar.gz) > "dist/checksy_${os}_${arch}-checksum.txt"

    echo "Done"

sign file key:
    gpg --batch --import "{{key}}"
    gpg --batch --yes --output "{{file}}.sig" --detach-sign "{{file}}"
    echo "Created {{file}}.sig"

release version:
    ./scripts/release.sh {{version}}

can-build:
    #!/usr/bin/env bash
    set -euo pipefail
    cmds=(cargo gpg just rustup sha256sum)
    missing=()
    for cmd in "${cmds[@]}"; do
      if ! which "$cmd" >/dev/null 2>&1; then
        missing+=("$cmd")
      fi
    done
    if [ ${#missing[@]} -eq 0 ]; then
      echo "All required commands available"
    else
      echo "Missing required commands: ${missing[*]}"
      exit 1
    fi

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
