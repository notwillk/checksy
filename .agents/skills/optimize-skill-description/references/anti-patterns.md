---
title: Anti-Patterns
---

# Common Mistakes in Description Optimization

This reference documents anti-patterns — common mistakes that undermine description effectiveness — and how to avoid them.

## Anti-Pattern 1: The Vague Statement

### The Mistake

```yaml
description: Helps with PDFs.
```

### Why It's Wrong

- "Helps with" conveys no specific capabilities
- Doesn't specify when to activate
- Could trigger for any PDF mention (even "I hate PDFs")
- Wastes the agent's time loading a skill that might not help

### The Fix

```yaml
description: >
  Extract text, fill forms, and merge PDF files. Use when handling 
  PDF documents, converting PDFs to other formats, or working with 
  fillable forms.
```

### Detection

Your description might be too vague if:
- It uses words like "helps", "assists", "works with"
- It doesn't list specific capabilities
- It's under 30 characters
- You can't tell what the skill does from the description alone

## Anti-Pattern 2: Implementation Focus

### The Mistake

```yaml
description: This skill uses PyPDF2 and pdfplumber to read files.
```

### Why It's Wrong

- Users don't care about implementation libraries
- Doesn't help agent decide when to activate
- What if libraries change? Description becomes misleading
- Focuses on how, not what

### The Fix

```yaml
description: >
  Extract text, metadata, and tables from PDF files. Use when working 
  with PDF documents or extracting content from portable document format files.
```

### Detection

Your description focuses on implementation if:
- It mentions specific libraries or tools
- It uses phrases like "This skill uses..." or "Implements..."
- It describes technical details rather than user outcomes

## Anti-Pattern 3: Passive Voice

### The Mistake

```yaml
description: This skill is designed for CSV file processing.
```

### Why It's Wrong

- Passive voice is less directive to the agent
- Doesn't explicitly tell agent when to activate
- Weak activation signal

### The Fix

```yaml
description: >
  Process CSV files to compute statistics and generate charts. 
  Use when analyzing tabular data or working with comma-separated values.
```

### Detection

Your description uses passive voice if:
- It starts with "This skill..."
- It uses "is designed", "is used", "is intended"
- The main verb is weak or buried

## Anti-Pattern 4: Over-Specificity

### The Mistake

```yaml
description: Use when the user explicitly types "analyze my sales CSV".
```

### Why It's Wrong

- Won't catch variations like "work with this spreadsheet"
- Too rigid for real-world usage
- Fails on typos, abbreviations, and rephrasings

### The Fix

```yaml
description: >
  Analyze CSV, TSV, spreadsheet, and Excel data. Use when the user 
  wants to explore, transform, or visualize tabular data — even if 
  they describe it as "working with data files" or "making sense of 
  this spreadsheet".
```

### Detection

Your description is too specific if:
- It includes exact phrases users must type
- It only covers one phrasing variation
- It doesn't include synonyms or related terms

## Anti-Pattern 5: Over-Generalization

### The Mistake

```yaml
description: Use for any file operations.
```

### Why It's Wrong

- Will trigger for every file-related task
- Creates false positives
- May conflict with more specific skills
- Clutters context with irrelevant skills

### The Fix

```yaml
description: >
  Analyze CSV and tabular data files. Use specifically for data exploration, 
  transformation, and visualization of comma-separated and tab-separated files. 
  For other file types, use the appropriate specialized skill.
```

### Detection

Your description is too broad if:
- It covers domains way outside the skill's scope
- It uses words like "any", "all", "everything"
- It doesn't exclude clearly unrelated tasks

## Anti-Pattern 6: Keyword Stuffing

### The Mistake

```yaml
description: >
  CSV spreadsheet Excel table data analysis statistics chart graph 
  visualization processing cleaning transformation exploration tabular 
  comma-separated values pivot aggregation filtering
```

### Why It's Wrong

- Unreadable
- Doesn't convey coherent capabilities
- Looks like spam
- Still might miss important triggers

### The Fix

```yaml
description: >
  Analyze CSV and Excel files to compute statistics, add derived columns, 
  and generate charts. Use when working with tabular data that needs 
  exploration, transformation, or visualization.
```

### Detection

You're keyword stuffing if:
- Description is just a list of terms
- No coherent sentences
- No trigger guidance
- Over 200 characters of just keywords

## Anti-Pattern 7: Overfitting to Test Cases

### The Mistake

After testing with query "Can you work with this spreadsheet?", the description becomes:

```yaml
description: >
  Analyze CSV files. Use when the user asks to work with spreadsheets 
  or when they say "Can you work with this spreadsheet" or when they 
  have tabular data or when they mention files with columns.
```

### Why It's Wrong

- Optimized for one specific query
- Won't generalize to new variations
- Bloated with specific phrases
- Wastes characters on exact wording

### The Fix

Identify the general concept, not the specific wording:

```yaml
description: >
  Analyze CSV, TSV, spreadsheet, and Excel data. Use when working 
  with tabular data or data organized in rows and columns.
```

### Detection

