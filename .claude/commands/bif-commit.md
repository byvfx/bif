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

### 6. Obsidian Wiki Sync (keep it fresh — do not skip)

The Obsidian vault at `wiki/` must stay in sync with each commit. Treat this as a commit gate, not optional polish. A commit that shipped non-obvious design work without a wiki touch is an incomplete commit.

- **Devlog cross-link:** Add a `## Wiki Links` section to any new/updated devlog entry with `[[Article Name]]` wikilinks + a brief reason for each.
- **Journal synthesis:** If today's session made architectural decisions, hit non-obvious gotchas, or wrapped a milestone, create or update a `wiki/journal/YYYY-MM-DD-<slug>.md` entry. Do not let devlogs accumulate without journal synthesis — a 3+ day gap between the newest journal entry and the newest devlog is a smell.
- **New concepts / patterns / ADRs:** If the session introduced a new reusable pattern, crate-level decision, or domain concept, create the article:
  - Architecture decision → `wiki/architecture/adr/NNN-<slug>.md` (next ADR number)
  - Reusable pattern or crate-level design → `wiki/architecture/<slug>.md`
  - Atomic concept note → `wiki/concepts/<slug>.md`
  - Follow `wiki/templates/` for frontmatter (title, type, tags, created, updated).
- **Index updates (mandatory when adding articles):**
  - Add a one-line entry with wikilink to the relevant section `_index.md` (`architecture/_index.md`, `concepts/_index.md`, `journal/_index.md`, etc.).
  - Bump the section `_index.md`'s `updated:` frontmatter date.
  - If article counts on the root `wiki/_index.md` table drift, correct them and bump its `updated:` date too.
- **Frontmatter hygiene:** When editing an existing article, bump its `updated:` date. Use wikilinks `[[...]]` for cross-references per CLAUDE.md.
- **Quick self-check before finishing the commit:**
  1. Does every non-obvious design move in this commit have a wiki home (journal entry, concept note, ADR, or architecture article)?
  2. Do the section `_index.md` files list every article that exists in their folder?
  3. Are the `updated:` dates on touched files current?

If all three are yes, you're done. If not, fix it in the same commit — wiki drift compounds fast.

### 7. Site Update

- Run `bash scripts/generate-site.sh` to regenerate the mdBook site from devlog/docs/changelog
- Check if site/src/ has changes via `git status`
- If site changed, stage `site/src/` and amend the commit (or create a follow-up commit)
- If no site changes, skip silently

### 8. Verify

- Run `git status` to confirm commit succeeded and working tree is clean

Report results concisely.
