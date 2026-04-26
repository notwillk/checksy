---
title: Validation Guide
---

# Skill Validation Guide

This guide covers how to validate your skills to ensure they follow the Agent Skills specification.

## Quick Validation

Run the built-in validation script:

```bash
python3 /path/to/create-agent-skill/scripts/validate-skill.py /path/to/your/skill
```

This checks:
- Directory structure
- SKILL.md exists and is readable
- YAML frontmatter is valid
- Name field follows all constraints
- Description field constraints
- Optional field constraints
- Directory name matches skill name

## Manual Validation Checklist

### Structure Validation

- [ ] Skill directory exists
- [ ] Directory name matches skill name in SKILL.md
- [ ] SKILL.md file exists in the directory
- [ ] File is readable (proper permissions)
- [ ] Optional directories use correct names (`scripts/`, `references/`, `assets/`)

### Frontmatter Validation

- [ ] YAML frontmatter is present (between `---` markers)
- [ ] Frontmatter is valid YAML (no syntax errors)
- [ ] `name` field exists
- [ ] `name` is 1-64 characters
- [ ] `name` contains only lowercase letters, numbers, and hyphens
- [ ] `name` doesn't start or end with a hyphen
- [ ] `name` doesn't contain consecutive hyphens
- [ ] `name` matches the parent directory name
- [ ] `description` field exists
- [ ] `description` is 1-1024 characters
- [ ] `description` is non-empty and meaningful
- [ ] Optional `license` field is present and reasonable
- [ ] Optional `compatibility` field is under 500 characters if present
- [ ] Optional `metadata` is a valid object mapping if present
- [ ] Optional `allowed-tools` is a string if present

### Body Validation

- [ ] Body content exists after frontmatter
- [ ] Content uses valid Markdown
- [ ] No broken relative links to references/
- [ ] Script references use correct relative paths
- [ ] Total length is under 500 lines (recommended)
- [ ] Total tokens are under 5000 (recommended)

### File References

- [ ] References to `references/*.md` use relative paths
- [ ] References to `scripts/*` use relative paths
- [ ] References to `assets/*` use relative paths
- [ ] Referenced files actually exist
- [ ] No deeply nested reference chains (>2 levels)

## Advanced Validation

### Testing Skill Activation

Test whether your description triggers correctly:

1. Install the skill in a compatible agent
2. Test prompts that SHOULD trigger the skill
   - Primary use cases (5-10 variations)
   - Edge cases and implicit triggers
3. Test prompts that should NOT trigger the skill
   - Unrelated domains
   - Near-misses (similar keywords, different intent)
4. Track trigger rate for each prompt
5. Iterate on description if needed

### Testing Skill Execution

Test the actual execution:

1. Run the skill through its primary workflow
2. Verify it produces expected outputs
3. Test error handling paths
4. Test edge cases mentioned in gotchas
5. Time the execution (check for inefficiencies)

### Cross-Client Testing

If possible, test in multiple agents:

- Claude Code
- VS Code with Copilot
- OpenCode
- Other Agent Skills compatible clients

Different clients may interpret instructions slightly differently.

## Common Validation Errors

### Name Errors

```
Error: Name 'My-Skill' contains uppercase letters
Fix: Use lowercase only — 'my-skill'
```

```
Error: Name '-my-skill' starts with hyphen
Fix: Remove leading hyphen — 'my-skill'
```

```
Error: Name 'my--skill' contains consecutive hyphens
Fix: Use single hyphen — 'my-skill'
```

```
Error: Name 'my_skill' contains underscores
Fix: Use hyphens — 'my-skill'
```

### Description Errors

```
Error: Description exceeds 1024 characters (current: 1250)
Fix: Shorten description to under 1024 characters
```

```
Error: Description is empty
Fix: Add a meaningful description
```

### YAML Errors

```
Error: Invalid YAML in frontmatter
Fix: Check for:
- Missing quotes around strings with special characters
- Proper indentation
- No tabs (use spaces)
- Balanced brackets and braces
```

### Structure Errors

```
Error: SKILL.md not found
Fix: Ensure SKILL.md exists in the skill directory
```

```
Error: Directory name 'pdf-processor' doesn't match skill name 'pdf-processing'
Fix: Rename directory to match skill name
```

## Validation Tools

### Built-in Validator

The `validate-skill.py` script provides comprehensive validation:

```bash
python3 scripts/validate-skill.py ./my-skill
```

Options:
- `--verbose` or `-v`: Detailed output
- `--strict`: Treat warnings as errors
- `--json`: Output results as JSON

### YAML Linting

Validate frontmatter syntax:

```bash
# Using yamllint
yamllint SKILL.md

# Using Python
python3 -c "import yaml; yaml.safe_load(open('SKILL.md'))"
```

### Link Checking

Verify file references:

```bash
# Check all markdown links
markdown-link-check references/*.md

# Manual check
grep -r "\[.*\](.*)" SKILL.md | while read line; do
  # Extract and verify paths
  echo "$line"
done
```

## Continuous Validation

Add validation to your workflow:

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

for skill in skills/*/; do
  if [ -f "$skill/SKILL.md" ]; then
    python3 scripts/validate-skill.py "$skill" || exit 1
  fi
done
```

### CI/CD Integration

```yaml
# .github/workflows/validate-skills.yml
name: Validate Skills
on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Validate all skills
        run: |
          for skill in skills/*/; do
            python3 scripts/validate-skill.py "$skill"
          done
```

## Validation Best Practices

1. **Validate early and often** — Don't wait until the end
2. **Test activation** — Description quality matters as much as structure
3. **Test execution** — A valid skill might still not work correctly
4. **Document assumptions** — What environment does the skill expect?
5. **Version control** — Track skill versions and validate on changes
6. **Peer review** — Have others test your skills before release

## Getting Help

If validation reveals issues you can't resolve:

1. Check the [Agent Skills specification](/specification)
2. Review the SKILL.md template in `references/SKILL.md.template`
3. Compare with working skills from the examples
4. Consult the community discussions

Remember: A passing validation doesn't guarantee the skill works perfectly — it just means the structure is correct. Always test execution separately.
