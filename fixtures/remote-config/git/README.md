# Git-based Remote Config Fixture

This fixture tests the git-based remote config feature with a real repository.

## Config Details

**File:** `.checksy.yaml`

```yaml
cachePath: ".git-cache"
rules:
  - remote: git+git@github.com:notwillk/checksy.git#main:fixtures/happy-path/.checksy.yaml
  - name: Local verification
    check: echo "Git remote test completed"
    severity: info
```

This config:
1. Uses a custom `cachePath` (`.git-cache` instead of default `.checksy-cache`)
2. References the checksy repository itself via SSH URL format
3. Points to the `fixtures/happy-path/.checksy.yaml` config within that repo
4. Adds a local verification rule after the remote rules

## Usage

### First Time - Cache the Git Remote

```bash
cd fixtures/remote-config/git
/workspaces/checksy/src/target/release/checksy install
```

Expected output:
```
📦 Caching 1 git remote(s)...
  [1/1] git@github.com:notwillk/checksy.git#main ✓
✅ All remotes cached
```

Cache location:
```
.git-cache/
└── git/
    └── git@github.com_notwillk_checksy.git/
        └── main/          # shallow clone of checksy repo
            └── fixtures/
                └── happy-path/
                    └── .checksy.yaml
```

### Run the Checks

```bash
cd fixtures/remote-config/git
/workspaces/checksy/src/target/release/checksy check
```

This will:
1. Load the remote config from the cached checksy repo
2. Run the `happy-path` rules (which include various severity levels)
3. Run the local verification rule

### Clean Up (Optional)

Remove the cache:
```bash
rm -rf fixtures/remote-config/git/.git-cache
```

## Notes

- This fixture requires network access for the initial `install`
- Uses SSH URL format (`git@github.com:...`) which may require SSH key authentication
- The cache uses URL-safe encoding: `git@github.com:` becomes `git@github.com_`

## Expected Behavior

When running `checksy check`, you may see failures from the remote rules because the happy-path fixture references scripts (like `./pass.sh`) with relative paths. When the config is loaded from the cache, these scripts are not in the current working directory. This demonstrates that:
1. ✅ The git remote config is being loaded from cache
2. ✅ The rules are being executed
3. ⚠️ Remote configs with relative script paths may need adjustment for cached usage

The local verification rule at the end should always pass.
