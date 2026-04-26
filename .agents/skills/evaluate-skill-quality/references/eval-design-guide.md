---
title: Evaluation Design Guide
---

# Designing Effective Skill Evaluations

This guide covers how to design test cases that effectively measure skill quality and guide improvement.

## The Purpose of Evaluations

Evaluations answer three questions:

1. **Does it work reliably?** — Consistency across varied prompts
2. **Does it handle edge cases?** — Behavior at boundaries
3. **Is it better than alternatives?** — Comparison with baseline or previous versions

## Test Case Components

### 1. Prompt

The user message that initiates the task.

**Characteristics of good prompts:**
- Realistic: Someone might actually say this
- Specific: Clear what success looks like
- Varied: Different from other test cases
- Contextual: Includes realistic details

**Example prompts:**

```json
// Good: Specific, realistic, includes context
"I have a CSV at ~/data/q4_sales.csv with columns Date, Revenue, Region. Can you find the top 3 regions by revenue and make a bar chart?"

// Good: Casual phrasing, abbreviated
"hey can u look at this csv for me and tell me whats in it? its in my downloads"

// Good: Complex, multi-step
"Download the sales data from the API, clean up any missing values, calculate monthly totals, and email me a summary with the top and bottom performing months"

// Weak: Too vague
"Process this file"

// Weak: Too formal, unrealistic
"Please execute the data processing workflow on the attached file"
```

### Prompt Design Dimensions

Vary your prompts along these axes:

**Phrasing style:**
- Formal: "Please analyze the quarterly sales data..."
- Casual: "hey can u look at this csv for me"
- Abbreviated: "anlz this data pls"
- Precise: "Parse the CSV at data/input.csv, drop rows where column B is null..."

**Detail level:**
- Terse: "analyze my sales CSV"
- Moderate: "analyze this CSV and make a chart"
- Detailed: "I have ~/data/q4_results.csv with columns Revenue, Expenses, Date..."

**Complexity:**
- Single-step: "Read this CSV"
- Multi-step: "Read this CSV, filter it, sort it, and save the result"
- Workflow: "Download, clean, analyze, visualize, and report"

**Context:**
- Minimal: "work with this file"
- Personal: "my manager asked me to..."
- Technical: "the file uses UTF-16 encoding..."

### 2. Expected Output

Human-readable description of what success looks like.

**Purpose:**
- Guides assertion writing
- Sets standard for human review
- Documents intent for future reference

**Example:**

```json
"A bar chart image file (PNG or PDF) showing exactly 3 bars for the top regions. The chart should have labeled X-axis (regions), labeled Y-axis (revenue), a title mentioning revenue, and bar values displayed or easily readable."
```

**Tips:**
- Be specific about format, structure, content
- Include quantitative criteria where possible
- Describe both what should be present AND what shouldn't

### 3. Input Files

Test data files the skill needs.

**Storage:**
```
skill-name/
└── evals/
    └── files/
        ├── sales_2025.csv
        ├── malformed.json
        └── large_dataset.csv
```

**File design principles:**

**Realistic data:**
- Use realistic values, not lorem ipsum
- Include edge cases (empty values, special characters)
- Match scale to real use cases

**Test coverage:**
- Normal cases: Typical, clean data
- Edge cases: Empty files, single row, very large files
- Error cases: Malformed data, wrong format, encoding issues
- Boundary cases: Exactly at limits (max length, max size)

**Minimal size:**
- Keep files as small as possible while preserving test value
- Use sample datasets, not full production data
- Compress if needed

### 4. Assertions

Verifiable statements about the output.

**Timing:**
- Write initial expected output before first run
- Add specific assertions AFTER seeing first output
- Iterate on assertions as you understand "good" better

**Good assertion characteristics:**

| Characteristic | Example |
|----------------|---------|
| **Specific** | `"The chart has labeled axes"` not `"The chart is good"` |
| **Observable** | Can be checked from the output |
| **Verifiable** | Clear pass/fail criteria |
| **Countable** | `"at least 3 findings"` not `"some findings"` |
| **Resilient** | Allows reasonable variation |

**Assertion types:**

**File-based:**
```json
"The outputs directory contains exactly 3 files"
"The main output file is named report.md"
"The output file is under 100KB"
```

**Content-based:**
```json
"The output contains a section titled 'Summary'"
"The JSON has a 'results' array with at least one element"
"All file paths in the output use relative paths"
```

**Format-based:**
```json
"The output is valid JSON"
"The markdown uses proper header hierarchy (#, ##, ###)"
"The CSV has headers and at least 2 data rows"
```

**Quality-based:**
```json
"The report includes at least 3 specific recommendations"
"All code examples in the output are syntactically valid"
"The summary is under 500 words"
```

## Eval Design Process

### Step 1: Define Success Criteria

Before writing test cases, ask:
- What does "good" mean for this skill?
- What are the most important qualities?
- What would indicate total failure?
- What are common mistakes to catch?

### Step 2: Design Test Cases

Create 2-3 initial test cases covering:
- Primary use case (happy path)
- Edge case (boundary condition)
- Error case (how it handles problems)

