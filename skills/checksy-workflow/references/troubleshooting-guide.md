# Troubleshooting Guide

Detailed troubleshooting for checksy errors and issues.

---

## Config Validation Errors

### Error: `failed to load config: decode config: ...`

**Problem:** Unknown severity value

```yaml
# Wrong
rules:
  - name: Test
    severity: warning  # Should be 'warn'
    check: echo hi
```

**Fix:** Use valid severities: `debug`, `info`, `warn`, `error`

---

### Error: `invalid remote rule ...: remote rule cannot have properties: check, name`

**Problem:** Remote rule with extra properties

```yaml
# Wrong
rules:
  - remote: shared.yaml
    name: This is wrong  # Not allowed
    check: echo hi       # Not allowed
```

**Fix:** Remote rules can ONLY have `remote`:

```yaml
rules:
  - remote: shared.yaml  # ✓ Correct
```

---

## Git Cache Issues

### Error: `git remote not cached: ...`

**Solution:**

```bash
# Cache the remote
checksy install

# Or use --fix to auto-cache
checksy check --fix
```

---

### Error: `Failed to cache ...`

**Check:**
- Git is installed: `git --version`
- Network access is available
- Repository URL is correct
- Ref (branch/tag) exists in the repo

**Debug commands:**

```bash
# Test git access manually
git ls-remote https://github.com/org/repo.git refs/heads/main

# Clear cache and retry
rm -rf .checksy-cache/git/
checksy install
```

---

## Rule Execution Failures

### Problem: Rule output not showing

Rules run with `bash -c`, so:
- Use full paths or ensure commands are in PATH
- Check working directory (rules run in config file's directory)

**Problem example:**

```yaml
rules:
  - name: Test
    check: ./script.sh  # Depends on where checksy is run from
```

**Fix:**

```yaml
rules:
  - name: Test
    check: "${PWD}/script.sh"  # Or use a known path
```

---

### Problem: Exit codes not matching expectations

- Exit 0 = Success (✅)
- Exit non-0 = Failure (❌ or ⚠️ depending on severity)

**Test your check directly:**

```bash
bash -c "your check command here"
echo "Exit code: $?"
```

---

## Other Common Errors

### Error: `failed to load config: remote config '...' not found`

- **Cause:** Relative path incorrect or file doesn't exist
- **Fix:** Verify file exists relative to including config; use absolute paths if needed

---

### Error: `check failed: No such file or directory (os error 2)`

- **Cause:** Pattern matched files that were deleted, or working directory mismatch
- **Fix:** Check pattern results: `ls scripts/checks/*.sh`

---

### Error: Rules run but produce no output

- **Cause:** Commands redirecting stdout/stderr, or silent success
- **Fix:** Add explicit output or remove redirection: `check: "./script.sh && echo 'OK'"`

---

## Unknown Errors: Diagnostic Steps

If you encounter an error not listed:

1. **Verify config syntax:**
   ```bash
   checksy schema > /tmp/schema.json
   # Validate YAML online or with yamllint
   ```

2. **Run with minimal config:**
   ```bash
   echo "rules: []" | checksy --stdin-config check
   ```

3. **Check version compatibility:**
   ```bash
   checksy version
   # Compare with schema version
   ```

4. **Enable all output:**
   ```bash
   checksy check --check-severity debug --no-fail
   ```

---

## Severity Filtering Issues

### Problem: Rules not running

Check severity hierarchy:

```bash
# Default: runs debug+ rules
checksy check

# Only run warn+ rules
checksy check --check-severity warn

# Run debug+ but only fail on error
checksy check --check-severity debug --fail-severity error
```

---

## Pattern Matching Issues

### Problem: Pattern not finding files

**Wrong path separator:**

```yaml
patterns:
  - "scripts\\checks\\*.sh"  # ❌ Backslashes
```

**Fix:**

```yaml
patterns:
  - "scripts/checks/*.sh"     # ✓ Forward slashes
```

**Debug patterns:**

```bash
# List files that should match
ls -la scripts/checks/*.sh

# Check if pattern is valid glob
echo scripts/checks/*.sh
```

---

## Circular Reference Issues

### Error: Rules missing from remote configs

Circular references are automatically detected and skipped.

Example chain:

```yaml
# a.yaml references b.yaml
# b.yaml references c.yaml
# c.yaml references a.yaml ← Circular!
```

Checksy will load: A rules → B rules → C rules (skips C's reference back to A)

---

## Windows-Specific Issues

### Path separators

Use forward slashes `/` in patterns, even on Windows:
- ✓ `"scripts/checks/*.sh"`
- ✗ `"scripts\\checks\\*.sh"`

---

### Shell execution

Rules run via `bash -c` on all platforms:
- Requires Git Bash, WSL, or Cygwin on Windows
- PowerShell commands need explicit wrapping

```yaml
# PowerShell command workaround
rules:
  - name: Windows service check
    check: "powershell.exe -Command 'Get-Service | Where-Object {$_.Status -eq Running}'"
```

---

### Line endings

Pattern-matched script files (`.sh`) need Unix line endings (LF):
- Convert with: `dos2unix scripts/checks/*.sh`
- Or set in `.gitattributes`:
  ```
  *.sh text eol=lf
  ```

---

## Exit Codes

| Exit Code | Meaning |
|-----------|---------|
| 0 | Success - all checks passed |
| 1 | Usage error (no command, help shown) |
| 2 | Runtime error (config not found, invalid flags) |
| 3 | Check failures (only if `--no-fail` not used) |
