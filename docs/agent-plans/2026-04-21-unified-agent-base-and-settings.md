# Simplified Shared Agent Config Base

## Summary

Create a small repo-owned shared instruction layer for Claude, Codex, and Kilo, and keep tool-specific settings files only where they are genuinely required. Do not build a CLI registry or dev-environment management layer. The repo will standardize shared prompts, workflows, roles, and handoff format; actual CLI invocation remains manual and user-driven.

Use a dedicated short-lived branch for this work:

`agent-config-unification`

## Key Changes

### 1. Add a minimal shared source tree under `agents/`

Create a top-level `agents/` folder with canonical shared docs:

- `agents/base/PROJECT.md`
- `agents/base/HANDOFF_CONTRACT.md`
- `agents/roles/planner.md`
- `agents/roles/executor.md`
- `agents/roles/reviewer.md`
- `agents/roles/vfx-code-reviewer.md`
- `agents/workflows/bif-commit.md`
- `agents/workflows/debug-issue.md`
- `agents/workflows/review-changes.md`
- `agents/workflows/refactor-safely.md`
- `agents/workflows/explore-codebase.md`

Keep domain knowledge in normal repo docs. Shared agent docs should reference `CLAUDE.md`, `SESSION_HANDOFF.md`, `MILESTONES.md`, `BUGLIST.md`, and wiki docs instead of copying repo facts.

### 2. Keep tool-specific settings, but only thin and shared where reasonable

Treat these as thin adapters around shared repo content:

- `.claude/agents/vfx-code-reviewer.md`
- `.claude/skills/*.md`
- `.claude/commands/bif-commit.md`
- `.kilo/agents/vfx-code-reviewer.md`
- `.kilo/skills/bif-commit/SKILL.md`

Settings/config files stay tool-specific and are only partially shared:

- `.claude/settings.json`
- `.claude/settings.local.json`
- `kilo.jsonc`
- `.mcp.json`

Rules:

- Shared behavior and workflow text lives under `agents/`
- Tool-specific files keep only the syntax, frontmatter, or config shape each tool needs
- Do not force full unification of permissions, hooks, or CLI config when the tools differ materially
- `.mcp.json` remains the repo MCP manifest
- User runs CLIs manually as needed; the plan does not attempt to model or manage installed CLIs

### 3. Add a light sync script for wrappers, not environment management

Add a non-destructive sync script:

- `scripts/sync-agent-config.ps1`

Behavior:

- Read canonical files under `agents/`
- Regenerate or update only the thin shared-content wrappers for Claude and Kilo
- Keep Codex on the repo-owned `agents/` docs directly; no separate generated skill layer is required yet

Flags:

- `-Claude`
- `-Kilo`
- `-Codex`
- `-All`
- `-WhatIf`

Non-goals for this script:

- no CLI install/remove logic
- no machine provisioning
- no automatic permission or hook unification

### 4. Standardize file-based Claude-plan / Codex-exec handoff

Use one repo-owned handoff path:

- Claude writes plans using `agents/base/HANDOFF_CONTRACT.md`
- Save execution handoffs under `docs/agent-handoffs/`
- Filename format: `YYYY-MM-DD-<slug>.md`
- Codex starts by reading the handoff plus referenced workflow docs
- Codex reports back against the same acceptance criteria

Keep architecture plans separate from execution handoffs:

- architecture or shared-agent plans go in `docs/agent-plans/`
- execution handoffs go in `docs/agent-handoffs/`

## Branch And Commit Strategy

Work on a dedicated branch:

- `agent-config-unification`

Recommended commit split:

1. Save the architecture plan and add the `docs/agent-plans/` convention.
2. Add canonical shared docs under `agents/`.
3. Convert Claude and Kilo wrappers to thin adapters around `agents/`.
4. Add `scripts/sync-agent-config.ps1`.
5. Do parity cleanup and workflow verification.

## Test Plan

1. Structural checks
   - Confirm every kept Claude skill, command, and agent has a canonical source in `agents/`
   - Confirm wrappers contain no unique business logic absent from canonical files
   - Confirm `.mcp.json` remains the only canonical MCP manifest
2. Sync checks
   - Run sync with `-WhatIf` and inspect planned changes
   - Run sync for Claude and Kilo and verify outputs match canonical source intent
   - Re-run sync and confirm idempotence
3. Workflow checks
   - Save this plan under `docs/agent-plans/`
   - Create one sample execution handoff under `docs/agent-handoffs/` when the first plan handoff is needed
   - Validate that Codex can execute from the handoff plus shared workflow docs without relying on hidden `.claude/*` logic
4. Regression checks
   - Verify current `bif-commit` workflow still covers build, clippy, fmt, BUGLIST, CHANGELOG, SESSION_HANDOFF, devlog, wiki, and site upkeep
   - Verify `vfx-code-reviewer` guidance remains equivalent across Claude and Kilo wrappers

## Assumptions

- Canonical shared behavior lives in the repo, not in user-home tool directories
- v1 unifies prompts, roles, workflows, and handoff format only
- v1 does not manage developer-machine CLIs or environment setup
- Tool-specific permissions, hooks, and config remain separate unless sharing them is straightforward and low-risk
- Existing repo docs remain the source of domain knowledge; `agents/` is for agent behavior and workflow only
