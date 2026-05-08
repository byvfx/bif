---
name: bif-commit
description: BIF project commit workflow — pre-commit checks, docs update, commit, devlog, site regen, wiki sync. Use when committing changes to the BIF repo.
---

# BIF Commit Workflow

Use this when preparing and committing changes in the BIF repo.

## 1. Gather Git Context

Start from live repo state:
- `git status`
- `git diff HEAD`
- `git branch --show-current`
- `git log --oneline -5`

## 2. Run Pre-Commit Checks

Run in parallel when practical:
- `cargo build`
- `cargo clippy -- -D warnings`
- `cargo fmt --check`
- Targeted tests for affected crates

Gotchas:
- `. .\setup_usd_env.ps1` before `bif_core` tests
- `cargo test -p bif_core -- --test-threads=1` (USD bridge not thread-safe)
- `test_should_restart_no_render` is known flaky

## 3. Analyze The Change

- What changed and why
- Draft concise commit message focused on **why**
- Be extremely concise, sacrifice grammar for brevity

## 4. Stage And Commit Carefully

- Stage only intended files — no blanket staging
- Keep commits focused
- Append `Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>` if applicable

## 5. Update Project Docs

Check touched scope against:
- `CHANGELOG.md` — add `## [Unreleased]` entry
- `SESSION_HANDOFF.md` — update summary, next steps, blockers
- `MILESTONES.md` — if scope changed
- `README.md` — if setup or commands changed
- `CLAUDE.md` — if conventions changed
- `BUGLIST.md` — if bugs fixed or found

## 6. Create Devlog Entry

Create/update `devlog/YYYY-MM/DEVLOG_YYYY-MM-DD.md`:

```markdown
# Development Log - YYYY-MM-DD

## Session Duration
[e.g., 2.5 hours]

## Goals
- What I planned to accomplish

## What I Did
- Changes made, files modified
- Key decisions and why
- Problems and solutions

## Learnings
- New concepts, architecture insights, mistakes

## Next Session
- Immediate next steps
- Blockers/questions
```

## 7. Update Wiki If Needed

For non-obvious design work, architecture changes, or new reusable patterns:
- Add or update relevant wiki note in `wiki/`
- Update section `_index.md` files when adding articles
- Keep frontmatter `updated:` dates current

## 8. Regenerate Site If Needed

If touched docs feed the generated site (`site/`):
```bash
bash scripts/generate-site.sh
```
Stage any resulting output.

## 9. Verify Before Finishing

- Confirm intended files are staged/committed
- Confirm working tree state is as expected
- Report checks run, checks skipped, residual risk

## 10. Confirm

Report:
- Commit hash
- Files changed
- Docs updated
- Checks passed/skipped
- Residual risk
