---
title: Common Workflow Patterns
---

# Common Workflow Patterns for Agent Skills

This reference documents proven patterns for structuring skill instructions. Use these patterns to make your skills more effective and easier for agents to follow.

## Pattern: Gotchas Section

Document non-obvious edge cases and environment-specific facts that defy reasonable assumptions.

### Purpose

- Prevents common mistakes the agent would make without guidance
- Captures domain-specific knowledge that general training lacks
- Reduces iteration on edge cases

### Format

```markdown
## Gotchas

- [Situation]: [Explanation and fix]
- [Inconsistency]: [Clarification]
- [Counterintuitive behavior]: [How to handle it]
```

### Example

```markdown
## Gotchas

- The `users` table uses soft deletes. Queries must include 
  `WHERE deleted_at IS NULL` or results will include deactivated accounts.
- The user ID is `user_id` in the database, `uid` in the auth service, 
  and `accountId` in the billing API. All three refer to the same value.
- The `/health` endpoint returns 200 as long as the web server is running, 
  even if the database connection is down. Use `/ready` to check full 
  service health.
- String comparison is case-sensitive. Usernames must match exactly; 
  "JohnDoe" ≠ "johndoe" ≠ "JOHNDOE".
```

### When to Use

- Always include for skills dealing with:
  - Database queries
  - API interactions
  - File formats with quirks
  - Systems with unusual conventions
  - Legacy codebases

### Best Practices

- Keep gotchas in SKILL.md (not separate files) so the agent reads them first
- Focus on concrete corrections, not general advice
- Add new gotchas as you discover them during testing
- Keep each gotcha to 1-2 sentences

## Pattern: Templates

Provide concrete output structures that the agent can pattern-match against.

### Purpose

- Ensures consistent output format
- Reduces variation in agent responses
- Makes output more predictable for downstream processing

### Format

```markdown
## [Output Type] Template

Use this template, adapting sections as needed:

```markdown
# [Title]

## [Section 1]
[Description of content]

## [Section 2]
[Description of content]

## [Section 3]
[Description of content]
```
```

### Example: Analysis Report

```markdown
## Report Template

Use this structure, adapting sections for the specific analysis:

```markdown
# [Analysis Title]

## Executive Summary
One-paragraph overview of key findings and recommendations

## Key Findings
- Finding 1 with supporting data
- Finding 2 with supporting data
- Finding 3 with supporting data

## Detailed Analysis
[In-depth explanation of methodology and results]

## Recommendations
1. Specific actionable recommendation
2. Specific actionable recommendation
3. Specific actionable recommendation

## Next Steps
- [ ] Action item 1
- [ ] Action item 2
```
```

### Example: Code Structure

```markdown
## Python Script Template

Use this structure for data processing scripts:

```python
#!/usr/bin/env python3
"""
[One-line description]

Usage: [command example]
"""

import argparse

def main():
    parser = argparse.ArgumentParser(description="[Description]")
    parser.add_argument("input", help="Input file path")
    parser.add_argument("--output", "-o", help="Output file path")
    args = parser.parse_args()
    
    # Implementation here
    
if __name__ == "__main__":
    main()
```
```

### When to Use

- Required output formats
- Repeated structures (reports, code, data)
- User-requested specific formatting
- Integration with downstream systems

### Best Practices

- Show concrete structure, not just descriptions
- Use `[Placeholders]` for variable content
- Provide one primary template (not multiple options)
- Store long templates in `assets/` and reference them

## Pattern: Checklists

Explicit checklists help agents track progress through multi-step workflows.

### Purpose

- Prevents skipping steps
- Makes dependencies clear
- Provides visible progress tracking
- Reduces errors in complex processes

### Format

```markdown
## [Workflow Name]

Progress:
- [ ] Step 1: [Action] ([script or command])
- [ ] Step 2: [Action] ([script or command])
- [ ] Step 3: [Action] ([script or command])
- [ ] Step 4: [Action] ([script or command])
```

### Example: Form Processing

```markdown
## Form Processing Workflow

Progress:
- [ ] Step 1: Analyze the form (run `scripts/analyze_form.py`)
- [ ] Step 2: Create field mapping (edit `fields.json`)
- [ ] Step 3: Validate mapping (run `scripts/validate_fields.py`)
- [ ] Step 4: Fill the form (run `scripts/fill_form.py`)
- [ ] Step 5: Verify output (run `scripts/verify_output.py`)
```

### Example: Deployment

