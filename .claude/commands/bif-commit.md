---
allowed-tools: Bash(git *), Bash(cargo *), Read, Edit, Write, Glob
description: BIF project commit workflow with checks and devlog
---

## Context

- Current git status: !`git status`
- Current git diff: !`git diff HEAD`
- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -5`

## Your Task

Follow the BIF commit workflow from CLAUDE.md:

### 1. Pre-commit Checks
Run these in parallel:
- `cargo build -p bif_viewport 2>&1 | tail -5` (no warnings)
- `cargo clippy -p bif_viewport -- -D warnings 2>&1 | tail -5`
- `cargo fmt --check`

If any fail, report and stop.

### 2. Analyze Changes
Based on the diff above:
- Identify what changed and why
- Draft concise commit message (1-2 sentences, focus on "why")

### 3. Stage & Commit
- Stage specific files (NOT `git add -A`)
- Commit with message ending in:
  ```
  Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
  ```

### 4. Update Devlog
- Check if `devlog/DEVLOG_YYYY-MM-DD.md` exists for today
- Append session summary if exists, create if not

### 5. Verify
- Run `git status` to confirm commit succeeded

Report results concisely.
