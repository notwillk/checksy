---
title: Description Examples
---

# Before and After: Description Examples

Real examples of skill descriptions with explanations of what makes them effective or ineffective.

## Example 1: CSV/Spreadsheet Analysis

### Before

```yaml
description: Helps with CSV files.
```

**Problems:**
- "Helps with" is vague — what does "helps" mean?
- No specific capabilities listed
- Doesn't specify when to activate
- "CSV files" might not catch "spreadsheet" or "table"
- Passive voice instead of imperative

**Expected behavior:**
- ❌ Won't activate for "analyze this spreadsheet"
- ❌ Won't activate for "work with this data file"
- ❌ User won't know what to expect from the skill

### After

```yaml
description: >
  Analyze CSV, TSV, and Excel files to compute summary statistics, 
  add derived columns, generate charts (bar, line, scatter), and 
  clean messy data. Use when the user has tabular data and wants 
  to explore, transform, or visualize it — even if they don't 
  explicitly mention "CSV", "spreadsheet", or "table".
```

**Improvements:**
- Lists specific capabilities (statistics, columns, charts, cleaning)
- Includes multiple file types (CSV, TSV, Excel)
- Uses imperative phrasing ("Use when...")
- Explicit trigger conditions (tabular data, exploration, transformation)
- Implicit trigger coverage ("even if...")
- Action-oriented language

**Expected behavior:**
- ✅ Activates for "analyze this spreadsheet"
- ✅ Activates for "make a chart from this CSV"
- ✅ Activates for "clean up this data file"
- ✅ Activates for "compute statistics on my Excel file"
- ❌ Won't activate for "convert this PDF to text" (different domain)

## Example 2: PDF Processing

### Before

```yaml
description: This skill uses PyPDF2 to read PDF files.
```

**Problems:**
- Focuses on implementation (PyPDF2), not user intent
- "This skill uses..." is passive
- Doesn't say when to activate
- Doesn't list capabilities
- Assumes user knows what PyPDF2 does

**Expected behavior:**
- ❌ Won't reliably activate for relevant tasks
- ❌ User doesn't know what the skill actually does

### After

```yaml
description: >
  Extract text and metadata from PDF files, fill PDF forms, and 
  merge multiple PDFs into one. Use when handling PDF documents, 
  converting PDFs to other formats, or working with fillable forms. 
  Also applies when the user mentions "documents", "PDF files", or 
  "portable document format".
```

**Improvements:**
- Lists concrete capabilities (extract text, metadata, forms, merge)
- Uses imperative phrasing ("Use when...")
- Multiple trigger conditions (handling PDFs, converting, forms)
- Includes implicit triggers ("documents", "portable document format")
- No implementation details

## Example 3: Code Review

### Before

```yaml
description: Does code reviews.
```

**Problems:**
- "Does" is passive
- What kind of code review? What's checked?
- No trigger conditions
- Too vague to differentiate from other skills

### After

```yaml
description: >
  Review code for security issues, style compliance, and best practices. 
  Use when the user asks for code review, PR review, security audit, 
  or wants feedback on their code. Includes checks for SQL injection, 
  authentication gaps, and common vulnerabilities.
```

**Improvements:**
- Specific capabilities (security, style, best practices)
- Multiple trigger phrases (code review, PR review, security audit)
- Outcome-focused ("wants feedback")
- Lists specific checks for credibility

## Example 4: API Client

### Before

```yaml
description: Makes API calls.
```

**Problems:**
- "Makes" is weak
- What kind of APIs? REST? GraphQL? SOAP?
- What can it do? Just make calls, or also handle auth, parsing, etc.?
- Too broad — might trigger for any API mention

### After

```yaml
description: >
  Interact with REST and GraphQL APIs — handle authentication, 
  request building, response parsing, and error handling. Use when 
  the user needs to call external services, fetch data from APIs, 
  or work with web services — even if they describe it as "getting 
  data from a website" or "connecting to a service".
```

