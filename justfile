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

release version:
    ./scripts/release.sh {{version}}

test:
    cd src && cargo test