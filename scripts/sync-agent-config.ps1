[CmdletBinding()]
param(
    [switch]$Claude,
    [switch]$Kilo,
    [switch]$Codex,
    [switch]$All,
    [switch]$WhatIf
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not ($Claude -or $Kilo -or $Codex -or $All)) {
    $All = $true
}

function Normalize-Content {
    param([string]$Text)
    return ($Text -replace "`r`n", "`n").TrimEnd("`n", "`r")
}

function Write-ManagedFile {
    param(
        [string]$Path,
        [string]$Content
    )

    $dir = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $dir)) {
        if ($WhatIf) {
            Write-Host "Would create directory $dir"
        } else {
            [void][System.IO.Directory]::CreateDirectory($dir)
        }
    }

    $normalizedNew = Normalize-Content $Content
    $exists = Test-Path -LiteralPath $Path
    $normalizedOld = ""

    if ($exists) {
        $normalizedOld = Normalize-Content ([System.IO.File]::ReadAllText($Path))
    }

    if ($exists -and $normalizedOld -eq $normalizedNew) {
        Write-Host "Unchanged $Path"
        return
    }

    if ($WhatIf) {
        if ($exists) {
            Write-Host "Would update $Path"
        } else {
            Write-Host "Would create $Path"
        }
        return
    }

    [System.IO.File]::WriteAllText($Path, $normalizedNew + "`n")
    if ($exists) {
        Write-Host "Updated $Path"
    } else {
        Write-Host "Created $Path"
    }
}

function Get-CanonicalPathText {
    param([string]$RelativePath)
    return Join-Path $repoRoot $RelativePath
}

function Get-ClaudeBifCommit {
    return @'
---
allowed-tools: Bash(git *), Bash(cargo *), Read, Edit, Write, Glob
description: BIF project commit workflow with checks and devlog
---

Follow the canonical workflow in `agents/workflows/bif-commit.md`.

Tool-specific notes:

- Use live repo context rather than pasted snapshots:
  - `git status`
  - `git diff HEAD`
  - `git branch --show-current`
  - `git log --oneline -5`
- If Claude creates the commit, append:

```
Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
```
'@
}

function Get-ClaudeSavePlan {
    return @'
---
allowed-tools: Read, Edit, Write, Glob
description: Save a completed plan into the repo with the correct path and filename
---

Follow the canonical workflow in `agents/workflows/write-handoff.md`.

When saving:

- Use `docs/agent-plans/` for architecture or tooling plans.
- Use `docs/agent-handoffs/` for execution-ready implementation handoffs.
- Name files `YYYY-MM-DD-<slug>.md`.
- Update an existing canonical file instead of creating duplicates when appropriate.

For execution handoffs, structure the content using `agents/base/HANDOFF_CONTRACT.md`.
'@
}

function Get-ClaudeDebugIssue {
    return @'
---
name: Debug Issue
description: Systematically debug issues using graph-powered code navigation
---

Use the canonical workflow in `agents/workflows/debug-issue.md`.

Tool-specific notes:

- Prefer code-review-graph MCP tools when they are available.
- Start with `get_minimal_context(...)` before larger graph queries.
'@
}

function Get-ClaudeExploreCodebase {
    return @'
---
name: Explore Codebase
description: Navigate and understand codebase structure using the knowledge graph
---

Use the canonical workflow in `agents/workflows/explore-codebase.md`.

Tool-specific notes:

- Prefer code-review-graph MCP tools when they are available.
- Start with `get_minimal_context(...)` before larger graph queries.
'@
}

function Get-ClaudeRefactorSafely {
    return @'
---
name: Refactor Safely
description: Plan and execute safe refactoring using dependency analysis
---

Use the canonical workflow in `agents/workflows/refactor-safely.md`.

Tool-specific notes:

- Prefer graph-backed dependency and impact tools when they are available.
- Start with `get_minimal_context(...)` before larger graph queries.
'@
}

function Get-ClaudeReviewChanges {
    return @'
---
name: Review Changes
description: Perform a structured code review using change detection and impact
---

Use the canonical workflow in `agents/workflows/review-changes.md`.

Tool-specific notes:

- Prefer graph-backed change and impact tools when they are available.
- Start with `get_minimal_context(...)` before larger graph queries.
'@
}

function Get-ClaudeVfxReviewer {
    return @'
---
name: vfx-code-reviewer
description: Use this agent when you need expert review of Rust or C++ code related to USD, rendering, or VFX pipelines. This agent will analyze recently written code for optimization opportunities, maintainability issues, and architectural concerns. The agent will challenge design decisions when better alternatives exist and ensure code follows best practices for VFX software development.\n\nExamples:\n- <example>\n  Context: User has just implemented a USD parser or scene graph traversal function\n  user: "I've implemented a function to parse USD files"\n  assistant: "Let me review this implementation with the vfx-code-reviewer agent"\n  <commentary>\n  Since new USD-related code was written, use the vfx-code-reviewer agent to analyze it for correctness, performance, and maintainability.\n  </commentary>\n</example>\n- <example>\n  Context: User has written rendering or GPU-related code\n  user: "Here's my new instancing system for the renderer"\n  assistant: "I'll use the vfx-code-reviewer agent to review this rendering code"\n  <commentary>\n  The user has implemented rendering functionality, so the vfx-code-reviewer should examine it for GPU efficiency and VFX pipeline best practices.\n  </commentary>\n</example>\n- <example>\n  Context: User is refactoring existing code for better performance\n  user: "I've optimized the BVH traversal algorithm"\n  assistant: "Let me have the vfx-code-reviewer agent analyze these optimizations"\n  <commentary>\n  Performance-critical code changes should be reviewed by the vfx-code-reviewer to ensure optimizations are correct and actually beneficial.\n  </commentary>\n</example>
model: opus
color: red
---

Follow the canonical reviewer role in `agents/roles/vfx-code-reviewer.md`.
'@
}

