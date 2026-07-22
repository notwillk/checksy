# Best Practices Guide

Guidelines for effective checksy configuration and usage.

---

## Naming Conventions

**Good:** Descriptive, specific names

```yaml
rules:
  - name: TypeScript compiles without errors
  - name: All tests pass
  - name: Docker image builds successfully
```

**Poor:** Vague, generic names

```yaml
rules:
  - name: Check 1
  - name: Test
```

---

## Rule Organization

Order by dependency and severity:

```yaml
# 1. Preconditions first (must pass)
preconditions:
  - name: Dependencies installed
  - name: Services running

# 2. Main rules by severity (errors first)
rules:
  # Critical checks
  - name: Security audit passes
    severity: error

  # Important checks
  - name: Tests pass
    severity: error

  # Nice-to-have checks
  - name: Linting passes
    severity: warn

  # Debugging info
  - name: Environment info
    severity: debug
```

---

## Severity Guidelines

| Severity | When to Use |
|----------|-------------|
| **error** | Blocking issues that must be fixed (tests, security, compilation) |
| **warn** | Non-blocking issues that should be addressed (linting, formatting) |
| **info** | Informational checks that don't indicate problems (stats, versions) |
| **debug** | Verbose diagnostics useful for troubleshooting (env dumps, detailed logs) |

---

## Decision Guide

### Preconditions vs Regular Rules

| Use Preconditions | Use Regular Rules |
|-------------------|-------------------|
| Must pass before anything else runs | Can fail independently |
| Dependencies, services, environment | Business logic, tests, validation |
| Fail fast - stop if prerequisites missing | Continue and report all results |

---

### Inline vs File Remote vs Git Remote

| Approach | Best For | Trade-off |
|----------|----------|-----------|
| **Inline** | Project-specific checks | Simple, not reusable |
| **File remote** | Team/org shared configs | Reusable, local file access |
| **Git remote** | Cross-org, versioned configs | Network required, versioned |

---

### When to use `--fix`

| Scenario | Action |
|----------|--------|
| First setup (missing dependencies) | `checksy check --fix` |
| CI/CD (deterministic environment) | Don't use --fix |
| Developer workstations | `checksy check --fix` |
| Shared configs | Define fix scripts in rules |

---

## Config File Locations

Standard locations (checked in order):

1. Path specified by `--config`
2. `.checksy.yaml` in current directory
3. `.checksy.yml` in current directory

**Recommended project structure:**

```
project/
├── .checksy.yaml          # Root config
├── .checksy.schema.json   # Generated schema (not committed)
├── scripts/
│   └── checks/           # Pattern-based rules
│       ├── check-deps.sh
│       └── check-env.sh
└── shared/
    └── team-checks.yaml   # Remote config
```

---

## Git Ignore

```gitignore
# .gitignore
# Don't commit generated schema (regenerate in CI)
.checksy.schema.json

# Cache directory (will be regenerated)
.checksy-cache/
```

---

## Schema Generation Workflow

Generate schema for IDE support but don't commit it:

```bash
# .gitignore
.checksy.schema.json

# Generate locally for IDE
just generate-schema  # Or: checksy schema > .checksy.schema.json

# VS Code settings
# .vscode/settings.json
{
  "yaml.schemas": {
    "./.checksy.schema.json": ".checksy.yaml"
  }
}
```

---

## Fix Script Best Practices

Use `fix` for commands that can run with `/dev/null` as stdin. Use
`interactive-fix` only when the repair genuinely requires a terminal, such as
opening an editor. A rule may not define both:

```yaml
rules:
  - name: Local environment configured
    check: test -f .env.local
    interactive-fix: '${EDITOR:-vi} .env.local'
```

Run automation as `checksy check --fix --non-interactive`. Ordinary fixes still
run, while a needed interactive repair remains a severity-governed failure
instead of prompting. Configuration supplied through stdin is always
non-interactive.

### Idempotent fixes

Fixes should be safe to run multiple times:

```yaml
# Good: Idempotent
rules:
  - name: Dependencies installed
    check: test -d node_modules
    fix: npm ci

# Good: Checks before acting
rules:
  - name: Config file exists
    check: test -f .env.local
    fix: |
      if [ ! -f .env.local ]; then
        cp .env.template .env.local
      fi
```

---

### Informative output

Fix scripts should explain what they're doing:

```yaml
rules:
  - name: Database migrated
    severity: error
    check: ./scripts/check-migrations.sh
    fix: |
      echo "Running database migrations..."
      npm run db:migrate
      echo "Migrations complete!"
```

---

### Multi-step fixes

For complex fixes, break into clear steps:

```yaml
rules:
  - name: Development environment ready
    severity: error
    check: test -f .env && test -d node_modules
    fix: |
      echo "Step 1: Installing dependencies..."
      npm ci
      echo "Step 2: Setting up environment..."
      cp .env.template .env
      echo "Step 3: Please edit .env with your values"
```

---

## Severity Configuration Best Practices

### Top-level defaults

Set sensible defaults for your project:

```yaml
# Development: Run everything
checkSeverity: debug
failSeverity: error

# CI/CD: Skip verbose checks
checkSeverity: warn
failSeverity: error
```

### Rule-level overrides

Override for critical checks:

```yaml
checkSeverity: warn  # Default

rules:
  - name: Security audit
    severity: error    # Always run, always fail
    check: npm audit

  - name: Optional linting
    severity: debug   # Only when explicitly requested
    check: npm run lint:strict
```

---

## Pattern Matching Best Practices

### Specific patterns

Be specific to avoid matching unintended files:

```yaml
# Good: Specific directory
patterns:
  - "scripts/health-checks/*.sh"

# Risky: Too broad
patterns:
  - "**/*.sh"  # Could match node_modules, .git, etc.
```

### Negation patterns

Use exclusions to filter:

```yaml
patterns:
  - "tests/**/*.test.sh"    # Include all test scripts
  - "!tests/**/slow.test.sh"  # But skip slow ones
  - "!tests/**/wip-*.sh"     # And work-in-progress
```

---

## Remote Config Best Practices

### Version pinning

Pin to specific versions for stability:

```yaml
rules:
  # Good: Pinned to tag
  - remote: git+https://github.com/org/shared-checks.git#v1.2.0

  # Risky: Floating on main
  - remote: git+https://github.com/org/shared-checks.git#main
```

### Documentation

Document your remote configs:

```yaml
# From github.com/org/shared-checks
# v1.2.0 - Security and compliance checks
rules:
  - remote: git+https://github.com/org/shared-checks.git#v1.2.0
```

---

## Performance Optimization

### Severity filtering in CI

Skip non-essential checks in CI:

```yaml
# CI config
checkSeverity: warn  # Skip debug/info

rules:
  - name: Fast smoke test
    severity: warn
    check: npm run test:smoke

  - name: Verbose diagnostics
    severity: debug  # Won't run in CI
    check: ./scripts/diagnostics.sh
```

### Rule ordering

Order by execution time (fastest first):

```yaml
rules:
  - name: Quick syntax check    # Fast
    check: npm run lint:syntax

  - name: Unit tests           # Medium
    check: npm test

  - name: Integration tests    # Slow
    check: npm run test:integration
```
