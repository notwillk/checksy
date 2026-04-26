---
title: Query Design Guide
---

# Designing Effective Eval Queries

This guide covers how to design test queries that effectively evaluate and improve your skill descriptions.

## What Are Eval Queries?

Eval queries are realistic user prompts with labels indicating whether your skill should activate for them:

```json
{
  "query": "Can you analyze this CSV file for me?",
  "should_trigger": true
}
```

## Designing Should-Trigger Queries

These test whether your description captures the skill's scope.

### Vary Along These Axes:

#### 1. Phrasing Variation

Test formal, casual, and typo variations:

```json
{ "query": "Please analyze the quarterly sales CSV file", "should_trigger": true }
{ "query": "hey can u look at this csv for me", "should_trigger": true }
{ "query": "Analyse this spreadsheet pls", "should_trigger": true }
{ "query": "Process this data file", "should_trigger": true }
```

#### 2. Explicitness

Some should name the domain directly, others should describe the need:

```json
// Explicit - names CSV directly
{ "query": "Analyze my sales.csv", "should_trigger": true }

// Implicit - describes need without naming CSV
{ "query": "My manager wants a chart from this data", "should_trigger": true }
```

#### 3. Detail Level

Mix terse and context-heavy prompts:

```json
// Terse
{ "query": "analyze my sales CSV", "should_trigger": true }

// Context-heavy
{ "query": "I've got a file in ~/data/q4_results.xlsx with revenue in column C and expenses in column D — can you add a profit margin column and highlight anything under 10%?", "should_trigger": true }
```

#### 4. Complexity

Vary the number of steps:

```json
// Single-step
{ "query": "Read this CSV", "should_trigger": true }

// Multi-step workflow
{ "query": "Download the data from the API, clean it up, calculate some stats, and email me a summary", "should_trigger": true }
```

### High-Value Should-Trigger Queries

The most valuable queries are ones where:
- The skill would help
- The connection isn't obvious from the query alone
- The query uses different terminology than the skill

**Example:**

```json
{ "query": "My boss wants to see trends in this spreadsheet from last quarter", "should_trigger": true }
```

This tests whether "spreadsheet" and "trends" activate a CSV skill.

## Designing Should-Not-Trigger Queries

These test whether the description is precise, not just broad.

### Focus on Near-Misses

Design queries that share keywords or concepts but actually need different capabilities:

**For a CSV analysis skill:**

```json
// Shares "spreadsheet" concept but needs Excel editing
{ "query": "Update the formulas in my Excel budget spreadsheet", "should_trigger": false }

// Shares "CSV" keyword but needs ETL, not analysis
{ "query": "Write a Python script that reads a CSV and uploads each row to our Postgres database", "should_trigger": false }

// Shares "data" concept but needs visualization, not analysis
{ "query": "Create a dashboard for our sales data", "should_trigger": false }
```

### Avoid Weak Negatives

Don't use obviously unrelated queries:

```json
// Weak - tests nothing useful
{ "query": "Write a fibonacci function", "should_trigger": false }
{ "query": "What's the weather today?", "should_trigger": false }

// Strong - tests boundary
{ "query": "Convert this CSV to JSON format", "should_trigger": false }
```

### Design by Adjacent Domains

Identify domains similar to yours and create queries that could be confused:

**If your skill: Analyzes CSV files**

Adjacent domains:
- Excel file editing
- JSON data processing
- Database querying
- PDF table extraction
- Data visualization

**Queries for adjacent domains:**

```json
{ "query": "Edit the formatting in this Excel file", "should_trigger": false }
{ "query": "Parse this JSON and validate the schema", "should_trigger": false }
{ "query": "Query the customer database for active users", "should_trigger": false }
{ "query": "Extract the table from this PDF", "should_trigger": false }
{ "query": "Create a dashboard with charts", "should_trigger": false }
```

## Tips for Realism

### Include Real-World Context

```json
{ 
  "query": "Can you analyze the file in ~/Downloads/sales_q4_final_v2_ACTUAL.csv? The revenue column is named 'Rev_AMT' and I need to find which regions are underperforming.",
  "should_trigger": true
}
```

Elements that add realism:
- **File paths**: `~/Downloads/report_final_v2.xlsx`
- **Personal context**: `"my manager asked me to..."`
- **Specific details**: Column names, company names, data values
- **Version numbers**: `v2`, `FINAL`, `ACTUAL`
- **Abbreviations**: `Rev_AMT`, `q4`, `csv`

