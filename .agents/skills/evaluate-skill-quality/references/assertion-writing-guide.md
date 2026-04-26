---
title: Assertion Writing Guide
---

# Writing Effective Assertions

Assertions are the foundation of skill evaluation. Well-written assertions catch real problems while avoiding false positives. Poorly written assertions waste time or miss critical issues.

## What Makes a Good Assertion

### The Five Characteristics

| Characteristic | Description | Example |
|----------------|-------------|---------|
| **Specific** | Precisely defines what to check | `"The chart has 3 bars"` not `"The chart looks good"` |
| **Observable** | Can be seen/measured in output | `"File exists"` not `"Quality is high"` |
| **Verifiable** | Clear pass/fail criteria | `"Count == 3"` not `"Count seems reasonable"` |
| **Countable** | Uses numbers when possible | `"At least 5 items"` not `"Multiple items"` |
| **Resilient** | Allows reasonable variation | `"Contains chart"` not `"Named exactly 'chart.png'"` |

## Assertion Types

### 1. Existence Assertions

Check that files or content exist.

**Basic:**
```json
"The outputs directory exists"
"The output file was created"
"A log file was generated"
```

**Specific:**
```json
"The outputs directory contains at least 1 file"
"The main output file has a .md extension"
"Exactly 3 supporting files were created"
```

**Graded:**
```json
"CRITICAL: The main output file exists"
"OPTIONAL: A debug log was generated"
```

### 2. Content Assertions

Check what's in the output.

**Structure:**
```json
"The markdown has a title (H1 header)"
"The report has exactly 3 H2 sections"
"The JSON has a 'results' top-level key"
"All required fields are present in the output"
```

**Values:**
```json
"The summary contains the total revenue amount"
"All monetary values are formatted with $ and 2 decimal places"
"The date range in the output matches the input file"
```

**References:**
```json
"All file paths in the output use relative paths (not absolute)"
"The output references the template from assets/template.md"
"No PII (names, emails, phone numbers) appears in the output"
```

### 3. Format Assertions

Check output format compliance.

**Syntax:**
```json
"The output is valid JSON (parses without errors)"
"The markdown uses proper header hierarchy (no skipping levels)"
"The CSV has the same number of columns in every row"
"The Python code is syntactically valid"
```

**Standards:**
```json
"The JSON follows the schema in assets/schema.json"
"The markdown follows the template in assets/template.md"
"The CSV uses RFC 4180 format (quoted fields with commas)"
```

**Encoding:**
```json
"The output file is UTF-8 encoded"
"No invalid characters appear in the output"
```

### 4. Quality Assertions

Check output quality characteristics.

**Completeness:**
```json
"The report includes at least 3 specific recommendations"
"All data points from the input appear in the output"
"The summary covers all major sections of the analysis"
```

**Accuracy:**
```json
"The calculated total matches the sum of individual items"
"The chart data matches the input CSV data"
"All links in the output are valid (return 200 OK)"
```

**Clarity:**
```json
"All acronyms are defined on first use"
"The summary is under 500 words"
"Code examples include comments explaining key steps"
```

### 5. Behavioral Assertions

Check how the skill behaved.

**Process:**
```json
"The skill read the input file before processing"
"The skill ran validation before producing output"
"The skill checked for errors and handled them gracefully"
```

**Safety:**
```json
"No destructive operations were performed on input files"
"The skill created backup files before modifying data"
"Temporary files were cleaned up after use"
```

**Efficiency:**
```json
"The skill completed in under 30 seconds"
"The skill used under 5000 tokens"
"No redundant operations were performed"
```

## Writing Assertions by Skill Type

### Data Processing Skills

```json
{
  "assertions": [
    "The output file format matches the requested format",
    "All input rows appear in the output (no data loss)",
    "Calculated columns have correct values (verified by spot check)",
    "The output has exactly the requested columns",
    "No null values appear in required fields",
    "Date formats are consistent and parseable"
  ]
}
```

### Code Generation Skills

```json
{
  "assertions": [
    "The generated code is syntactically valid",
    "The code includes error handling for edge cases",
    "All function parameters are documented",
    "The code follows the project's style conventions",
    "The code handles the specific input case correctly",
    "No hardcoded values that should be configurable"
  ]
}
```

### Visualization Skills

```json
{
  "assertions": [
    "A chart image file was generated",
    "The chart shows the correct number of data points",
    "Axes are labeled with correct units",
    "The chart title describes the content",
    "Data values are visually distinguishable",
    "The chart type matches the data (e.g., line for time series)"
  ]
}
```

### Documentation Skills

```json
{
  "assertions": [
    "The output follows the requested template structure",
    "All required sections are present",
    "Code examples are syntactically valid",
    "All links are valid and point to existing resources",
    "Technical terms are defined or linked",
    "The document is self-contained (can be understood without external context)"
  ]
}
```

### API Integration Skills

```json
{
  "assertions": [
    "API calls use correct endpoints",
    "Authentication headers are present",
    "Request payloads match the API schema",
    "Error responses are handled gracefully",
    "Response data is correctly parsed",
    "Rate limiting is respected"
  ]
}
```

## Common Assertion Patterns

### Range Checks

```json
"The value is between 10 and 100",
"The output contains at least 3 but no more than 10 items",
"The file size is under 1MB"
```

### Inclusion Checks

