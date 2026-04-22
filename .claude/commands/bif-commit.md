---
allowed-tools: Bash(git *), Bash(cargo *), Read, Edit, Write, Glob
description: BIF project commit workflow with checks and devlog
---

Follow the canonical workflow in `agents/workflows/bif-commit.md`.

Tool-specific notes:

- Use live repo context rather than pasted snapshots:
  - `git status`
  - `git diff HEAD`
  - `git branch --show-current`
  - `git log --oneline -5`
- If Claude creates the commit, append:

```
Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```
