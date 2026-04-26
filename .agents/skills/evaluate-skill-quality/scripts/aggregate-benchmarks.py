#!/usr/bin/env python3
# /// script
# dependencies = [
#   "pyyaml>=6.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Aggregate benchmark statistics from evaluation results.

This script aggregates timing and grading data across all eval cases
in an iteration to produce summary statistics.

Usage:
    python aggregate-benchmarks.py --workspace ./my-skill-workspace --iteration 1 --output benchmark.json
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List, Any, Optional
from statistics import mean, stdev


def load_timing_data(timing_file: Path) -> Optional[Dict[str, Any]]:
    """Load timing data from a timing.json file."""
    if not timing_file.exists():
        return None
    
    try:
        with open(timing_file, 'r') as f:
            return json.load(f)
    except Exception as e:
        print(f"Warning: Could not load {timing_file}: {e}")
        return None


def load_grading_data(grading_file: Path) -> Optional[Dict[str, Any]]:
    """Load grading data from a grading.json file."""
    if not grading_file.exists():
        return None
    
    try:
        with open(grading_file, 'r') as f:
            return json.load(f)
    except Exception as e:
        print(f"Warning: Could not load {grading_file}: {e}")
        return None


def aggregate_iteration(workspace_path: Path, iteration: int) -> Optional[Dict[str, Any]]:
    """
    Aggregate benchmark data for an iteration.
    
    Collects data from all eval cases and computes summary statistics.
    """
    iteration_dir = workspace_path / f"iteration-{iteration}"
    
    if not iteration_dir.exists():
        print(f"Error: Iteration directory not found: {iteration_dir}")
        return None
    
    # Find all eval directories
    eval_dirs = [d for d in iteration_dir.iterdir() if d.is_dir() and d.name.startswith('eval')]
    
    if not eval_dirs:
        print(f"Warning: No eval directories found in {iteration_dir}")
        return None
    
    print(f"Found {len(eval_dirs)} eval cases in iteration {iteration}")
    
    # Collect data
    with_skill_data = {
        "pass_rates": [],
        "times": [],
        "tokens": [],
        "eval_cases": []
    }
    
    without_skill_data = {
        "pass_rates": [],
        "times": [],
        "tokens": [],
        "eval_cases": []
    }
    
    for eval_dir in sorted(eval_dirs):
        eval_name = eval_dir.name
        
        # Process with_skill
        with_skill_dir = eval_dir / "with_skill"
        timing_file = with_skill_dir / "timing.json"
        grading_file = with_skill_dir / "grading.json"
        
        timing = load_timing_data(timing_file)
        grading = load_grading_data(grading_file)
        
        if timing and grading:
            case_data = {
                "eval_name": eval_name,
                "pass_rate": grading.get("pass_rate", 0),
                "total_tokens": timing.get("total_tokens", 0),
                "duration_ms": timing.get("duration_ms", 0)
            }
            
            with_skill_data["eval_cases"].append(case_data)
            with_skill_data["pass_rates"].append(case_data["pass_rate"])
            with_skill_data["tokens"].append(case_data["total_tokens"])
            with_skill_data["times"].append(case_data["duration_ms"] / 1000)  # Convert to seconds
        
        # Process without_skill
        without_skill_dir = eval_dir / "without_skill"
        timing_file = without_skill_dir / "timing.json"
        grading_file = without_skill_dir / "grading.json"
        
        timing = load_timing_data(timing_file)
        grading = load_grading_data(grading_file)
        
        if timing and grading:
            case_data = {
                "eval_name": eval_name,
                "pass_rate": grading.get("pass_rate", 0),
                "total_tokens": timing.get("total_tokens", 0),
                "duration_ms": timing.get("duration_ms", 0)
            }
            
            without_skill_data["eval_cases"].append(case_data)
            without_skill_data["pass_rates"].append(case_data["pass_rate"])
            without_skill_data["tokens"].append(case_data["total_tokens"])
            without_skill_data["times"].append(case_data["duration_ms"] / 1000)
    
    # Compute statistics
    def compute_stats(values: List[float]) -> Dict[str, float]:
        if not values:
            return {"mean": 0, "stddev": 0, "min": 0, "max": 0, "count": 0}
        
        return {
            "mean": mean(values),
            "stddev": stdev(values) if len(values) > 1 else 0,
            "min": min(values),
            "max": max(values),
            "count": len(values)
        }
    
    with_skill_stats = {
        "pass_rate": compute_stats(with_skill_data["pass_rates"]),
        "time_seconds": compute_stats(with_skill_data["times"]),
        "tokens": compute_stats(with_skill_data["tokens"]),
        "eval_cases": with_skill_data["eval_cases"]
    }
    
    without_skill_stats = {
        "pass_rate": compute_stats(without_skill_data["pass_rates"]),
        "time_seconds": compute_stats(without_skill_data["times"]),
        "tokens": compute_stats(without_skill_data["tokens"]),
        "eval_cases": without_skill_data["eval_cases"]
    }
    
    # Compute delta
    delta = {
        "pass_rate": with_skill_stats["pass_rate"]["mean"] - without_skill_stats["pass_rate"]["mean"],
        "time_seconds": with_skill_stats["time_seconds"]["mean"] - without_skill_stats["time_seconds"]["mean"],
        "tokens": with_skill_stats["tokens"]["mean"] - without_skill_stats["tokens"]["mean"]
    }
    
    return {
        "iteration": iteration,
        "timestamp": "",  # Could add actual timestamp
        "summary": {
            "total_eval_cases": len(eval_dirs),
            "with_skill_cases": len(with_skill_data["eval_cases"]),
            "without_skill_cases": len(without_skill_data["eval_cases"])
        },
        "with_skill": with_skill_stats,
        "without_skill": without_skill_stats,
        "delta": delta
    }


