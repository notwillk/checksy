---
name: optimize-skill-description
description: >
  Optimize skill descriptions for reliable triggering. Use when a skill is not activating 
  on the right prompts, activates too often on irrelevant tasks, or when you need to 
  improve the description field to make it more effective. Also use when the user wants 
  to test description accuracy, design eval queries, or iterate on skill descriptions 
  systematically.
license: MIT
metadata:
  author: agentskills
  version: "1.0.0"
  category: skill-optimization
---

# Optimize Skill Description

A skill's `description` field determines when it activates. An under-specified description means the skill won't trigger when it should. An over-broad description means it triggers when it shouldn't. This skill helps you optimize your descriptions for reliable triggering.

## Quick Start

To optimize a skill description:

1. Design eval queries (should-trigger and should-not-trigger)
2. Split into train (60%) and validation (40%) sets
3. Run multiple evaluations per query (3+ runs)
4. Calculate trigger rates
5. Identify failures and revise description
6. Repeat until satisfied
7. Select best iteration based on validation performance

## Understanding Skill Triggering

### Progressive Disclosure

Agents use progressive disclosure to manage context:

1. **Discovery (startup)**: Load only `name` and `description` (~100 tokens)
2. **Activation**: Load full `SKILL.md` when description matches task
3. **Execution**: Load referenced files as needed

The description carries the entire burden of the activation decision.

### When Skills Activate

Agents typically consult skills for tasks requiring specialized knowledge:

- Unfamiliar APIs or tools
- Domain-specific workflows
- Uncommon formats or procedures
- Project-specific conventions

Simple tasks the agent handles well alone may not trigger skills even with perfect descriptions.

## Designing Eval Queries

Create a set of realistic user prompts labeled with whether they should trigger your skill.

### Format

```json
{
  "queries": [
    {
      "query": "Realistic user prompt here",
      "should_trigger": true
    },
    {
      "query": "Another realistic prompt",
      "should_trigger": false
    }
  ]
}
```

### Quantity

Aim for about 20 queries:
- 10 that should trigger (positive examples)
- 10 that should not trigger (negative examples)

### Should-Trigger Queries

Test whether the description captures the skill's scope. Vary along these axes:

| Axis | Examples |
|------|----------|
| **Phrasing** | Formal, casual, typos, abbreviations |
| **Explicitness** | Names domain directly vs. describes need |
| **Detail** | Terse vs. context-heavy (file paths, column names) |
| **Complexity** | Single-step vs. multi-step workflows |

**Most useful queries** are ones where the skill would help but the connection isn't obvious. If the query already asks for exactly what the skill does, any reasonable description would trigger.

### Should-Not-Trigger Queries

Focus on **near-misses** — queries that share keywords but need something different:

**Weak negative examples:**
```json
{ "query": "Write a fibonacci function", "should_trigger": false }
{ "query": "What's the weather today?", "should_trigger": false }
```
These are too easy and test nothing useful.

**Strong negative examples:**
```json
{ "query": "Update formulas in my Excel budget spreadsheet", "should_trigger": false }
{ "query": "Write a Python script that reads a CSV and uploads rows to Postgres", "should_trigger": false }
```

These share concepts (spreadsheet, CSV) but need different capabilities.

### Tips for Realism

Real user prompts contain context that generic queries lack:

```json
{ 
  "query": "I've got a spreadsheet in ~/data/q4_results.xlsx with revenue in col C and expenses in col D — can you add a profit margin column?",
  "should_trigger": true 
}
```

Include:
- **File paths**: `~/Downloads/report_final_v2.xlsx`
- **Personal context**: `"my manager asked me to..."`
- **Specific details**: Column names, values, constraints
- **Casual language**: Typos, abbreviations, conversational tone

## Testing Trigger Rates

### Running Evaluations

Test each query by running it through your agent with the skill installed.

**Basic approach:**
1. Start with clean context
2. Send the query to the agent
3. Observe whether the skill loads
4. Record result

### Trigger Rate

Since model behavior is nondeterministic, run each query multiple times (3+ runs) and compute:

```
trigger_rate = (number of times skill triggered) / (total runs)
```

**Pass threshold**: 0.5 (50%)

- Should-trigger query passes if trigger_rate ≥ 0.5
- Should-not-trigger query passes if trigger_rate < 0.5

## Train/Validation Split

### The Problem: Overfitting

If you optimize the description against all your queries, you risk overfitting — crafting a description that works for these specific phrasings but fails on new ones.

### The Solution: Split Your Data

**Train set (~60%)**: Queries you use to identify failures and guide improvements.

**Validation set (~40%)**: Queries you set aside to check whether improvements generalize.

### Splitting Guidelines

- Shuffle randomly before splitting
- Keep proportional mix of should-trigger/should-not-trigger in both sets
- Keep the split fixed across iterations (compare apples to apples)
- Use the same train/validation queries for all iterations of a skill

### Usage

1. Use **train set failures** to guide changes
2. Keep **validation set results** out of the revision process
3. Select the best description by **validation pass rate**

## The Optimization Loop

Progress through these steps iteratively:

