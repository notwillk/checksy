build:
  @PATH="$(go env GOPATH)/bin:$PATH" goreleaser build --clean --snapshot --skip=validate

compile:
  @mkdir -p dist
  @cd src && go build -o ../dist/checksy ./cmd/checksy

dev:
  @echo "Watching src for changes. Press Ctrl+C to stop."
  @find src -type f | entr -r sh -c 'just compile && echo "Rebuilt command"'

release:
  @PATH="$(go env GOPATH)/bin:$PATH" goreleaser release --clean