**Example for CSV analyzer:**

```json
{
  "evals": [
    {
      "id": 1,
      "name": "happy-path-analysis",
      "prompt": "Analyze this CSV of sales data and tell me the top 3 products",
      "expected_output": "A summary with top 3 products by sales volume, including product names and numbers"
    },
    {
      "id": 2,
      "name": "empty-file-edge-case",
      "prompt": "What happens if you try to analyze this empty CSV?",
      "expected_output": "Graceful handling with clear error message explaining the file is empty"
    },
    {
      "id": 3,
      "name": "malformed-data-error",
      "prompt": "Can you clean up this CSV with mixed formats in the date column?",
      "expected_output": "Either successfully normalizes dates OR reports which rows couldn't be parsed"
    }
  ]
}
```

### Step 3: Run and Observe

Run the skill on each test case:
- Observe actual behavior
- Note unexpected outcomes
- Document execution path
- Capture outputs

### Step 4: Write Assertions

Based on observed outputs, write specific assertions:

```json
{
  "id": 1,
  "assertions": [
    "The output lists exactly 3 products",
    "Each product has a name and sales number",
    "The products are ordered by sales (highest first)",
    "The total adds up correctly"
  ]
}
```

### Step 5: Expand Test Set

Add more test cases to cover:
- Different input formats
- Various user phrasings
- Complexity variations
- Error conditions

Aim for 5-10 test cases for thorough coverage.

## Test Case Categories

### Happy Path (40% of tests)

Normal, expected usage:

```json
{
  "prompt": "Analyze this standard CSV file with sales data",
  "expected": "Correct analysis with proper formatting"
}
```

### Edge Cases (30% of tests)

Boundary conditions:

```json
{
  "prompt": "Analyze this CSV with only one row",
  "expected": "Handles gracefully without errors"
}
```

### Error Handling (20% of tests)

Problem scenarios:

```json
{
  "prompt": "Analyze this corrupted file",
  "expected": "Clear error message, no crash"
}
```

### Complex Scenarios (10% of tests)

Multi-step or ambiguous:

```json
{
  "prompt": "Download the API data, clean it, analyze it, and visualize it",
  "expected": "Complete workflow executed successfully"
}
```

## Avoiding Common Mistakes

### Too Many Trivial Assertions

```json
// Bad: Always pass, add no value
"The output is not empty"
"The skill ran without crashing"
"There is some output"
```

### Too Many Brittle Assertions

```json
// Bad: Too specific, fail on reasonable variations
"The output contains exactly the phrase 'Total Revenue: $1,234.56'"
"The code uses exactly 4 spaces for indentation"
"The chart has exactly 3 colors: red, blue, green"
```

### Missing Critical Checks

```json
// Bad: Doesn't check the actual value
"The output includes a number"
// Good: Checks the meaning
"The output includes the total revenue calculated correctly"
```

### Unverifiable Assertions

```json
// Bad: Can't objectively verify
"The output is well-written"
"The approach is optimal"
"The user would be satisfied"
```

## Test Data Management

### Creating Test Data

**Synthetic data:**
```python
import pandas as pd
import numpy as np

df = pd.DataFrame({
    'date': pd.date_range('2025-01-01', periods=100),
    'revenue': np.random.normal(1000, 200, 100),
    'region': np.random.choice(['North', 'South', 'East', 'West'], 100)
})
df.to_csv('evals/files/sales_2025.csv', index=False)
```

**Anonymized real data:**
- Remove PII
- Shuffle/scramble identifiers
- Adjust values to hide proprietary info
- Keep structure and patterns

### Version Control for Test Data

```bash
# Large files: use Git LFS
git lfs track "evals/files/*.csv"

# Or generate on demand
python scripts/generate-test-data.py --output evals/files/
```

### Reproducibility

Fix random seeds for reproducible test data:

```python
np.random.seed(42)
random.seed(42)
```

## Baseline Comparison

Always compare against:

1. **Without skill**: Agent's baseline performance
2. **Previous version**: Earlier skill version
3. **Alternative approach**: Different skill or method

**Example comparison:**

```
Test case: Generate a chart from CSV data

Without skill:
- Pass rate: 40%
- Time: 30 seconds
- Tokens: 2000

With skill v1:
- Pass rate: 65%
- Time: 35 seconds  
- Tokens: 2800

With skill v2:
- Pass rate: 85%
- Time: 32 seconds
- Tokens: 2500

Conclusion: v2 significantly better than baseline, modest improvement over v1
```

## Evaluation Frequency

### During Development

- Run evals after each significant change
- Quick sanity check: 2-3 test cases
- Full eval: all test cases

### Before Release

- Complete eval suite (all test cases, multiple runs)
- Comparison with previous version
- Documentation of known limitations

### Regression Testing

- Quick eval when dependencies change
- Full eval before major releases
- Monitor for unexpected changes

## Summary

Good evaluations:
- Have realistic, varied prompts
- Include clear expected outputs
- Use minimal but representative test data
- Contain specific, verifiable assertions
- Compare against baselines
- Cover happy paths, edge cases, and errors
- Guide iteration with actionable feedback
