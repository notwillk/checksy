# checksy Configuration Examples

Quick reference and comprehensive examples for checksy configurations.

---

## Quick Examples

### Minimal Config

```yaml
rules:
  - check: echo "Hello World"
```

### Basic Config with Preconditions

```yaml
preconditions:
  - name: Dependencies installed
    severity: error
    check: test -d node_modules
    fix: npm ci

rules:
  - name: Tests pass
    severity: error
    check: npm test
```

### Interactive Repair

Use a terminal-capable repair only when the command genuinely needs user input.
It is mutually exclusive with ordinary `fix` and is considered only after the
check fails during `check --fix`:

```yaml
rules:
  - name: Local environment configured
    severity: error
    check: test -f .env.local
    interactive-fix: '${EDITOR:-vi} .env.local'
    timeout: 30m
```

`checksy check --fix --non-interactive` still runs ordinary fixes but leaves a
needed interactive repair as a normal rule failure. Stdin configurations are
always non-interactive.

### Config with All Features

```yaml
# Settings
cachePath: .checksy-cache
checkSeverity: warn
failSeverity: error

# Preconditions
preconditions:
  - name: Docker running
    severity: error
    check: docker info >/dev/null 2>&1
    fix: open -a Docker

  - name: Dependencies installed
    severity: error
    check: test -d node_modules
    fix: npm ci

# Rules
rules:
  - name: TypeScript compiles
    severity: error
    check: npx tsc --noEmit
    fix: npm run typecheck
    hint: Run 'npm run typecheck' for details

  - name: Linting passes
    severity: warn
    check: npm run lint
    fix: npm run lint:fix

  - name: Local environment configured
    severity: error
    check: test -f .env.local
    interactive-fix: '${EDITOR:-vi} .env.local'
    timeout: 30m

  # Remote config
  - remote: shared/team-checks.yaml

  # Git-based remote
  - remote: git+https://github.com/org/shared-checks.git#main

# Patterns
patterns:
  - "scripts/check-*.sh"
  - "tests/health/*.bats"
  - "!**/*-skip.sh"
```

---

## Complete Production Example

This example shows a full `.checksy.yaml` for a Node.js project with:
- Preconditions (dependency checks)
- Multiple rule types (inline, remote, patterns)
- Severity levels
- Auto-fixes
- Interactive terminal repair where user input is required
- Git-based remote configs

```yaml
# .checksy.yaml - Production-ready configuration
# checksy version: >=0.7.0

# ============================================
# Optional Settings
# ============================================

# Cache location for git-based remote configs
cachePath: .checksy-cache

# Default severities
checkSeverity: warn      # Run warn+ rules (default: debug)
failSeverity: error      # Only errors cause failure (default: error)

# ============================================
# Preconditions
# Must all pass before rules are executed
# ============================================

preconditions:
  - name: Environment file exists
    severity: error
    check: test -f .env
    fix: cp .env.template .env
    hint: Copy .env.template to .env and fill in values

  - name: Dependencies installed
    severity: error
    check: test -d node_modules
    fix: npm ci

  - name: Required tools available
    severity: error
    check: which node && which npm

# ============================================
# Main Rules
# ============================================

rules:
  # ----------------------------------------
  # Code Quality (error severity = blocking)
  # ----------------------------------------
  
  - name: TypeScript compiles
    severity: error
    check: npx tsc --noEmit
    fix: npm run typecheck
    hint: Run 'npm run typecheck' to see detailed errors

  - name: No TypeScript errors in strict mode
    severity: error
    check: npx tsc --strict --noEmit

  # ----------------------------------------
  # Testing (error severity = blocking)
  # ----------------------------------------
  
  - name: Unit tests pass
    severity: error
    check: npm test -- --watchAll=false
    hint: Run 'npm test' to see failures

  - name: Test coverage meets threshold
    severity: warn
    check: npm test -- --coverage --watchAll=false --coverageThreshold='{"global":{"branches":80}}'
    fix: npm test -- --coverage --watchAll=false --updateSnapshot

  # ----------------------------------------
  # Linting & Formatting (warn severity = non-blocking)
  # ----------------------------------------
  
  - name: ESLint passes
    severity: warn
    check: npm run lint
    fix: npm run lint:fix

  - name: Prettier formatting
    severity: warn
    check: npx prettier --check "src/**/*.{ts,tsx,js,json}"
    fix: npx prettier --write "src/**/*.{ts,tsx,js,json}"

  # ----------------------------------------
  # Security (error severity = blocking)
  # ----------------------------------------
  
  - name: No npm audit vulnerabilities
    severity: error
    check: npm audit --audit-level=moderate
    fix: npm audit fix
    hint: Run 'npm audit' for details and manual fixes

  - name: No secrets in code
    severity: error
    check: npx secretlint "**/*"

  # ----------------------------------------
  # Documentation (info severity = FYI)
  # ----------------------------------------
  
  - name: README is up to date
    severity: info
    check: test README.md -nt package.json
    hint: Update README.md if package.json changed

  - name: API documentation generated
    severity: info
    check: test -f docs/api.md && test docs/api.md -nt src/

  # ----------------------------------------
  # Remote Configs
#   Include shared team/org configs
  # ----------------------------------------
  
  # Local file remote
  - remote: ./shared/team-checks.yaml

  # Git-based remote (requires 'checksy install' first)
  - remote: git+https://github.com/org/shared-checks.git#main

  # Git remote with custom path
  - remote: git+https://github.com/org/security-checks.git#v1.2.0:security.yaml

# ============================================
# Pattern-Based Rules
# Glob-matched script files run as rules
# ============================================

patterns:
  # Include all check scripts
  - "scripts/validate-*.sh"
  - "scripts/check-*.py"
  - "tests/integration/*.test.sh"

  # Exclude specific patterns
  - "!scripts/validate-skip.sh"           # Skip this one
  - "!**/*-wip.sh"                        # Skip work-in-progress
  - "!**/node_modules/**"                 # Never match node_modules

# ============================================
# Notes
# ============================================

# Rules execute in this order:
# 1. All preconditions (must pass)
# 2. Inline rules in config order
# 3. Pattern-based rules (alphabetically by path)

# Severity override hierarchy:
# CLI flags (--cs/--fs) > Rule-level > Config defaults

# Fix mode (--fix) workflow:
# 1. Run check
# 2. If fails and fix exists, run fix
# 3. If fix succeeds, re-run check
# 4. Report final result
```

## Key Features Demonstrated

| Feature | Example Location |
|---------|------------------|
| Preconditions | Lines 15-28 |
| Severity levels | Lines 35, 43, 51, 59, 67, 75, 83 |
| Auto-fix scripts | Lines 18, 24, 37, 45, 55, 63, 71 |
| Hints | Lines 19, 40, 49, 69 |
| File remotes | Line 93 |
| Git remotes | Lines 96-98 |
| Pattern matching | Lines 106-114 |
| Pattern negation | Lines 111-114 |

## Usage

```bash
# First time setup
checksy install              # Cache git remotes
checksy check --fix          # Run with auto-fix

# Daily development
checksy check                # Quick validation
checksy check --cs error     # Only blocking checks

# CI/CD pipeline
checksy check --cs warn --fs error  # Run warn+, fail on error
```
