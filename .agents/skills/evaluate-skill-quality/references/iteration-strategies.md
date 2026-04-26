---
title: Iteration Strategies
---

# Systematic Skill Iteration Strategies

Once you have evaluation results, you need a systematic approach to improving the skill. Random changes waste time; structured iteration converges faster on quality.

## The Iteration Loop

```
┌─────────────────────────────────────────────────────────────┐
│  1. ANALYZE                                                 │
│  - Review failed assertions                                 │
│  - Read human feedback                                      │
│  - Study execution transcripts                            │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│  2. SYNTHESIZE                                              │
│  - Identify patterns across failures                        │
│  - Find root causes (not just symptoms)                     │
│  - Prioritize issues by impact                              │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│  3. REVISE                                                  │
│  - Update SKILL.md with LLM assistance                      │
│  - Generalize fixes (not narrow patches)                    │
│  - Bundle repeated logic into scripts                       │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│  4. VALIDATE                                                │
│  - Run evals in new iteration directory                     │
│  - Compare with previous version                            │
│  - Check for regressions                                    │
└──────────────────┬──────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────────┐
│  5. DECIDE                                                  │
│  - Satisfied? → Stop                                        │
│  - Better? → Continue iterating                             │
│  - Worse? → Revert and try different approach               │
│  - Plateaued? → Consider if good enough                     │
└─────────────────────────────────────────────────────────────┘
```

## Step 1: Analyze Results

### Review Failed Assertions

**Categorize failures:**

| Type | Example | Likely Cause |
|------|---------|--------------|
| Missing step | "Validation not run" | Instruction unclear or missing |
| Wrong output | "Chart has wrong data" | Logic error in instructions |
| Incomplete | "Only 2 of 3 items present" | Edge case not covered |
| Format error | "Invalid JSON" | Missing format guidance |

**Look for patterns:**
- Same assertion fails across multiple test cases → Systematic issue
- Specific test case always fails → Edge case not handled
- Intermittent failures → Ambiguous instructions

### Read Human Feedback

**Focus on:**
- "Wrong approach" — Skill misunderstood the task
- "Poor structure" — Output organization issues
- "Misses the point" — Relevance problems
- "Technically correct but..." — Quality gaps

**Extract actionable items:**
```
Feedback: "The chart is missing axis labels and months are in alphabetical order instead of chronological"

Action items:
1. Add instruction to label axes
2. Add instruction to sort time series chronologically
3. Add example showing proper time series chart
```

### Study Execution Transcripts

**What to look for:**

**Agent behavior:**
- Ignored instruction → Too ambiguous?
- Did steps in wrong order → Dependencies unclear?
- Wasted time on unproductive steps → Wrong path suggested?
- Repeated similar work → Need to script it?

**Error patterns:**
- Same error type across runs → Missing error handling guidance
- Errors at specific steps → Those steps need clarification
- Silent failures → Need validation guidance

## Step 2: Synthesize Findings

### Find Root Causes

Don't treat symptoms — find causes:

```
Symptom: "Chart doesn't have axis labels"
Surface fix: "Add axis labels to the chart"
Root cause: "No template or example showing what a complete chart looks like"
Better fix: "Add chart template with all required elements labeled"

Symptom: "Agent ignores validation step"
Surface fix: "Emphasize validation more strongly"
Root cause: "Unclear when/how to validate"
Better fix: "Add explicit validation loop with specific command"
```

### Prioritize Issues

**Impact × Effort matrix:**

| | Low Effort | High Effort |
|---|------------|-------------|
| **High Impact** | Do first | Plan carefully |
| **Low Impact** | Quick wins | Consider skipping |

**High impact examples:**
- Fixes multiple failing assertions
- Addresses core functionality
- Prevents major errors

**Low impact examples:**
- Cosmetic improvements
- Edge cases that rarely occur
- Already good enough

### Identify Patterns

**Common pattern: Missing defaults**
```
Multiple failures: Agent asks user which option to choose
Pattern: Instructions present multiple approaches without default
Fix: Add "Default approach:" section
```

**Common pattern: Ambiguous sequencing**
```
Multiple failures: Steps done out of order
Pattern: No explicit ordering or dependencies
Fix: Add numbered steps with dependency notes
```

**Common pattern: Missing examples**
```
Multiple failures: Output format varies wildly
Pattern: No concrete examples provided
Fix: Add template or example output
```

## Step 3: Revise SKILL.md

### Use LLM Assistance

**Prompt structure:**

```
Given:
1. Current SKILL.md: [content]
2. Failed assertions: [list with specific failures]
3. Human feedback: [specific complaints]
4. Execution issues: [what agent did wrong]

Revise SKILL.md following these principles:
- GENERALIZE: Address underlying issues, not specific test cases
- LEAN: Remove instructions that lead to wasted work
- CLEAR: Add examples where agent was confused
- SCRIPT: Move repeated logic to scripts/
- EXPLAIN: Add "why" for reasoning-based instructions

Focus on the highest-impact changes first.
```

### Revision Strategies

**Strategy 1: Add Examples**

When the agent is confused about format or approach:

```markdown
## Output Format

Follow this template:

```markdown
# [Analysis Title]

## Summary
[One paragraph]

## Findings
- [Specific finding with data]

## Recommendations
1. [Actionable item]
```
```

**Strategy 2: Clarify Sequencing**

When steps are done out of order:

