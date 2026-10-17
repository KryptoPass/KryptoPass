#!/usr/bin/env python3
"""Reject tracked changes and untracked files in the current Git worktree."""
import subprocess
import sys

result = subprocess.run(["git", "status", "--porcelain=v1", "--untracked-files=normal"], check=True, capture_output=True, text=True)
if result.stdout:
    print(result.stdout, end="", file=sys.stderr)
    print("Working tree is not clean; include or remove the outstanding changes.", file=sys.stderr)
    sys.exit(1)
