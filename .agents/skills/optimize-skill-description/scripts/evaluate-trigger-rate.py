#!/usr/bin/env python3
# /// script
# dependencies = [
#   "requests>=2.31.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Evaluate trigger rates for a skill against a set of queries.

This script tests how often a skill activates for different prompts.
Run each query multiple times to account for model non-determinism.

Usage:
    python evaluate-trigger-rate.py queries.json --skill my-skill --runs 3 --output results.json

Note: This is a template implementation. You'll need to adapt the `check_triggered`
function to work with your specific agent client (Claude Code, OpenCode, etc.).

Exit codes:
    0 - Evaluation completed
    1 - Error in evaluation
"""

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Dict, List, Optional, Any
from dataclasses import dataclass, asdict


@dataclass
class QueryResult:
    """Result of evaluating a single query."""
    query: str
    should_trigger: bool
    runs: int
    triggers: int
    trigger_rate: float
    passed: bool
    details: List[Dict[str, Any]]


class TriggerEvaluator:
    """Evaluates how often a skill triggers for given queries."""
    
    def __init__(self, queries_file: str, skill_name: str, runs: int = 3, 
                 threshold: float = 0.5, verbose: bool = False):
        self.queries_file = Path(queries_file)
        self.skill_name = skill_name
        self.runs = runs
        self.threshold = threshold
        self.verbose = verbose
        self.results: List[QueryResult] = []
        self.queries: List[Dict] = []
        
    def load_queries(self) -> bool:
        """Load queries from JSON file."""
        try:
            with open(self.queries_file, 'r') as f:
                data = json.load(f)
                
            # Handle both flat list and nested structure
            if isinstance(data, list):
                self.queries = data
            elif isinstance(data, dict) and 'queries' in data:
                self.queries = data['queries']
            else:
                print(f"Error: Invalid queries file format")
                return False
                
            return True
        except Exception as e:
            print(f"Error loading queries file: {e}")
            return False
    
    def check_triggered(self, query: str) -> tuple[bool, Dict[str, Any]]:
        """
        Check if the skill triggers for a given query.
        
        IMPORTANT: This is a placeholder. You must implement this to work with
        your specific agent client (Claude Code, OpenCode, VS Code Copilot, etc.)
        
        Returns:
            (triggered: bool, details: dict with additional info)
        
        Example implementations:
        
        For Claude Code (hypothetical):
            Use the API to send the query and check if Skill tool is called.
            
        For OpenCode (hypothetical):
            Check session logs for skill activation.
            
        For testing:
            Return random or fixed results.
        """
        
        # PLACEHOLDER IMPLEMENTATION
        # Replace this with actual client-specific code
        
        print(f"  [PLACEHOLDER] Checking: {query[:50]}...")
        print(f"  [NOTE] You need to implement check_triggered() for your agent client")
        
        # Simulated result for demonstration
        # In reality, you would:
        # 1. Start a fresh agent session
        # 2. Send the query
        # 3. Monitor tool calls / skill activations
        # 4. Return whether the skill was invoked
        
        triggered = False  # Replace with actual check
        details = {
            "method": "placeholder",
            "note": "Implement actual check for your agent client"
        }
        
        return triggered, details
    
    def evaluate_query(self, query_data: Dict) -> QueryResult:
        """Evaluate a single query across multiple runs."""
        query_text = query_data.get("query", "")
        should_trigger = query_data.get("should_trigger", False)
        
        if self.verbose:
            print(f"\nEvaluating: {query_text[:80]}...")
            print(f"  Expected: {'trigger' if should_trigger else 'no trigger'}")
        
        triggers = 0
        details = []
        
        for run in range(1, self.runs + 1):
            if self.verbose:
                print(f"  Run {run}/{self.runs}...", end=" ")
            
            try:
                triggered, run_details = self.check_triggered(query_text)
                
                if triggered:
                    triggers += 1
                    if self.verbose:
                        print("TRIGGERED")
                else:
                    if self.verbose:
                        print("not triggered")
                
                details.append({
                    "run": run,
                    "triggered": triggered,
                    "details": run_details
                })
                
            except Exception as e:
                if self.verbose:
                    print(f"ERROR: {e}")
                details.append({
                    "run": run,
                    "error": str(e)
                })
            
            # Brief delay between runs (if using real API)
            if run < self.runs:
                time.sleep(0.1)
        
        trigger_rate = triggers / self.runs
        
        # Determine if passed
        if should_trigger:
            passed = trigger_rate >= self.threshold
        else:
            passed = trigger_rate < self.threshold
        
        result = QueryResult(
            query=query_text,
            should_trigger=should_trigger,
            runs=self.runs,
            triggers=triggers,
            trigger_rate=trigger_rate,
            passed=passed,
            details=details
        )
        
        if self.verbose:
            status = "✓ PASS" if passed else "✗ FAIL"
            print(f"  Result: {status} (rate: {trigger_rate:.2f})")
        
        return result
    
    def evaluate_all(self) -> bool:
        """Evaluate all queries."""
        if not self.load_queries():
            return False
        
        print(f"Evaluating {len(self.queries)} queries")
        print(f"Skill: {self.skill_name}")
        print(f"Runs per query: {self.runs}")
        print(f"Pass threshold: {self.threshold}")
        print("=" * 60)
        
        for i, query_data in enumerate(self.queries, 1):
            print(f"\n[{i}/{len(self.queries)}] ", end="")
            result = self.evaluate_query(query_data)
            self.results.append(result)
        
        return True
    
    def get_summary(self) -> Dict[str, Any]:
        """Generate summary statistics."""
        if not self.results:
            return {}
        
        total = len(self.results)
        passed = sum(1 for r in self.results if r.passed)
        failed = total - passed
        
        positive_results = [r for r in self.results if r.should_trigger]
        negative_results = [r for r in self.results if not r.should_trigger]
        
        summary = {
            "total_queries": total,
            "passed": passed,
            "failed": failed,
            "pass_rate": passed / total if total > 0 else 0,
            "skill_name": self.skill_name,
            "runs_per_query": self.runs,
            "threshold": self.threshold,
            "positive_queries": {
                "count": len(positive_results),
                "passed": sum(1 for r in positive_results if r.passed),
                "average_trigger_rate": sum(r.trigger_rate for r in positive_results) / len(positive_results) if positive_results else 0
            },
            "negative_queries": {
                "count": len(negative_results),
                "passed": sum(1 for r in negative_results if r.passed),
                "average_trigger_rate": sum(r.trigger_rate for r in negative_results) / len(negative_results) if negative_results else 0
            }
        }
        
        return summary
    
    def print_results(self):
        """Print evaluation results to console."""
        summary = self.get_summary()
        
        print("\n" + "=" * 60)
        print("EVALUATION RESULTS")
        print("=" * 60)
        
        print(f"\nSummary:")
        print(f"  Total queries: {summary['total_queries']}")
        print(f"  Passed: {summary['passed']}")
        print(f"  Failed: {summary['failed']}")
        print(f"  Pass rate: {summary['pass_rate']:.1%}")
        
        print(f"\nPositive queries (should trigger):")
        pos = summary['positive_queries']
        print(f"  Count: {pos['count']}")
        print(f"  Passed: {pos['passed']}")
        print(f"  Avg trigger rate: {pos['average_trigger_rate']:.2f}")
        
        print(f"\nNegative queries (should not trigger):")
        neg = summary['negative_queries']
        print(f"  Count: {neg['count']}")
        print(f"  Passed: {neg['passed']}")
        print(f"  Avg trigger rate: {neg['average_trigger_rate']:.2f}")
        
        print(f"\nDetailed results:")
        print("-" * 60)
        
        for result in self.results:
            status = "✓" if result.passed else "✗"
            expected = "trigger" if result.should_trigger else "no trigger"
            print(f"{status} [{result.triggers}/{result.runs}] {expected:11} | {result.query[:60]}...")
    
    def save_results(self, output_path: str):
        """Save results to JSON file."""
        output_data = {
            "metadata": {
                "skill_name": self.skill_name,
                "runs_per_query": self.runs,
                "threshold": self.threshold,
                "queries_file": str(self.queries_file)
            },
            "summary": self.get_summary(),
            "results": [asdict(r) for r in self.results]
        }
        
        with open(output_path, 'w') as f:
            json.dump(output_data, f, indent=2)
        
        print(f"\nResults saved to: {output_path}")


def main():
    parser = argparse.ArgumentParser(
        description="Evaluate skill trigger rates against test queries"
    )
    parser.add_argument(
        "queries_file",
        help="Path to JSON file containing test queries"
    )
    parser.add_argument(
        "--skill", "-s",
        required=True,
        help="Name of the skill to evaluate"
    )
    parser.add_argument(
        "--runs", "-r",
        type=int,
        default=3,
        help="Number of runs per query (default: 3)"
    )
    parser.add_argument(
        "--threshold", "-t",
        type=float,
        default=0.5,
        help="Trigger rate threshold for pass/fail (default: 0.5)"
    )
    parser.add_argument(
        "--output", "-o",
        help="Output file path for results JSON"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Show detailed output for each run"
    )
    
    args = parser.parse_args()
    
    evaluator = TriggerEvaluator(
        queries_file=args.queries_file,
        skill_name=args.skill,
        runs=args.runs,
        threshold=args.threshold,
        verbose=args.verbose
    )
    
    if not evaluator.evaluate_all():
        sys.exit(1)
    
    evaluator.print_results()
    
    if args.output:
        evaluator.save_results(args.output)
    
    # Exit with error if pass rate is poor
    summary = evaluator.get_summary()
    if summary.get('pass_rate', 0) < 0.5:
        print("\n⚠️  Warning: Pass rate below 50%. Description needs improvement.")
        sys.exit(2)
    
    sys.exit(0)


if __name__ == "__main__":
    main()