### Use Casual Language

```json
{ "query": "hey can u help with this csv thing? need to like get some stats from it", "should_trigger": true }
```

Include:
- Lowercase
- Abbreviations ("u" for "you")
- Informal phrasing ("like", "thing")
- Missing punctuation

### Include Typos

```json
{ "query": "anlyze this spredsheet for me pls", "should_trigger": true }
```

Common patterns:
- Missing letters ("anlyze")
- Transposed letters ("spredsheet")
- Wrong but close words

### Vary Request Directness

```json
// Direct request
{ "query": "Analyze this CSV and compute statistics", "should_trigger": true }

// Indirect need
{ "query": "I have this data file and need to understand what's in it", "should_trigger": true }

// Goal-oriented
{ "query": "I need to present findings from this dataset to my team", "should_trigger": true }
```

## Query Structure Template

For a skill with N primary capabilities, design queries like this:

```
Should-trigger (10 total):
  - 2 direct requests for each primary capability
  - 2 indirect/implicit requests
  - 1 complex multi-step workflow
  - 1 with extensive context (file paths, details)
  - 1 casual/abbreviated
  - 1 with typos

Should-not-trigger (10 total):
  - 2 from each adjacent domain
  - 2 that share keywords but different intent
  - 1 obviously unrelated (to test basic filtering)
```

## Example Set: CSV Analysis Skill

```json
{
  "queries": [
    // Should-trigger (10)
    { "query": "Analyze this CSV file", "should_trigger": true },
    { "query": "Compute statistics on my sales data", "should_trigger": true },
    { "query": "Generate a chart from this spreadsheet", "should_trigger": true },
    { "query": "Clean up messy data in this table", "should_trigger": true },
    { "query": "My boss wants to see trends in this data file", "should_trigger": true },
    { "query": "Add a calculated column to this Excel export", "should_trigger": true },
    { "query": "hey can u look at this csv for me and tell me whats in it", "should_trigger": true },
    { "query": "I have ~/data/q4_results.csv with columns Revenue, Expenses, Date — need profit margins and monthly breakdown", "should_trigger": true },
    { "query": "Download the API data, clean it, analyze it, and email me the top 10 findings", "should_trigger": true },
    { "query": "anlyze this spredsheet pls", "should_trigger": true },
    
    // Should-not-trigger (10)
    { "query": "Update formulas in my Excel budget spreadsheet", "should_trigger": false },
    { "query": "Parse this JSON configuration file", "should_trigger": false },
    { "query": "Query the customer database for active users", "should_trigger": false },
    { "query": "Extract tables from this PDF report", "should_trigger": false },
    { "query": "Create a dashboard with charts and graphs", "should_trigger": false },
    { "query": "Write a Python script that reads CSV and uploads to Postgres", "should_trigger": false },
    { "query": "Convert this CSV to JSON format", "should_trigger": false },
    { "query": "Merge multiple Excel files into one", "should_trigger": false },
    { "query": "Validate the schema of this YAML file", "should_trigger": false },
    { "query": "Write a fibonacci function in Python", "should_trigger": false }
  ]
}
```

## Evaluating Query Quality

Good queries are:
- **Realistic**: Someone might actually say this
- **Specific**: Clear about what should/shouldn't happen
- **Diverse**: Cover different angles and variations
- **Balanced**: Equal positive and negative examples
- **Challenging**: Include near-misses and edge cases

Poor queries are:
- **Vague**: "Process this" — process what how?
- **Unrealistic**: Too formal or too garbled
- **One-dimensional**: All similar phrasings
- **Unbalanced**: All positive or all negative
- **Too easy**: No boundary testing

## Iterating on Queries

As you test and optimize:

1. **Note which queries fail**
   - Are they poorly labeled?
   - Are they too hard/easy?
   - Do they reveal description problems?

2. **Replace weak queries**
   - Remove obviously unrelated negatives
   - Add more near-misses
   - Increase diversity

3. **Expand the set**
   - Start with 10-12 queries
   - Expand to 20 as you find edge cases
   - Keep the original set for consistency

## Integration with Optimization

See the `optimize-skill-description` skill for:
- Train/validation splits
- Running evaluations
- Analyzing results
- Iterating on descriptions

Your queries are the foundation — invest in making them good!
