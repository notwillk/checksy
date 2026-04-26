---
name: create-agent-skill
description: Create Agent Skills compatible with the Agent Skills specification. Use when the user wants to create a new skill, needs help with skill structure, wants to validate a skill, or needs guidance on SKILL.md format, naming conventions, or directory structure. Also use when the user mentions creating capabilities for AI agents, packaging expertise, or making reusable agent workflows.
license: MIT
compatibility: Requires Python 3.8+ for validation scripts
metadata:
  author: agentskills
  version: "1.0.0"
  category: skill-creation
---

# Create Agent Skill

This skill helps you create Agent Skills that follow the Agent Skills specification. A skill is a directory containing at minimum a `SKILL.md` file with YAML frontmatter and Markdown instructions.

## Quick Start

To create a new skill:

1. Create a directory matching your skill name (lowercase letters, numbers, hyphens only)
2. Create `SKILL.md` with proper frontmatter
3. Add optional directories as needed (`scripts/`, `references/`, `assets/`)
4. Validate the skill structure

## Skill Creation Workflow

Follow these steps to create a well-structured skill:

### 1. Choose and Validate the Skill Name

The skill name must:
- Be 1-64 characters long
- Contain only lowercase letters (`a-z`), numbers (`0-9`), and hyphens (`-`)
- Not start or end with a hyphen
- Not contain consecutive hyphens (`--`)
- Match the parent directory name exactly

**Valid names:**
```yaml
name: pdf-processing
name: data-analysis
name: code-review
name: api-client-v2
```

**Invalid names:**
```yaml
name: PDF-Processing      # Uppercase not allowed
name: pdf_processing      # Underscores not allowed (use hyphens)
name: -pdf-processing     # Cannot start with hyphen
name: pdf--processing     # Consecutive hyphens not allowed
name: 123-skill           # Cannot start with number (while allowed, not recommended)
```

### 2. Write the Description Field

The description is critical — it determines when your skill activates. Requirements:
- Must be 1-1024 characters
- Use imperative phrasing ("Use when...", "Activate when...")
- Focus on user intent, not implementation
- Be specific but comprehensive
- List contexts where the skill applies

**Good description:**
```yaml
description: Analyze CSV and tabular data files — compute summary statistics, add derived columns, generate charts, and clean messy data. Use this skill when the user has a CSV, TSV, or Excel file and wants to explore, transform, or visualize the data, even if they don't explicitly mention "CSV" or "analysis."
```

**Poor description:**
```yaml
description: Helps with CSV files.
```

### 3. Create the Directory Structure

```
skill-name/
├── SKILL.md              # Required: metadata + instructions
├── scripts/              # Optional: executable code
│   ├── helper.py
│   └── process.sh
├── references/           # Optional: documentation
│   ├── API.md
│   └── FORMS.md
└── assets/               # Optional: templates, resources
    ├── template.json
    └── schema.yaml
```

### 4. Write SKILL.md Content

See `references/SKILL.md.template` for a complete template.

**Frontmatter structure:**
```yaml
---
name: skill-name                    # Required
description: When to use this skill  # Required
license: MIT                         # Optional
compatibility: Requires Python 3.8+  # Optional
metadata:                            # Optional
  author: your-name
  version: "1.0.0"
allowed-tools: Bash Read Edit       # Optional (experimental)
---
```

**Body content guidelines:**
- Keep under 500 lines and 5000 tokens for the main SKILL.md
- Move detailed reference material to separate files in `references/`
- Use clear, step-by-step instructions
- Include concrete examples
- Add a "Gotchas" section for non-obvious edge cases
- Use checklists for multi-step workflows

### 5. Add Optional Fields

**License field:**
```yaml
license: Apache-2.0
# or
license: Proprietary. See LICENSE.txt for complete terms
```

**Compatibility field:**
```yaml
compatibility: Requires git, docker, and access to the internet
compatibility: Designed for Claude Code or similar products
compatibility: Requires Python 3.14+ and uv
```

**Metadata field:**
```yaml
metadata:
  author: example-org
  version: "1.0.0"
  category: data-processing
  tags: [csv, analysis, visualization]
```

**Allowed-tools field (experimental):**
```yaml
allowed-tools: Bash(git:*) Bash(jq:*) Read Edit
```

### 6. Validate the Skill

Run the validation script to check your skill:

```bash
python3 /path/to/create-agent-skill/scripts/validate-skill.py /path/to/your/skill
```

The validator checks:
- Directory structure
- SKILL.md exists and is readable
- Frontmatter YAML is valid
- Name field follows all constraints
- Description field length and content
- Optional field constraints

## Progressive Disclosure

Agents load skills progressively to manage context efficiently:

1. **Discovery (startup)**: Only `name` and `description` are loaded (~100 tokens)
2. **Activation**: Full `SKILL.md` body is loaded when the skill matches (<5000 tokens recommended)
3. **Execution**: Files in `scripts/`, `references/`, `assets/` loaded only when referenced

