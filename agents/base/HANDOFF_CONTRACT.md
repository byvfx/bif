# Planner To Executor Handoff Contract

Use this structure for any file-based plan that one agent writes and another executes.

## Required Sections

### Goal

State the user-visible outcome in one short paragraph.

### Scope

List what is in scope and what is intentionally out of scope.

### Files Or Modules

Name the files, crates, modules, or surfaces expected to change.

### Constraints

Capture hard requirements such as:

- API compatibility
- performance constraints
- repo conventions
- tool or environment assumptions

### Checks To Run

List the exact commands or validation steps the executor should run.

### Acceptance Criteria

Make success testable. Use concrete outcomes, not vague intent.

### Out Of Scope

Call out nearby work that should not be pulled in.

### Unresolved Questions

List any open questions. If none remain, write `None`.

## Execution Notes

- The executor should read the handoff first, then inspect the actual code before changing anything.
- For Codex execution, the bootstrap set is:
  - `agents/base/PROJECT.md`
  - `agents/base/HANDOFF_CONTRACT.md`
  - the current handoff file
  - any referenced workflow docs
- If the handoff conflicts with the repo state, the executor should follow the repo state and report the mismatch.
- The executor should report deltas against the acceptance criteria and note any residual risk.