def format_benchmark_report(benchmark: Dict[str, Any]) -> str:
    """Format benchmark data as a readable report."""
    lines = []
    
    lines.append("=" * 70)
    lines.append("BENCHMARK REPORT")
    lines.append("=" * 70)
    lines.append("")
    lines.append(f"Iteration: {benchmark['iteration']}")
    lines.append(f"Eval cases: {benchmark['summary']['total_eval_cases']}")
    lines.append("")
    
    # With skill
    ws = benchmark['with_skill']
    lines.append("WITH SKILL")
    lines.append("-" * 70)
    lines.append(f"  Pass rate: {ws['pass_rate']['mean']:.1%} (±{ws['pass_rate']['stddev']:.1%})")
    lines.append(f"  Time: {ws['time_seconds']['mean']:.1f}s (±{ws['time_seconds']['stddev']:.1f}s)")
    lines.append(f"  Tokens: {ws['tokens']['mean']:.0f} (±{ws['tokens']['stddev']:.0f})")
    lines.append("")
    
    # Without skill
    wos = benchmark['without_skill']
    lines.append("WITHOUT SKILL (Baseline)")
    lines.append("-" * 70)
    lines.append(f"  Pass rate: {wos['pass_rate']['mean']:.1%} (±{wos['pass_rate']['stddev']:.1%})")
    lines.append(f"  Time: {wos['time_seconds']['mean']:.1f}s (±{wos['time_seconds']['stddev']:.1f}s)")
    lines.append(f"  Tokens: {wos['tokens']['mean']:.0f} (±{wos['tokens']['stddev']:.0f})")
    lines.append("")
    
    # Delta
    delta = benchmark['delta']
    lines.append("DELTA (Skill Impact)")
    lines.append("-" * 70)
    
    pass_change = delta['pass_rate'] * 100
    time_change = delta['time_seconds']
    token_change = delta['tokens']
    
    pass_emoji = "📈" if pass_change > 0 else "📉" if pass_change < 0 else "➡️"
    time_emoji = "⚡" if time_change < 0 else "🐌" if time_change > 0 else "➡️"
    token_emoji = "📉" if token_change < 0 else "📈" if token_change > 0 else "➡️"
    
    lines.append(f"  {pass_emoji} Pass rate: {pass_change:+.1f} percentage points")
    lines.append(f"  {time_emoji} Time: {time_change:+.1f} seconds")
    lines.append(f"  {token_emoji} Tokens: {token_change:+.0f}")
    lines.append("")
    
    # Interpretation
    lines.append("INTERPRETATION")
    lines.append("-" * 70)
    
    if pass_change > 0.2:
        lines.append("  ✅ Significant quality improvement from skill")
    elif pass_change > 0:
        lines.append("  ✓ Modest quality improvement")
    elif pass_change > -0.1:
        lines.append("  ➡️ No significant change in quality")
    else:
        lines.append("  ⚠️ Quality regression detected")
    
    if abs(time_change) < 5:
        lines.append("  ➡️ No significant time impact")
    elif time_change > 0:
        lines.append(f"  🐌 Takes {abs(time_change):.1f}s longer (is the improvement worth it?)")
    else:
        lines.append(f"  ⚡ {abs(time_change):.1f}s faster")
    
    if abs(token_change) < 500:
        lines.append("  ➡️ No significant token cost impact")
    elif token_change > 0:
        lines.append(f"  📈 Uses {abs(token_change):.0f} more tokens (cost consideration)")
    else:
        lines.append(f"  📉 Uses {abs(token_change):.0f} fewer tokens")
    
    lines.append("")
    lines.append("=" * 70)
    
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description="Aggregate benchmark statistics from evaluation results"
    )
    parser.add_argument(
        "--workspace", "-w",
        required=True,
        help="Path to the workspace directory"
    )
    parser.add_argument(
        "--iteration", "-i",
        type=int,
        required=True,
        help="Iteration number to aggregate"
    )
    parser.add_argument(
        "--output", "-o",
        help="Output file path for benchmark JSON"
    )
    parser.add_argument(
        "--report", "-r",
        action="store_true",
        help="Print formatted report to stdout"
    )
    
    args = parser.parse_args()
    
    workspace_path = Path(args.workspace)
    
    if not workspace_path.exists():
        print(f"Error: Workspace directory does not exist: {workspace_path}")
        sys.exit(1)
    
    benchmark = aggregate_iteration(workspace_path, args.iteration)
    
    if not benchmark:
        sys.exit(1)
    
    # Print report
    if args.report or not args.output:
        print(format_benchmark_report(benchmark))
    
    # Save to file
    if args.output:
        output_path = Path(args.output)
        with open(output_path, 'w') as f:
            json.dump(benchmark, f, indent=2)
        print(f"\nBenchmark data saved to: {output_path}")
    
    sys.exit(0)


if __name__ == "__main__":
    main()