**Design implications:**
- Keep SKILL.md concise — move details to references/
- Tell the agent *when* to load reference files (e.g., "Read `references/API.md` if you encounter authentication errors")
- Avoid deeply nested reference chains

## Common Patterns

### Gotchas Section

Document environment-specific facts that defy reasonable assumptions:

```markdown
## Gotchas

- The `users` table uses soft deletes. Always include `WHERE deleted_at IS NULL`.
- The user ID is `user_id` in the database, `uid` in auth service, and `accountId` in billing API.
- The `/health` endpoint returns 200 even if the database is down. Use `/ready` for full health checks.
```

### Templates for Output

Provide concrete templates for required output formats:

```markdown
## Report Template

Use this structure, adapting sections as needed:

```markdown
# [Analysis Title]

## Executive Summary
[One-paragraph overview]

## Key Findings
- Finding 1 with data
- Finding 2 with data

## Recommendations
1. Actionable recommendation
2. Actionable recommendation
```
```

### Checklists for Workflows

Track progress through multi-step processes:

```markdown
## Processing Workflow

Progress:
- [ ] Step 1: Extract data (run `scripts/extract.py`)
- [ ] Step 2: Transform data (run `scripts/transform.py`)
- [ ] Step 3: Validate output (run `scripts/validate.py`)
- [ ] Step 4: Load to destination
```

### Validation Loops

Instruct agents to validate work before proceeding:

```markdown
## Workflow

1. Make your edits
2. Run validation: `python scripts/validate.py output/`
3. If validation fails:
   - Review errors
   - Fix issues
   - Run validation again
4. Only proceed when validation passes
```

### Plan-Validate-Execute

For batch or destructive operations:

```markdown
## Database Migration

1. Generate migration plan: `scripts/plan.py > plan.json`
2. Review plan with user and get approval
3. Create backup: `scripts/backup.py`
4. Execute migration: `scripts/migrate.py plan.json`
5. Verify: `scripts/verify.py`
```

## File References

Always use relative paths from the skill directory root:

```markdown
See [API Reference](references/API.md) for endpoint details.

Run the extraction script:
```bash
python3 scripts/extract.py input.txt
```
```

## Bundling Scripts

When you notice the agent reinventing the same logic repeatedly, bundle it as a script in `scripts/`.

### Self-Contained Python Scripts

Use PEP 723 inline metadata:

```python
# /// script
# dependencies = [
#   "requests>=2.31.0",
#   "pandas>=2.0.0",
# ]
# requires-python = ">=3.9"
# ///

import requests
import pandas as pd

# Your script logic here
```

Run with:
```bash
uv run scripts/myscript.py
# or
pipx run scripts/myscript.py
```

### One-Off Commands

For existing tools, use runtime package managers:

```bash
# Python with uvx
uvx ruff@0.8.0 check .

# Node.js with npx
npx eslint@9 --fix .

# Go
go run golang.org/x/tools/cmd/goimports@v0.28.0 .
```

## Best Practices

1. **Start from real expertise**: Extract patterns from actual tasks you've completed, not generic knowledge
2. **Refine with execution**: Test your skill on real tasks and iterate
3. **Add what the agent lacks**: Focus on project-specific conventions, edge cases, and non-obvious requirements
4. **Omit what the agent knows**: Don't explain basic concepts like HTTP, JSON, or what a PDF is
5. **Design coherent units**: Scope skills like functions — one coherent unit of work
6. **Provide defaults, not menus**: Pick one default approach and mention alternatives briefly
7. **Favor procedures over declarations**: Teach *how to approach* problems, not *what to produce*
8. **Match specificity to fragility**: Be prescriptive for fragile operations, flexible for robust ones

## Examples

### Simple Skill: Roll Dice

```markdown
---
name: roll-dice
description: Roll dice using a random number generator. Use when asked to roll a die (d6, d20, etc.), roll dice, or generate a random dice roll.
---

To roll a die with N sides, use:

```bash
echo $((RANDOM % N + 1))
```

Replace N with the number of sides (6 for d6, 20 for d20, etc.).
```

### Complex Skill: Database Analysis

See `references/SKILL.md.template` for a comprehensive example with all optional fields, multiple sections, and script integration.

## Quick-Reference Validation Error Card

| Error | Common Cause | Quick Fix |
|-------|--------------|-----------|
| `name: contains uppercase` | Used `My-Skill` instead of `my-skill` | Convert to lowercase: `my-skill` |
| `name: starts with hyphen` | Used `-my-skill` | Remove leading hyphen: `my-skill` |
| `name: consecutive hyphens` | Used `my--skill` | Use single hyphen: `my-skill` |
| `name: contains underscores` | Used `my_skill` | Use hyphens: `my-skill` |
| `Directory name mismatch` | Folder `pdf-processor` but name `pdf-processing` | Rename folder to match name field |
| `description exceeds 1024 chars` | Description too long | Trim to under 1024 characters |
| `Invalid YAML in frontmatter` | Missing quotes, bad indentation | Check YAML syntax with online validator |
| `SKILL.md not found` | Wrong directory or missing file | Ensure SKILL.md exists in skill root |
| `File references don't exist` | Referenced `references/API.md` but file missing | Create missing files or fix paths |
| `Body exceeds 500 lines` | Too much content inline | Move sections to `references/` |

