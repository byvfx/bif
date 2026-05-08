# BIF Project Guidance

This file is the shared repo-wide operating context for agent wrappers.

## Repo Shape

BIF is a USD orchestration tool for VFX: layer-aware USD editing, procedural scene assembly, and integrated rendering.

- Prefer clarity and maintainability over cleverness.
- Push back on over-engineered or poorly scoped work.
- Keep subsystem APIs UI-agnostic unless the current milestone explicitly says otherwise.

## Source Of Truth

Do not duplicate fast-changing repo facts here. Read the live project docs when needed:

- `CLAUDE.md` for repo conventions, checks, and current working norms
- `MILESTONES.md` for active release scope
- `SESSION_HANDOFF.md` for latest state and next steps
- `BUGLIST.md` for known issues
- `wiki/_index.md` for Obsidian knowledge-base navigation

## Checks And Validation

When code changes are involved, default to the normal Rust validation path unless the handoff says otherwise:

- `cargo build`
- `cargo clippy -- -D warnings`
- `cargo fmt --check`
- targeted `cargo test ...`

Remember the USD environment requirements from `CLAUDE.md` before running `bif_core` tests or loading USD scenes.

## Documentation Hygiene

For non-trivial changes, keep the repo docs in sync where relevant:

- `CHANGELOG.md`
- `SESSION_HANDOFF.md`
- `MILESTONES.md`
- `devlog/`
- `wiki/`

Use the wiki for non-obvious design decisions, reusable patterns, and architecture notes rather than stuffing those details into tool wrappers.

## Tooling Expectations

- Use available repo search and code-navigation tools first.
- Prefer repo-local docs over memory when project-specific behavior matters.
- Treat MCP, hooks, and permissions as tool-specific wiring; keep shared behavior here.
