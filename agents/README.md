# Shared Agent Docs

This folder is the repo-owned source of truth for shared agent behavior.

- `base/` holds repo-wide operating context and the planner-to-executor handoff contract.
- `roles/` holds reusable role prompts.
- `workflows/` holds reusable task workflows.
- Tool-specific wrappers in `.claude/` and `.kilo/` should stay thin and point back here.

Keep repo facts in normal project docs such as `CLAUDE.md`, `MILESTONES.md`, `SESSION_HANDOFF.md`, `BUGLIST.md`, and `wiki/`.

After editing shared docs here, re-run `scripts/sync-agent-config.ps1` so Claude and Kilo wrappers stay aligned with the canonical source.

Codex is not auto-injected with these docs at session start. For Codex execution, explicitly point it at:

- `agents/base/PROJECT.md`
- `agents/base/HANDOFF_CONTRACT.md`
- the specific handoff file under `docs/agent-handoffs/`
- any referenced workflow docs under `agents/workflows/`

## Day-To-Day Cheat Sheet

- Edit shared behavior in `agents/`, not in `.claude/` or `.kilo/`.
- Use `docs/agent-plans/` for architecture or tooling plans.
- Use `docs/agent-handoffs/` for execution-ready handoffs.
- Treat `.claude/` and `.kilo/` as thin adapters around the canonical docs here.

## Common Changes

- Commit workflow: `agents/workflows/bif-commit.md`
- Debug workflow: `agents/workflows/debug-issue.md`
- Code exploration workflow: `agents/workflows/explore-codebase.md`
- Review workflow: `agents/workflows/review-changes.md`
- Plan saving workflow: `agents/workflows/write-handoff.md`
- Shared review persona: `agents/roles/vfx-code-reviewer.md`
- Planner/executor contract: `agents/base/HANDOFF_CONTRACT.md`
- Repo-local Codex skills: `agents/generated/codex/`

## Sync

Preview wrapper changes:

```powershell
pwsh -File scripts/sync-agent-config.ps1 -All -WhatIf
```

Apply wrapper changes:

```powershell
pwsh -File scripts/sync-agent-config.ps1 -All
```

Scope sync if needed:

- Claude only: `-Claude`
- Kilo only: `-Kilo`
- Codex generated artifacts only: `-Codex`

Install generated Codex skills into your local Codex skills folder:

```powershell
pwsh -File scripts/install-codex-skills.ps1
```

## Working Rule

If a change is shared behavior, put it in `agents/` first, then sync wrappers.

## Claude Planning Workflow

Claude does not appear to have a repo-local setting that forces built-in plan-mode saves into this repo. The practical workflow is:

1. Plan normally in Claude.
2. Decide whether the result is:
   - an architecture or tooling plan for `docs/agent-plans/`
   - an execution-ready handoff for `docs/agent-handoffs/`
3. Use `/save-plan` to write the repo-owned copy with the correct path and filename.

Examples:

```text
/save-plan Save this architecture plan to docs/agent-plans/2026-04-21-my-plan.md
```

```text
/save-plan Save this as an execution handoff to docs/agent-handoffs/2026-04-21-my-task.md using agents/base/HANDOFF_CONTRACT.md
```

For execution handoffs, keep the content concrete:

- goal
- scope
- files or modules
- constraints
- exact checks to run
- acceptance criteria
- out of scope
- unresolved questions

## Claude To Codex Handoff

When handing implementation from Claude to Codex, give Codex the bootstrap set explicitly:

- `agents/base/PROJECT.md`
- `agents/base/HANDOFF_CONTRACT.md`
- the saved handoff file under `docs/agent-handoffs/`
- any referenced workflow docs under `agents/workflows/`

Suggested Codex prompt:

```text
Read:
- agents/base/PROJECT.md
- agents/base/HANDOFF_CONTRACT.md
- docs/agent-handoffs/YYYY-MM-DD-my-task.md
- agents/workflows/<relevant-workflow>.md

Implement the handoff, run the listed checks, and report any mismatch between the handoff and the repo state.
```

## Codex `bif-commit`

For the full step-by-step setup and usage flow for the repo-local Codex `bif-commit` skill, see:

- `agents/codex-bif-commit.md`