```json
"The output contains all of: summary, details, recommendations",
"The code uses the required libraries: requests, pandas",
"The report mentions each of the input files"
```

### Exclusion Checks

```json
"The output does not contain placeholder text like 'TODO' or 'FIXME'",
"No absolute file paths from the development environment appear",
"The output does not include PII (check for names, emails, phones)"
```

### Relationship Checks

```json
"The total equals the sum of line items",
"The end date is after the start date",
"The percentage values sum to 100% (within rounding)"
```

### Conditional Checks

```json
"IF the input has more than 100 rows, THEN a summary is provided",
"IF errors occurred, THEN an error log was generated",
"IF the output is JSON, THEN it validates against the schema"
```

## Grading Principles

### 1. Require Concrete Evidence

Don't pass based on assumption:

```
Assertion: "The summary section exists"
❌ PASS: "There's a section titled 'Summary'"
✅ PASS: "Section titled 'Summary' contains 3 paragraphs describing key findings"
```

### 2. Distinguish Label from Substance

```
Assertion: "The report includes recommendations"
❌ PASS: "There's a heading 'Recommendations' with empty list"
✅ PASS: "Section includes 3 specific, actionable recommendations with supporting rationale"
```

### 3. Allow Reasonable Variation

Don't be too rigid:

```
Assertion: "The chart has a title"
✅ PASS: Title is present, content reasonably describes the data
❌ FAIL: Title doesn't match expected text exactly
```

### 4. Weight by Criticality

```json
{
  "assertions": [
    { "text": "The output file exists", "critical": true },
    { "text": "The output is valid JSON", "critical": true },
    { "text": "The JSON has pretty formatting", "critical": false },
    { "text": "The output includes helpful comments", "critical": false }
  ]
}
```

## Grading Methods

### Automated Script Grading

For objective, mechanical checks:

```python
def grade_assertion(assertion, output_dir):
    if assertion == "output file exists":
        return Path(output_dir / "output.json").exists()
    
    elif assertion == "valid JSON":
        try:
            json.loads(Path(output_dir / "output.json").read_text())
            return True
        except:
            return False
    
    elif assertion == "has 3 items":
        data = json.loads(Path(output_dir / "output.json").read_text())
        return len(data.get("items", [])) == 3
```

**Pros:** Fast, consistent, no bias
**Cons:** Can't handle subjective qualities

### LLM-Based Grading

For subjective or complex checks:

```
Given:
- Assertion: "The recommendations are specific and actionable"
- Output: [full output content]

Evaluate: Does the output meet this criterion?
Respond with:
- PASS or FAIL
- Evidence: Quote specific parts that support your evaluation
- Reasoning: Explain your decision
```

**Pros:** Handles nuance, subjective quality
**Cons:** Slower, potential inconsistency, cost

### Human Grading

For final validation:

1. Review output alongside assertion
2. Make judgment call
3. Record specific evidence
4. Note borderline cases for assertion refinement

**Pros:** Most accurate, catches unexpected issues
**Cons:** Slowest, least scalable

### Hybrid Approach

Recommended workflow:

1. **Scripts** for objective checks (file existence, valid JSON, counts)
2. **LLM** for moderate complexity (structure compliance, content presence)
3. **Human** for final review and edge cases

## Avoiding Common Mistakes

### Too Vague

```json
// Bad
"The output is good"
"The quality is acceptable"
"It works"
```

### Too Brittle

```json
// Bad
"The output contains exactly the string 'Total: $1,234.56'"
"The code uses exactly 4 spaces for indentation"
"The chart has exactly RGB(255,0,0) for the first bar"
```

### Too Easy

```json
// Bad - always pass, add no signal
"The output is not empty"
"The skill completed without error"
"Something was generated"
```

### Too Hard

```json
// Bad - impossible to achieve
"The code has zero bugs"
"The output is perfect"
"The solution is optimal in all dimensions"
```

### Unverifiable

```json
// Bad - can't objectively check
"The user would be satisfied"
"The approach is best practice"
"The quality is high"
```

## Assertion Iteration

As you evaluate, refine your assertions:

### Iteration 1: Basic

```json
"The output is a bar chart"
```

### Iteration 2: More Specific

```json
"The output includes a bar chart image file"
"The chart has 3 bars"
```

### Iteration 3: Comprehensive

```json
"The output includes a bar chart image file (PNG or PDF)",
"The chart shows exactly 3 bars for the top months",
"Both axes are labeled (X: months, Y: revenue)",
"The chart has a title mentioning revenue",
"The bars are distinguishable and labeled"
```

### Iteration 4: Balanced

Remove assertions that always pass, split ones that are too broad:

```json
"The output includes a bar chart image file",
"The chart shows exactly 3 months (verified by OCR or metadata)",
"Both axes have labels (not just titles)",
"The chart title or caption mentions 'revenue' or 'sales'"
```

## Summary

Good assertions are:
- **Specific** — Precisely defined criteria
- **Observable** — Can be checked from output
- **Verifiable** — Clear pass/fail
- **Countable** — Use numbers when possible
- **Resilient** — Allow reasonable variation

Grade with:
- **Concrete evidence** — No assumptions
- **Substance over labels** — Check content, not just headings
- **Appropriate methods** — Scripts for objective, LLM for subjective
- **Continuous refinement** — Iterate based on results
