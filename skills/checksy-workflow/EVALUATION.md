# checksy-workflow Skill Quality Evaluation

**Date:** 2026-04-26  
**Skill:** checksy-workflow  
**Location:** `/workspaces/checksy/skills/checksy-workflow/SKILL.md`  
**Size:** 956 lines

---

## Executive Summary

**Overall Quality: GOOD (7.5/10)**

The checksy-workflow skill is comprehensive and well-structured. It covers all major use cases for checksy and provides detailed guidance. However, there are opportunities for improvement in test coverage, conciseness, and handling of ambiguous scenarios.

---

## Strengths

### 1. ✅ Comprehensive Coverage (Score: 9/10)
**What's Good:**
- Covers all 10 major topics (Installation, Configuration, Running, Remote configs, Fix mode, CI/CD, Testing, Debugging, Best Practices)
- Includes all user-requested features (devcontainer, IDE schema, fix mode deep-dive)
- 956 lines of detailed content
- Multiple examples for every concept

**Evidence:**
- 11 main sections with clear hierarchy
- Every checksy command is documented
- Configuration examples cover simple to complex scenarios

---

### 2. ✅ Well-Organized Structure (Score: 9/10)
**What's Good:**
- Clear table of contents with anchor links
- Consistent section hierarchy (##, ###, ####)
- Quick Reference at the top for immediate value
- Logical flow: Install → Configure → Run → Advanced → Debug → Best Practices

**Evidence:**
```
16:## Table of Contents
31:## Quick Reference
69:## Installation & Setup
121:## Configuration
244:## Running checksy
347:## Remote Configs (Advanced)
...
```

---

### 3. ✅ Good Examples (Score: 8/10)
**What's Good:**
- YAML examples for every configuration concept
- Shell command examples with comments
- Real-world scenarios (TypeScript compilation, Docker, npm)
- Before/after comparison in troubleshooting section

**Evidence:**
- 50+ code examples throughout
- "Good vs Poor" comparisons in Best Practices
- Practical CI/CD examples for GitHub Actions, GitLab, pre-commit

---

### 4. ✅ Trigger-Optimized Description (Score: 8/10)
**What's Good:**
- Concise (377 chars vs original 487)
- Implicit triggers ("health checks", "workspace validation", "YAML config issues")
- Pushy framing ("even if the user just says...")
- Covers major use cases

**Current Description:**
> Configure, run, and debug checksy workspace health checks. Use when creating or editing .checksy.yaml files, troubleshooting rule failures, setting up git-based remote configs, using --fix auto-repair, integrating into CI/CD, or generating JSON schemas for IDE validation. Covers severity levels (debug/info/warn/error), preconditions, patterns, caching, and common errors — even if the user just says "health checks", "workspace validation", or mentions YAML config issues without naming checksy explicitly.

---

## Weaknesses & Recommendations

### 1. ⚠️ No Eval/Test Cases (Score: 3/10) → **CRITICAL**
**Problem:**
- No `evals/` directory with test cases
- No systematic way to verify skill quality
- Can't measure improvement over time
- Can't catch regressions

**Impact:** HIGH - Without evals, you can't know if the skill actually works reliably across different prompts

**Recommendation:**
Create `evals/evals.json` with test cases:

```json
{
  "skill_name": "checksy-workflow",
  "evals": [
    {
      "id": 1,
      "prompt": "I need to set up checksy for my Node.js project. What should my .checksy.yaml look like?",
      "expected_output": "A valid .checksy.yaml with preconditions for node_modules and rules for tests, linting, and type checking",
      "files": [],
      "assertions": [
        "Output contains valid YAML with 'rules' array",
        "Includes at least one precondition for dependencies",
        "Includes severity configuration",
        "Examples are relevant to Node.js projects"
      ]
    },
    {
      "id": 2,
      "prompt": "My checksy check is failing with 'git remote not cached' error. How do I fix this?",
      "expected_output": "Clear instructions to run 'checksy install' or use --fix flag",
      "files": [],
      "assertions": [
        "Mentions 'checksy install' command",
        "Mentions --fix flag as alternative",
        "Explains why the error occurs"
      ]
    },
    {
      "id": 3,
      "prompt": "How do I integrate checksy into my GitHub Actions workflow?",
      "expected_output": "Complete GitHub Actions YAML with checksy installation and execution",
      "files": [],
      "assertions": [
        "Provides valid GitHub Actions workflow syntax",
        "Includes checksy installation step",
        "Includes 'checksy install' for remotes",
        "Includes 'checksy check' execution"
      ]
    },
    {
      "id": 4,
      "prompt": "I have a failing check that says 'npm test' failed. Can I make it auto-fix?",
      "expected_output": "Instructions on adding 'fix' property with appropriate npm command",
      "files": [],
      "assertions": [
        "Explains the 'fix' property",
        "Shows example fix command for npm",
        "Mentions that fix runs before re-checking"
      ]
    },
    {
      "id": 5,
      "prompt": "I want to share checksy configs across my team's repos. What's the best approach?",
      "expected_output": "Explanation of git-based remote configs with proper URL format",
      "files": [],
      "assertions": [
        "Mentions 'remote:' property in rules",
        "Shows git+URL format",
        "Mentions 'checksy install' requirement"
      ]
    }
  ]
}
```

---

### 2. ⚠️ Too Long / Could Be More Concise (Score: 5/10) → **MODERATE**
**Problem:**
- 956 lines is very long for a skill
- Model may lose context when processing
- Some sections could be more concise without losing value
- Risk of TL;DR effect

**Impact:** MEDIUM - Long skills can overwhelm the model and user

**Specific Issues & Fixes:**

#### A. Redundancy in Examples
**Current:** Multiple similar YAML examples in Configuration section
**Fix:** Consolidate into one "Complete Example" with annotations:

```yaml
# Complete .checksy.yaml Example

# Optional settings
cachePath: .checksy-cache
checkSeverity: warn      # Run warn+ rules by default
failSeverity: error      # Only errors cause failure

# Preconditions must pass before rules run
preconditions:
  - name: Dependencies installed
    severity: error
    check: test -d node_modules
    fix: npm ci            # Auto-fix if missing

# Main validation rules
rules:
  # Full rule with all properties
  - name: TypeScript compiles
    severity: error
    check: npx tsc --noEmit
    fix: npm run typecheck
    hint: Run 'npm run typecheck' for details

  # Minimal rule (name auto-generated from check)
  - check: echo "Quick validation"

  # Remote config reference
  - remote: git+https://github.com/org/shared-checks.git#main

# Pattern-based rules (glob matching)
patterns:
  - "scripts/check-*.sh"   # Include
  - "!**/*-skip.sh"       # Exclude
```

#### B. Over-Verbose CI/CD Section
**Current:** ~100 lines with separate examples for GitHub, GitLab, pre-commit, Make
**Fix:** Create a "CI/CD Template" with variables:

```yaml
# Template: checksy in CI/CD

# Generic steps (apply to any CI platform):
# 1. Install: curl -fsSL .../install.sh | bash
# 2. Cache remotes: checksy install  
# 3. Run: checksy check

# Platform-specific examples:
# [GitHub Actions, GitLab CI, etc. with 10-15 lines each]
```

#### C. Duplicate Command References
**Current:** Commands shown in Quick Reference AND again in Running checksy section
**Fix:** Keep Quick Reference table, expand Running checksy with context/explanations only

---

### 3. ⚠️ Missing Edge Case Handling (Score: 5/10) → **MODERATE**
**Problem:**
- No guidance on what to do when multiple configs exist
- No handling of partially valid configs
- No mention of Windows-specific issues (paths, shell differences)
- No guidance on performance optimization for large configs

**Impact:** MEDIUM - Users will hit these edge cases and the skill won't help

**Recommendations - Add New Subsections:**

#### A. Multiple Config Scenarios (to Configuration section)
```markdown
### Working with Multiple Configs

**Scenario: Global + Local configs**
Use remote references to compose:

```yaml
# .checksy.yaml (local)
rules:
  - remote: ~/.config/checksy/global.yaml
  - name: Project-specific check
    check: ./test.sh
```

**Scenario: Environment-specific configs**
Use `--config` flag with environment detection:

```bash
# In your wrapper script
if [ "$ENV" = "ci" ]; then
  checksy --config=.checksy.ci.yaml check
else
  checksy check
fi
```
```

#### B. Windows Considerations (to Troubleshooting section)
```markdown
### Windows-Specific Issues

**Path separators:**
- Use forward slashes `/` in patterns, even on Windows
- ✓ `"scripts/checks/*.sh"`
- ✗ `"scripts\\checks\\*.sh"`

**Shell execution:**
- Rules run via `bash -c` on all platforms
- Requires Git Bash, WSL, or Cygwin on Windows
- PowerShell-specific commands need wrapping

**Workaround for PowerShell commands:**
```yaml
rules:
  - name: PowerShell check
    check: "powershell.exe -Command 'Get-Service | Where-Object {$_.Status -eq \"Running\"}'"
```
```

#### C. Performance for Large Configs (to Best Practices)
```markdown
### Optimizing Large Configs

**Problem:** 50+ rules taking too long

**Solutions:**

1. **Use severity to filter:**
   ```yaml
   checkSeverity: warn  # Skip debug/info rules in CI
   ```

2. **Parallel-friendly patterns:**
   - Rules run sequentially, but checks within a rule can parallelize
   - Use `&` and `wait` in check scripts

3. **Split into remote configs:**
   - Group by domain (security, tests, linting)
   - Run specific groups: `checksy --config=security.yaml check`

4. **Cache expensive checks:**
   ```yaml
   rules:
     - name: Dependency audit (cached)
       check: |
         if [ ! -f .audit-cache ] || [ package.json -nt .audit-cache ]; then
           npm audit > .audit-cache
         fi
         grep -q "0 vulnerabilities" .audit-cache
   ```
```

---

### 4. ⚠️ Weak Error Pattern Coverage (Score: 6/10) → **MODERATE**
**Problem:**
- Debugging section covers only 5 common errors
- Missing obscure but important errors
- No decision tree for unknown errors

**Impact:** MEDIUM - Users with uncommon errors won't find help

**Recommendations - Add to Debugging section:**

```markdown
### Other Common Errors

**Error:** `failed to load config: remote config '...' not found`
- **Cause:** Relative path incorrect or file doesn't exist
- **Fix:** Use absolute path or verify file exists relative to including config

**Error:** `check failed: No such file or directory (os error 2)`
- **Cause:** Pattern matched files that were deleted, or working directory mismatch
- **Fix:** Check pattern glob results with `ls scripts/checks/*.sh`

**Error:** Rules run but produce no output
- **Cause:** Rule commands redirecting stdout/stderr
- **Fix:** Remove redirection or add explicit output: `check: "./script.sh && echo 'Success'"`

### Unknown Errors: Diagnostic Steps

If you encounter an error not listed above:

1. **Verify config syntax:**
   ```bash
   checksy schema > /tmp/schema.json
   # Use online YAML validator with schema
   ```

2. **Run with minimal config:**
   ```bash
   checksy --config=/dev/null check  # Test with empty stdin
   ```

3. **Check version compatibility:**
   ```bash
   checksy version
   # Compare with schema version in SKILL.md
   ```

4. **Enable verbose output:**
   ```bash
   checksy check --check-severity debug  # See all rules including debug
   ```
```

---

### 5. ⚠️ Missing Decision Frameworks (Score: 6/10) → **MODERATE**
**Problem:**
- Skill provides "how" but lacks "when to use what"
- Users don't know which approach to choose
- No comparison of trade-offs

**Impact:** MEDIUM - Users may choose wrong approach for their use case

**Recommendations:**

#### A. Add "Decision Guide" subsection to Best Practices
```markdown
## Decision Guide

### When to use preconditions vs regular rules?

| Use Preconditions When | Use Regular Rules When |
|------------------------|------------------------|
| Must pass before anything else runs | Can fail independently |
| Dependencies, services, environment | Business logic checks |
| You want to fail fast | You want all results |

### File remote vs git remote vs inline rules?

| Approach | Best For | Trade-off |
|----------|----------|-----------|
| **Inline** | Project-specific, single repo | Simple, but not reusable |
| **File remote** | Team/org shared configs | Reusable, requires file access |
| **Git remote** | Cross-org, versioned configs | Network required, versioned |

### Which severity should I use?

```
error → Blocking (tests, security, build)
warn  → Should fix (linting, formatting)
info  → FYI only (stats, versions)
debug → Troubleshooting (verbose diagnostics)
```
```

---

### 6. ⚠️ No Version/Changelog Info (Score: 7/10) → **LOW**
**Problem:**
- Skill version is "1.0.0" but no context on what changed
- Checksy itself may update - skill could become outdated
- Users don't know if skill covers latest checksy features

**Impact:** LOW - Maintenance issue over time

**Recommendation:**
Add to metadata or new `## Changelog` section:

```yaml
metadata:
  author: opencode
  version: "1.0.0"
  category: dev-tools
  checksy_version_compatibility: ">=0.7.0"
  last_updated: "2026-04-26"
  changelog:
    - version: "1.0.0"
      date: "2026-04-26"
      notes: "Initial skill covering all checksy 0.7.0 features"
```

---

## Summary of Recommendations (Priority Order)

### 🔴 CRITICAL (Do First)
1. **Create evals/evals.json** with 5 test cases
2. **Run baseline evaluation** to measure current performance

### 🟡 MODERATE (Do Next)
3. **Consolidate redundant examples** (reduce ~150 lines)
4. **Add Edge Case section** (multiple configs, Windows, performance)
5. **Expand Debugging section** with more error patterns
6. **Add Decision Guide** to Best Practices

### 🟢 LOW (Nice to Have)
7. **Add version/changelog metadata**
8. **Create troubleshooting decision tree diagram**
9. **Add video/tutorial links** if available

---

## Metrics Summary

| Category | Score | Status |
|----------|-------|--------|
| Coverage | 9/10 | ✅ Good |
| Structure | 9/10 | ✅ Good |
| Examples | 8/10 | ✅ Good |
| Description | 8/10 | ✅ Good |
| Test Coverage | 3/10 | 🔴 Critical |
| Conciseness | 5/10 | 🟡 Moderate |
| Edge Cases | 5/10 | 🟡 Moderate |
| Error Coverage | 6/10 | 🟡 Moderate |
| Decision Frameworks | 6/10 | 🟡 Moderate |
| Versioning | 7/10 | 🟢 Low |

**Overall: 7.5/10 - Good but needs evals and could be more concise**

---

## Next Steps

1. Create `skills/checksy-workflow/evals/evals.json` (see recommendation #1)
2. Run evaluation using `evaluate-skill-quality` skill
3. Address issues in priority order
4. Re-evaluate after changes