function Get-KiloBifCommit {
    return @'
---
name: bif-commit
description: BIF project commit workflow with checks and devlog
---

Follow the canonical workflow in `agents/workflows/bif-commit.md`.

Before starting, gather live repo context:

```bash
git status
git diff HEAD
git branch --show-current
git log --oneline -5
```
'@
}

function Get-KiloVfxReviewer {
    return @'
---
name: vfx-code-reviewer
description: Use this agent when you need expert review of Rust or C++ code related to USD, rendering, or VFX pipelines. This agent will analyze recently written code for optimization opportunities, maintainability issues, and architectural concerns. The agent will challenge design decisions when better alternatives exist and ensure code follows best practices for VFX software development.

Examples:
- <example>
  Context: User has just implemented a USD parser or scene graph traversal function
  user: "I've implemented a function to parse USD files"
  assistant: "Let me review this implementation with the vfx-code-reviewer agent"
  <commentary>
  Since new USD-related code was written, use the vfx-code-reviewer agent to analyze it for correctness, performance, and maintainability.
  </commentary>
</example>
- <example>
  Context: User has written rendering or GPU-related code
  user: "Here's my new instancing system for the renderer"
  assistant: "I'll use the vfx-code-reviewer agent to review this rendering code"
  <commentary>
  The user has implemented rendering functionality, so the vfx-code-reviewer should examine it for GPU efficiency and VFX pipeline best practices.
  </commentary>
</example>
- <example>
  Context: User is refactoring existing code for better performance
  user: "I've optimized the BVH traversal algorithm"
  assistant: "Let me have the vfx-code-reviewer agent analyze these optimizations"
  <commentary>
  Performance-critical code changes should be reviewed by the vfx-code-reviewer to ensure optimizations are correct and actually beneficial.
  </commentary>
</example>
model: opus
mode: code
---

Follow the canonical reviewer role in `agents/roles/vfx-code-reviewer.md`.
'@
}

function Get-ClaudeSavePlan {
    return @'
---
allowed-tools: Read, Edit, Write, Glob
description: Save a completed plan into the repo with the correct path and filename
---

Follow the canonical workflow in `agents/workflows/write-handoff.md`.

When saving:

- Use `docs/agent-plans/` for architecture or tooling plans.
- Use `docs/agent-handoffs/` for execution-ready implementation handoffs.
- Name files `YYYY-MM-DD-<slug>.md`.
- Update an existing canonical file instead of creating duplicates when appropriate.

For execution handoffs, structure the content using `agents/base/HANDOFF_CONTRACT.md`.
'@
}

function Get-CodexReadme {
    return @'
# Codex Repo-Owned Source

This folder holds repo-generated Codex skill artifacts.

Current artifact:

- `bif-commit/SKILL.md`

These artifacts are generated from the repo-owned canonical docs under `agents/`.

To install them into your local Codex skills directory, use:

```powershell
pwsh -File scripts/install-codex-skills.ps1
```
'@
}

function Get-CodexBifCommit {
    return @'
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
'@
}

if ($All -or $Claude) {
    Write-ManagedFile (Get-CanonicalPathText ".claude/commands/bif-commit.md") (Get-ClaudeBifCommit)
    Write-ManagedFile (Get-CanonicalPathText ".claude/commands/save-plan.md") (Get-ClaudeSavePlan)
    Write-ManagedFile (Get-CanonicalPathText ".claude/skills/debug-issue.md") (Get-ClaudeDebugIssue)
    Write-ManagedFile (Get-CanonicalPathText ".claude/skills/explore-codebase.md") (Get-ClaudeExploreCodebase)
    Write-ManagedFile (Get-CanonicalPathText ".claude/skills/refactor-safely.md") (Get-ClaudeRefactorSafely)
    Write-ManagedFile (Get-CanonicalPathText ".claude/skills/review-changes.md") (Get-ClaudeReviewChanges)
    Write-ManagedFile (Get-CanonicalPathText ".claude/agents/vfx-code-reviewer.md") (Get-ClaudeVfxReviewer)
}

if ($All -or $Kilo) {
    Write-ManagedFile (Get-CanonicalPathText ".kilo/skills/bif-commit/SKILL.md") (Get-KiloBifCommit)
    Write-ManagedFile (Get-CanonicalPathText ".kilo/agents/vfx-code-reviewer.md") (Get-KiloVfxReviewer)
}

if ($All -or $Codex) {
    Write-ManagedFile (Get-CanonicalPathText "agents/generated/codex/README.md") (Get-CodexReadme)
    Write-ManagedFile (Get-CanonicalPathText "agents/generated/codex/bif-commit/SKILL.md") (Get-CodexBifCommit)
}
