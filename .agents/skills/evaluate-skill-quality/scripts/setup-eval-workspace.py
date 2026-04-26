#!/usr/bin/env python3
# /// script
# dependencies = [
#   "pyyaml>=6.0",
# ]
# requires-python = ">=3.8"
# ///

"""
Initialize evaluation workspace structure for skill testing.

This script sets up the directory structure for running skill evaluations.

Usage:
    python setup-eval-workspace.py --skill ./my-skill --workspace ./my-skill-workspace --iteration 1
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Optional


def setup_workspace(skill_path: Path, workspace_path: Path, iteration: int) -> bool:
    """
    Set up the evaluation workspace structure.
    
    Creates:
    - workspace/iteration-{N}/ directory structure
    - Subdirectories for each eval case
    - Placeholder files for outputs, timing, grading
    """
    
    # Validate skill path
    if not skill_path.exists():
        print(f"Error: Skill path does not exist: {skill_path}")
        return False
    
    skill_md = skill_path / "SKILL.md"
    if not skill_md.exists():
        print(f"Error: SKILL.md not found in skill directory: {skill_path}")
        return False
    
    # Create workspace structure
    iteration_dir = workspace_path / f"iteration-{iteration}"
    
    try:
        # Create main directories
        iteration_dir.mkdir(parents=True, exist_ok=True)
        
        # Load evals.json if it exists
        evals_file = skill_path / "evals" / "evals.json"
        eval_cases = []
        
        if evals_file.exists():
            try:
                with open(evals_file, 'r') as f:
                    eval_data = json.load(f)
                    if isinstance(eval_data, dict) and 'evals' in eval_data:
                        eval_cases = eval_data['evals']
                    elif isinstance(eval_data, list):
                        eval_cases = eval_data
            except Exception as e:
                print(f"Warning: Could not load evals.json: {e}")
        
        # Create structure for each eval case
        for i, eval_case in enumerate(eval_cases, 1):
            eval_name = eval_case.get('name') or eval_case.get('id') or f"eval-{i}"
            if isinstance(eval_name, int):
                eval_name = f"eval-{eval_name}"
            
            # Sanitize eval name for filesystem
            eval_name = str(eval_name).replace(' ', '-').replace('/', '-')
            
            eval_dir = iteration_dir / eval_name
            
            # Create with_skill and without_skill subdirectories
            for config in ['with_skill', 'without_skill']:
                config_dir = eval_dir / config
                outputs_dir = config_dir / 'outputs'
                
                outputs_dir.mkdir(parents=True, exist_ok=True)
                
                # Create placeholder files
                (config_dir / 'timing.json').write_text(json.dumps({
                    "status": "not_run",
                    "note": "Run the skill and fill in timing data"
                }, indent=2))
                
                (config_dir / 'grading.json').write_text(json.dumps({
                    "status": "not_graded",
                    "note": "Run grading after skill execution"
                }, indent=2))
                
                # Create .gitkeep in outputs directory
                (outputs_dir / '.gitkeep').touch()
        
        # If no eval cases defined, create a template structure
        if not eval_cases:
            print("No evals.json found. Creating template structure...")
            template_dir = iteration_dir / "eval-template"
            
            for config in ['with_skill', 'without_skill']:
                config_dir = template_dir / config
                outputs_dir = config_dir / 'outputs'
                outputs_dir.mkdir(parents=True, exist_ok=True)
                
                (config_dir / 'timing.json').write_text(json.dumps({
                    "status": "not_run",
                    "note": "Run the skill and fill in timing data"
                }, indent=2))
                
                (config_dir / 'grading.json').write_text(json.dumps({
                    "status": "not_graded",
                    "note": "Run grading after skill execution"
                }, indent=2))
                
                (outputs_dir / '.gitkeep').touch()
        
        # Create iteration metadata
        metadata = {
            "iteration": iteration,
            "skill_path": str(skill_path.resolve()),
            "workspace_path": str(workspace_path.resolve()),
            "eval_cases": len(eval_cases),
            "status": "initialized"
        }
        
        (iteration_dir / 'iteration.json').write_text(json.dumps(metadata, indent=2))
        
        print(f"Workspace initialized: {iteration_dir}")
        print(f"  Skill: {skill_path.name}")
        print(f"  Eval cases: {len(eval_cases) if eval_cases else 'template (no evals.json found)'}")
        print(f"  Structure: iteration-{iteration}/eval-*/{{with_skill,without_skill}}/")
        
        return True
        
    except Exception as e:
        print(f"Error setting up workspace: {e}")
        return False


def main():
    parser = argparse.ArgumentParser(
        description="Initialize evaluation workspace for skill testing"
    )
    parser.add_argument(
        "--skill", "-s",
        required=True,
        help="Path to the skill directory"
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
        help="Iteration number (1, 2, 3, ...)"
    )
    
    args = parser.parse_args()
    
    skill_path = Path(args.skill)
    workspace_path = Path(args.workspace)
    
    if setup_workspace(skill_path, workspace_path, args.iteration):
        print("\nNext steps:")
        print(f"  1. Ensure evals/evals.json exists in your skill directory")
        print(f"  2. Run the skill for each eval case:")
        print(f"     - With skill: save outputs to iteration-{args.iteration}/eval-*/with_skill/outputs/")
        print(f"     - Without skill: save outputs to iteration-{args.iteration}/eval-*/without_skill/outputs/")
        print(f"  3. Record timing data in timing.json")
        print(f"  4. Run grading with grade-assertions.py")
        print(f"  5. Aggregate results with aggregate-benchmarks.py")
        sys.exit(0)
    else:
        sys.exit(1)


if __name__ == "__main__":
    main()
