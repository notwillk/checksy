# checksy Command Cheatsheet

Quick reference for common checksy commands and configuration patterns.

## Essential Commands

| Command | Description |
|---------|-------------|
| `checksy check` | Run all checks with default config |
| `checksy check --fix` | Run checks with auto-fix for failures |
| `checksy check --fix --non-interactive` | Run ordinary fixes but prohibit terminal repairs |
| `checksy check --cs=warn --fs=error` | Run warn+ rules, fail only on errors |
| `checksy check --no-fail` | Run checks, always exit 0 |
| `checksy install` | Cache all git-based remote configs |
| `checksy install --prune` | Update caches and remove unused |
| `checksy init` | Create starter `.checksy.config.yaml` |
| `checksy schema > .checksy.schema.json` | Generate JSON schema for IDE |
| `checksy version` | Show installed version |
| `checksy help` | Show usage information |

## Global Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--config PATH` | | Specify config file path |
| `--stdin-config` | | Read config from stdin |

## Check Command Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--check-severity LEVEL` | `--cs` | Minimum severity to run (debug/info/warn/error) |
| `--fail-severity LEVEL` | `--fs` | Minimum severity to fail on |
| `--fix` | | Auto-apply fixes for failed checks |
| `--non-interactive` | | Prohibit `interactive-fix`; ordinary `fix` remains enabled |
| `--no-fail` | | Never exit with non-zero status |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success - all checks passed |
| 1 | Usage error (help shown, missing command) |
| 2 | Runtime error (config not found, invalid flags) |
| 3 | Check failures (without --no-fail) |

## Severity Levels

Hierarchy (lowest to highest): `debug` < `info` < `warn` < `error`

| Level | CLI Flag | Config Key | Use Case |
|-------|----------|------------|----------|
| debug | `--cs=debug` | `checkSeverity: debug` | Verbose diagnostics |
| info | `--cs=info` | `checkSeverity: info` | Informational checks |
| warn | `--cs=warn` | `checkSeverity: warn` | Non-blocking issues |
| error | `--cs=error` | `checkSeverity: error` | Blocking failures |

## Config File Structure

```yaml
# Optional settings
cachePath: .checksy-cache              # Custom cache location
checkSeverity: warn                    # Default: debug
failSeverity: error                    # Default: error

# Preconditions - must pass before rules
preconditions:
  - name: Dependencies installed
    severity: error
    check: test -d node_modules
    fix: npm ci

# Main validation rules
rules:
  - name: TypeScript compiles
    severity: error
    check: npx tsc --noEmit
    fix: npm run typecheck
    hint: Run 'npm run typecheck' for details

  - name: Local environment configured
    severity: error
    check: test -f .env.local
    interactive-fix: '${EDITOR:-vi} .env.local'

  # Remote config reference
  - remote: shared/team-checks.yaml

# Pattern-based rules (glob matching)
patterns:
  - "scripts/check-*.sh"               # Include
  - "!**/*-skip.sh"                   # Exclude
```

## Git Remote Format

```yaml
# Default ref (main) and path (.checksy.yaml)
remote: git+https://github.com/org/shared-checks.git

# Specific branch
remote: git+https://github.com/org/shared-checks.git#develop

# Specific tag
remote: git+https://github.com/org/shared-checks.git#v1.0.0

# Custom config path within repo
remote: git+https://github.com/org/shared-checks.git#main:configs/dev.yaml
```

## Common Patterns

**Check if file exists:**
```yaml
check: test -f file.txt
```

**Check command output:**
```yaml
check: grep -q "pattern" file.txt
```

**Multiple commands:**
```yaml
check: |
  echo "Step 1" &&
  echo "Step 2" &&
  ./final-check.sh
```

**Conditional fix:**
```yaml
check: test -d node_modules
fix: npm ci
```

**Terminal-capable fix:**
```yaml
check: test -f .env.local
interactive-fix: '${EDITOR:-vi} .env.local'
```

`fix` and `interactive-fix` are mutually exclusive. The latter is considered
only during `check --fix` after its check fails. It requires a file-backed run
with a usable terminal; `--non-interactive` and stdin configuration leave the
rule failed without running it.
