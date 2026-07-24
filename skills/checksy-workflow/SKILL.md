---
name: checksy-workflow
description: Configure, run, and debug checksy workspace health checks. Use when creating or editing .checksy.yaml files, troubleshooting rule failures, setting up git-based remote configs, choosing ordinary or interactive --fix repairs, integrating into CI/CD, or generating JSON schemas for IDE validation. Covers severity levels (debug/info/warn/error), preconditions, patterns, caching, terminal/headless execution, and common errors — even if the user just says "health checks", "workspace validation", or mentions YAML config issues without naming checksy explicitly. Do NOT use for infrastructure health checks (Kubernetes probes, Docker HEALTHCHECK, application monitoring); use only for checksy CLI configuration and workspace validation.
license: MIT
compatibility: Requires checksy binary in PATH. Git required for remote configs. Bash required for rule execution.
metadata:
  author: opencode
  version: "1.0.1"
  category: dev-tools
  checksy_version_compatibility: ">=0.7.0"
  last_updated: "2026-07-22"
  changelog:
    - version: "1.0.1"
      date: "2026-07-22"
      notes: "Document interactive-fix and explicit non-interactive operation"
    - version: "1.0.0"
      date: "2026-04-26"
      notes: "Initial release"
---

# checksy Workflow

This skill guides you through configuring, running, and debugging [checksy](https://github.com/notwillk/checksy) — a Rust-based CLI tool for running lightweight health checks in development workspaces.

## Table of Contents

1. [Quick Reference](#quick-reference)
2. [Installation & Setup](#installation--setup)
3. [Configuration](#configuration)
4. [Running checksy](#running-checksy)
5. [Remote Configs](#remote-configs)
6. [Fix Mode](#fix-mode)
7. [CI/CD Integration](#cicd-integration)
8. [Testing Configs](#testing-configs)
9. [Edge Cases](#edge-cases)
10. [Debugging](#debugging)
11. [Best Practices](#best-practices)
12. [Quick Command Reference](#quick-command-reference)
13. [Feedback & Issues](#feedback--issues)
14. [Resources](#resources)

---

## Quick Reference

### Essential Commands

```bash
checksy check                    # Run all checks
checksy check --fix             # Run with auto-fix
checksy check --fix --non-interactive  # Allow only headless repairs
checksy check --cs warn --fs error  # Filter by severity
checksy install                 # Cache git remotes
checksy schema > .checksy.schema.json  # Generate IDE schema
```

### Severity Levels

debug < info < warn < error

| Level | CLI Flag | Use For |
|-------|----------|---------|
| debug | `--cs debug` | Verbose diagnostics |
| info | `--cs info` | Informational checks |
| warn | `--cs warn` | Non-blocking issues |
| error | `--cs error` | Blocking failures |

---

## Installation & Setup

### Standard Installation

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/install.sh | bash
checksy version
```

### Devcontainer

```json
{
  "features": {
    "ghcr.io/notwillk/checksy-feature:latest": {}
  }
}
```

### Build from Source

```bash
git clone https://github.com/notwillk/checksy.git
cd checksy && just compile
```

---

## Configuration

### Basic Structure

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
  - remote: shared/team-checks.yaml

patterns:
  - "scripts/check-*.sh"
  - "!**/*-skip.sh"
```

### Rule Properties

| Property | Required | Description |
|----------|----------|-------------|
| `name` | No | Display name |
| `check` | Yes* | Shell command |
| `severity` | No | debug/info/warn/error |
| `fix` | No | Non-interactive repair command |
| `interactive-fix` | No | Terminal-capable repair command; mutually exclusive with `fix` |
| `timeout` | No | Per-command timeout (`1ms` through `2h`) |
| `remote` | Yes* | Config file path |

*Either `check` OR `remote`, not both.

### Severity Cascade

CLI flags → Rule-level → Top-level defaults:

```bash
checksy check --cs warn --fs error
```

**For detailed examples** including production configs and patterns, see [references/config-examples.md](references/config-examples.md).

---

## Running checksy

### The check Command

```bash
checksy check                          # Default config
checksy --config=./team.yaml check     # Specific config
cat config.yaml | checksy --stdin-config check  # From stdin
checksy check --fix                    # Auto-fix failures
checksy check --fix --non-interactive  # Ordinary fixes only; prohibit terminal repairs
checksy check --no-fail                # Never exit with failure
```

### The install Command

Cache git remotes before using:

```bash
checksy install           # Cache remotes
checksy install --prune   # Update and clean
```

### The init Command

Create starter config:

```bash
checksy init  # Creates .checksy.config.yaml
```

### The schema Command

Generate JSON Schema for IDE:

```bash
checksy schema > .checksy.schema.json
```

---

## Remote Configs

### File Remotes

```yaml
rules:
  - remote: shared/team-checks.yaml
```

### Git Remotes

Format: `git+<url>#<ref>:<path>`

```yaml
rules:
  - remote: git+https://github.com/org/shared-checks.git
  - remote: git+https://github.com/org/shared-checks.git#develop
  - remote: git+https://github.com/org/shared-checks.git#v1.0.0:configs/dev.yaml
```

**Important:** Run `checksy install` before using git remotes.

### Remote Rules Limitations

- Can ONLY have `remote` property
- Cannot have `name`, `check`, `severity`, `fix`, `interactive-fix`, `hint`, or
  `timeout`
- Active include cycles fail with the ordered include chain; completed repeated
  includes are deduplicated

---

## Fix Mode

### How It Works

When `--fix` is enabled:

1. Run `check` command
2. If it fails, run its one configured `fix` or `interactive-fix`
3. If the repair succeeds, re-run `check` non-interactively
4. Report final result

### Example

```yaml
rules:
  - name: Dependencies installed
    check: test -d node_modules
    fix: npm ci
```

Use `interactive-fix` only when the repair genuinely needs a terminal:

```yaml
rules:
  - name: Local environment configured
    check: test -f .env.local
    interactive-fix: '${EDITOR:-vi} .env.local'
```

Checksy opens a terminal only after this check fails during `check --fix`; it
does not add a confirmation prompt. File-backed runs require a usable foreground
terminal. `--non-interactive`, `--stdin-config`, and `--config -` prohibit the
interactive repair but do not disable ordinary `fix` commands. A required
interactive repair that cannot run remains a normal severity-governed check
failure, so CI should use `--non-interactive` explicitly.

### Limitations

- Only works with inline rules (not patterns)
- A completed nonzero repair doesn't stop execution; operational supervision
  errors do
- Re-check runs after successful fix
- `fix` and `interactive-fix` cannot appear on the same rule

---

## CI/CD Integration

### Template

```bash
# Install
curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/install.sh | bash

# Cache remotes (if using git configs)
checksy install

# Run checks
checksy check
```

### GitHub Actions

```yaml
name: Health Checks
on: [push, pull_request]
jobs:
  checks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/install.sh | bash
      - run: checksy install
      - run: checksy check
```

### GitLab CI

```yaml
checksy:
  stage: test
  before_script:
    - curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/install.sh | bash
    - checksy install
  script: checksy check
```

---

## Testing Configs

### Dry-Run Validation

```bash
checksy check --check-severity debug --no-fail
```

### Fixture-Based Testing

Create test fixtures in `fixtures/`:

```
fixtures/
├── passing/
│   └── .checksy.yaml
└── failing/
    └── .checksy.yaml
```

### Schema Validation

```bash
checksy schema > /tmp/schema.json
# Validate configs against schema
```

---

## Edge Cases

- **Multiple configs:** Use `remote:` to compose global + local
- **Performance:** Use `--check-severity` to filter; split by domain
- **Partial configs:** Test with `--no-fail` flag
- **Windows:** Use forward slashes in patterns; requires Git Bash/WSL

**For advanced scenarios** including performance optimization and complex configurations, see [references/edge-cases-guide.md](references/edge-cases-guide.md).

---

## Debugging

### Common Errors

| Error | Solution |
|-------|----------|
| `git remote not cached` | Run `checksy install` or use `--fix` |
| `failed to load config` | Check YAML syntax; use valid severities |
| `invalid remote rule` | Remote rules can ONLY have `remote` property |
| Rules not running | Check severity hierarchy with `--cs` flag |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Usage error |
| 2 | Runtime error |
| 3 | Check failures |

**For detailed troubleshooting** including Windows issues, diagnostic steps, and error patterns, see [references/troubleshooting-guide.md](references/troubleshooting-guide.md).

---

## Best Practices

### Decision Guide

| Question | Answer |
|----------|--------|
| Preconditions vs Rules? | Preconditions = must pass first; Rules = can fail independently |
| Inline vs Remote? | Inline = project-specific; File = team; Git = cross-org |
| When to use --fix? | Use for provisioning; add `--non-interactive` in CI/headless automation |

### Severity Guidelines

| Level | Use For |
|-------|---------|
| error | Blocking (tests, security) |
| warn | Non-blocking (linting) |
| info | FYI only |
| debug | Troubleshooting |

### Config Structure

```yaml
# 1. Preconditions first
preconditions:
  - name: Dependencies installed

# 2. Rules by severity (errors first)
rules:
  - name: Critical check
    severity: error
  - name: Optional check
    severity: warn
```

**For detailed best practices** including naming conventions, organization patterns, and advanced guidance, see [references/best-practices-guide.md](references/best-practices-guide.md).

---

## Quick Command Reference

| Command | Purpose |
|---------|---------|
| `checksy check` | Run all checks |
| `checksy check --fix` | Run with auto-fix |
| `checksy check --fix --non-interactive` | Run ordinary fixes but prohibit terminal repairs |
| `checksy check --cs warn --fs error` | Filter by severity |
| `checksy check --no-fail` | Never exit with failure |
| `checksy install` | Cache git remotes |
| `checksy install --prune` | Update and clean cache |
| `checksy init` | Create starter config |
| `checksy schema` | Output JSON schema |
| `checksy version` | Show version |
| `checksy help` | Show help |

---

## Feedback & Issues

If this skill gives incorrect advice, contains outdated information, or could be improved:

1. **Open an issue or PR** at https://github.com/notwillk/checksy/
2. **Include:**
   - What you asked for
   - What the skill suggested
   - What you expected instead
   - Your checksy version (`checksy version`)

3. **For quick fixes:** Check the [source code](https://github.com/notwillk/checksy) for the latest SKILL.md updates

---

## Resources

- **Source Code:** https://github.com/notwillk/checksy
- **Issues:** https://github.com/notwillk/checksy/issues
- **Schema:** Run `checksy schema` for official JSON Schema

---

## Reference Guides

For detailed guidance on specific topics:

- **[Configuration Examples](references/config-examples.md)** - Detailed YAML examples and production configs
- **[Command Cheatsheet](references/command-cheatsheet.md)** - Quick command reference
- **[Troubleshooting Guide](references/troubleshooting-guide.md)** - Error patterns, diagnostics, Windows issues
- **[Best Practices Guide](references/best-practices-guide.md)** - Naming conventions, decision frameworks
- **[Edge Cases Guide](references/edge-cases-guide.md)** - Advanced scenarios, performance optimization
