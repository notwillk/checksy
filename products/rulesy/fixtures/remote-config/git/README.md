# Git-based Remote Config Fixture

This fixture tests the git-based remote config feature with a real repository.

## Config Details

**File:** `.rulesy.yaml`

```yaml
cachePath: ".git-cache"
rules:
  - remote: git+git@github.com:notwillk/rulesy.git#main:products/rulesy/fixtures/happy-path/.rulesy.yaml
  - name: Local verification
    check: echo "Git remote test completed"
    severity: info
```

This config:
1. Uses a custom `cachePath` (`.git-cache` instead of default `.rulesy-cache`)
2. References the Rulesy repository itself via SSH URL format
3. Points to the `products/rulesy/fixtures/happy-path/.rulesy.yaml` config within that repo
4. Adds a local verification rule after the remote rules

## Usage

### First Time - Cache the Git Remote

From the repository root, build the local Rulesy binary and enter this fixture:

```bash
cargo build --release --manifest-path products/rulesy/Cargo.toml
cd products/rulesy/fixtures/remote-config/git
../../../target/release/rulesy install
```

Expected output:
```
📦 Caching 1 git remote(s)...
  [1/1] git@github.com:notwillk/rulesy.git#main ✓
✅ All remotes cached
```

Cache location:
```
.git-cache/
└── git/
    └── git@github.com_notwillk_rulesy.git/
        └── main/          # shallow clone of Rulesy repo
            └── products/
                └── rulesy/
                    └── fixtures/
                        └── happy-path/
                            └── .rulesy.yaml
```

### Run the Checks

```bash
cd products/rulesy/fixtures/remote-config/git
../../../target/release/rulesy check
```

This will:
1. Load the remote config from the cached Rulesy repo
2. Run the `happy-path` rules (which include various severity levels)
3. Run the local verification rule

### Clean Up (Optional)

Remove the cache:
```bash
rm -rf .git-cache
```

## Notes

- This fixture requires network access for the initial `install`
- Uses SSH URL format (`git@github.com:...`) which may require SSH key authentication
- The cache uses URL-safe encoding: `git@github.com:` becomes `git@github.com_`

## Expected Behavior

When running `rulesy check`, cached rules retain the directory of their
defining configuration, so relative scripts such as `./pass.sh` resolve inside
the cached checkout. This demonstrates that:
1. ✅ The git remote config is being loaded from cache
2. ✅ The rules are being executed from their defining working directory
3. ✅ Relative pattern and asset references remain local to that definition

The local verification rule at the end should always pass.
