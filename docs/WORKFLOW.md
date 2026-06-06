# BIF Workflow

Quick reference. Full rules live in [CLAUDE.md](../CLAUDE.md) (`Workflow` section).

## Feature / bugfix

1. Branch: `git switch -c <type>/<short-desc>` (`feat/`, `fix/`, `refactor/`, `docs/`).
2. Commit in focused increments. Per-commit gate: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` green.
3. Once per branch: update CHANGELOG `[Unreleased]`, devlog, SESSION_HANDOFF.
4. Open PR: `gh pr create`. CI `check` + auto Claude review run on the diff.
5. Review the PR diff: run `/vfx-code-reviewer` (use `/code-review ultra` for big/risky PRs). Address + push fixes.
6. Squash-merge when green + reviewed: `gh pr merge --squash --delete-branch`.

## Trivial change (typo, version bump, docs/devlog only)

- May commit straight to `main` — no branch/PR needed.

## Release `vX.Y.Z` (from `main`)

1. Bump `[workspace.package] version` in `Cargo.toml`; `cargo update --workspace`.
2. Stamp CHANGELOG: `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD`; leave a fresh empty `[Unreleased]`.
3. Commit `release: vX.Y.Z`, push `main`.
4. Tag: `git tag -a vX.Y.Z -m ...` && `git push origin vX.Y.Z`.
5. CI Release job builds the Windows zip + pulls notes from the `## [X.Y.Z]` changelog section.
6. Smoke-run the published zip locally (CI is headless — can't launch the exe).