```markdown
## Workflow (MUST follow this order)

1. Load data ← MUST complete before step 2
2. Validate schema ← MUST pass before step 3
3. Transform data
4. Output results

Do not proceed to step N+1 until step N is complete.
```

**Strategy 3: Add Validation**

When outputs are wrong but agent thinks they're done:

```markdown
## Quality Check

Before finishing, verify:
- [ ] All input data appears in output (no data loss)
- [ ] Calculations are correct (spot-check: total = sum of parts)
- [ ] Format matches requirements

If any check fails, fix the issue and re-verify.
```

**Strategy 4: Extract Script**

When agent repeatedly does the same complex task:

```markdown
## Analysis

Use the provided script instead of manual analysis:

```bash
python scripts/analyze.py input.csv --output results.json
```

This ensures consistent, tested analysis logic.
```

**Strategy 5: Add Gotchas**

When agent makes the same mistake repeatedly:

```markdown
## Gotchas

- The API returns 200 for soft errors. Check the response body for error codes.
- Column names are case-sensitive. "UserID" ≠ "userId".
- Empty strings are not null. Filter with `field != ''` not `field is not null`.
```

**Strategy 6: Explain Why**

When agent doesn't follow instructions consistently:

```markdown
## Data Cleaning

Sort data by timestamp BEFORE aggregating. If you aggregate first,
temporal patterns will be lost and trends will be inaccurate.

```python
df = df.sort_values('timestamp')  # Must do this first
```
```

### Generalization Principle

**Don't add narrow patches:**

```
Test case: "Analyze the CSV at ~/data/sales.csv"
Fails: Agent looks in wrong directory

❌ Narrow fix: "If the user mentions ~/data/, look there first"
✅ General fix: "File paths starting with ~ are relative to user's home directory"
```

**Address the category, not the instance:**

```
Failure: Query uses word "spreadsheet" but skill only mentions "CSV"

❌ Narrow fix: Add "spreadsheet" as keyword
✅ General fix: Include common synonyms and related terms for the domain
```

## Step 4: Validate Changes

### Run Full Eval Suite

Always test in a fresh iteration directory:

```bash
python scripts/setup-eval-workspace.py --skill ./my-skill --workspace ./my-skill-workspace --iteration 2

# Run all evals
for eval in workspace/iteration-2/eval-*; do
  # Run with skill
  # Run without skill (baseline)
done
```

### Compare Versions

Check for:
1. **Improvements**: More assertions passing
2. **Regressions**: Fewer assertions passing (unexpected)
3. **Side effects**: Changes in unrelated areas
4. **Efficiency**: Time and token changes

**Comparison structure:**

```
Iteration 1 vs Iteration 2:

Positive queries:
- Pass rate: 60% → 75% (+15%)
- Average time: 45s → 42s (-3s)

Negative findings:
- 2 assertions that passed in v1 now fail in v2
- Investigation: These were "always pass" assertions, removal is OK

Conclusion: Net improvement, no concerning regressions
```

### Check for Overfitting

**Signs of overfitting:**
- Perfect train performance, worse validation performance
- Description growing with specific keywords
- Fixes that only help specific test cases

**Remediation:**
- Use validation set results for selection
- Generalize fixes more broadly
- Add diverse test cases

## Step 5: Decide Next Steps

### When to Stop

Stop iterating when:

1. **Satisfied with results**
   - All critical assertions pass
   - Human review finds no major issues
   - Performance is acceptable

2. **Diminishing returns**
   - No improvement in last 2-3 iterations
   - Small gains requiring large effort
   - Time better spent elsewhere

3. **Good enough**
   - Meets requirements
   - Known limitations are acceptable
   - Further improvement not cost-effective

### When to Continue

Continue iterating when:

1. **Clear improvement path**
   - Obvious issues with known fixes
   - High-impact changes identified

2. **Significant gaps**
   - Critical assertions still failing
   - Core functionality unreliable

3. **Learning opportunity**
   - Understanding the domain better
   - Discovering effective patterns

### When to Try Different Approach

Consider a different approach when:

1. **Plateaued with current strategy**
   - Multiple iterations, no improvement
   - Hitting fundamental limitations

2. **Regressions accumulating**
   - Fixing one thing breaks another
   - Whack-a-mole pattern

3. **Wrong foundation**
   - Skill approach fundamentally flawed
   - Better to restart than patch

## Advanced Strategies

### A/B Testing Revisions

Test multiple approaches simultaneously:

```
Iteration 2a: Add detailed examples approach
Iteration 2b: Simplify to checklist approach
Iteration 2c: Script-heavy approach

Compare all three, select best performing
```

### Feature Flags

Test specific changes in isolation:

```markdown
## New Approach (Experimental)

Try this revised method:
1. ...
2. ...

If results are worse, fall back to the standard method above.
```

### Rollback Plan

Always keep previous version:

```bash
# Before major changes
cp -r my-skill my-skill-backup-v1/

# If new version is worse
cp -r my-skill-backup-v1/* my-skill/
```

## Summary

Effective iteration:
1. **Analyze deeply** — Find root causes, not symptoms
2. **Synthesize patterns** — Identify common issues
3. **Revise generally** — Fix categories, not instances
4. **Validate thoroughly** — Check for regressions
5. **Decide deliberately** — Know when to stop or pivot

Structure beats randomness. Systematic iteration converges faster on quality.