**Step 1: Evaluate**
- Test current description on train AND validation sets
- Record trigger rates for all queries
- Calculate overall pass rate

**Step 2: Identify Failures**
- Find should-trigger queries that didn't trigger (description too narrow)
- Find should-not-trigger queries that did trigger (description too broad)
- Look for patterns across failures

**Step 3: Revise Description**
- Broaden scope if should-trigger queries fail
- Add specificity if should-not-trigger queries fail
- Generalize fixes, don't add specific keywords from failed queries
- Keep under 1024 characters

**Step 4: Repeat**
- Re-run evaluation with revised description
- Compare validation performance
- Continue until satisfied or plateaued

**Stopping criteria:**
- All train set queries pass
- Validation performance stops improving
- 5+ iterations without meaningful improvement
- Can't identify clear patterns in failures

### Step 1: Evaluate

Test the current description on both train and validation sets:

```bash
# Example with train queries
python scripts/evaluate-trigger-rate.py train_queries.json --skill my-skill --runs 3

# Example with validation queries
python scripts/evaluate-trigger-rate.py validation_queries.json --skill my-skill --runs 3
```

### Step 2: Identify Failures

**Should-trigger failures**: Description is too narrow
- Missing synonyms or related terms
- Too specific to exact phrasings
- Missing implicit trigger contexts

**Should-not-trigger failures**: Description is too broad
- Includes keywords that overlap with unrelated domains
- Doesn't clarify boundaries with adjacent skills
- Triggers on near-misses

### Step 3: Revise Description

**If should-trigger queries are failing:**
- Broaden the scope description
- Add synonyms and related terms
- Include implicit trigger contexts
- Add "even if" clauses for edge cases

**If should-not-trigger queries are false-triggering:**
- Add specificity about what the skill does NOT do
- Clarify boundaries with adjacent skills
- Distinguish similar domains explicitly

**General principles:**
- Avoid adding specific keywords from failed queries (overfitting)
- Find the general category or concept instead
- If stuck, try structurally different approaches
- Keep description under 1024 characters

### Step 4: Select Best Iteration

After several iterations:

1. Compare all descriptions by **validation pass rate**
2. Note that the best may not be the last one
3. Earlier iterations might have higher validation performance
4. Select the description with highest validation pass rate

### Stopping Criteria

Stop when:
- All train set queries pass
- Validation performance stops improving
- You've done 5+ iterations without meaningful improvement
- You can't identify clear patterns in failures

Five iterations is usually enough. If performance isn't improving, the issue may be with the queries (too easy, too hard, poorly labeled) rather than the description.

## Before and After Examples

### Example 1: CSV Analysis Skill

**Before:**
```yaml
description: Process CSV files.
```

**Problems:**
- Too vague — doesn't say what "process" means
- Doesn't specify when to use it
- Won't catch variations like "spreadsheet" or "table"

**After:**
```yaml
description: >
  Analyze CSV, TSV, and Excel files to compute summary statistics, 
  add derived columns, and generate charts. Use when the user has 
  tabular data and wants to explore, transform, or visualize it — 
  even if they don't explicitly mention "CSV" or "spreadsheet".
```

**Improvements:**
- Lists specific capabilities (statistics, columns, charts)
- Includes multiple file types (CSV, TSV, Excel)
- Explicit trigger conditions
- Covers implicit triggers ("even if...")

### Additional Examples

See `references/description-examples.md` for:
- API Client skill (implementation-focused to intent-focused)
- Code Review skill (passive to imperative)
- Database Operations skill (boundary clarification)

## Using Scripts

### Generate Eval Queries

Generate test queries from a skill description:

```bash
python scripts/generate-eval-queries.py \
  --description "Your skill description here" \
  --count 20 \
  --output queries.json
```

### Evaluate Trigger Rate

Evaluate a skill against a set of queries:

```bash
python scripts/evaluate-trigger-rate.py \
  queries.json \
  --skill my-skill \
  --runs 3 \
  --output results.json
```

### Analyze Results

Analyze optimization results across iterations:

```bash
python scripts/analyze-results.py \
  --baseline iteration-1/results.json \
  --current iteration-2/results.json \
  --output analysis.md
```

## Best Practices

1. **Design diverse queries**: Cover phrasing variations, detail levels, and complexity
2. **Focus on near-misses**: Strong negative examples are most valuable
3. **Use train/validation split**: Prevent overfitting
4. **Run multiple times**: Account for model non-determinism
5. **Generalize from failures**: Don't just add failed keywords
6. **Try structural changes**: Different framing can break through plateaus
7. **Stop at diminishing returns**: 5 iterations is usually sufficient

## Gotchas

Common mistakes when optimizing descriptions:

