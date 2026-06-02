# BIF Commit Workflow

Use this workflow when preparing a commit in this repo.

If you edit this shared workflow, re-run `scripts/sync-agent-config.ps1` so the Claude and Kilo wrappers stay aligned.

## 1. Gather Context

Start from the live repo state, not stale pasted output.

Review at minimum:

- current git status
- current diff against `HEAD`
- current branch
- recent commits for context

## 2. Run Pre-Commit Checks

Run these in parallel when practical:

- `cargo build`
- `cargo clippy -- -D warnings`
- `cargo fmt --check`

If code changed materially, run targeted tests for the affected area. If a required check cannot run, report that clearly.

## 3. Analyze The Change

- Identify what changed.
- Identify why it changed.
- Draft a concise commit message focused on why.

## 4. Stage And Commit Carefully

- Stage only the intended files.
- Do not use blanket staging when unrelated changes exist.
- Keep commits focused.

## 5. Update Project Docs When Needed

Check the touched scope against:

- `BUGLIST.md`
- `CHANGELOG.md`
- `MILESTONES.md`
- `ROADMAP_DETAIL.md`
- `SESSION_HANDOFF.md` — add a new session entry at the top; keep only the 5 most recent sessions. When adding a 6th, move the oldest entry to `docs/archive/SESSION_HANDOFF_ARCHIVE.md` (prepend after the archive header).
- `devlog/`

Update only the docs that are genuinely affected by the change.

## 6. Keep The Wiki In Sync

For non-obvious design work, architecture changes, or new reusable patterns:

- add or update the relevant wiki note
- update section `_index.md` files when new articles are added
- keep frontmatter `updated:` dates current

## 7. Regenerate The Site If Needed

If the touched docs feed the generated site, run the site generation step and stage any resulting output that belongs with the change.

## 8. Verify Before Finishing

- confirm the intended files are staged or committed
- confirm the working tree state is what you expect
- report checks run, checks skipped, and residual risk
