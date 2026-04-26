---
name: evaluate-skill-quality
description: >
  Evaluate skill output quality using systematic testing. Use when you need to test 
  whether a skill produces correct results, set up evaluation frameworks, compare 
  skill versions, measure improvement over baseline, or iterate on skill quality 
  based on test results. Also use when the user wants to grade skill outputs, 
  analyze performance metrics, or create reproducible skill evaluation workflows.
license: MIT
metadata:
  author: agentskills
  version: "1.0.0"
  category: skill-evaluation
---

# Evaluate Skill Quality

Running a skill on one prompt and getting a good result doesn't mean it works reliably. Systematic evaluation (evals) tests skills across varied prompts, edge cases, and comparison with baselines to ensure consistent, high-quality output.

## Quick Start

To evaluate a skill:

1. Design test cases (prompt, expected output, input files, assertions)
2. Create eval workspace structure
3. Run skill on test cases with and without the skill (baseline comparison)
4. Grade outputs against assertions
5. Aggregate benchmark statistics
6. Analyze patterns in results
7. Human review of outputs
8. Iterate on skill based on findings

## What Makes a Good Test Case

A test case has three essential parts:

### 1. Prompt

A realistic user message — the kind someone would actually type:

```json
"I have a CSV of monthly sales data in data/sales_2025.csv. Can you find the top 3 months by revenue and make a bar chart?"
```

**Tips:**
- Start with 2-3 test cases, expand as needed
- Vary phrasings (formal, casual, precise)
- Cover edge cases (malformed input, unusual requests)
- Use realistic context (file paths, column names, personal details)
- Include at least one "ambiguous" case that tests instruction clarity

### 2. Expected Output

Human-readable description of what success looks like:

```json
"A bar chart image showing the top 3 months by revenue, with labeled axes and values."
```

This guides assertion writing and human review.

### 3. Input Files (Optional)

Files the skill needs to work with:

```json
["evals/files/sales_2025.csv"]
```

Store test data in `evals/files/` within your skill directory.

### 4. Assertions (Added After First Run)

Verifiable statements about the output:

```json
[
  "The output includes a bar chart image file",
  "The chart shows exactly 3 months",
  "Both axes are labeled",
  "The chart title or caption mentions revenue"
]
```

You often don't know what "good" looks like until the skill runs once.

## Test Case Storage

Store test cases in `evals/evals.json` inside your skill directory:

```json
{
  "skill_name": "csv-analyzer",
  "evals": [
    {
      "id": 1,
      "prompt": "I have a CSV of monthly sales data in data/sales_2025.csv...",
      "expected_output": "A bar chart image showing the top 3 months by revenue...",
      "files": ["evals/files/sales_2025.csv"],
      "assertions": [
        "The output includes a bar chart image file",
        "The chart shows exactly 3 months",
        "Both axes are labeled"
      ]
    }
  ]
}
```

## Workspace Structure

Organize eval results in a workspace directory:

```
skill-name/
├── SKILL.md
└── evals/
    ├── evals.json           # Test cases you author
    └── files/               # Input test data
        └── sales_2025.csv

skill-name-workspace/
└── iteration-1/
    ├── eval-top-months-chart/
    │   ├── with_skill/
    │   │   ├── outputs/       # Files produced by the run
    │   │   ├── timing.json  # Tokens and duration
    │   │   └── grading.json # Assertion results
    │   └── without_skill/
    │       ├── outputs/
    │       ├── timing.json
    │       └── grading.json
    ├── eval-clean-missing-emails/
    │   └── ...
    └── benchmark.json       # Aggregated statistics
```

**Key principle**: Each test case gets run twice — once with the skill, once without (baseline).

## Running Evaluations

### Setup

Initialize the workspace structure:

```bash
python scripts/setup-eval-workspace.py \
  --skill ./my-skill \
  --workspace ./my-skill-workspace \
  --iteration 1
```

### Spawning Runs

Each eval run starts with clean context. Provide:
- Skill path (or no skill for baseline)
- Test prompt
- Input files
- Output directory

**Example instructions for agent:**

```
Execute this task:
- Skill path: /path/to/csv-analyzer
- Task: I have a CSV of monthly sales data in data/sales_2025.csv.
  Can you find the top 3 months by revenue and make a bar chart?
- Input files: evals/files/sales_2025.csv
- Save outputs to: csv-analyzer-workspace/iteration-1/eval-top-months-chart/with_skill/outputs/
```

For baseline (same prompt, no skill):

```
- Skill path: none
- Save outputs to: csv-analyzer-workspace/iteration-1/eval-top-months-chart/without_skill/outputs/
```

When improving an existing skill, use the previous version as baseline:

```bash
# Snapshot before editing
cp -r my-skill my-skill-workspace/skill-snapshot/

# Baseline run uses snapshot
```

### Capturing Timing Data

Record token count and duration for comparison:

