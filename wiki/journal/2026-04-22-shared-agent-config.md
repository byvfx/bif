---
title: "Journal: 2026-04-22"
type: journal
tags: [learning]
created: "2026-04-22"
updated: "2026-04-22"
---

# 2026-04-22 — Learning Journal

## What I Learned

- A shared agent-config layer works best when the repo owns behavior and tool-specific files stay thin. Putting the real workflow text under `agents/` makes Claude, Kilo, and Codex converge on the same source of truth without pretending their wrapper formats are interchangeable.
- Codex has a real bootstrap gap that Claude/Kilo do not. The repo can own the generated skill artifact, but Codex still needs either explicit bootstrap prompts or an install step into `~/.codex/skills`.
- The right mental model is three layers, not one:
  - canonical source in `agents/`
  - generated wrappers/artifacts via `scripts/sync-agent-config.ps1`
  - optional Codex local install via `scripts/install-codex-skills.ps1`

## What Surprised Me

- The smallest practical Claude fix was not a magical settings override for plan-mode saves. A repo-local `/save-plan` command is enough to get plans into `docs/agent-plans/` and `docs/agent-handoffs/` without fighting Claude's user-folder defaults.
- `bif-commit` is a good first Codex generated skill because it is repeated enough to justify the extra install path. Not every workflow needs that much ceremony.

## Questions to Explore

- Should more Codex workflows be installable, or should the generated/installable path stay reserved for only the few repeated workflows that really benefit from it?
- Would a later repo-local launcher make Codex bootstrap cleaner, or is explicit handoff/bootstrap already good enough?

## Related

- [[journal/_index]] — journal entry list
- [[2026-04-22-shared-agent-config]] — this entry
- [[2026-04-15-phase-e2-first-pass]] — another example where documenting the real bridge/bootstrapping behavior mattered more than trying to hide the mechanism
