---
title: High Standard Deviation Interpretation
---

# Interpreting High Standard Deviation in Evaluations

This guide helps diagnose and fix high variance (flakiness) in skill evaluations.

## Understanding Standard Deviation Values

| stddev Range | Interpretation | Action Required |
|--------------|----------------|-----------------|
| **0-10%** | Excellent consistency | None - skill is reliable |
| **10-20%** | Good consistency | Monitor, no action needed |
| **20-30%** | Moderate variance | Investigation recommended |
| **30-50%** | High variance | Significant issues - skill needs work |
| **>50%** | Extreme variance | Critical - skill unpredictable |

## Common Causes of High stddev

### 1. Ambiguous Skill Instructions

**Symptom:** Pass rate 60%, stddev 35% across 5+ runs

**Root cause:** Instructions can be interpreted multiple ways

**Diagnosis:**
- Review execution transcripts — does agent take different approaches?
- Check if instructions present multiple options without clear default
- Look for vague terms: "appropriate method", "suitable approach"

**Solution:**
- Add concrete examples showing the ONE way to do it
- Replace "choose" with specific instructions
- Add step-by-step sequence

**Before (high stddev):**
```markdown
Process the data using an appropriate method.
```

**After (low stddev):**
```markdown
Process the data:
1. Read CSV: `pd.read_csv("input.csv")`
2. Clean nulls: `df.dropna()`
3. Save: `df.to_csv("output.csv")`
```

### 2. Flaky Evaluation Criteria

**Symptom:** stddev high on specific assertion

**Root cause:** Criteria changes between runs (timing, random elements)

**Diagnosis:**
- Check if assertion involves time-based elements
- See if external dependencies vary
- Look for order-dependent checks

**Solution:**
- Remove timing-dependent assertions
- Use deterministic assertions
- Add settle-time before checking

**Flaky:**
```json
"Processing completes in under 5 seconds"
```

**Stable:**
```json
"Processing produces output file"
```

### 3. Model Non-Determinism Sensitivity

**Symptom:** stddev increases with model temperature

**Root cause:** Skill relies on model reasoning vs explicit instructions

**Diagnosis:**
- Does skill ask agent to "decide" or "choose"?
- Are there judgment calls without criteria?
- Does output format vary significantly?

**Solution:**
- Reduce reasoning requirements
- Provide output templates
- Add validation steps

**Before:**
```markdown
Analyze the sentiment and categorize appropriately.
```

**After:**
```markdown
Analyze sentiment: positive, negative, or neutral.
Output: {"sentiment": "[category]", "confidence": 0-1}
```

### 4. Edge Case Sensitivity

**Symptom:** stddev spikes on specific test cases

**Root cause:** Edge cases not handled consistently

**Diagnosis:**
- Which eval cases show high variance?
- Do they share characteristics (empty inputs, special chars)?
- Are failures clustered in specific sections?

**Solution:**
- Add gotchas section for edge cases
- Provide explicit handling
- Consider splitting edge cases into separate skill

**Example gotcha:**
```markdown
## Gotchas

- **Empty files:** Return `{"records": []}` not error
- **Unicode:** Use UTF-8 encoding explicitly
- **Large files:** Stream processing required
```

## Calculating and Monitoring stddev

**Minimum calculation:**
```python
from statistics import stdev, mean

pass_rates = [0.6, 0.8, 0.4, 0.7, 0.9]
avg = mean(pass_rates)
variance = stdev(pass_rates)

print(f"Mean: {avg:.1%}, stddev: {variance:.1%}")
```

**Alert thresholds:**
- 🟢 < 20%: Consistent, reliable
- 🟡 20-30%: Monitor
- 🔴 > 30%: Requires attention

## When High stddev is Acceptable

✅ **OK (20-30%) when:**
- Creative/generative tasks
- Multiple valid outputs
- User wants variety
- Edge cases inherently ambiguous

❌ **Not OK when:**
- Consistent results required
- Output feeds automated systems
- Reproducibility expected
- Data processing/analysis tasks
