---
name: plan-build-review
description: Plan with deep reasoning, review the plan, implement, review, correct, and commit
---

## planner
model: openrouter/deepseek/deepseek-v4-pro:high
output: plan.md
progress: true

Analyze the codebase and create a detailed implementation plan for {task}. Include:
- Files to modify or create
- Architecture decisions and rationale
- Edge cases and error handling
- Testing strategy
- Migration or rollback approach where relevant

At the end list unresolved questions, if any. Be extremely concise, sacrifice grammar for brevity.

## oracle
reads: plan.md
model: openrouter/deepseek/deepseek-v4-flash:high

Read {previous} and critically review the implementation plan from {chain_dir}/plan.md. Challenge assumptions, identify risks, and suggest improvements. Return an approved version of the plan with any changes clearly noted.

## worker
reads: plan.md
model: openrouter/deepseek/deepseek-v4-pro:high
progress: true

Implement {task} following the approved plan from {chain_dir}/plan.md. Read any existing files you need to modify first. Return a summary of what was implemented and which files were changed.

## reviewer
skill: vfx-reviewer
model: openrouter/deepseek/deepseek-v4-flash:high
output: review/correctness.md

Review the implementation from {previous} for correctness with VFX production focus. Check:
- Logic errors, race conditions, edge cases
- Security vulnerabilities
- Proper error handling
- Test coverage
- USD traversal and GPU sync discipline
Return evidence-backed findings with file/line references.

## reviewer
skill: vfx-reviewer
model: openrouter/deepseek/deepseek-v4-pro:medium
output: review/quality.md

Review the implementation from {previous} for code quality:
- Unnecessary complexity or dead code
- Naming and readability
- Consistency with existing codebase patterns
- Performance concerns
- Hardcoded limits that won't scale
Return evidence-backed findings with file/line references.

## worker
reads: review/correctness.md, review/quality.md
model: openrouter/deepseek/deepseek-v4-pro:high
progress: true

Apply the fixes recommended in the review findings from {chain_dir}/review/correctness.md and {chain_dir}/review/quality.md. Only apply changes that clearly improve the implementation. Skip nitpicks or opinion-only suggestions. Return a summary of what was fixed.

## delegate
skill: bif-commit
model: openrouter/deepseek/deepseek-v4-flash:medium

Run the full bif-commit workflow on the changes made. Do not skip pre-commit checks. Stage carefully, commit with a concise why-focused message. Update project docs, create devlog entry, update wiki if needed, regenerate site if needed. Report the commit hash.
