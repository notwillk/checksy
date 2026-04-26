---
title: Grading Criteria
---

# Principles for Grading Skill Outputs

Grading is the process of evaluating whether skill outputs meet the expected quality standards. Consistent, principled grading produces reliable evaluation data that drives skill improvement.

## Core Principles

### 1. Evidence-Based Grading

Every pass/fail decision must be supported by concrete evidence from the output.

**Good evidence:**
```
Assertion: "The report includes at least 3 recommendations"
✅ PASS
Evidence: "Found 4 recommendations in section 'Recommendations': 
          1. Implement caching... 2. Add validation... 3. Update docs... 4. Monitor..."
```

**Poor evidence:**
```
Assertion: "The report includes at least 3 recommendations"  
✅ PASS
Evidence: "The report has a recommendations section"
(No specific count or content verification)
```

### 2. Substance Over Form

Distinguish between having the right structure and having meaningful content.

**Example:**

```
Assertion: "The summary section provides an overview"

❌ FAIL - Structure only
Output has: "## Summary" followed by "[Summary goes here]"

✅ PASS - Meaningful content  
Output has: "## Summary" followed by "This analysis examined Q4 sales data
across 4 regions. Key findings: North region exceeded targets by 15%, while
South region fell short by 8%. Recommendations focus on improving..."
```

### 3. Consistency

Apply the same standards across all test cases and iterations.

**Consistency checklist:**
- Use the same evidence standards for similar assertions
- Don't let one "good enough" pass when another failed
- Document edge cases and how you handled them
- Review your own grading for drift over time

### 4. Calibration

Regularly check that your grading aligns with actual quality.

**Calibration exercise:**
1. Grade a set of outputs
2. Have another person grade the same outputs
3. Compare and discuss disagreements
4. Adjust criteria based on insights

### 5. Separation of Concerns

Distinguish between:
- **Technical correctness** — Does it work? (objective)
- **Quality/polish** — Is it well done? (subjective)
- **Appropriateness** — Is it the right solution? (contextual)

Grade each separately when possible.

## Grading Workflow

### Step 1: Review Without Bias

Before grading:
- Don't look at which version produced the output
- Don't recall previous grades
- Don't check if it's "with skill" or "without skill"

**Blind grading** reduces bias toward expecting improvement.

### Step 2: Check Each Assertion

For each assertion:
1. Read the assertion carefully
2. Examine the output for relevant content
3. Form an initial judgment
4. Find specific evidence
5. Record pass/fail with evidence

### Step 3: Flag for Review

Mark assertions where you're uncertain:
- Borderline cases
- Ambiguous outputs
- Assertions that might need revision

### Step 4: Document Edge Cases

Note unusual situations:
```
"Output was technically correct but used deprecated syntax"
"Assertion passed but quality was borderline"
"Skill took unexpected approach but achieved same result"
```

### Step 5: Calculate Summary

Aggregate results:
```
Total assertions: 10
Passed: 8
Failed: 2
Pass rate: 80%
```

## Grading Specific Output Types

### Code/Scripts

**Check:**
- Syntactic validity (parses/compiles)
- Logical correctness (produces expected result)
- Error handling (graceful failure)
- Style compliance (follows conventions)
- Documentation (comments, docstrings)
- Efficiency (no obvious waste)

**Evidence to collect:**
- Does it run without errors?
- Does output match expected?
- What does it do with edge cases?

### Text/Documents

**Check:**
- Structure compliance (follows template)
- Completeness (covers all required topics)
- Accuracy (facts are correct)
- Clarity (understandable to target audience)
- Concision (no unnecessary content)
- Formatting (proper markdown/HTML/etc.)

**Evidence to collect:**
- Section headings present?
- Specific content items mentioned?
- Word count in expected range?
- No obvious errors?

### Data/Structured Output

**Check:**
- Schema compliance (matches expected structure)
- Data completeness (all required fields)
- Data accuracy (values are correct)
- Format compliance (valid JSON/CSV/etc.)
- Relationships (foreign keys, references work)

**Evidence to collect:**
- Validates against schema?
- Spot-check sample values?
- All rows present?

### Visualizations

**Check:**
- File existence (image generated)
- Content accuracy (shows correct data)
- Aesthetics (clear, readable)
- Labeling (axes, legends, titles)
- Completeness (all data represented)