**Improvements:**
- Specific API types (REST, GraphQL)
- Complete workflow (auth, requests, parsing, errors)
- Multiple trigger contexts (external services, APIs, web services)
- Implicit triggers ("getting data from a website")
- Comprehensive capability list

## Example 5: Database Operations

### Before

```yaml
description: Works with databases.
```

**Problems:**
- "Works with" is vague
- What operations? Query? Migrate? Admin?
- What databases? SQL? NoSQL?
- Too broad, will overlap with many other skills

### After

```yaml
description: >
  Query SQL databases and generate reports. Use for data retrieval, 
  aggregation, and analysis from PostgreSQL, MySQL, or SQLite databases. 
  For database migrations or schema changes, use db-migration-skill instead.
```

**Improvements:**
- Specific operation (query and report)
- Specific database types (PostgreSQL, MySQL, SQLite)
- Clear scope (retrieval, aggregation, analysis)
- Explicitly excludes overlapping domain (migrations)
- Differentiates from adjacent skill

## Example 6: Git Operations

### Before

```yaml
description: Git commands.
```

**Problems:**
- Not a sentence
- What commands? All of them?
- No trigger conditions
- Too brief

### After

```yaml
description: >
  Execute git workflows including branching, merging, rebasing, and 
  history management. Use when the user needs git operations beyond 
  basic commit/push, such as resolving merge conflicts, cleaning up 
  history, or managing branches.
```

**Improvements:**
- Lists specific workflows
- Clear scope (beyond basic operations)
- Examples of trigger contexts
- Action-oriented language

## Example 7: Web Scraping

### Before

```yaml
description: Scrapes websites.
```

**Problems:**
- "Scrapes" might be unclear
- What can it extract? Text? Images? Data?
- What websites? Any? Specific types?
- No trigger guidance

### After

```yaml
description: >
  Extract structured data from HTML web pages. Handle pagination, 
  forms, and JavaScript-rendered content. Use when the user needs 
  to collect data from websites, extract tables from pages, or 
  gather information from multiple web pages.
```

**Improvements:**
- Specific extraction type (structured data from HTML)
- Advanced capabilities (pagination, forms, JS content)
- Multiple trigger contexts (collect data, extract tables, gather info)
- Clear use cases

## Example 8: Testing/QA

### Before

```yaml
description: Testing utilities.
```

**Problems:**
- "Utilities" is vague
- What kind of testing? Unit? Integration? E2E?
- What utilities? Frameworks? Tools?
- No activation guidance

### After

```yaml
description: >
  Create and run automated tests for software projects. Supports 
  unit tests, integration tests, and test data generation. Use 
  when the user wants to write tests, run test suites, improve 
  test coverage, or debug failing tests.
```

**Improvements:**
- Specific test types (unit, integration)
- Complete workflow (create and run)
- Multiple trigger conditions (write tests, run suites, coverage, debug)
- Additional capabilities (test data generation)

## Patterns from Examples

### Effective Descriptions Always Have:

1. **Specific capabilities** — What the skill actually does
2. **Imperative phrasing** — "Use when..." not "This skill does..."
3. **User intent focus** — Outcomes, not implementation
4. **Multiple triggers** — Various ways users might ask
5. **Implicit coverage** — "Even if they don't say..."
6. **Appropriate scope** — Neither too narrow nor too broad

### Ineffective Descriptions Often:

1. **Use passive voice** — "This skill helps..."
2. **Focus on implementation** — "Uses library X..."
3. **Are too vague** — "Helps with..."
4. **Are too narrow** — Only catch exact phrasings
5. **Are too broad** — Overlap with unrelated skills
6. **Lack trigger guidance** — Don't say when to activate

## Applying These Patterns

When writing your own descriptions:

1. List 5-10 specific capabilities
2. Write 5-10 ways users might ask for them
3. Draft description with imperative phrasing
4. Add implicit trigger coverage
5. Check boundaries with adjacent skills
6. Test with real prompts
7. Iterate based on results

See the `optimize-skill-description` skill for systematic testing and optimization workflows.
