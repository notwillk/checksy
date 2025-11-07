build:
	@mkdir -p dist
	@cd src && go build -o ../dist/workspace-doctor ./cmd/workspace-doctor
