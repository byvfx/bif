# Codex `bif-commit` Step-By-Step

This is the full end-to-end flow for using the repo-owned `bif-commit` skill in Codex.

## 1. Edit The Canonical Workflow

The source of truth is not the installed Codex skill. It is:

- `agents/workflows/bif-commit.md`

If you want to change how `bif-commit` behaves, edit that file first.

Related shared context it depends on:

- `agents/base/PROJECT.md`
- `agents/generated/codex/bif-commit/SKILL.md`

## 2. Regenerate The Repo-Owned Codex Artifact

After changing the canonical workflow, regenerate the repo-local Codex artifact.

Run:

```powershell
pwsh -File scripts/sync-agent-config.ps1 -Codex
```

What this does:

- reads the canonical repo-owned source
- rewrites the generated Codex artifact under `agents/generated/codex/`
- keeps the generated file aligned with the shared docs

If you want to preview first:

```powershell
pwsh -File scripts/sync-agent-config.ps1 -Codex -WhatIf
```

## 3. Confirm The Generated Skill Exists

The generated repo-local Codex skill should now exist here:

- `agents/generated/codex/bif-commit/SKILL.md`

That file is the repo-owned Codex artifact.

It tells Codex:

- this is the `bif-commit` skill
- read `agents/base/PROJECT.md`
- read `agents/workflows/bif-commit.md`
- use those as the real source of truth

## 4. Install The Repo-Owned Skill Into Your Local Codex Skills Folder

Codex does not automatically discover `agents/generated/codex/` inside the repo. You need to copy the generated skill into your local Codex skills directory.

Run:

```powershell
pwsh -File scripts/install-codex-skills.ps1
```

What this does:

- copies each skill from `agents/generated/codex/`
- installs it into your local Codex skills directory
- by default, the target is `~/.codex/skills`

If you want to preview first:

```powershell
pwsh -File scripts/install-codex-skills.ps1 -WhatIf
```

## 5. What The Installer Actually Copies

Right now the repo generates and installs this Codex skill:

- `bif-commit`

Repo source:

- `agents/generated/codex/bif-commit/SKILL.md`

Installed destination will be effectively:

- `~/.codex/skills/bif-commit/SKILL.md`

That installed copy is what Codex can actually invoke as a skill.

## 6. Start Codex In The BIF Repo

Open Codex in this repo root:

- `G:\__projects\_programming\rust\bif`

This matters because the skill says "use this only inside the BIF repo" and refers to repo-local files like:

- `agents/base/PROJECT.md`
- `agents/workflows/bif-commit.md`

If you invoke the skill outside this repo, the instructions will not line up with the filesystem.

## 7. Invoke The Skill In Codex

Once installed, you can use it like this:

```text
Use $bif-commit for the current changes.
```

Or:

```text
Use the bif-commit skill for the current changes in this repo.
```

What Codex should then do:

- gather git context
- run `cargo build`
- run `cargo clippy -- -D warnings`
- run `cargo fmt --check`
- analyze the diff
- draft a why-focused commit message
- update docs if needed
- show staged files and commit message before committing, unless you explicitly asked it to commit immediately

## 8. If You Want Preparation Only, Say So Explicitly

Example:

```text
Use $bif-commit for the current changes, but do not commit yet. Show me the staged file set and proposed commit message first.
```

This is the safer default for normal use.

## 9. If You Want It To Actually Commit, Say That Explicitly

Example:

```text
Use $bif-commit for the current changes and commit if all checks pass.
```

That removes ambiguity.

## 10. Day-To-Day Update Loop

If you later change the shared commit workflow again, the correct order is:

1. Edit `agents/workflows/bif-commit.md`
2. Regenerate the repo-owned Codex artifact:

```powershell
pwsh -File scripts/sync-agent-config.ps1 -Codex
```

3. Reinstall the local Codex skill:

```powershell
pwsh -File scripts/install-codex-skills.ps1
```

4. Use `$bif-commit` in Codex

That is the full maintenance loop.

## 11. What Happens If You Skip Install

If you only update the repo files but do not run the installer:

- the repo-owned generated skill is updated
- your local Codex-installed skill may still be old
- Codex may keep using stale skill instructions

So for Codex specifically, `sync` alone is not enough. You need:

- `sync-agent-config.ps1 -Codex`
- then `install-codex-skills.ps1`

## 12. What Happens If You Skip Sync

If you edit `agents/workflows/bif-commit.md` but do not run sync:

- the canonical workflow is updated
- the generated Codex skill artifact may still be stale
- install will copy the stale generated file if it was not regenerated

So the correct dependency chain is:

1. edit canonical doc
2. sync generated artifact
3. install generated artifact
4. use skill

## 13. Files Involved

Canonical source:

- `agents/workflows/bif-commit.md`

Shared repo context:

- `agents/base/PROJECT.md`

Generated repo-local Codex artifact:

- `agents/generated/codex/bif-commit/SKILL.md`

Sync script:

- `scripts/sync-agent-config.ps1`

Installer script:

- `scripts/install-codex-skills.ps1`

Overview doc:

- `agents/README.md`

## 14. Short Version

If nothing changed and you just want to use it:

```powershell
pwsh -File scripts/install-codex-skills.ps1
```

Then in Codex:

```text
Use $bif-commit for the current changes.
```

If you changed the workflow first:

```powershell
pwsh -File scripts/sync-agent-config.ps1 -Codex
pwsh -File scripts/install-codex-skills.ps1
```

Then in Codex:

```text
Use $bif-commit for the current changes.
```
