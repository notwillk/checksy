---
title: Frontmatter Reference
---

# SKILL.md Frontmatter Reference

Complete documentation of all frontmatter fields in `SKILL.md`.

## Field Summary

| Field | Required | Type | Max Length | Description |
|-------|----------|------|------------|-------------|
| `name` | Yes | string | 64 chars | Unique skill identifier |
| `description` | Yes | string | 1024 chars | When to use the skill |
| `license` | No | string | - | License identifier or file reference |
| `compatibility` | No | string | 500 chars | Environment requirements |
| `metadata` | No | object | - | Arbitrary key-value pairs |
| `allowed-tools` | No | string | - | Pre-approved tools (experimental) |

## name

**Required** — The skill identifier.

### Constraints

- Must be 1-64 characters
- Only lowercase letters (`a-z`), numbers (`0-9`), and hyphens (`-`)
- Must not start or end with a hyphen
- Must not contain consecutive hyphens (`--`)
- Must match the parent directory name exactly

### Examples

**Valid:**
```yaml
name: pdf-processing
name: data-analysis
name: code-review
name: api-client-v2
name: my-skill-123
```

**Invalid:**
```yaml
name: PDF-Processing        # Uppercase not allowed
name: pdf_processing        # Underscores not allowed
name: pdf processing        # Spaces not allowed
name: -pdf-processing       # Cannot start with hyphen
name: pdf-processing-      # Cannot end with hyphen
name: pdf--processing       # Consecutive hyphens not allowed
name:                      # Cannot be empty
name: abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijkl # Too long (>64)
```

### Best Practices

- Use descriptive names that indicate the skill's purpose
- Prefer full words over abbreviations (`code-review` not `cr`)
- For versioned skills, use suffixes like `-v2` or `-v3`
- Use hyphens to separate words for readability

## description

**Required** — Determines when the skill activates.

### Constraints

- Must be 1-1024 characters
- Must be non-empty
- Should describe both what the skill does AND when to use it

### Writing Effective Descriptions

**Key principles:**

1. **Use imperative phrasing**: Frame as instructions to the agent
   - Good: "Use when...", "Activate when...", "Apply this skill when..."
   - Poor: "This skill does...", "This skill helps with..."

2. **Focus on user intent**: Describe what the user is trying to achieve
   - Good: "when the user wants to visualize data"
   - Poor: "implements matplotlib charts"

3. **Be specific about scope**: List concrete capabilities
   - Good: "compute summary statistics, add derived columns, generate charts"
   - Poor: "helps with CSV files"

4. **Include implicit triggers**: Cover cases where the user doesn't name the domain directly
   - Good: "even if they don't explicitly mention 'CSV' or 'analysis'"

5. **Err on the side of being pushy**: It's better to activate too often than miss relevant tasks

### Examples

**Excellent (detailed and specific):**
```yaml
description: >
  Analyze CSV, TSV, and Excel files to compute summary statistics, 
  add derived columns, generate charts (bar, line, scatter), and clean 
  messy data. Use when the user has tabular data and wants to explore, 
  transform, or visualize it — even if they don't explicitly mention 
  "CSV" or "analysis" or use terms like "spreadsheet", "data file", 
  or "table".
```

**Good (clear but brief):**
```yaml
description: >
  Extract text and metadata from PDF files, fill PDF forms, and merge 
  multiple PDFs. Use when handling PDF documents or when the user 
  mentions PDFs, forms, or document extraction.
```

**Poor (too vague):**
```yaml
description: Helps with PDFs.
```

**Poor (implementation-focused):**
```yaml
description: This skill uses PyPDF2 and pdfplumber to read PDF files.
```

## license

**Optional** — Specifies the license applied to the skill.

### Format

Can be either:
- A standard license identifier (e.g., `MIT`, `Apache-2.0`, `GPL-3.0`)
- A reference to a bundled license file (e.g., `See LICENSE.txt`)
- A proprietary notice

### Examples

```yaml
license: MIT
license: Apache-2.0
license: GPL-3.0-or-later
license: Proprietary. See LICENSE.txt for complete terms
license: BSD-3-Clause
```