- **Optimizing without train/validation split**: Always hold out 40% of queries for validation. Without this, you'll overfit to your test set.
- **Adding keywords blindly**: Don't copy failed query text into your description. Find the general concept instead.
- **Description bloat**: Stay under 1024 characters. Remove redundant terms rather than just adding more.
- **Single-run evaluation**: Run each query 3+ times. Model behavior is non-deterministic.
- **Too few queries**: 20 queries minimum (10 positive, 10 negative). Fewer queries give unreliable signal.
- **Ignoring near-misses**: Weak negative examples ("What's the weather?") don't test boundaries. Use adjacent domain examples.
- **Perfect train, poor validation**: This signals overfitting. Select the description with best validation performance, not the last iteration.
- **Forgetting implicit triggers**: Users don't always name the domain directly ("spreadsheet" vs "CSV"). Cover these cases with "even if" clauses.

## Complete Eval Queries Example

Abbreviated example (5 of 20 queries) for a CSV analysis skill:

```json
{
  "queries": [
    { "query": "Analyze this CSV file", "should_trigger": true },
    { "query": "hey can u look at this csv for me", "should_trigger": true },
    { "query": "Update formulas in my Excel budget spreadsheet", "should_trigger": false },
    { "query": "Write a Python script that reads CSV and uploads to Postgres", "should_trigger": false },
    { "query": "Write a fibonacci function", "should_trigger": false }
  ]
}
```

See `references/query-design-guide.md` for complete 20-query example with all variations (formal, casual, typos, context-heavy, multi-step).

## Eval Query Starter Template

Use this template to generate your first set of eval queries:

```json
{
  "queries": [
    // === POSITIVE EXAMPLES (Should Trigger) ===
    // Direct request
    { "query": "[Primary action verb] [target]", "should_trigger": true },
    
    // Casual phrasing
    { "query": "hey can u [action] [target] for me", "should_trigger": true },
    
    // Implicit/context-heavy
    { "query": "I have [target] in [location] with [details] — can you [goal]?", "should_trigger": true },
    
    // Multi-step workflow
    { "query": "[Step 1], [Step 2], and [Step 3] with [target]", "should_trigger": true },
    
    // Abbreviated/typo
    { "query": "[misspelled action] [target] pls", "should_trigger": true },
    
    // === NEGATIVE EXAMPLES (Should Not Trigger) ===
    // Adjacent domain (shares keywords, different intent)
    { "query": "[Similar action] [related but different target]", "should_trigger": false },
    
    // Different action on same target
    { "query": "[Different action] [target]", "should_trigger": false },
    
    // Completely unrelated
    { "query": "[Unrelated task]", "should_trigger": false },
    
    // Overlapping keywords, different workflow
    { "query": "Write code that [uses target but for different purpose]", "should_trigger": false }
  ]
}
```

**Fill in the blanks:**
- `[Primary action verb]`: analyze, process, extract, convert, etc.
- `[target]`: csv, pdf, json, data, api, etc.
- `[location]`: ~/Downloads/, ~/data/, ./input/, etc.
- `[details]`: column names, file attributes, specific values
- `[goal]`: compute stats, generate chart, clean data, etc.

**Aim for 10 positive and 10 negative examples total.**

## Optimization Plateau Troubleshooting

When your description stops improving after 3-5 iterations, diagnose the issue:

### Diagnosis Checklist

| Symptom | Likely Cause | Solution |
|---------|--------------|----------|
| Perfect train, poor validation | **Overfitting** | Select earlier iteration with better validation score |
| All queries trigger / none trigger | **Description too broad/narrow** | Reset with different framing approach |
| High variance (inconsistent trigger rates) | **Ambiguous description** | Add concrete examples, clarify edge cases |
| Near-misses always fail | **Missing domain boundaries** | Explicitly clarify what skill does NOT do |
| Casual/typo queries fail | **Missing phrasing variations** | Add synonyms and informal language |
| Complex/multi-step queries fail | **Scope too narrow** | Broaden to include workflow contexts |

### When to Reset vs. Persist

**Reset and try new approach when:**
- ✅ 5+ iterations with no validation improvement
- ✅ Each fix breaks something else (whack-a-mole pattern)
- ✅ Description over 900 characters and still failing
- ✅ Can't identify clear patterns in failures

**Persist and refine when:**
- ✅ Clear pattern in failures (e.g., only typo queries fail)
- ✅ Validation improving but slowly (plateauing upward)
- ✅ Some edge cases failing but core cases solid
- ✅ Description under 800 characters with room to add specificity

### Emergency Reset Template

If stuck, start fresh with this template:

```yaml
description: >
  [Specific action] [target] to achieve [outcome]. 
  Use when [primary trigger], including [secondary trigger], 
  even if [implicit trigger]. 
  Does NOT handle [excluded case 1] or [excluded case 2] — 
  use [other skill] for those cases.
```

## Integration with Skill Creation

After creating a skill with `create-agent-skill`:

1. Write initial description following the patterns
2. Design eval queries (20 total, diverse)
3. Run systematic optimization
4. Apply optimized description to SKILL.md
5. Validate with `validate-skill.py`
6. Evaluate output quality with `evaluate-skill-quality`

## Resources

- `references/trigger-evaluation-guide.md` — How progressive disclosure works
- `references/description-examples.md` — Before/after examples
- `references/query-design-guide.md` — Detailed query design
- `references/anti-patterns.md` — Common mistakes to avoid