```json
{
  "total_tokens": 84852,
  "duration_ms": 23332
}
```

Save as `timing.json` in each run directory.

**Why timing matters:**
- Skill quality improvement vs. token cost tradeoff
- A skill that's 50% better but 3x more expensive is different from one that's better AND cheaper
- Helps identify inefficient patterns

## Writing Assertions

Add assertions after seeing your first outputs. You often don't know what "good" looks like until the skill runs.

**Good assertions are:**
- **Specific and observable**: `"The bar chart has labeled axes"`
- **Verifiable**: `"The output file is valid JSON"`
- **Countable**: `"The report includes at least 3 recommendations"`

**Weak assertions:**
- Too vague: `"The output is good"`
- Too brittle: `"Uses exactly the phrase 'Total Revenue: $X'"`
- Not verifiable: `"The code follows best practices"`

**Common types:**
- **File existence**: `"outputs directory contains chart.png"`
- **Content validation**: `"JSON has 'results' array with at least one element"`
- **Format compliance**: `"Markdown follows template in assets/report-template.md"`
- **Count-based**: `"Summary includes exactly 3 key findings"`

See `references/assertion-writing-guide.md` for comprehensive guidance on crafting effective assertions.

## Grading Outputs

### Process

Evaluate each assertion against actual outputs:

```json
{
  "assertion_results": [
    {
      "text": "The output includes a bar chart image file",
      "passed": true,
      "evidence": "Found chart.png (45KB) in outputs directory"
    },
    {
      "text": "Both axes are labeled",
      "passed": false,
      "evidence": "Y-axis is labeled but X-axis has no label"
    }
  ],
  "summary": {
    "passed": 1,
    "failed": 1,
    "total": 2,
    "pass_rate": 0.50
  }
}
```

### Grading Principles

1. **Require concrete evidence for PASS**
   - Don't give benefit of the doubt
   - "Summary section exists" needs substance, not just heading

2. **Review the assertions themselves**
   - Are they too easy? (always pass regardless of quality)
   - Are they too hard? (always fail even when good)
   - Are they unverifiable?

### Grading Methods

**Hybrid approach (recommended):**
- **Scripts** for objective checks: file existence, valid JSON, counts, format compliance
- **LLM** for subjective checks: quality, coherence, completeness
- **Human** for final review: edge cases, holistic quality, unexpected issues

**LLM prompt template:**
```
Evaluate these assertions against the output:
For each: PASS/FAIL + specific evidence
```

### Automated Grading Implementation

Scripts can handle objective checks (file existence, valid JSON, counts). See `references/grading-implementation.md` for:
- Complete grading script template
- Integration with grade-assertions.py
- Best practices and common patterns
- Testing your grading logic

**Quick example:**
```python
def grade_assertion(assertion: str, outputs_dir: Path) -> dict:
    # File existence check
    if "contains" in assertion.lower():
        files = list(outputs_dir.iterdir())
        return {"passed": len(files) > 0, "evidence": f"Found {len(files)} files"}
    
    # JSON validation
    if "valid json" in assertion.lower():
        json_files = list(outputs_dir.glob("*.json"))
        try:
            json.loads(json_files[0].read_text())
            return {"passed": True, "evidence": "JSON valid"}
        except:
            return {"passed": False, "evidence": "JSON invalid"}
    
    return {"passed": False, "evidence": "Requires manual grading"}
```

## Gotchas

Common mistakes when evaluating skills:

- **Always-pass assertions**: Assertions that pass with AND without skill (e.g., "output is markdown") provide no signal. Remove them.
- **Always-fail assertions**: If an assertion fails in both configurations, it's either broken, too hard, or checking the wrong thing. Fix or remove it.
- **Missing baseline**: Always compare "with skill" vs "without skill". You can't claim improvement without knowing the baseline.
- **Forgetting timing data**: Capture tokens and duration. A skill that's 50% better but 3x more expensive is different from one that's better AND cheaper.
- **Too few test cases**: Start with 2-3, but expand to 5+ for meaningful signal. Single test cases can be flukes.
- **Single-run flakiness**: High stddev in pass rate suggests ambiguous instructions or flaky evals. Add examples or tighten guidance.
- **Overfitting evals**: Don't write assertions to match your skill's output. Write them to define "good" independently.
- **Not reviewing failures**: Failed assertions point to skill gaps. Read execution transcripts to understand why, don't just count failures.
- **No human review**: Assertions only catch what you thought to assert. Human review finds unexpected issues and holistic quality problems.

## Aggregating Results

Compute benchmark statistics once all evals are graded. See `aggregate-benchmarks.py` for automated aggregation. The benchmark tracks:
- Pass rates (with skill vs without)
- Time and token deltas
- Consistency (standard deviation)

**Save results as `benchmark.json`** in the iteration directory.

### The Delta

The delta tells you what the skill costs and what it buys:

```
Pass rate: +50 percentage points (83% vs 33%)
Time: +13 seconds per task
Tokens: +1,700 per task
```