**Evidence to collect:**
- Can view the image?
- Data matches input?
- Labels present and correct?
- Visual encoding appropriate?

## Handling Ambiguity

### When Output Doesn't Clearly Pass or Fail

**Options:**
1. **Fail with explanation** — "Borderline: X was present but incomplete"
2. **Pass with caveat** — "Passed but note Y issue for future"
3. **Flag for revision** — "Unclear: update assertion to be more specific"
4. **Consult another grader** — Get second opinion

**Default to:** Fail if uncertain (conservative grading)

### When Skill Takes Unexpected Approach

**Evaluate by:**
1. Did it achieve the goal?
2. Is the approach reasonable?
3. Are there downsides to this approach?
4. Would this approach work for similar tasks?

**Document:**
- The unexpected approach
- Why it still passes (or fails)
- Whether approach should be encouraged or discouraged

### When Output is Partial

**Examples:**
- Some assertions pass, others fail
- Task partially completed
- Mix of good and poor quality

**Grade:**
- Pass individual assertions that are met
- Fail those that aren't
- Note partial completion in evidence
- Consider whether partial = fail overall

## Grading Bias to Avoid

### 1. The Halo Effect

**Don't let:** One good aspect influence the whole grade

```
Output has excellent formatting but wrong results
❌ "It looks great! PASS"
✅ "Formatting excellent, but results incorrect. FAIL"
```

### 2. The Horn Effect

**Don't let:** One bad aspect influence the whole grade

```
Output has one typo but otherwise correct
❌ "There's a typo. FAIL"
✅ "One typo found, but meets all criteria. PASS (with note)"
```

### 3. Expectation Bias

**Don't let:** Expectations influence perception

```
Grading "with skill" version
❌ "This should be better" (grade leniently)
❌ "This isn't much better" (grade harshly)
✅ Grade against objective criteria
```

### 4. First Impression Bias

**Don't let:** Initial impression dominate

```
Output starts poorly but improves
❌ "Bad beginning, so FAIL"
✅ Evaluate complete output fairly
```

### 5. Leniency/Severity Bias

**Don't:** Be consistently too easy or too hard

```
Always giving benefit of doubt
❌ "Probably good enough. PASS"
✅ "Doesn't clearly meet criteria. FAIL"

Never giving benefit of doubt
❌ "Not perfect. FAIL"
✅ "Meets criteria with minor issues. PASS"
```

## Documentation

### What to Record

For each graded output:

```json
{
  "grading": {
    "timestamp": "2025-04-25T10:30:00Z",
    "grader": "human-name or llm-model",
    "results": [
      {
        "assertion": "The output includes a chart",
        "passed": true,
        "evidence": "Found chart.png in outputs directory, 45KB",
        "notes": ""
      },
      {
        "assertion": "The chart has labeled axes",
        "passed": false,
        "evidence": "Y-axis labeled 'Revenue', X-axis has no label",
        "notes": "Partial credit - one of two axes labeled"
      }
    ],
    "summary": {
      "total": 10,
      "passed": 8,
      "failed": 2,
      "pass_rate": 0.8
    },
    "overall_notes": "Good output overall, minor issue with axis labeling"
  }
}
```

### Evidence Quality

**Excellent evidence:**
- Quotes specific text from output
- Cites specific file names/sizes
- References line numbers or sections
- Includes calculated verification

**Poor evidence:**
- Vague statements ("looks good")
- No reference to output content
- Opinion without support
- Missing key details

## Review and Calibration

### Self-Review

After grading a batch:
1. Review a sample of your grades
2. Check for consistency
3. Note any drift in standards
4. Adjust if needed

### Cross-Review

Have another person grade the same outputs:
1. Compare pass/fail rates
2. Discuss disagreements
3. Identify ambiguous assertions
4. Update criteria if needed

### Regular Calibration

Every 10-20 test cases:
1. Review grading standards
2. Check for bias creep
3. Update documentation
4. Align with team if applicable

## Summary

Principled grading:
- **Requires evidence** — No assumptions
- **Values substance** — Not just structure
- **Maintains consistency** — Same standards throughout
- **Avoids bias** — Objective criteria
- **Documents thoroughly** — For future reference
- **Calibrates regularly** — Improves over time

Good grading produces trustworthy data that drives real skill improvement.
