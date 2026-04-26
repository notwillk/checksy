#!/usr/bin/env python3
# /// script
# dependencies = [
#   "pyyaml>=6.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Generate eval queries from a skill description.

This script helps create test queries for evaluating skill descriptions.
It generates diverse queries covering different phrasing styles, 
explicitness levels, and complexity.

Usage:
    python generate-eval-queries.py --description "Your skill description" --count 20 --output queries.json

The generated queries include:
- Direct requests
- Indirect/implicit requests  
- Casual phrasing
- Context-heavy requests
- Typos and abbreviations
- Near-miss negatives
"""

import argparse
import json
import random
import sys
from typing import List, Dict, Any


class QueryGenerator:
    """Generates diverse eval queries from skill descriptions."""
    
    # Templates for different query styles
    DIRECT_TEMPLATES = [
        "{action} {target}",
        "Can you {action} {target}?",
        "Please {action} {target}",
        "I need to {action} {target}",
        "Help me {action} {target}",
    ]
    
    INDIRECT_TEMPLATES = [
        "I have {target} and need to {goal}",
        "My manager wants me to {goal} with {target}",
        "I need to {goal} from {target}",
        "Working with {target} to {goal}",
        "The user has {target} and wants to {goal}",
    ]
    
    CASUAL_TEMPLATES = [
        "hey can u {action} {target} for me",
        "need to {action} {target} pls",
        "{action} this {target} real quick",
        "can you like {action} {target}?",
        "help with {target} thx",
    ]
    
    COMPLEX_TEMPLATES = [
        "{action} {target} and then {secondary_action}",
        "I have {target} in {location} with {details} — can you {goal}?",
        "First {action} {target}, then {secondary_action}, and finally {tertiary_action}",
        "My {source} contains {target} with {details}. I need to {goal} and {secondary_goal}.",
    ]
    
    TYPO_PATTERNS = [
        ("analyze", ["anlyze", "analyse", "anlze"]),
        ("spreadsheet", ["spredsheet", "spreadhseet", "sprdsheet"]),
        ("calculate", ["calculte", "calculat", "clculate"]),
        ("document", ["documnet", "docment", "dcoument"]),
        ("process", ["proces", "procss", "porcess"]),
        ("extract", ["extrct", "extact", "extrat"]),
        ("generate", ["genrate", "generat", "generte"]),
        ("convert", ["conert", "convrt", "conver"]),
    ]
    
    def __init__(self, description: str, count: int = 20):
        self.description = description
        self.count = count
        self.queries: List[Dict[str, Any]] = []
        
    def _extract_key_terms(self) -> Dict[str, List[str]]:
        """Extract key terms from the description."""
        # This is a simplified extraction
        # In a real implementation, you might use NLP or the user provides terms
        desc_lower = self.description.lower()
        
        terms = {
            "actions": [],
            "targets": [],
            "goals": [],
            "details": []
        }
        
        # Common action verbs
        action_keywords = [
            "analyze", "process", "extract", "generate", "create", "build",
            "convert", "transform", "compute", "calculate", "validate",
            "review", "check", "test", "deploy", "migrate"
        ]
        
        # Common target nouns
        target_keywords = [
            "csv", "pdf", "json", "data", "file", "document", "spreadsheet",
            "database", "api", "code", "test", "report", "chart", "image"
        ]
        
        # Extract from description
        for keyword in action_keywords:
            if keyword in desc_lower:
                terms["actions"].append(keyword)
        
        for keyword in target_keywords:
            if keyword in desc_lower:
                terms["targets"].append(keyword)
        
        # Default fallbacks
        if not terms["actions"]:
            terms["actions"] = ["process", "analyze", "handle"]
        if not terms["targets"]:
            terms["targets"] = ["file", "data", "document"]
        
        terms["goals"] = ["understand it", "get insights", "extract information", "fix it"]
        terms["details"] = ["columns", "rows", "metadata", "content"]
        
        return terms
    
    def _apply_typos(self, text: str) -> str:
        """Apply random typo patterns to text."""
        for correct, typos in self.TYPO_PATTERNS:
            if correct in text.lower() and random.random() < 0.3:
                typo = random.choice(typos)
                # Replace one occurrence
                parts = text.lower().split(correct, 1)
                if len(parts) == 2:
                    text = parts[0] + typo + parts[1]
        return text
    
    def _generate_positive(self, terms: Dict[str, List[str]], style: str) -> Dict[str, Any]:
        """Generate a should-trigger query."""
        action = random.choice(terms["actions"])
        target = random.choice(terms["targets"])
        goal = random.choice(terms["goals"])
        details = random.choice(terms["details"])
        
        if style == "direct":
            template = random.choice(self.DIRECT_TEMPLATES)
            query = template.format(action=action, target=target)
            
        elif style == "indirect":
            template = random.choice(self.INDIRECT_TEMPLATES)
            query = template.format(target=target, goal=goal)
            
        elif style == "casual":
            template = random.choice(self.CASUAL_TEMPLATES)
            query = template.format(action=action, target=target)
            query = query.replace("you", "u").replace("please", "pls")
            
        elif style == "complex":
            template = random.choice(self.COMPLEX_TEMPLATES)
            secondary_action = random.choice([a for a in terms["actions"] if a != action])
            query = template.format(
                action=action,
                target=target,
                secondary_action=secondary_action,
                goal=goal,
                location="~/data/",
                details=details,
                source="file",
                secondary_goal="save it"
            )
            
        elif style == "typo":
            template = random.choice(self.DIRECT_TEMPLATES)
            query = template.format(action=action, target=target)
            query = self._apply_typos(query)
            
        else:  # context-heavy
            query = f"I have {target} in ~/Downloads/data_final_v2.{target[:3]} with {details} that need attention — can you {action} it and tell me what you find?"
        
        return {
            "query": query,
            "should_trigger": True,
            "style": style
        }
    
    def _generate_negative(self, terms: Dict[str, List[str]], style: str) -> Dict[str, Any]:
        """Generate a should-not-trigger query (near-miss)."""
        target = random.choice(terms["targets"])
        
        # Adjacent domain queries (share keywords but different intent)
        adjacent_domains = {
            "csv": ["Excel", "JSON", "database", "table in PDF"],
            "pdf": ["Word doc", "scan", "image", "webpage"],
            "json": ["YAML", "XML", "CSV", "config file"],
            "data": ["text", "document", "file", "content"],
            "api": ["library", "module", "package", "function"],
            "code": ["pseudocode", "algorithm", "logic", "design"],
        }
        
        # Create near-misses
        if style == "adjacent":
            if target in adjacent_domains:
                adjacent = random.choice(adjacent_domains[target])
                query = f"Work with this {adjacent} file"
            else:
                query = f"Convert this {target} to a different format"
                
        elif style == "different_action":
            query = f"Delete this {target} file"
            
        elif style == "unrelated":
            unrelated = ["weather today", "fibonacci function", "sorting algorithm", "hello world"]
            query = f"Create a {random.choice(unrelated)}"
            
        else:  # overlapping_keywords
            query = f"Write a script that processes {target} files"
        
        return {
            "query": query,
            "should_trigger": False,
            "style": style
        }
    
    def generate(self) -> List[Dict[str, Any]]:
        """Generate the full set of eval queries."""
        terms = self._extract_key_terms()
        
        # Calculate split (50% positive, 50% negative)
        positive_count = self.count // 2
        negative_count = self.count - positive_count
        
        positive_styles = ["direct", "indirect", "casual", "complex", "typo", "context-heavy"]
        negative_styles = ["adjacent", "different_action", "unrelated", "overlapping_keywords"]
        
        # Generate positive queries
        for i in range(positive_count):
            style = positive_styles[i % len(positive_styles)]
            query = self._generate_positive(terms, style)
            self.queries.append(query)
        
        # Generate negative queries
        for i in range(negative_count):
            style = negative_styles[i % len(negative_styles)]
            query = self._generate_negative(terms, style)
            self.queries.append(query)
        
        # Shuffle to mix positive and negative
        random.shuffle(self.queries)
        
        return self.queries
    
    def split_train_validation(self, train_ratio: float = 0.6) -> tuple:
        """Split queries into train and validation sets."""
        queries = self.queries.copy()
        random.shuffle(queries)
        
        split_point = int(len(queries) * train_ratio)
        train = queries[:split_point]
        validation = queries[split_point:]
        
        return train, validation


def main():
    parser = argparse.ArgumentParser(
        description="Generate eval queries for skill description testing"
    )
    parser.add_argument(
        "--description", "-d",
        required=True,
        help="The skill description to generate queries for"
    )
    parser.add_argument(
        "--count", "-c",
        type=int,
        default=20,
        help="Number of queries to generate (default: 20)"
    )
    parser.add_argument(
        "--output", "-o",
        default="queries.json",
        help="Output file path (default: queries.json)"
    )
    parser.add_argument(
        "--split",
        action="store_true",
        help="Split into train (60%) and validation (40%) sets"
    )
    parser.add_argument(
        "--seed",
        type=int,
        help="Random seed for reproducibility"
    )
    
    args = parser.parse_args()
    
    if args.seed:
        random.seed(args.seed)
    
    # Generate queries
    generator = QueryGenerator(args.description, args.count)
    queries = generator.generate()
    
    output_data = {
        "description": args.description,
        "total_queries": len(queries),
        "queries": queries
    }
    
    # Split if requested
    if args.split:
        train, validation = generator.split_train_validation()
        
        train_data = {
            "description": args.description,
            "split": "train",
            "total_queries": len(train),
            "queries": train
        }
        
        validation_data = {
            "description": args.description,
            "split": "validation",
            "total_queries": len(validation),
            "queries": validation
        }
        
        # Write train set
        train_path = args.output.replace('.json', '_train.json')
        with open(train_path, 'w') as f:
            json.dump(train_data, f, indent=2)
        print(f"Generated {len(train)} train queries: {train_path}")
        
        # Write validation set
        validation_path = args.output.replace('.json', '_validation.json')
        with open(validation_path, 'w') as f:
            json.dump(validation_data, f, indent=2)
        print(f"Generated {len(validation)} validation queries: {validation_path}")
    else:
        # Write single file
        with open(args.output, 'w') as f:
            json.dump(output_data, f, indent=2)
        print(f"Generated {len(queries)} queries: {args.output}")
    
    # Print summary
    positive = sum(1 for q in queries if q["should_trigger"])
    negative = len(queries) - positive
    
    print(f"\nSummary:")
    print(f"  Should-trigger queries: {positive}")
    print(f"  Should-not-trigger queries: {negative}")
    print(f"\nQuery styles included:")
    styles = set(q.get("style", "unknown") for q in queries)
    for style in sorted(styles):
        count = sum(1 for q in queries if q.get("style") == style)
        print(f"  - {style}: {count}")


if __name__ == "__main__":
    main()
