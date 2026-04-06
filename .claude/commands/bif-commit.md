---
allowed-tools: Bash(git *), Bash(cargo *), Read, Edit, Write, Glob
description: BIF project commit workflow with checks and devlog
---

# Context

- Current git status: !`git status`
- Current git diff: !`git diff HEAD`
- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -5`

## Your Task

Follow the BIF commit workflow from CLAUDE.md:

### 1. Pre-commit Checks

Run these in parallel (check all modified crates, not just bif_viewport):

- `cargo build 2>&1 | tail -5` (no warnings)
- `cargo clippy -- -D warnings 2>&1 | tail -5`
- `cargo fmt --check`

If any fail, report and stop.

### 2. Analyze Changes

Based on the diff above:

- Identify what changed and why
- Draft concise commit message (1-2 sentences, focus on "why")

### 3. Stage & Commit

- Stage specific files (NOT `git add -A`)
- Commit with message ending in:

 txt```
  Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>

 txt```

### 4. BUGLIST Check

- Read `BUGLIST.md` and compare against the changes being committed:
  - Were any **active bugs fixed** by this work? → Move them to the `## Fixed` section with a date note.
  - Were any **new bugs discovered** during this session? → Add them to `## Active Bugs`.
  - Were any **investigate items resolved** or invalidated? → Update or remove them.
- If no BUGLIST changes are needed, skip silently (don't add noise).

### 5. Update Documentation and Devlog

- Update `CHANGELOG.md`: add entries under `## [Unreleased]` in the appropriate section (Added/Changed/Fixed). Keep entries concise (one line each). Do NOT create a new version heading — that happens at release time.
- Update `MILESTONES.md` if a version's status changed (e.g., mark version complete, update in-progress)
- Update `ROADMAP_DETAIL.md` if tasks within the current version were completed
- Update `SESSION_HANDOFF.md` if relevant (e.g. note any important context for next session)
- Note: `MILESTONES_HISTORY.md` is only updated at release time (when moving a completed version entry)
- Add a new entry to `devlog/DEVLOG_YYYY-MM-DD.md` with today's date, summarizing the session:
  - Duration
  - Goals
  - What was done (high-level summary)
  - Any issues found
  - Current state of the project
  - Learnings
  - Next steps for next session

- Check if `devlog/DEVLOG_YYYY-MM-DD.md` exists for today, if one exists, append to it; if not, create it with today's date
- Append session summary if exists, create if not

### 6. Wiki Links

- Add `## Wiki Links` section to any new or updated devlog entries with Obsidian wikilinks to relevant wiki articles
- If new concepts, architecture decisions, or domain knowledge were learned this session, create/update articles in `wiki/`
- Update the relevant section `_index.md` if new wiki articles were added
- Use `[[Article Name]]` wikilink format with a brief reason for each link

### 7. Verify

- Run `git status` to confirm commit succeeded

Report results concisely.
