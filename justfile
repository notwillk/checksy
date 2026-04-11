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
    echo "Cross-compiling for $arch $os ($target)"
    cargo install cross --git https://github.com/cross-rs/cross
    mkdir -p dist
    cd src && cross build --release --target "$target"
    echo "Cross-compiled for $target. Copying binary to dist/checksy_${os}_${arch}"
    cp target/"$target"/release/checksy ../dist/checksy_${os}_${arch}
    echo "Built $target"

release version:
    ./scripts/release.sh {{version}}

test:
    cd src && cargo test