### Best Practices

- Keep it short and clear
- If using a standard license, use the SPDX identifier
- For proprietary skills, reference a bundled file rather than including full text

## compatibility

**Optional** — Indicates environment requirements.

### Constraints

- Maximum 500 characters if provided
- Only include if your skill has specific requirements
- Most skills do NOT need this field

### Use Cases

Include when your skill requires:
- Specific system packages (git, docker, jq, etc.)
- Network access
- Specific programming language versions
- Particular agent products or versions

### Examples

```yaml
compatibility: Designed for Claude Code (or similar products)
compatibility: Requires git, docker, jq, and access to the internet
compatibility: Requires Python 3.14+ and uv
compatibility: Works best with Node.js 18+ and npm
compatibility: Requires AWS CLI configured with appropriate credentials
```

### Best Practices

- Be specific about versions when relevant
- List all major dependencies
- Indicate if network access is required
- Mention any authentication requirements

## metadata

**Optional** — Arbitrary key-value mapping for additional properties.

### Format

A YAML mapping where both keys and values are strings:

```yaml
metadata:
  key1: value1
  key2: value2
```

### Common Keys

| Key | Purpose | Example |
|-----|---------|---------|
| `author` | Creator of the skill | `jane-doe` |
| `version` | Skill version | `"1.0.0"` |
| `category` | Functional category | `data-processing` |
| `tags` | List of tags (comma-separated or as list) | `[csv, analysis]` |
| `created` | Creation date | `2025-01-15` |
| `updated` | Last update date | `2025-04-20` |

### Examples

```yaml
metadata:
  author: example-org
  version: "1.0.0"
  category: data-processing
  tags: [csv, analysis, visualization]
  created: "2025-01-15"
```

```yaml
metadata:
  author: jane-doe
  version: "2.1.0"
  original-author: john-smith
  forked-from: some-other-skill
```

### Best Practices

- Use reasonably unique key names to avoid conflicts
- Keep values as simple strings (avoid nested objects)
- Use semantic versioning for version field
- Clients may use this data for filtering, display, or organization

## allowed-tools

**Optional, Experimental** — Pre-approved tools the skill may use.

### Format

A space-separated string of tool specifications:

```yaml
allowed-tools: Bash(git:*) Bash(jq:*) Read Edit Write
```

### Syntax

- Simple tool name: `Read`, `Edit`, `Write`, `Glob`, `Grep`, etc.
- Tool with command restriction: `Bash(git:*)` — allows `git` commands only
- Tool with specific command: `Bash(jq:*)` — allows `jq` commands only

### Purpose

This field allows agents to pre-approve specific tools for a skill, potentially:
- Reducing permission prompts during execution
- Providing transparency about what the skill will do
- Enabling security reviews of skill capabilities

### Important Notes

- This field is **experimental** — support varies between agent implementations
- Not all clients respect this field
- Use it as documentation of intent, not as a security boundary

### Examples

```yaml
# Read-only skill
allowed-tools: Read Glob Grep

# Skill that edits files and runs git commands
allowed-tools: Read Edit Write Bash(git:*)

# Skill that processes JSON with jq
allowed-tools: Read Bash(jq:*) Bash(cat:*)

# Data processing skill
allowed-tools: Read Edit Write Bash(python3:*) Bash(jq:*)
```

## Complete Example

```yaml
---
name: comprehensive-skill
description: >
  Process PDF files to extract text, metadata, and images. Convert 
  PDFs to other formats (HTML, Markdown, text). Use when working with 
  PDF documents, even if the user just says "document" or "file" 
  without specifying PDF.
license: Apache-2.0
compatibility: Requires Python 3.9+ and poppler-utils
metadata:
  author: pdf-experts
  version: "2.0.0"
  category: document-processing
  tags: [pdf, extraction, conversion]
  updated: "2025-04-20"
allowed-tools: Read Edit Write Bash(python3:*) Bash(pdftotext:*) Bash(pdfimages:*)
---
```
