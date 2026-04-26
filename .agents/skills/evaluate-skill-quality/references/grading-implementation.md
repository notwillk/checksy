---
title: Automated Grading Implementation
---

# Automated Grading Implementation Guide

This reference provides detailed implementation guidance for automated assertion grading.

## Script-Based Grading Example

```python
#!/usr/bin/env python3
# /// script
# dependencies = []
# requires-python = ">=3.8"
# ///

"""
Grade assertions automatically for common patterns.
"""

import json
import os
from pathlib import Path

def grade_assertion(assertion: str, outputs_dir: Path) -> dict:
    assertion_lower = assertion.lower()
    
    # File existence checks
    if "contains" in assertion_lower or "exists" in assertion_lower:
        files = list(outputs_dir.iterdir()) if outputs_dir.exists() else []
        file_count = len([f for f in files if f.is_file()])
        
        # Check for specific filename
        for ext in ['.json', '.csv', '.png', '.md', '.txt']:
            if ext in assertion_lower:
                matching = [f for f in files if f.suffix == ext]
                if matching:
                    return {
                        "passed": True,
                        "evidence": f"Found {len(matching)} {ext} file(s)"
                    }
        
        # Generic count check
        if "at least" in assertion_lower:
            import re
            match = re.search(r'at least (\d+)', assertion_lower)
            if match:
                required = int(match.group(1))
                return {
                    "passed": file_count >= required,
                    "evidence": f"Found {file_count} files (required {required})"
                }
    
    # JSON validation
    if "valid json" in assertion_lower:
        json_files = list(outputs_dir.glob("*.json"))
        if json_files:
            try:
                json.loads(json_files[0].read_text())
                return {"passed": True, "evidence": "JSON parses successfully"}
            except:
                return {"passed": False, "evidence": "JSON parse error"}
    
    # Content search
    if any(word in assertion_lower for word in ["contains", "has", "includes"]):
        text_content = ""
        for f in outputs_dir.iterdir():
            if f.is_file() and f.stat().st_size < 1024*1024:
                try:
                    text_content += f.read_text(encoding='utf-8', errors='ignore')
                except:
                    pass
        
        # Extract key terms from assertion
        keywords = [word for word in assertion_lower.split() if len(word) > 4]
        found = [k for k in keywords if k in text_content.lower()]
        
        return {
            "passed": len(found) > 0,
            "evidence": f"Found {len(found)}/{len(keywords)} keywords"
        }
    
    # Default: requires manual grading
    return {
        "passed": False,
        "evidence": "Requires manual or LLM grading"
    }

# Usage
if __name__ == "__main__":
    import sys
    outputs_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("./outputs")
    
    assertions = [
        "The output contains a JSON file",
        "The report has at least 3 sections"
    ]
    
    results = []
    for assertion in assertions:
        result = grade_assertion(assertion, outputs_dir)
        results.append({
            "assertion": assertion,
            **result
        })
    
    print(json.dumps(results, indent=2))
```

## Integration with grade-assertions.py

Extend the script with custom grading functions:

```python
# In grade-assertions.py, add to _grade_auto():

if assertion.startswith("CUSTOM:"):
    return self._grade_custom(assertion.replace("CUSTOM:", "").strip())

def _grade_custom(self, custom_logic: str) -> GradingResult:
    # Implement your custom grading logic here
    # Return GradingResult with pass/fail and evidence
    pass
```

## Best Practices for Automated Grading

1. **Start with file existence checks** — Most reliable and objective
2. **Use regex for flexible content matching** — Don't require exact strings
3. **Validate structured formats** — JSON, CSV, YAML have clear validity rules
4. **Spot-check rather than exhaustive** — Checking 3 items represents the pattern
5. **Flag for manual review when uncertain** — Better to ask than guess wrong
6. **Version your grading logic** — Update as you learn what works
7. **Log raw outputs for debugging** — Save what you evaluated

## Common Grading Patterns

### File Pattern Matching

```python
def check_file_pattern(pattern: str, outputs_dir: Path) -> dict:
    """Check for files matching glob pattern."""
    matches = list(outputs_dir.glob(pattern))
    return {
        "passed": len(matches) > 0,
        "evidence": f"Found {len(matches)} files matching {pattern}"
    }
```

### JSON Schema Validation

```python
import jsonschema

def validate_schema(data: dict, schema: dict) -> dict:
    try:
        jsonschema.validate(data, schema)
        return {"passed": True, "evidence": "Validates against schema"}
    except jsonschema.ValidationError as e:
        return {"passed": False, "evidence": str(e)}
```

### Content Extraction and Counting

```python
import re

def count_occurrences(pattern: str, text: str) -> int:
    return len(re.findall(pattern, text))

def check_minimum_count(pattern: str, text: str, minimum: int) -> dict:
    count = count_occurrences(pattern, text)
    return {
        "passed": count >= minimum,
        "evidence": f"Found {count} occurrences (required {minimum})"
    }
```

## Testing Your Grading Scripts

Always test grading scripts against known-good and known-bad outputs:

```python
def test_grader():
    # Create test outputs
    test_dir = Path("test_outputs")
    test_dir.mkdir(exist_ok=True)
    (test_dir / "output.json").write_text('{"valid": true}')
    
    # Test assertions
    assert grade_assertion("Contains JSON file", test_dir)["passed"] == True
    assert grade_assertion("Contains CSV file", test_dir)["passed"] == False
    
    # Cleanup
    import shutil
    shutil.rmtree(test_dir)
```
