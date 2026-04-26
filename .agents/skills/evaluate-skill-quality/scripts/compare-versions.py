#!/usr/bin/env python3
# /// script
# dependencies = [
#   "pyyaml>=6.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Compare two skill versions for blind evaluation.

This script helps perform blind comparisons between skill versions
to avoid bias in quality assessment.

Usage:
    python compare-versions.py --version1 ./workspace/iteration-1/ --version2 ./workspace/iteration-2/ --output comparison.md
"""

import argparse
import json
import random
import sys
from pathlib import Path
from typing import Dict, List, Any, Optional


def load_iteration_data(iteration_path: Path) -> Dict[str, Any]:
    """Load all eval data from an iteration."""
    data = {
        "path": str(iteration_path),
        "evals": {}
    }
    
    if not iteration_path.exists():
        return data
    
    # Find all eval directories
    for eval_dir in iteration_path.iterdir():
        if not eval_dir.is_dir() or not eval_dir.name.startswith('eval'):
            continue
        
        eval_name = eval_dir.name
        eval_data = {
            "with_skill": {},
            "without_skill": {}
        }
        
        # Load with_skill data
        ws_dir = eval_dir / "with_skill"
        if ws_dir.exists():
            grading_file = ws_dir / "grading.json"
            if grading_file.exists():
                try:
                    with open(grading_file, 'r') as f:
                        eval_data["with_skill"]["grading"] = json.load(f)
                except:
                    pass
            
            timing_file = ws_dir / "timing.json"
            if timing_file.exists():
                try:
                    with open(timing_file, 'r') as f:
                        eval_data["with_skill"]["timing"] = json.load(f)
                except:
                    pass
            
            # Check for output files
            outputs_dir = ws_dir / "outputs"
            if outputs_dir.exists():
                eval_data["with_skill"]["outputs"] = [
                    f.name for f in outputs_dir.iterdir() if f.is_file()
                ]
        
        # Load without_skill data
        wos_dir = eval_dir / "without_skill"
        if wos_dir.exists():
            grading_file = wos_dir / "grading.json"
            if grading_file.exists():
                try:
                    with open(grading_file, 'r') as f:
                        eval_data["without_skill"]["grading"] = json.load(f)
                except:
                    pass
            
            timing_file = wos_dir / "timing.json"
            if timing_file.exists():
                try:
                    with open(timing_file, 'r') as f:
                        eval_data["without_skill"]["timing"] = json.load(f)
                except:
                    pass
        
        data["evals"][eval_name] = eval_data
    
    return data


def prepare_blind_comparison(version1_path: Path, version2_path: Path) -> Dict[str, Any]:
    """
    Prepare a blind comparison between two versions.
    
    Returns data structure with anonymized outputs for unbiased comparison.
    """
    v1_data = load_iteration_data(version1_path)
    v2_data = load_iteration_data(version2_path)
    
    # Find common eval cases
    v1_evals = set(v1_data["evals"].keys())
    v2_evals = set(v2_data["evals"].keys())
    common_evals = sorted(v1_evals & v2_evals)
    
    if not common_evals:
        print("Error: No common eval cases found between versions")
        return {}
    
    comparison = {
        "version_a": {
            "label": "Version A (anonymized)",
            "original_path": str(version1_path),
            "data": v1_data
        },
        "version_b": {
            "label": "Version B (anonymized)",
            "original_path": str(version2_path),
            "data": v2_data
        },
        "eval_cases": {}
    }
    
    for eval_name in common_evals:
        v1_eval = v1_data["evals"][eval_name]
        v2_eval = v2_data["evals"][eval_name]
        
        # Get pass rates
        v1_pass_rate = v1_eval.get("with_skill", {}).get("grading", {}).get("pass_rate", 0)
        v2_pass_rate = v2_eval.get("with_skill", {}).get("grading", {}).get("pass_rate", 0)
        
        # Get timing
        v1_time = v1_eval.get("with_skill", {}).get("timing", {}).get("duration_ms", 0) / 1000
        v2_time = v2_eval.get("with_skill", {}).get("timing", {}).get("duration_ms", 0) / 1000
        
        v1_tokens = v1_eval.get("with_skill", {}).get("timing", {}).get("total_tokens", 0)
        v2_tokens = v2_eval.get("with_skill", {}).get("timing", {}).get("total_tokens", 0)
        
        comparison["eval_cases"][eval_name] = {
            "version_a": {
                "pass_rate": v1_pass_rate,
                "time_seconds": v1_time,
                "tokens": v1_tokens
            },
            "version_b": {
                "pass_rate": v2_pass_rate,
                "time_seconds": v2_time,
                "tokens": v2_tokens
            },
            "difference": {
                "pass_rate": v2_pass_rate - v1_pass_rate,
                "time_seconds": v2_time - v1_time,
                "tokens": v2_tokens - v1_tokens
            }
        }
    
    return comparison


def generate_comparison_report(comparison: Dict[str, Any]) -> str:
    """Generate a comparison report."""
    lines = []
    
    lines.append("# Blind Comparison Report")
    lines.append("")
    lines.append("## Overview")
    lines.append("")
    lines.append(f"- **Version A**: {comparison['version_a']['label']}")
    lines.append(f"  - Original: {comparison['version_a']['original_path']}")
    lines.append(f"- **Version B**: {comparison['version_b']['label']}")
    lines.append(f"  - Original: {comparison['version_b']['original_path']}")
    lines.append(f"- **Eval cases compared**: {len(comparison['eval_cases'])}")
    lines.append("")
    lines.append("---")
    lines.append("")
    
    # Aggregate statistics
    a_pass_rates = []
    b_pass_rates = []
    a_times = []
    b_times = []
    a_tokens = []
    b_tokens = []
    
    for eval_name, eval_data in comparison["eval_cases"].items():
        a_pass_rates.append(eval_data["version_a"]["pass_rate"])
        b_pass_rates.append(eval_data["version_b"]["pass_rate"])
        a_times.append(eval_data["version_a"]["time_seconds"])
        b_times.append(eval_data["version_b"]["time_seconds"])
        a_tokens.append(eval_data["version_a"]["tokens"])
        b_tokens.append(eval_data["version_b"]["tokens"])
    
    def avg(values):
        return sum(values) / len(values) if values else 0
    
    lines.append("## Aggregate Statistics")
    lines.append("")
    lines.append("| Metric | Version A | Version B | Difference |")
    lines.append("|--------|-----------|-----------|------------|")
    lines.append(f"| Pass Rate | {avg(a_pass_rates):.1%} | {avg(b_pass_rates):.1%} | {avg(b_pass_rates) - avg(a_pass_rates):+.1%} |")
    lines.append(f"| Time (s) | {avg(a_times):.1f} | {avg(b_times):.1f} | {avg(b_times) - avg(a_times):+.1f} |")
    lines.append(f"| Tokens | {avg(a_tokens):.0f} | {avg(b_tokens):.0f} | {avg(b_tokens) - avg(a_tokens):+.0f} |")
    lines.append("")
    lines.append("---")
    lines.append("")
    
    # Detailed comparison
    lines.append("## Detailed Comparison by Eval Case")
    lines.append("")
    
    for eval_name, eval_data in sorted(comparison["eval_cases"].items()):
        a = eval_data["version_a"]
        b = eval_data["version_b"]
        diff = eval_data["difference"]
        
        lines.append(f"### {eval_name}")
        lines.append("")
        lines.append("| Metric | Version A | Version B | Difference |")
        lines.append("|--------|-----------|-----------|------------|")
        lines.append(f"| Pass Rate | {a['pass_rate']:.1%} | {b['pass_rate']:.1%} | {diff['pass_rate']:+.1%} |")
        lines.append(f"| Time (s) | {a['time_seconds']:.1f} | {b['time_seconds']:.1f} | {diff['time_seconds']:+.1f} |")
        lines.append(f"| Tokens | {a['tokens']:.0f} | {b['tokens']:.0f} | {diff['tokens']:+.0f} |")
        lines.append("")
        
        # Verdict
        if diff["pass_rate"] > 0.1:
            lines.append("✅ **Verdict**: Version B shows significant improvement")
        elif diff["pass_rate"] < -0.1:
            lines.append("❌ **Verdict**: Version B shows significant regression")
        else:
            lines.append("➡️ **Verdict**: Versions are comparable")
        
        lines.append("")
    
    lines.append("---")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    
    # Overall verdict
    better_count = sum(1 for e in comparison["eval_cases"].values() if e["difference"]["pass_rate"] > 0.05)
    worse_count = sum(1 for e in comparison["eval_cases"].values() if e["difference"]["pass_rate"] < -0.05)
    same_count = len(comparison["eval_cases"]) - better_count - worse_count
    
    lines.append(f"- **Better**: Version B was better in {better_count} eval cases")
    lines.append(f"- **Worse**: Version B was worse in {worse_count} eval cases")
    lines.append(f"- **Similar**: Results were similar in {same_count} eval cases")
    lines.append("")
    
    if better_count > worse_count and better_count > same_count:
        lines.append("### Overall: Version B is superior")
    elif worse_count > better_count and worse_count > same_count:
        lines.append("### Overall: Version B is inferior (consider reverting)")
    else:
        lines.append("### Overall: Versions are comparable (consider other factors like maintainability)")
    
    lines.append("")
    
    # Cost-benefit analysis
    pass_improvement = avg(b_pass_rates) - avg(a_pass_rates)
    time_cost = avg(b_times) - avg(a_times)
    token_cost = avg(b_tokens) - avg(a_tokens)
    
    lines.append("### Cost-Benefit Analysis")
    lines.append("")
    lines.append(f"- Quality improvement: {pass_improvement:+.1%} pass rate")
    lines.append(f"- Time cost: {time_cost:+.1f} seconds per task")
    lines.append(f"- Token cost: {token_cost:+.0f} tokens per task")
    lines.append("")
    
    if pass_improvement > 0.1 and time_cost < 10:
        lines.append("✅ **Recommendation**: Version B provides good value — significant improvement at reasonable cost")
    elif pass_improvement > 0 and time_cost > 30:
        lines.append("⚠️ **Recommendation**: Consider whether the improvement is worth the additional time")
    elif pass_improvement < 0:
        lines.append("❌ **Recommendation**: Version B is not recommended — quality regression with no compensating benefits")
    else:
        lines.append("➡️ **Recommendation**: Both versions are viable — choose based on other factors")
    
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description="Compare two skill versions for blind evaluation"
    )
    parser.add_argument(
        "--version1", "-v1",
        required=True,
        help="Path to first version (iteration directory)"
    )
    parser.add_argument(
        "--version2", "-v2",
        required=True,
        help="Path to second version (iteration directory)"
    )
    parser.add_argument(
        "--output", "-o",
        help="Output file path for comparison report (Markdown)"
    )
    parser.add_argument(
        "--json", "-j",
        help="Output file path for comparison data (JSON)"
    )
    parser.add_argument(
        "--randomize", "-r",
        action="store_true",
        help="Randomize which version is A vs B (for true blind comparison)"
    )
    
    args = parser.parse_args()
    
    version1_path = Path(args.version1)
    version2_path = Path(args.version2)
    
    if not version1_path.exists():
        print(f"Error: Version 1 path does not exist: {version1_path}")
        sys.exit(1)
    
    if not version2_path.exists():
        print(f"Error: Version 2 path does not exist: {version2_path}")
        sys.exit(1)
    
    # Prepare comparison
    comparison = prepare_blind_comparison(version1_path, version2_path)
    
    if not comparison:
        sys.exit(1)
    
    # Optionally randomize labels
    if args.randomize:
        if random.random() < 0.5:
            # Swap the labels
            comparison["version_a"], comparison["version_b"] = comparison["version_b"], comparison["version_a"]
            comparison["randomized"] = True
            print("Note: Versions have been randomized (A/B labels may not match input order)")
        else:
            comparison["randomized"] = False
    
    # Generate report
    report = generate_comparison_report(comparison)
    
    # Output
    if args.output:
        with open(args.output, 'w') as f:
            f.write(report)
        print(f"Comparison report saved to: {args.output}")
    else:
        print(report)
    
    if args.json:
        with open(args.json, 'w') as f:
            json.dump(comparison, f, indent=2)
        print(f"Comparison data saved to: {args.json}")
    
    sys.exit(0)


if __name__ == "__main__":
    main()
