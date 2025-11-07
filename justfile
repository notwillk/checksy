build:
  @mkdir -p dist
  @cd src && go build -o ../dist/workspace-doctor ./cmd/workspace-doctor

dev:
  @echo "Watching src for changes. Press Ctrl+C to stop."
  @find src -type f | entr -r sh -c 'just build && echo "Rebuilt command"'