## Before/After Interactive Examples

### Example 1: Fixing a Name Error

**❌ Before (Invalid):**
```yaml
---
name: PDF_Processor
description: Helps with PDFs.
---
```

**🔍 Identify the errors:**
- `PDF_Processor` contains uppercase and underscores
- Description is vague ("Helps with PDFs")

**✅ After (Valid):**
```yaml
---
name: pdf-processor
description: >
  Extract text and metadata from PDF files, fill forms, and merge documents. 
  Use when working with PDF files or converting PDFs to other formats.
---
```

**Key changes:**
- Lowercase with hyphens: `pdf-processor`
- Specific capabilities listed
- Clear trigger conditions

### Example 2: Fixing Progressive Disclosure

**❌ Before (Too Long):**
```markdown
## Detailed API Guide
[300 lines...]
## Database Schema  
[150 lines...]
## Error Handling
[100 lines...]
```
**Problem:** 550+ lines exceeds limit, loads everything upfront

**✅ After:**
```markdown
## Quick Reference
Basic workflow: do X, then Y, then Z.

## Details
See [API Guide](references/API.md) for endpoints.
See [Schema](references/schema.md) for structure.
See [Errors](references/errors.md) for status codes.
```
**Key changes:** Main file ~50 lines, details loaded on demand

### Example 3: Adding Missing Examples

**❌ Before (No Examples):**
```markdown
## Processing Workflow
Process the data using the appropriate method.
Validate the output.
Save results.
```
**Problem:** "Appropriate method" is ambiguous

**✅ After (With Examples):**
```markdown
## Processing Workflow
1. **Parse:** `scripts/parse.py input.csv`  
   Example: `scripts/parse.py data/sales_2025.csv`
2. **Transform:** `scripts/transform.py --format json`
   Expected: `{"records": [...], "total": 150}`
3. **Validate:** `python scripts/validate.py output.json`
   Should see: `✓ Valid: 150 records`
4. **Save:** Write to `output/processed.json`
```
**Key changes:** Specific commands with examples, clear validation criteria

## Gotchas

Common mistakes when creating skills:

- **Directory name mismatch**: The directory name must exactly match the `name` field in SKILL.md. `name: my-skill` requires directory `my-skill/`, not `my_skill/` or `My-Skill/`.
- **Uppercase in descriptions**: While descriptions can use normal capitalization, the skill name must be lowercase with hyphens only.
- **Missing frontmatter closing**: Remember to close frontmatter with `---` on its own line, not just open it.
- **Consecutive hyphens**: Names like `my--skill` are invalid. Use single hyphens: `my-skill`.
- **Description too vague**: "Helps with CSVs" won't trigger reliably. Be specific about capabilities and when to use them.
- **Absolute paths in references**: Always use relative paths from skill root. Use `scripts/process.py`, not `/full/path/to/process.py`.
- **Forgetting progressive disclosure**: Keep SKILL.md under 500 lines. Move detailed content to `references/` and tell the agent when to load it.
- **No examples**: Instructions without examples lead to inconsistent results. Add concrete examples for complex tasks.

## Skill Creation Checklist

Use this checklist when creating a new skill:

- [ ] Directory name matches `name` field (lowercase, hyphens, no consecutive hyphens)
- [ ] `SKILL.md` exists in the directory
- [ ] Frontmatter has opening and closing `---`
- [ ] `name` field is 1-64 characters, lowercase alphanumeric + hyphens only
- [ ] `description` field is 1-1024 characters, describes capabilities and when to use
- [ ] Description uses imperative phrasing ("Use when...", "Activate when...")
- [ ] SKILL.md body is under 500 lines
- [ ] Gotchas section added for non-obvious requirements
- [ ] Examples provided for complex workflows
- [ ] File references use relative paths
- [ ] Scripts (if any) use PEP 723 inline metadata
- [ ] Validated with `validate-skill.py`

## Validation

Always validate your skill before using it:

```bash
python3 scripts/validate-skill.py ./my-skill
```

For more advanced validation, use the skill validation reference library when available.

## Next Steps

After creating your skill:

1. Test it with real tasks
2. Optimize the description using `optimize-skill-description` skill
3. Evaluate output quality using `evaluate-skill-quality` skill
4. Iterate based on results

## Resources

- `references/SKILL.md.template` — Complete template file
- `references/frontmatter-reference.md` — Detailed frontmatter documentation
- `references/description-patterns.md` — Patterns for writing effective descriptions
- `references/common-workflows.md` — Reusable workflow patterns
- `references/validation-guide.md` — Advanced validation guidance
