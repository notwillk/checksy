#!/usr/bin/env python3
# /// script
# dependencies = [
#   "pyyaml>=6.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Grade assertions against skill outputs.

This script evaluates whether skill outputs meet the specified criteria.
Can use automated checks or LLM-based grading.

Usage:
    python grade-assertions.py \
        --outputs ./workspace/iteration-1/eval-1/with_skill/outputs/ \
        --evals ./my-skill/evals/evals.json \
        --eval-id 1 \
        --method auto \
        --output grading.json
"""

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, asdict


@dataclass
class GradingResult:
    """Result of grading a single assertion."""
    assertion: str
    passed: bool
    evidence: str
    notes: str = ""


class AssertionGrader:
    """Grades assertions against skill outputs."""
    
    def __init__(self, outputs_dir: Path, evals_file: Path, eval_id: str, method: str = "auto"):
        self.outputs_dir = outputs_dir
        self.evals_file = evals_file
        self.eval_id = eval_id
        self.method = method
        self.eval_case: Optional[Dict] = None
        self.results: List[GradingResult] = []
        
    def load_eval_case(self) -> bool:
        """Load the specific eval case from evals.json."""
        try:
            with open(self.evals_file, 'r') as f:
                data = json.load(f)
            
            evals = data.get('evals', []) if isinstance(data, dict) else data
            
            # Find eval case by ID
            for eval_case in evals:
                case_id = str(eval_case.get('id', ''))
                case_name = str(eval_case.get('name', ''))
                
                if case_id == str(self.eval_id) or case_name == str(self.eval_id):
                    self.eval_case = eval_case
                    return True
            
            print(f"Error: Eval case '{self.eval_id}' not found in {self.evals_file}")
            return False
            
        except Exception as e:
            print(f"Error loading evals file: {e}")
            return False
    
    def grade_assertion(self, assertion: str) -> GradingResult:
        """
        Grade a single assertion against the output.
        
        Supports multiple grading methods:
        - auto: Automated checks for common assertions
        - llm: LLM-based grading (placeholder)
        - manual: Manual grading input
        """
        if self.method == "auto":
            return self._grade_auto(assertion)
        elif self.method == "llm":
            return self._grade_llm(assertion)
        elif self.method == "manual":
            return self._grade_manual(assertion)
        else:
            return GradingResult(
                assertion=assertion,
                passed=False,
                evidence="Unknown grading method",
                notes=f"Method: {self.method}"
            )
    
    def _grade_auto(self, assertion: str) -> GradingResult:
        """Automated grading for common assertion patterns."""
        assertion_lower = assertion.lower()
        
        # File existence checks
        file_patterns = [
            r"output (?:directory|dir|folder) contains",
            r"(?:file|files?) (?:exists?|were created|was generated)",
            r"(?:at least|exactly) \d+ (?:files?|outputs?)",
        ]
        
        for pattern in file_patterns:
            if re.search(pattern, assertion_lower):
                return self._check_files(assertion)
        
        # Content checks
        content_patterns = [
            r"contains? (?:a|the)",
            r"has (?:a|the|an)",
            r"includes?",
            r"present",
        ]
        
        for pattern in content_patterns:
            if re.search(pattern, assertion_lower):
                return self._check_content(assertion)
        
        # Format checks
        format_patterns = [
            r"valid (?:json|yaml|csv|xml)",
            r"proper (?:format|structure)",
            r"follows? (?:the|template)",
        ]
        
        for pattern in format_patterns:
            if re.search(pattern, assertion_lower):
                return self._check_format(assertion)
        
        # Count checks
        count_patterns = [
            r"(?:at least|exactly|no more than) \d+",
            r"\d+ (?:or more|or fewer)",
        ]
        
        for pattern in count_patterns:
            if re.search(pattern, assertion_lower):
                return self._check_count(assertion)
        
        # Default: manual review needed
        return GradingResult(
            assertion=assertion,
            passed=False,
            evidence="Could not automatically evaluate",
            notes="This assertion requires manual or LLM-based grading"
        )
    
    def _check_files(self, assertion: str) -> GradingResult:
        """Check file existence assertions."""
        # Extract expected file patterns
        files = list(self.outputs_dir.iterdir()) if self.outputs_dir.exists() else []
        file_count = len([f for f in files if f.is_file()])
        
        # Look for specific filename
        filename_match = re.search(r"['\"]?([\w\-\.]+\.(?:md|json|csv|png|jpg|pdf|txt|py))['\"]?", assertion)
        if filename_match:
            expected_file = filename_match.group(1)
            file_path = self.outputs_dir / expected_file
            
            if file_path.exists():
                file_size = file_path.stat().st_size
                return GradingResult(
                    assertion=assertion,
                    passed=True,
                    evidence=f"Found {expected_file} ({file_size} bytes) in outputs",
                    notes=""
                )
            else:
                available = ", ".join([f.name for f in files if f.is_file()][:5])
                return GradingResult(
                    assertion=assertion,
                    passed=False,
                    evidence=f"Expected {expected_file}, not found. Available: {available}",
                    notes=""
                )
        
        # Check count patterns
        count_match = re.search(r"(?:at least|exactly) (\d+)", assertion.lower())
        if count_match:
            expected_count = int(count_match.group(1))
            if file_count >= expected_count:
                return GradingResult(
                    assertion=assertion,
                    passed=True,
                    evidence=f"Found {file_count} files (expected at least {expected_count})",
                    notes=""
                )
            else:
                return GradingResult(
                    assertion=assertion,
                    passed=False,
                    evidence=f"Found {file_count} files (expected at least {expected_count})",
                    notes=""
                )
        
        # Generic file check
        if file_count > 0:
            return GradingResult(
                assertion=assertion,
                passed=True,
                evidence=f"Found {file_count} files in outputs directory",
                notes=""
            )
        else:
            return GradingResult(
                assertion=assertion,
                passed=False,
                evidence="No files found in outputs directory",
                notes=""
            )
    
    def _check_content(self, assertion: str) -> GradingResult:
        """Check content assertions by searching output files."""
        # Read all text files in outputs
        text_content = ""
        if self.outputs_dir.exists():
            for file_path in self.outputs_dir.iterdir():
                if file_path.is_file() and file_path.stat().st_size < 1024 * 1024:  # Max 1MB
                    try:
                        text_content += file_path.read_text(encoding='utf-8', errors='ignore') + "\n"
                    except:
                        pass
        
        # Look for keywords in the assertion
        keywords = re.findall(r'"([^"]+)"', assertion)
        if not keywords:
            # Try to extract meaningful terms
            words = assertion.lower().split()
            keywords = [w for w in words if len(w) > 4 and w not in ['contains', 'includes', 'output', 'assertion']]
        
        found_keywords = []
        missing_keywords = []
        
        for keyword in keywords:
            if keyword.lower() in text_content.lower():
                found_keywords.append(keyword)
            else:
                missing_keywords.append(keyword)
        
        if not missing_keywords:
            return GradingResult(
                assertion=assertion,
                passed=True,
                evidence=f"Found all key content: {', '.join(found_keywords[:3])}",
                notes=""
            )
        else:
            return GradingResult(
                assertion=assertion,
                passed=len(missing_keywords) < len(keywords),  # Partial credit
                evidence=f"Missing: {', '.join(missing_keywords[:3])}. Found: {', '.join(found_keywords[:3])}",
                notes="Partial match" if found_keywords else ""
            )
    
    def _check_format(self, assertion: str) -> GradingResult:
        """Check format assertions (JSON, etc.)."""
        assertion_lower = assertion.lower()
        
        # Check for JSON files
        if "json" in assertion_lower:
            json_files = list(self.outputs_dir.glob("*.json"))
            if json_files:
                valid_count = 0
                for json_file in json_files:
                    try:
                        json.loads(json_file.read_text())
                        valid_count += 1
                    except:
                        pass
                
                if valid_count > 0:
                    return GradingResult(
                        assertion=assertion,
                        passed=True,
                        evidence=f"{valid_count} valid JSON file(s) found",
                        notes=""
                    )
                else:
                    return GradingResult(
                        assertion=assertion,
                        passed=False,
                        evidence="JSON files present but invalid",
                        notes=""
                    )
            else:
                return GradingResult(
                    assertion=assertion,
                    passed=False,
                    evidence="No JSON files found",
                    notes=""
                )
        
        # Generic format check
        return GradingResult(
            assertion=assertion,
            passed=False,
            evidence="Format check requires manual or LLM grading",
            notes=""
        )
    
    def _check_count(self, assertion: str) -> GradingResult:
        """Check count-based assertions."""
        # This is a simplified implementation
        # Real implementation would need to know what to count
        
        return GradingResult(
            assertion=assertion,
            passed=False,
            evidence="Count-based check requires manual or LLM grading",
            notes="Could not determine what to count"
        )
    
    def _grade_llm(self, assertion: str) -> GradingResult:
        """LLM-based grading (placeholder)."""
        # This would integrate with an LLM API
        # For now, just mark as needing manual review
        
        return GradingResult(
            assertion=assertion,
            passed=False,
            evidence="LLM grading not implemented in this version",
            notes="Use --method manual or review output yourself"
        )
    
    def _grade_manual(self, assertion: str) -> GradingResult:
        """Interactive manual grading."""
        print(f"\nAssertion: {assertion}")
        
        # Show available output files
        if self.outputs_dir.exists():
            files = [f.name for f in self.outputs_dir.iterdir() if f.is_file()]
            if files:
                print(f"Available files: {', '.join(files[:5])}")
        
        while True:
            response = input("Pass or fail? (p/f/skip): ").lower().strip()
            if response in ['p', 'pass', 'y', 'yes']:
                evidence = input("Evidence (optional): ").strip()
                return GradingResult(
                    assertion=assertion,
                    passed=True,
                    evidence=evidence or "Manually graded as passing",
                    notes="Manual grading"
                )
            elif response in ['f', 'fail', 'n', 'no']:
                evidence = input("Evidence (required): ").strip()
                return GradingResult(
                    assertion=assertion,
                    passed=False,
                    evidence=evidence or "Manually graded as failing",
                    notes="Manual grading"
                )
            elif response in ['s', 'skip', '']:
                return GradingResult(
                    assertion=assertion,
                    passed=False,
                    evidence="Skipped in manual grading",
                    notes="Requires review"
                )
            else:
                print("Please enter 'p' (pass), 'f' (fail), or 's' (skip)")
    
    def grade_all(self) -> bool:
        """Grade all assertions for the eval case."""
        if not self.load_eval_case():
            return False
        
        assertions = self.eval_case.get('assertions', [])
        
        if not assertions:
            print(f"Warning: No assertions defined for eval case '{self.eval_id}'")
            print("Add assertions to evals.json to enable grading")
            return False
        
        print(f"Grading {len(assertions)} assertions for eval case '{self.eval_id}'")
        print(f"Outputs directory: {self.outputs_dir}")
        print(f"Method: {self.method}")
        print("-" * 60)
        
        for i, assertion in enumerate(assertions, 1):
            print(f"\n[{i}/{len(assertions)}] Grading: {assertion[:60]}...")
            result = self.grade_assertion(assertion)
            self.results.append(result)
            
            status = "✓ PASS" if result.passed else "✗ FAIL"
            print(f"  {status}")
            print(f"  Evidence: {result.evidence[:80]}...")
        
        return True
    
    def get_summary(self) -> Dict[str, Any]:
        """Get grading summary."""
        total = len(self.results)
        passed = sum(1 for r in self.results if r.passed)
        failed = total - passed
        
        return {
            "eval_id": self.eval_id,
            "eval_name": self.eval_case.get('name', '') if self.eval_case else '',
            "total_assertions": total,
            "passed": passed,
            "failed": failed,
            "pass_rate": passed / total if total > 0 else 0,
            "method": self.method,
            "results": [asdict(r) for r in self.results]
        }
    
    def save_results(self, output_path: Path):
        """Save grading results to file."""
        summary = self.get_summary()
        
        with open(output_path, 'w') as f:
            json.dump(summary, f, indent=2)
        
        print(f"\nResults saved to: {output_path}")
        print(f"  Pass rate: {summary['pass_rate']:.1%}")
        print(f"  Passed: {summary['passed']}/{summary['total_assertions']}")


def main():
    parser = argparse.ArgumentParser(
        description="Grade assertions against skill outputs"
    )
    parser.add_argument(
        "--outputs", "-o",
        required=True,
        help="Path to the outputs directory"
    )
    parser.add_argument(
        "--evals", "-e",
        required=True,
        help="Path to the evals.json file"
    )
    parser.add_argument(
        "--eval-id", "-i",
        required=True,
        help="ID or name of the eval case to grade"
    )
    parser.add_argument(
        "--method", "-m",
        choices=['auto', 'llm', 'manual'],
        default='auto',
        help="Grading method (default: auto)"
    )
    parser.add_argument(
        "--output", "-out",
        help="Output file path for grading results"
    )
    
    args = parser.parse_args()
    
    outputs_dir = Path(args.outputs)
    evals_file = Path(args.evals)
    
    if not outputs_dir.exists():
        print(f"Error: Outputs directory does not exist: {outputs_dir}")
        sys.exit(1)
    
    if not evals_file.exists():
        print(f"Error: Evals file does not exist: {evals_file}")
        sys.exit(1)
    
    grader = AssertionGrader(outputs_dir, evals_file, args.eval_id, args.method)
    
    if not grader.grade_all():
        sys.exit(1)
    
    if args.output:
        grader.save_results(Path(args.output))
    else:
        # Print summary
        summary = grader.get_summary()
        print("\n" + "=" * 60)
        print("GRADING SUMMARY")
        print("=" * 60)
        print(f"Eval case: {summary['eval_id']}")
        print(f"Total assertions: {summary['total_assertions']}")
        print(f"Passed: {summary['passed']}")
        print(f"Failed: {summary['failed']}")
        print(f"Pass rate: {summary['pass_rate']:.1%}")
    
    sys.exit(0)


if __name__ == "__main__":
    main()
