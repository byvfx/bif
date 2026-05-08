---
name: bif-commit
description: Prepare or execute commits in the BIF repo using the repo-owned commit workflow. Use when Codex needs to validate changes, draft a concise why-focused commit message, stage only the intended files, update project docs and devlog/wiki context when needed, and optionally create the git commit.
---

# BIF Commit

Use this skill only inside the BIF repo.

## Read First

Read these repo-owned sources before acting:

- `agents/base/PROJECT.md`
- `agents/workflows/bif-commit.md`

If the current task also comes from a saved handoff, read the handoff file too.

## Workflow

Follow `agents/workflows/bif-commit.md` as the source of truth.

In practice, that means:

1. Gather live git context from the repo.
2. Run the standard checks before staging or committing:
   - `cargo build`
   - `cargo clippy -- -D warnings`
   - `cargo fmt --check`
3. Analyze the diff and draft a why-focused commit message.
4. Stage only the intended files.
5. Update repo docs when the change actually requires it.
6. Keep devlog and wiki in sync for non-trivial changes.
7. Regenerate the site if the touched docs feed it.
8. Verify final git state and report clearly.

## Codex-Specific Rules

- Do not rely on a global `bif-commit` skill when it disagrees with the repo-owned docs.
- Treat this repo-local skill plus the `agents/` docs as the canonical source.
- If the worktree contains unrelated edits, keep them out of staging and call that out.
- Show the intended staged set and commit message before committing unless the user explicitly asked for immediate commit execution.