```markdown
## Deployment Checklist

Pre-deployment:
- [ ] Run test suite: `pytest tests/`
- [ ] Check code coverage: `pytest --cov`
- [ ] Update version in `pyproject.toml`
- [ ] Update CHANGELOG.md

Deployment:
- [ ] Create git tag: `git tag -a v{X.Y.Z} -m "Version {X.Y.Z}"`
- [ ] Push to remote: `git push origin v{X.Y.Z}`
- [ ] Build package: `python -m build`
- [ ] Upload to PyPI: `twine upload dist/*`

Post-deployment:
- [ ] Verify package on PyPI
- [ ] Test installation: `pip install package-name`
```

### When to Use

- Multi-step workflows with dependencies
- Critical processes where skipping steps causes problems
- Processes with validation gates
- User-visible progress tracking needs

### Best Practices

- Mark steps as the agent completes them
- Group related steps under subheadings
- Include the specific command or script for each step
- Keep the list manageable (break into sub-checklists if too long)

## Pattern: Validation Loops

Instruct agents to validate work before moving forward.

### Purpose

- Catches errors early
- Prevents propagating mistakes
- Ensures quality at each stage
- Reduces overall debugging time

### Format

```markdown
## [Workflow Name]

1. [Do the work]
2. Run validation: `[validation command]`
3. If validation fails:
   - Review the error message
   - Fix the issues
   - Run validation again
4. Only proceed when validation passes
```

### Example: Code Editing

```markdown
## Editing Workflow

1. Make your edits to the file
2. Run validation: `python scripts/validate.py`
3. If validation fails:
   - Read the error output carefully
   - Fix the indicated issues
   - Run validation again
4. Only proceed to testing when validation passes
```

### Example: Data Processing

```markdown
## Data Processing Workflow

1. Extract data: `scripts/extract.py input/ output/raw/`
2. Transform data: `scripts/transform.py output/raw/ output/processed/`
3. Validate output:
   ```bash
   python scripts/validate.py \
     --schema schemas/output.json \
     --data output/processed/
   ```
4. If validation fails:
   - Check `validation_errors.json` for details
   - Fix issues in transformation logic
   - Clear output directory and re-run from step 2
5. Only proceed when validation passes
```

### When to Use

- Every workflow where validation is possible
- Destructive operations
- Multi-stage pipelines
- When output quality is critical

### Best Practices

- Provide specific validation commands
- Document how to interpret validation output
- Make the loop explicit ("run again until passes")
- Don't let agents proceed past failed validation

## Pattern: Plan-Validate-Execute

For complex or destructive operations, create and validate a plan before execution.

### Purpose

- Prevents irreversible mistakes
- Allows human review of intended actions
- Catches logical errors before execution
- Provides audit trail

### Format

```markdown
## [Operation Name]

1. Generate plan: `[command]` → `[plan file]`
   (describes what will be done)
2. Review plan with user and get explicit approval
3. Validate plan against source of truth: `[validation command]`
4. If validation fails, revise plan and re-validate
5. Execute plan: `[execution command]`
6. Verify results: `[verification command]`
```

### Example: Database Migration

```markdown
## Database Migration

1. Generate migration plan:
   ```bash
   python scripts/migrate.py --plan-only --output plan.json
   ```
   (Review plan.json to see what changes will be made)

2. **Get user approval** before proceeding

3. Create backup:
   ```bash
   python scripts/backup.py --output backup.sql
   ```

4. Execute migration:
   ```bash
   python scripts/migrate.py --execute plan.json
   ```

5. Verify migration:
   ```bash
   python scripts/verify_migration.py --before backup.sql --after current
   ```

6. If verification fails, consider rollback:
   ```bash
   python scripts/rollback.py backup.sql
   ```
```

### Example: Batch File Operations

```markdown
## Batch Rename Operation

1. Analyze files to rename: `scripts/analyze_names.py input_dir/` → `rename_plan.json`
2. Review `rename_plan.json` to confirm changes are correct
3. Get user confirmation before proceeding
4. Execute renames: `scripts/execute_rename.py rename_plan.json`
5. Verify no conflicts: `scripts/verify_names.py input_dir/`
```

### When to Use

- Destructive operations (deletions, overwrites)
- Batch operations affecting many files/records
- Schema changes
- Operations that are hard to undo
- Any operation requiring human approval

### Best Practices

- Always generate a machine-readable plan file
- Make the plan human-reviewable
- Require explicit user approval
- Create backups before execution
- Verify after execution
- Document rollback procedures

## Pattern: If-Then Decision Trees

Guide agents through conditional logic.

### Purpose

- Handles multiple scenarios systematically
- Prevents wrong-path execution
- Makes decision logic explicit

### Format

```markdown
## Decision Process

Determine approach based on [criteria]:

**If** [condition A]:
- Use [approach 1]
- Run: `[command 1]`

**If** [condition B]:
- Use [approach 2]
- Run: `[command 2]`

**If** [condition C]:
- Use [approach 3]
- Run: `[command 3]`
- Additional consideration: [note]
```

### Example: File Type Handling

```markdown
## File Processing

Identify file type and process accordingly:

**If file extension is `.csv` or `.tsv`**:
- Use pandas read_csv
- Handle headers automatically
- Command: `python scripts/process_table.py`

**If file extension is `.json`**:
- Parse with json module
- Validate against schema
- Command: `python scripts/process_json.py`

**If file extension is `.xlsx` or `.xls`**:
- Use openpyxl engine
- Process sheet by sheet
- Command: `python scripts/process_excel.py`
- Note: May need to handle multiple sheets

**If file type is unknown**:
- Use `file` command to detect type
- Ask user for clarification if still unclear
```

### When to Use

- Multiple input formats or types
- Different approaches for different scenarios
- Error handling paths
- Feature detection logic

### Best Practices

- Order conditions by likelihood or specificity
- Make conditions mutually exclusive when possible
- Provide a default/else case
- Include the specific command for each branch

## Combining Patterns

These patterns work well together. A complex skill might use:

1. **Gotchas** for non-obvious facts
2. **Templates** for consistent output
3. **Checklists** for tracking progress
4. **Validation Loops** for quality at each stage
5. **Plan-Validate-Execute** for critical operations

Example integration:

```markdown
## Database Schema Update

Review gotchas before starting:
- [ ] Remember that `users` table has soft deletes
- [ ] Check that you have write permissions

Plan:
1. Generate migration plan: `scripts/plan.py > migration.json`
2. Review plan with user
3. Get approval

Execute:
- [ ] Backup database: `scripts/backup.py`
- [ ] Run migration: `scripts/migrate.py migration.json`
- [ ] Validate: `scripts/validate.py`
- [ ] If validation fails, rollback: `scripts/rollback.py`

Output:
Use [Migration Report Template](assets/migration-template.md)
```
