---
title: Description Patterns
---

# Patterns for Writing Effective Descriptions

This reference provides proven patterns for writing skill descriptions that activate reliably on relevant tasks.

## The Description's Job

The `description` field is the primary mechanism for skill activation. Agents use it to decide whether to load your skill for a given task. A good description:

- Triggers when the skill would help
- Doesn't trigger when it wouldn't help
- Is specific enough to differentiate from adjacent skills
- Is broad enough to catch variations in user phrasing

## Pattern 1: The Standard Formula

The most reliable structure:

```
[What the skill does]. Use when [when to use it], even if [implicit triggers].
```

**Example:**
```yaml
description: >
  Analyze CSV and Excel files to compute statistics, add columns, 
  and generate charts. Use when the user has spreadsheet data to 
  explore or visualize, even if they don't explicitly mention 
  "CSV" or "spreadsheet".
```

## Pattern 2: The Capability List

List specific capabilities for precision:

```yaml
description: >
  [Skill name] provides: [capability 1], [capability 2], and [capability 3]. 
  Use when [primary trigger] or [secondary trigger], including cases 
  where [edge case trigger].
```

**Example:**
```yaml
description: >
  PDF Processor provides: text extraction, metadata reading, form filling, 
  and page merging. Use when handling PDF files or document conversion, 
  including cases where the user mentions "forms", "merging documents", 
  or "extracting text from files".
```

## Pattern 3: The Domain Expert

Position as domain expertise:

```yaml
description: >
  Specialized skill for [domain]. Handles: [task 1], [task 2], [task 3]. 
  Activate when the user works with [domain keywords] or needs [domain outcomes].
```

**Example:**
```yaml
description: >
  Specialized skill for database migrations. Handles: schema versioning, 
  data transformation, rollback procedures, and validation. Activate when 
  the user works with database changes, schema updates, or data migration.
```

## Pattern 4: The Task Identifier

Focus on the user's task rather than the skill's capabilities:

```yaml
description: >
  Use when the user wants to [achieve outcome]. Supports: [method 1], 
  [method 2], and [method 3]. Also applies when [related situation].
```

**Example:**
```yaml
description: >
  Use when the user wants to deploy code to production. Supports: 
  containerized deployments, serverless functions, and static hosting. 
  Also applies when the user mentions releases, rollouts, or going live.
```

## Anti-Patterns to Avoid

### 1. The Vague Statement

```yaml
# Bad
description: Helps with PDFs.

# Why it's bad
# - Doesn't say what it actually does
# - Doesn't say when to activate
# - Could trigger for any PDF mention, even unrelated
```

### 2. The Implementation Detail

```yaml
# Bad
description: This skill uses PyPDF2 to read files.

# Why it's bad
# - Focuses on how, not what
# - User doesn't care about implementation
# - Doesn't help agent decide when to use it
```

### 3. The Passive Description

```yaml
# Bad
description: This skill is for analyzing data.

# Why it's bad
# - Passive voice is less directive
# - Missing imperative "Use when..."
# - Weak activation signal
```

### 4. The Overly Narrow

```yaml
# Bad
description: Use when the user explicitly types "analyze my sales CSV".

# Why it's bad
# - Won't catch variations like "work with this spreadsheet"
# - Too specific to be useful
```

### 5. The Overly Broad

```yaml
# Bad
description: Use for any file operations.

# Why it's bad
# - Will trigger for unrelated file tasks
# - Creates false positives
# - May conflict with more specific skills
```

## Optimization Strategies

### Include Synonyms and Related Terms

Users describe the same thing differently:

```yaml
description: >
  Analyze CSV, TSV, spreadsheet, and Excel data. Use for data exploration, 
  table manipulation, or working with comma-separated files.
```

### Cover Implicit Contexts

Include cases where the user doesn't name the domain directly:

```yaml
description: >
  Process CSV files. Use when the user mentions data files, spreadsheets, 
  tables, or columns of data — even if they don't explicitly say "CSV".
```

### Differentiate from Adjacent Skills

If other skills handle similar domains, clarify boundaries:

```yaml
# For a CSV skill
description: >
  Read and analyze CSV files to extract insights. For Excel files specifically, 
  the excel-processor skill is preferred. Use this for raw CSV/TSV data.

# For an Excel skill  
description: >
  Process Excel files with formulas, formatting, and multiple sheets. 
  For simple CSV files, csv-analyzer is sufficient. Use this for 
  .xlsx files or complex spreadsheet operations.
```

### Use Pushy Language

Don't be shy about claiming relevance:

```yaml
description: >
  DEFINITELY use this skill when working with [domain]. This is the primary 
  skill for [tasks]. Also apply for [related contexts].
```

## Before and After Examples

### Example 1: PDF Processing

**Before:**
```yaml
description: Helps with PDFs.
```

**After:**
```yaml
description: >
  Extract text, fill forms, and merge PDF files. Use when handling PDF 
  documents, converting PDFs to other formats, or working with fillable 
  forms. Also applies when the user mentions "documents", "PDF files", 
  or "portable document format".
```

### Example 2: Code Review

**Before:**
```yaml
description: Does code reviews.
```

**After:**
```yaml
description: >
  Review code for security issues, style compliance, and best practices. 
  Use when the user asks for code review, PR review, security audit, or 
  wants feedback on their code. Includes checks for SQL injection, 
  authentication, and common vulnerabilities.
```

### Example 3: API Client

**Before:**
```yaml
description: Makes API calls.
```

**After:**
```yaml
description: >
  Interact with REST and GraphQL APIs. Handle authentication, request 
  building, response parsing, and error handling. Use when the user needs 
  to call external services, fetch data from APIs, or work with web services — 
  even if they describe it as "getting data from a website" or 
  "connecting to a service".
```

## Testing Your Description

After writing your description, test it mentally against these prompts:

**Should trigger:**
- "Can you [primary task]?"
- "I need help with [domain]"
- "Work with this [file type]"
- "Handle this [related concept]"

**Should NOT trigger:**
- "Do something completely unrelated"
- Tasks handled by other, more specific skills
- Vague requests that don't touch your domain

## The Iterative Process

Good descriptions rarely emerge on the first try. Plan to:

1. Write initial description
2. Test with real prompts (5-10 variations)
3. Note which prompts trigger correctly and which don't
4. Revise based on patterns in the results
5. Repeat until consistent activation

See the `optimize-skill-description` skill for systematic description optimization workflows.