You're overfitting if:
- You add exact phrases from failed test cases
- Description grows with each iteration
- Performance on new queries is worse than training queries
- Description looks like a concatenation of queries

### Prevention

- Use train/validation split
- Select best description by validation performance
- Generalize from specific failures
- Ask "what category does this query represent?"

## Anti-Pattern 8: Missing Implicit Triggers

### The Mistake

```yaml
description: >
  Analyze CSV files. Use when the user mentions CSV files.
```

### Why It's Wrong

- Users might say "spreadsheet" or "data file" instead of "CSV"
- Misses implicit contexts where skill would help
- Too literal

### The Fix

```yaml
description: >
  Analyze CSV, TSV, and spreadsheet data. Use when working with 
  tabular data files — even if the user doesn't explicitly mention 
  "CSV" and describes it as a "spreadsheet", "data file", or "table".
```

### Detection

You're missing implicit triggers if:
- Description only lists exact keywords
- No "even if" or "also applies when" clauses
- Assumes users will name the domain directly

## Anti-Pattern 9: No Boundary Definition

### The Mistake

```yaml
description: >
  Process Excel files. Use when working with spreadsheets.
```

For a skill that should handle CSV but NOT Excel:

### Why It's Wrong

- Overlaps with Excel-specific skills
- Doesn't clarify what it doesn't do
- Creates skill conflicts

### The Fix

```yaml
description: >
  Process CSV and TSV files. Use for raw comma-separated and tab-separated 
  data. For Excel files with formulas or formatting, use the 
  excel-processor skill instead.
```

### Detection

You lack boundary definition if:
- Similar skills exist in the same domain
- No mention of what the skill doesn't do
- No references to alternative skills

## Anti-Pattern 10: Bloated Description

### The Mistake

```yaml
description: >
  [800+ characters of exhaustive detail about every possible capability, 
   edge case, and variation, exceeding the 1024 limit]
```

### Why It's Wrong

- Exceeds 1024-character limit
- Bloats agent context
- Signal-to-noise ratio decreases
- May be truncated

### The Fix

Prioritize and be concise:

```yaml
description: >
  Analyze CSV and tabular data — compute statistics, add columns, 
  generate charts. Use when exploring, transforming, or visualizing 
  data in CSV, TSV, or Excel format.
```

### Detection

Your description is bloated if:
- Approaches or exceeds 1024 characters
- Lists every minor capability
- Includes redundant phrasings
- Could be cut by 50% without losing meaning

## Anti-Pattern 11: Missing "Use When"

### The Mistake

```yaml
description: >
  This skill provides comprehensive PDF processing capabilities including 
  text extraction, metadata reading, form filling, and document merging.
```

### Why It's Wrong

- No clear trigger condition
- Agent doesn't know when to activate
- Just a capability list without guidance

### The Fix

```yaml
description: >
  Extract text, metadata, and fill forms in PDF files. Use when handling 
  PDF documents, extracting content, or working with fillable forms.
```

### Detection

You're missing trigger guidance if:
- No "Use when", "Activate when", or "Apply when"
- Just a list of capabilities
- No connection to user intent

## Anti-Pattern 12: Ignoring the Train/Validation Split

### The Mistake

Optimizing description against all 20 queries without holding any back.

### Why It's Wrong

- Overfits to specific query phrasings
- Doesn't test generalization
- Can't detect if you're just memorizing queries

### The Fix

- Split queries: 60% train, 40% validation
- Optimize against train set only
- Select best description by validation performance
- Validation queries must not influence revisions

### Detection

You're not using proper split if:
- All queries influence every revision
- No held-out test set
- Performance claims based on training data only

## Summary Table

| Anti-Pattern | Symptom | Fix |
|-------------|---------|-----|
| Vague Statement | "Helps with..." | List specific capabilities |
| Implementation Focus | Mentions libraries | Describe outcomes, not tools |
| Passive Voice | "This skill is..." | Use imperative "Use when..." |
| Over-Specificity | Exact phrases required | Generalize with synonyms |
| Over-Generalization | "Any file operations" | Narrow scope, add constraints |
| Keyword Stuffing | List of terms | Coherent sentences |
| Overfitting | Adding exact query phrases | Use train/validation split |
| Missing Implicit Triggers | Only exact keywords | Add "even if" clauses |
| No Boundary Definition | Overlaps with other skills | Clarify exclusions |
| Bloated Description | Near 1024 char limit | Prioritize, be concise |
| Missing "Use When" | Just capabilities | Add trigger conditions |
| No Train/Validation Split | Optimize on all data | Split 60/40, select by validation |

## Avoiding Anti-Patterns

1. **Use the patterns in `description-patterns.md`** — Follow proven structures
2. **Test with diverse queries** — Catch overfitting early
3. **Use train/validation split** — Ensure generalization
4. **Review descriptions with fresh eyes** — "Would I know when to use this?"
5. **Keep it under 1024 characters** — Forces prioritization
6. **Focus on user intent** — What are they trying to achieve?

See the `optimize-skill-description` skill for systematic approaches to avoiding these pitfalls.
