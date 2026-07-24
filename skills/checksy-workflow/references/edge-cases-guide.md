# Edge Cases & Advanced Scenarios

Advanced checksy configurations and scenarios for complex use cases.

---

## Multiple Config Files

### Global + Local composition

Reference global configs in project configs:

```yaml
# ~/.config/checksy/global.yaml
rules:
  - name: Git configured
    check: git config --global user.email

# Project .checksy.yaml
rules:
  - remote: ~/.config/checksy/global.yaml
  - name: Project tests
    check: npm test
```

### Environment-specific configs

Use wrapper scripts with environment detection:

```bash
# Wrapper script with environment detection
if [ "$CI" = "true" ]; then
  checksy --config=.checksy.ci.yaml check
elif [ "$ENV" = "dev" ]; then
  checksy --config=.checksy.dev.yaml check
else
  checksy check
fi
```

---

## Performance Optimization

### Problem: 50+ rules taking too long

**Solutions:**

1. **Filter by severity in CI:**
   ```yaml
   checkSeverity: warn  # Skip debug/info rules in CI
   ```

2. **Parallelize within rules:**
   ```yaml
   rules:
     - name: Parallel checks
       check: |
         ./check-a.sh &
         ./check-b.sh &
         wait
   ```

3. **Split by domain:**
   ```yaml
   # security.yaml, tests.yaml, lint.yaml
   # Run specific group: checksy --config=security.yaml check
   ```

4. **Cache expensive operations:**
   ```yaml
   rules:
     - name: Dependency audit (cached)
       check: |
         if [ ! -f .audit-cache ] || [ package.json -nt .audit-cache ]; then
           npm audit > .audit-cache
         fi
         grep -q "0 vulnerabilities" .audit-cache
   ```

---

## Partial/Invalid Configs

### Handling syntax errors gracefully

Test configs without failing:

```bash
# Test a config without failing
checksy --config=./experimental.yaml check --no-fail 2>&1 | head -20

# Validate syntax only
checksy schema > /tmp/schema.json
# Use with online YAML validator
```

---

## Windows-Specific Considerations

### Path separators

Use forward slashes `/` in patterns, even on Windows:

- ✓ `"scripts/checks/*.sh"`
- ✗ `"scripts\\checks\\*.sh"`

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

### Line endings

Pattern-matched script files (`.sh`) need Unix line endings (LF):

```bash
# Convert with dos2unix
dos2unix scripts/checks/*.sh
```

Or set in `.gitattributes`:

```
*.sh text eol=lf
```

---

## Circular Reference Handling

Active include cycles are detected and rejected before any configured command
runs.

Example chain:

```yaml
# a.yaml references b.yaml
# b.yaml references c.yaml
# c.yaml references a.yaml ← Circular!
```

Checksy reports the ordered `a.yaml -> b.yaml -> c.yaml -> a.yaml` include chain.
Repeated includes encountered after a definition has completed are
deduplicated instead.

---

## Advanced Remote Config Patterns

### Multiple git remotes with different refs

```yaml
rules:
  - remote: git+https://github.com/org/security-checks.git#v1.0.0
  - remote: git+https://github.com/org/shared-checks.git#main:team/dev.yaml
  - remote: git+https://github.com/org/legacy-checks.git#legacy-branch
```

### Local override pattern

```yaml
# shared/team-checks.yaml (shared)
rules:
  - name: Shared rule
    check: echo "from shared"

# .checksy.yaml (local override)
rules:
  - remote: shared/team-checks.yaml
  - name: Local override
    check: echo "local takes precedence"
```
