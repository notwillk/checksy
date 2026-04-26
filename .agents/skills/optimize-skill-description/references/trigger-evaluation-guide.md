---
title: Trigger Evaluation Guide
---

# Skill Triggering and Progressive Disclosure

Understanding how agents decide when to activate skills is essential for writing effective descriptions.

## Progressive Disclosure Explained

Agents load skills in three stages to manage context efficiently:

### Stage 1: Discovery (Startup)

**What happens:**
- Agent scans skill directories
- Loads only `name` and `description` fields
- Stores ~100 tokens per skill

**Purpose:**
- Know what skills are available
- Determine which might be relevant
- Keep startup fast and memory-efficient

**Example loaded data:**
```yaml
name: csv-analyzer
description: Analyze CSV files to compute statistics and generate charts.
```

### Stage 2: Activation (Task Matching)

**What happens:**
- User sends a task/prompt
- Agent compares task to all skill descriptions
- Matching skill: full `SKILL.md` is loaded
- Non-matching skills: stay at discovery level

**Purpose:**
- Pull in relevant skills only
- Avoid loading irrelevant context
- Keep active context focused

**How matching works:**
1. Agent analyzes user intent from the prompt
2. Compares to each skill's description
3. Decides whether the skill's capabilities would help
4. If yes: loads full instructions

### Stage 3: Execution (Running the Skill)

**What happens:**
- Agent follows `SKILL.md` instructions
- Loads referenced files from `references/`, `scripts/`, `assets/` as needed
- Executes commands or uses tools as instructed

**Purpose:**
- Get detailed instructions only when needed
- Load resources on demand
- Minimize upfront context

## The Description's Burden

The description at Stage 1 carries the entire burden of the activation decision. It must:

1. **Convey capabilities**: What the skill can do
2. **Specify triggers**: When it should be used
3. **Cover variations**: Synonyms, implicit contexts, near-misses
4. **Differentiate**: Not overlap too much with adjacent skills

**If the description fails:**
- The skill never activates, even when it would help
- Or it activates inappropriately, cluttering context

## When Skills Activate (And When They Don't)

### Skills Typically Activate When:

- Task requires specialized knowledge the agent lacks
- Domain-specific procedures or conventions
- Unfamiliar APIs, tools, or formats
- Project-specific context or constraints

**Examples:**
- User: "Analyze this CSV using our company conventions"
  - Skill activates: provides company-specific conventions
- User: "Read this PDF and extract text"
  - May NOT activate: agent can handle basic PDF reading without skill

### Skills Typically Don't Activate When:

- Task is simple and within agent's general capabilities
- No specialized knowledge is required
- Description doesn't clearly indicate relevance

**Examples:**
- User: "What is 2 + 2?"
  - No skill needed: agent knows this
- User: "Read this file"
  - No skill needed: Read tool is standard

### The Nuance

Even with a perfect description, simple tasks may not trigger skills because:

- The agent doesn't need specialized help
- Loading the skill would add unnecessary context
- The agent prefers to use general capabilities

**This is expected behavior**, not a description failure.

## How Agents Match Descriptions

Agents use various techniques to match user tasks to skill descriptions:

### 1. Semantic Matching

Understanding meaning beyond keywords:

```
User: "I need to work with this spreadsheet"
Skill: "Analyze CSV and Excel files..."
Result: MATCH ("spreadsheet" → "Excel files")
```

### 2. Intent Analysis

Focusing on what the user wants to achieve:

```
User: "Make a chart from this data"
Skill: "...generate charts"
Result: MATCH (intent: visualization)
```

### 3. Context Awareness

Considering surrounding context:

```
User: "Clean up this data file in ~/reports/"
Skill: "...clean messy data"
Result: MATCH (context: data file, likely tabular)
```

### 4. Keyword Matching

Matching explicit terms:

```
User: "Analyze this CSV"
Skill: "Analyze CSV files..."
Result: MATCH (direct keyword match)
```

## Writing for Activation

Given how agents match, optimize your descriptions for:

### Semantic Coverage

Include synonyms and related concepts:

```yaml
description: >
  Analyze CSV, TSV, spreadsheet, and Excel data...
```

### Intent Focus

Describe outcomes, not implementation:

```yaml
description: >
  ...to compute summary statistics and visualize trends...
  # NOT: "...uses pandas and matplotlib..."
```

### Implicit Triggers

Cover cases where users don't name the domain:

```yaml
description: >
  ...even if they don't explicitly mention "CSV"...
```

### Differentiation

Clarify boundaries with similar skills:

```yaml
description: >
  Process Excel files with formulas and formatting. For simple CSV,
  use csv-analyzer instead.
```

## Testing Activation

### Manual Testing

1. Install the skill
2. Send test prompts
3. Observe whether skill loads (check agent logs or indicators)
4. Document which prompts trigger and which don't

### Automated Testing

See the `optimize-skill-description` skill for systematic approaches:
- Design eval queries
- Run multiple evaluations
- Calculate trigger rates
- Iterate on description

### Indicators of Good Activation

- Should-trigger prompts consistently activate skill
- Should-not-trigger prompts consistently don't activate
- Near-misses are handled correctly
- Various phrasings of the same task trigger similarly

## Common Activation Issues

### Issue: Skill Never Activates

**Symptoms:**
- Relevant tasks don't load the skill
- Agent proceeds without using skill

**Causes:**
- Description too vague
- Missing keywords or synonyms
- Too narrow scope
- Passive voice instead of imperative

**Fixes:**
- Add specific capabilities
- Include synonyms
- Broaden scope
- Use "Use when..." phrasing

### Issue: Skill Activates Too Often

**Symptoms:**
- Irrelevant tasks load the skill
- Skill conflicts with other skills

**Causes:**
- Description too broad
- Overlapping with adjacent skills
- Missing negative constraints

**Fixes:**
- Add specificity
- Clarify boundaries with other skills
- Explicitly exclude certain cases

### Issue: Inconsistent Activation

**Symptoms:**
- Same prompt sometimes triggers, sometimes doesn't
- Results vary between runs

**Causes:**
- Ambiguous description
- Model non-determinism
- Borderline relevance

**Fixes:**
- Clarify the description
- Run multiple tests for trigger rate
- Adjust pass threshold

## Progressive Disclosure and Skill Design

### Keep SKILL.md Focused

Since the full file loads on activation:

- Keep main SKILL.md under 500 lines
- Move detailed reference to `references/`
- Tell agent when to load reference files

### Structure for Disclosure

```markdown
# SKILL.md
## Quick Reference (always loaded)
- Main workflow
- Common commands

## Details (loaded as needed)
See [API Reference](references/API.md) for endpoint details.
See [Error Handling](references/ERRORS.md) for troubleshooting.
```

### Progressive Reference Loading

Instead of:
```markdown
Read references/API.md for all API details.
```

Use:
```markdown
Read references/API.md if you need authentication details.
Read references/ERRORS.md if you encounter non-200 status codes.
```

This lets agents load context on demand.

## Summary

- **Discovery**: Only name + description loaded (~100 tokens)
- **Activation**: Description decides if full skill loads
- **Execution**: Referenced files load on demand
- **Description burden**: Must convey capabilities, triggers, and variations
- **Good descriptions**: Specific but broad, imperative, user-intent focused