**Interpretation:**
- +50% pass rate for +13s and +1700 tokens = probably worth it
- +2% pass rate for 2x tokens = probably not worth it

### Standard Deviation

Standard deviation (`stddev`) indicates consistency.

| stddev Range | Interpretation | Action |
|--------------|----------------|--------|
| **0-10%** | Excellent consistency | None needed |
| **10-20%** | Good consistency | Monitor |
| **20-30%** | Moderate variance | Investigate |
| **30-50%** | High variance | Fix required |
| **>50%** | Extreme variance | Critical issue |

**Common causes of high stddev:**
- Ambiguous instructions (agent interprets differently each run)
- Flaky assertions (timing-dependent criteria)
- Model sensitivity (relies on reasoning vs explicit steps)
- Edge case sensitivity (unhandled edge cases)

**Alert thresholds:**
- 🟢 < 20%: Consistent
- 🟡 20-30%: Monitor
- 🔴 > 30%: Requires attention

See `references/stddev-guide.md` for detailed troubleshooting guide with diagnosis steps and solutions for each cause.

## Analyzing Patterns

Aggregate statistics hide important patterns. After computing benchmarks:

### Key Patterns to Check

**1. Always-pass assertions**
- Pass in both with-skill and without-skill configurations
- Provide no signal about skill value
- Action: Remove from evals

**2. Always-fail assertions**  
- Fail in both configurations
- Indicates broken assertion, too-hard test case, or wrong criteria
- Action: Fix assertion or test case

**3. Skill-added value**
- Pass with skill, fail without
- Shows where skill genuinely helps
- Action: Double down on these patterns

**4. High variance (high stddev)**
- Same eval passes sometimes, fails others
- Indicates ambiguous instructions or flaky eval
- Action: Add examples, tighten guidance

**5. Time/token outliers**
- One eval takes 3x longer than others
- Action: Read execution transcript for bottlenecks

See `references/iteration-strategies.md` for detailed pattern analysis guidance.

## Human Review

Assertions catch what you thought to assert; humans catch what you didn't:

**Humans find:**
- Unanticipated issues
- Technically correct but misses the point
- Holistic quality (organization, polish)
- Problems hard to express as pass/fail

**Recording feedback:**
```json
{
  "eval-top-months-chart": "Chart missing axis labels, months in wrong order (alphabetical not chronological)",
  "eval-clean-missing-emails": ""
}
```

**Good feedback:** Specific, actionable, references actual output
**Empty feedback:** Test case passed review

## Iterating on the Skill

Three sources of signal for improvement:

1. **Failed assertions** → Missing steps, unclear instructions, unhandled cases
2. **Human feedback** → Wrong approach, poor structure, quality issues  
3. **Execution transcripts** → Ignored instructions, unproductive steps, repeated work

### Iteration Loop

1. **Analyze** failed assertions, feedback, transcripts
2. **Revise** SKILL.md (use LLM to help generalize fixes)
3. **Re-run** evals in new `iteration-N+1/` directory
4. **Stop** when satisfied, feedback is empty, or plateaued

See `references/iteration-strategies.md` for detailed revision strategies and LLM prompts.

## Best Practices

1. **Start small**: 2-3 test cases, expand as needed
2. **Baseline comparison**: Always run with AND without skill
3. **Diverse prompts**: Vary phrasing, detail, complexity
4. **Edge cases**: Include at least one boundary test
5. **Concrete evidence**: Require it for all passes
6. **Review assertions**: Check they're not too easy/hard
7. **Human review**: Always do it, even with good assertion coverage
8. **Track timing**: Understand cost vs. benefit
9. **Iteration discipline**: Use systematic approach, don't wing it
10. **Generalize**: Fixes should help across many prompts

## Integration with Other Skills

**After creating a skill with `create-agent-skill`:**
1. Optimize description with `optimize-skill-description`
2. Evaluate quality with `evaluate-skill-quality`
3. Iterate between both until satisfied

## Scripts Reference

```bash
# Setup workspace
python scripts/setup-eval-workspace.py --skill ./my-skill --workspace ./my-workspace --iteration 1

# Grade assertions
python scripts/grade-assertions.py --outputs ./workspace/iteration-1/eval-1/with_skill/outputs/ --evals ./my-skill/evals/evals.json --eval-id 1 --output grading.json

# Aggregate benchmarks  
python scripts/aggregate-benchmarks.py --workspace ./my-workspace --iteration 1 --output benchmark.json

# Compare versions
python scripts/compare-versions.py --version1 ./workspace/iteration-1/ --version2 ./workspace/iteration-2/ --output comparison.md
```

## Resources

- `references/eval-design-guide.md` — Detailed test case design
- `references/assertion-writing-guide.md` — Crafting effective assertions
- `references/grading-criteria.md` — Principles for grading
- `references/iteration-strategies.md` — Systematic iteration approaches
