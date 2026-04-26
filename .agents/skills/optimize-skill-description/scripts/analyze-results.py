#!/usr/bin/env python3
# /// script
# dependencies = [
#   "pyyaml>=6.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Analyze optimization results across iterations.

Compare baseline and current results to identify improvements and regressions.
Generate recommendations for further optimization.

Usage:
    python analyze-results.py --baseline iteration-1/results.json --current iteration-2/results.json --output analysis.md
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List, Any, Optional
from dataclasses import dataclass


@dataclass
class ComparisonResult:
    """Comparison between baseline and current for a single query."""
    query: str
    should_trigger: bool
    baseline_passed: bool
    current_passed: bool
    baseline_rate: float
    current_rate: float
    improvement: bool
    regression: bool
    unchanged: bool


class ResultsAnalyzer:
    """Analyzes and compares optimization results."""
    
    def __init__(self, baseline_path: Optional[str], current_path: str):
        self.baseline_data = self._load_results(baseline_path) if baseline_path else None
        self.current_data = self._load_results(current_path)
        self.comparisons: List[ComparisonResult] = []
        
    def _load_results(self, path: str) -> Optional[Dict]:
        """Load results from JSON file."""
        try:
            with open(path, 'r') as f:
                return json.load(f)
        except Exception as e:
            print(f"Error loading {path}: {e}")
            return None
    
    def _find_result(self, query: str, results: List[Dict]) -> Optional[Dict]:
        """Find result for a specific query."""
        for result in results:
            if result.get("query") == query:
                return result
        return None
    
    def compare(self) -> bool:
        """Compare baseline and current results."""
        if not self.current_data:
            print("Error: Current results could not be loaded")
            return False
        
        current_results = self.current_data.get("results", [])
        baseline_results = self.baseline_data.get("results", []) if self.baseline_data else []
        
        for current in current_results:
            query = current.get("query", "")
            should_trigger = current.get("should_trigger", False)
            current_passed = current.get("passed", False)
            current_rate = current.get("trigger_rate", 0.0)
            
            baseline = self._find_result(query, baseline_results)
            
            if baseline:
                baseline_passed = baseline.get("passed", False)
                baseline_rate = baseline.get("trigger_rate", 0.0)
                
                improvement = not baseline_passed and current_passed
                regression = baseline_passed and not current_passed
                unchanged = baseline_passed == current_passed
            else:
                baseline_passed = False
                baseline_rate = 0.0
                improvement = False
                regression = False
                unchanged = True  # No baseline to compare
            
            self.comparisons.append(ComparisonResult(
                query=query,
                should_trigger=should_trigger,
                baseline_passed=baseline_passed,
                current_passed=current_passed,
                baseline_rate=baseline_rate,
                current_rate=current_rate,
                improvement=improvement,
                regression=regression,
                unchanged=unchanged and baseline is not None
            ))
        
        return True
    
    def generate_analysis(self) -> Dict[str, Any]:
        """Generate comprehensive analysis."""
        if not self.comparisons:
            return {}
        
        # Overall statistics
        total = len(self.comparisons)
        improvements = sum(1 for c in self.comparisons if c.improvement)
        regressions = sum(1 for c in self.comparisons if c.regression)
        unchanged = sum(1 for c in self.comparisons if c.unchanged)
        
        # Positive queries (should trigger)
        positive = [c for c in self.comparisons if c.should_trigger]
        positive_improved = sum(1 for c in positive if c.improvement)
        positive_regressed = sum(1 for c in positive if c.regression)
        
        # Negative queries (should not trigger)
        negative = [c for c in self.comparisons if not c.should_trigger]
        negative_improved = sum(1 for c in negative if c.improvement)
        negative_regressed = sum(1 for c in negative if c.regression)
        
        # Trigger rate changes
        rate_changes = [c.current_rate - c.baseline_rate for c in self.comparisons]
        avg_rate_change = sum(rate_changes) / len(rate_changes) if rate_changes else 0
        
        analysis = {
            "overall": {
                "total_queries": total,
                "improvements": improvements,
                "regressions": regressions,
                "unchanged": unchanged,
                "net_change": improvements - regressions,
                "average_trigger_rate_change": avg_rate_change
            },
            "positive_queries": {
                "count": len(positive),
                "improvements": positive_improved,
                "regressions": positive_regressed,
                "now_passing": sum(1 for c in positive if c.current_passed),
                "still_failing": sum(1 for c in positive if not c.current_passed)
            },
            "negative_queries": {
                "count": len(negative),
                "improvements": negative_improved,
                "regressions": negative_regressed,
                "now_passing": sum(1 for c in negative if c.current_passed),
                "still_failing": sum(1 for c in negative if not c.current_passed)
            },
            "improved_queries": [
                {
                    "query": c.query,
                    "type": "positive" if c.should_trigger else "negative",
                    "baseline_rate": c.baseline_rate,
                    "current_rate": c.current_rate
                }
                for c in self.comparisons if c.improvement
            ],
            "regressed_queries": [
                {
                    "query": c.query,
                    "type": "positive" if c.should_trigger else "negative",
                    "baseline_rate": c.baseline_rate,
                    "current_rate": c.current_rate
                }
                for c in self.comparisons if c.regression
            ],
            "persistent_failures": [
                {
                    "query": c.query,
                    "type": "positive" if c.should_trigger else "negative",
                    "rate": c.current_rate
                }
                for c in self.comparisons 
                if not c.current_passed and not c.improvement and not c.regression
            ]
        }
        
        return analysis
    
    def generate_recommendations(self) -> List[str]:
        """Generate recommendations based on analysis."""
        analysis = self.generate_analysis()
        recommendations = []
        
        if not analysis:
            return recommendations
        
        # Check for overall trends
        net_change = analysis["overall"]["net_change"]
        if net_change > 0:
            recommendations.append(f"✓ Overall improvement: {net_change} more queries now pass")
        elif net_change < 0:
            recommendations.append(f"⚠ Overall regression: {abs(net_change)} queries no longer pass")
        
        # Check positive queries
        pos = analysis["positive_queries"]
        if pos["still_failing"] > 0:
            recommendations.append(
                f"⚠ {pos['still_failing']} positive queries still failing. "
                "Consider broadening description to cover these cases."
            )
        
        if pos["improvements"] > 0:
            recommendations.append(
                f"✓ {pos['improvements']} positive queries improved. "
                "The description now catches previously missed cases."
            )
        
        # Check negative queries
        neg = analysis["negative_queries"]
        if neg["regressions"] > 0:
            recommendations.append(
                f"⚠ {neg['regressions']} negative queries now trigger incorrectly. "
                "Add specificity to exclude these cases."
            )
        
        # Check for patterns in failures
        persistent = analysis.get("persistent_failures", [])
        positive_failures = [f for f in persistent if f["type"] == "positive"]
        negative_failures = [f for f in persistent if f["type"] == "negative"]
        
        if len(positive_failures) >= 3:
            recommendations.append(
                f"⚠ {len(positive_failures)} positive queries consistently failing. "
                "These may indicate missing keywords or concepts in the description."
            )
        
        if len(negative_failures) >= 3:
            recommendations.append(
                f"⚠ {len(negative_failures)} negative queries consistently triggering. "
                "These may indicate the description is too broad."
            )
        
        # Rate change analysis
        avg_change = analysis["overall"]["average_trigger_rate_change"]
        if abs(avg_change) > 0.1:
            direction = "increased" if avg_change > 0 else "decreased"
            recommendations.append(
                f"ℹ Average trigger rate has {direction} by {abs(avg_change):.2f}. "
                f"This suggests the description scope has shifted {direction}."
            )
        
        return recommendations
    
    def generate_markdown_report(self) -> str:
        """Generate a Markdown report."""
        analysis = self.generate_analysis()
        recommendations = self.generate_recommendations()
        
        if not analysis:
            return "# Analysis Error\n\nCould not generate analysis."
        
        lines = [
            "# Skill Description Optimization Analysis",
            "",
            "## Summary",
            "",
            f"- **Total queries evaluated**: {analysis['overall']['total_queries']}",
            f"- **Improvements**: {analysis['overall']['improvements']}",
            f"- **Regressions**: {analysis['overall']['regressions']}",
            f"- **Net change**: {analysis['overall']['net_change']:+d}",
            "",
            "## Positive Queries (Should Trigger)",
            "",
            f"- **Count**: {analysis['positive_queries']['count']}",
            f"- **Now passing**: {analysis['positive_queries']['now_passing']}",
            f"- **Still failing**: {analysis['positive_queries']['still_failing']}",
            f"- **Improvements**: {analysis['positive_queries']['improvements']}",
            f"- **Regressions**: {analysis['positive_queries']['regressions']}",
            "",
            "## Negative Queries (Should Not Trigger)",
            "",
            f"- **Count**: {analysis['negative_queries']['count']}",
            f"- **Now passing** (correctly not triggering): {analysis['negative_queries']['now_passing']}",
            f"- **Still failing** (incorrectly triggering): {analysis['negative_queries']['still_failing']}",
            f"- **Improvements**: {analysis['negative_queries']['improvements']}",
            f"- **Regressions**: {analysis['negative_queries']['regressions']}",
            "",
            "## Recommendations",
            ""
        ]
        
        for rec in recommendations:
            lines.append(f"- {rec}")
        
        lines.extend([
            "",
            "## Detailed Results",
            "",
            "### Improved Queries",
            "",
            "These queries now pass when they previously failed:",
            ""
        ])
        
        for item in analysis.get("improved_queries", [])[:5]:  # Top 5
            lines.append(f"- **{item['type'].upper()}**: {item['query'][:80]}...")
            lines.append(f"  Rate: {item['baseline_rate']:.2f} → {item['current_rate']:.2f}")
            lines.append("")
        
        if len(analysis.get("improved_queries", [])) > 5:
            lines.append(f"*... and {len(analysis['improved_queries']) - 5} more*")
            lines.append("")
        
        lines.extend([
            "### Regressed Queries",
            "",
            "These queries now fail when they previously passed:",
            ""
        ])
        
        for item in analysis.get("regressed_queries", [])[:5]:  # Top 5
            lines.append(f"- **{item['type'].upper()}**: {item['query'][:80]}...")
            lines.append(f"  Rate: {item['baseline_rate']:.2f} → {item['current_rate']:.2f}")
            lines.append("")
        
        if len(analysis.get("regressed_queries", [])) > 5:
            lines.append(f"*... and {len(analysis['regressed_queries']) - 5} more*")
            lines.append("")
        
        if analysis.get("persistent_failures"):
            lines.extend([
                "### Persistent Failures",
                "",
                "These queries consistently fail and may need special attention:",
                ""
            ])
            
            for item in analysis["persistent_failures"][:5]:
                lines.append(f"- **{item['type'].upper()}** (rate: {item['rate']:.2f}): {item['query'][:80]}...")
            
            lines.append("")
        
        lines.extend([
            "## Next Steps",
            "",
            "1. Review improved queries to understand what changes worked",
            "2. Analyze regressed queries for over-correction",
            "3. Address persistent failures with targeted description changes",
            "4. Re-run evaluation after making changes",
            "5. Compare against validation set to check for overfitting",
            ""
        ])
        
        return "\n".join(lines)
    
    def print_analysis(self):
        """Print analysis to console."""
        analysis = self.generate_analysis()
        recommendations = self.generate_recommendations()
        
        if not analysis:
            print("Error: Could not generate analysis")
            return
        
        print("\n" + "=" * 60)
        print("OPTIMIZATION ANALYSIS")
        print("=" * 60)
        
        print(f"\nOverall:")
        print(f"  Total queries: {analysis['overall']['total_queries']}")
        print(f"  Improvements: {analysis['overall']['improvements']}")
        print(f"  Regressions: {analysis['overall']['regressions']}")
        print(f"  Net change: {analysis['overall']['net_change']:+d}")
        
        print(f"\nPositive queries:")
        pos = analysis['positive_queries']
        print(f"  Passing: {pos['now_passing']}/{pos['count']}")
        print(f"  Improvements: {pos['improvements']}")
        
        print(f"\nNegative queries:")
        neg = analysis['negative_queries']
        print(f"  Correctly not triggering: {neg['now_passing']}/{neg['count']}")
        print(f"  Regressions: {neg['regressions']}")
        
        print(f"\nRecommendations:")
        for rec in recommendations:
            print(f"  {rec}")


def main():
    parser = argparse.ArgumentParser(
        description="Analyze skill description optimization results"
    )
    parser.add_argument(
        "--baseline", "-b",
        help="Path to baseline results JSON (optional)"
    )
    parser.add_argument(
        "--current", "-c",
        required=True,
        help="Path to current results JSON"
    )
    parser.add_argument(
        "--output", "-o",
        help="Output file path for Markdown report"
    )
    parser.add_argument(
        "--json", "-j",
        help="Output file path for JSON analysis"
    )
    
    args = parser.parse_args()
    
    analyzer = ResultsAnalyzer(args.baseline, args.current)
    
    if not analyzer.compare():
        sys.exit(1)
    
    analyzer.print_analysis()
    
    if args.output:
        report = analyzer.generate_markdown_report()
        with open(args.output, 'w') as f:
            f.write(report)
        print(f"\nReport saved to: {args.output}")
    
    if args.json:
        analysis = analyzer.generate_analysis()
        with open(args.json, 'w') as f:
            json.dump(analysis, f, indent=2)
        print(f"JSON analysis saved to: {args.json}")
    
    sys.exit(0)


if __name__ == "__main__":
    main()
