#!/usr/bin/env python3
# /// script
# dependencies = [
#   "pyyaml>=6.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Validate Agent Skills structure and frontmatter.

Usage:
    python validate-skill.py /path/to/skill [options]

Options:
    --verbose, -v    Show detailed output
    --strict         Treat warnings as errors
    --json           Output results as JSON

Exit codes:
    0 - Validation passed
    1 - Validation failed
"""

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Union

try:
    import yaml
except ImportError:
    print("Error: PyYAML is required. Install with: pip install pyyaml")
    sys.exit(1)


class ValidationError:
    """Represents a validation error or warning."""
    
    def __init__(self, message: str, severity: str = "error", field: Optional[str] = None):
        self.message = message
        self.severity = severity  # "error", "warning", "info"
        self.field = field
    
    def __str__(self):
        prefix = f"[{self.severity.upper()}]"
        if self.field:
            prefix += f" {self.field}:"
        return f"{prefix} {self.message}"


class SkillValidator:
    """Validates an Agent Skill structure and content."""
    
    # Constants from specification
    MAX_NAME_LENGTH = 64
    MAX_DESCRIPTION_LENGTH = 1024
    MAX_COMPATIBILITY_LENGTH = 500
    RECOMMENDED_MAX_LINES = 500
    
    # Regex patterns
    NAME_PATTERN = re.compile(r'^[a-z0-9]+(-[a-z0-9]+)*$')
    CONSECUTIVE_HYPHENS = re.compile(r'--')
    
    def __init__(self, skill_path: str, verbose: bool = False, strict: bool = False):
        self.skill_path = Path(skill_path).resolve()
        self.verbose = verbose
        self.strict = strict
        self.errors: List[ValidationError] = []
        self.warnings: List[ValidationError] = []
        self.infos: List[ValidationError] = []
        self.frontmatter: Optional[Dict] = None
        self.body_content: Optional[str] = None
        self.skill_md_content: Optional[str] = None
    
    def _add_error(self, message: str, field: Optional[str] = None):
        self.errors.append(ValidationError(message, "error", field))
    
    def _add_warning(self, message: str, field: Optional[str] = None):
        if self.strict:
            self._add_error(message, field)
        else:
            self.warnings.append(ValidationError(message, "warning", field))
    
    def _add_info(self, message: str, field: Optional[str] = None):
        self.infos.append(ValidationError(message, "info", field))
    
    def validate(self) -> bool:
        """Run all validation checks. Returns True if valid."""
        self._validate_structure()
        
        if not self.errors:
            self._validate_frontmatter()
            self._validate_body()
            self._validate_optional_fields()
        
        return len(self.errors) == 0
    
    def _validate_structure(self):
        """Validate basic directory and file structure."""
        if not self.skill_path.exists():
            self._add_error(f"Skill path does not exist: {self.skill_path}")
            return
        
        if not self.skill_path.is_dir():
            self._add_error(f"Skill path is not a directory: {self.skill_path}")
            return
        
        skill_md_path = self.skill_path / "SKILL.md"
        if not skill_md_path.exists():
            self._add_error("SKILL.md not found in skill directory")
            return
        
        if not skill_md_path.is_file():
            self._add_error("SKILL.md exists but is not a file")
            return
        
        # Check readability
        try:
            self.skill_md_content = skill_md_path.read_text(encoding='utf-8')
        except Exception as e:
            self._add_error(f"Cannot read SKILL.md: {e}")
            return
        
        # Validate directory name matches name field (will check after parsing)
        self._add_info(f"Found skill directory: {self.skill_path.name}")
    
    def _validate_frontmatter(self):
        """Extract and validate YAML frontmatter."""
        content = self.skill_md_content
        if not content:
            return
        
        # Check for frontmatter delimiters
        if not content.startswith('---'):
            self._add_error("SKILL.md must start with frontmatter delimiter '---'")
            return
        
        # Find frontmatter boundaries
        lines = content.split('\n')
        if len(lines) < 3:
            self._add_error("SKILL.md is too short to contain frontmatter")
            return
        
        # Find the second ---
        frontmatter_end = -1
        for i, line in enumerate(lines[1:], 1):
            if line.strip() == '---':
                frontmatter_end = i
                break
        
        if frontmatter_end == -1:
            self._add_error("Frontmatter closing delimiter '---' not found")
            return
        
        # Extract frontmatter
        frontmatter_lines = lines[1:frontmatter_end]
        frontmatter_text = '\n'.join(frontmatter_lines)
        
        # Parse YAML
        try:
            self.frontmatter = yaml.safe_load(frontmatter_text)
        except yaml.YAMLError as e:
            self._add_error(f"Invalid YAML in frontmatter: {e}")
            return
        
        if not isinstance(self.frontmatter, dict):
            self._add_error("Frontmatter must be a YAML mapping (key-value pairs)")
            return
        
        # Store body content
        self.body_content = '\n'.join(lines[frontmatter_end + 1:])
        
        # Validate required fields
        self._validate_name_field()
        self._validate_description_field()
    
    def _validate_name_field(self):
        """Validate the name field."""
        if 'name' not in self.frontmatter:
            self._add_error("Required field 'name' is missing", "name")
            return
        
        name = self.frontmatter['name']
        
        if not isinstance(name, str):
            self._add_error("Field 'name' must be a string", "name")
            return
        
        if not name:
            self._add_error("Field 'name' cannot be empty", "name")
            return
        
        # Check length
        if len(name) > self.MAX_NAME_LENGTH:
            self._add_error(
                f"Field 'name' exceeds maximum length of {self.MAX_NAME_LENGTH} characters "
                f"(current: {len(name)})",
                "name"
            )
            return
        
        # Check pattern
        if not self.NAME_PATTERN.match(name):
            if name.startswith('-'):
                self._add_error("Field 'name' cannot start with a hyphen", "name")
            elif name.endswith('-'):
                self._add_error("Field 'name' cannot end with a hyphen", "name")
            elif self.CONSECUTIVE_HYPHENS.search(name):
                self._add_error("Field 'name' cannot contain consecutive hyphens", "name")
            elif not re.match(r'^[a-z0-9-]+$', name):
                self._add_error(
                    "Field 'name' must contain only lowercase letters, numbers, and hyphens",
                    "name"
                )
            else:
                self._add_error("Field 'name' contains invalid characters", "name")
            return
        
        # Check that directory name matches
        if name != self.skill_path.name:
            self._add_error(
                f"Field 'name' ('{name}') must match directory name ('{self.skill_path.name}')",
                "name"
            )
            return
        
        self._add_info(f"Valid skill name: '{name}'", "name")
    
    def _validate_description_field(self):
        """Validate the description field."""
        if 'description' not in self.frontmatter:
            self._add_error("Required field 'description' is missing", "description")
            return
        
        description = self.frontmatter['description']
        
        if not isinstance(description, str):
            self._add_error("Field 'description' must be a string", "description")
            return
        
        if not description:
            self._add_error("Field 'description' cannot be empty", "description")
            return
        
        # Check length
        if len(description) > self.MAX_DESCRIPTION_LENGTH:
            self._add_error(
                f"Field 'description' exceeds maximum length of {self.MAX_DESCRIPTION_LENGTH} "
                f"characters (current: {len(description)})",
                "description"
            )
            return
        
        # Check for good practices
        if len(description) < 20:
            self._add_warning(
                "Field 'description' is very short. Consider adding more detail about "
                "what the skill does and when to use it.",
                "description"
            )
        
        if 'this skill' in description.lower() or 'this skill does' in description.lower():
            self._add_warning(
                "Description uses passive phrasing ('this skill...'). Consider using imperative "
                "phrasing like 'Use when...' or 'Activate when...'",
                "description"
            )
        
        self._add_info(f"Description length: {len(description)} characters", "description")
    
    def _validate_optional_fields(self):
        """Validate optional frontmatter fields."""
        if not self.frontmatter:
            return
        
        # Validate license field
        if 'license' in self.frontmatter:
            license_val = self.frontmatter['license']
            if not isinstance(license_val, str):
                self._add_warning("Field 'license' should be a string", "license")
            else:
                self._add_info(f"License: {license_val}", "license")
        
        # Validate compatibility field
        if 'compatibility' in self.frontmatter:
            compat = self.frontmatter['compatibility']
            if not isinstance(compat, str):
                self._add_error("Field 'compatibility' must be a string", "compatibility")
            elif len(compat) > self.MAX_COMPATIBILITY_LENGTH:
                self._add_error(
                    f"Field 'compatibility' exceeds maximum length of {self.MAX_COMPATIBILITY_LENGTH} "
                    f"characters (current: {len(compat)})",
                    "compatibility"
                )
            else:
                self._add_info(f"Compatibility: {compat}", "compatibility")
        
        # Validate metadata field
        if 'metadata' in self.frontmatter:
            metadata = self.frontmatter['metadata']
            if not isinstance(metadata, dict):
                self._add_error("Field 'metadata' must be an object/map", "metadata")
            else:
                self._add_info(f"Metadata keys: {list(metadata.keys())}", "metadata")
                
                # Check that all values are strings
                for key, value in metadata.items():
                    if not isinstance(value, (str, int, float, bool)):
                        self._add_warning(
                            f"Metadata key '{key}' has non-primitive value. Consider using a string.",
                            "metadata"
                        )
        
        # Validate allowed-tools field
        if 'allowed-tools' in self.frontmatter:
            tools = self.frontmatter['allowed-tools']
            if not isinstance(tools, str):
                self._add_warning("Field 'allowed-tools' should be a string", "allowed-tools")
            else:
                self._add_info(f"Allowed tools: {tools}", "allowed-tools")
                self._add_info(
                    "Note: 'allowed-tools' is experimental and support varies between clients",
                    "allowed-tools"
                )
        
        # Check for unknown fields
        known_fields = {'name', 'description', 'license', 'compatibility', 'metadata', 'allowed-tools'}
        unknown_fields = set(self.frontmatter.keys()) - known_fields
        if unknown_fields:
            self._add_warning(
                f"Unknown frontmatter fields: {', '.join(unknown_fields)}. "
                f"These will be ignored by most clients.",
                "frontmatter"
            )
    
    def _validate_body(self):
        """Validate the body content."""
        if not self.body_content:
            self._add_warning("No body content after frontmatter")
            return
        
        body_lines = self.body_content.strip().split('\n')
        body_length = len(body_lines)
        
        self._add_info(f"Body length: {body_length} lines")
        
        if body_length == 0:
            self._add_warning("Body content is empty")
        elif body_length > self.RECOMMENDED_MAX_LINES:
            self._add_warning(
                f"Body is {body_length} lines, which exceeds the recommended maximum of "
                f"{self.RECOMMENDED_MAX_LINES}. Consider moving content to references/.",
                "body"
            )
        
        # Check for file references
        ref_pattern = re.compile(r'\[([^\]]+)\]\(([^)]+)\)')
        matches = ref_pattern.findall(self.body_content)
        
        for text, path in matches:
            # Check if it's a relative reference to references/, scripts/, or assets/
            if path.startswith('references/') or path.startswith('scripts/') or path.startswith('assets/'):
                full_path = self.skill_path / path
                if not full_path.exists():
                    self._add_warning(f"Referenced file does not exist: {path}")
                else:
                    self._add_info(f"Verified reference: {path}")
    
    def get_results(self) -> Dict:
        """Get validation results as a dictionary."""
        return {
            "valid": len(self.errors) == 0,
            "skill_path": str(self.skill_path),
            "skill_name": self.skill_path.name,
            "errors": [{"message": e.message, "field": e.field} for e in self.errors],
            "warnings": [{"message": w.message, "field": w.field} for w in self.warnings],
            "infos": [{"message": i.message, "field": i.field} for i in self.infos],
            "summary": {
                "error_count": len(self.errors),
                "warning_count": len(self.warnings),
                "info_count": len(self.infos)
            }
        }
    
    def print_results(self):
        """Print validation results to stdout."""
        print(f"\n{'='*60}")
        print(f"Validating: {self.skill_path.name}")
        print(f"Path: {self.skill_path}")
        print(f"{'='*60}\n")
        
        # Print errors
        for error in self.errors:
            print(f"❌ {error}")
        
        # Print warnings
        for warning in self.warnings:
            print(f"⚠️  {warning}")
        
        # Print infos (only in verbose mode)
        if self.verbose:
            for info in self.infos:
                print(f"ℹ️  {info}")
        
        # Print summary
        print(f"\n{'='*60}")
        error_count = len(self.errors)
        warning_count = len(self.warnings)
        
        if error_count == 0:
            if warning_count == 0:
                print("✅ Validation PASSED - No errors or warnings")
            else:
                print(f"⚠️  Validation PASSED with {warning_count} warning(s)")
        else:
            print(f"❌ Validation FAILED - {error_count} error(s), {warning_count} warning(s)")
        
        print(f"{'='*60}\n")


def main():
    parser = argparse.ArgumentParser(
        description="Validate Agent Skills structure and frontmatter"
    )
    parser.add_argument(
        "skill_path",
        help="Path to the skill directory"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Show detailed output"
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Treat warnings as errors"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output results as JSON"
    )
    
    args = parser.parse_args()
    
    validator = SkillValidator(
        args.skill_path,
        verbose=args.verbose,
        strict=args.strict
    )
    
    is_valid = validator.validate()
    
    if args.json:
        print(json.dumps(validator.get_results(), indent=2))
    else:
        validator.print_results()
    
    sys.exit(0 if is_valid else 1)


if __name__ == "__main__":
    main()
