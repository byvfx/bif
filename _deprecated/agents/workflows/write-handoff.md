# Write Handoff Workflow

Use this workflow when saving a plan or execution handoff into the repo.

## 1. Classify The Document

Decide which type this is before writing anything:

- **Architecture or tooling plan** — captures a design decision, approach comparison, or tooling change.
- **Execution handoff** — a structured task brief that another agent will execute directly.

When in doubt: if the document is meant to be *acted on directly by an executor*, it is a handoff. Otherwise it is a plan.

## 2. Check For Existing Files

Before creating a new file:

- Scan the target directory for files with a similar slug or topic.
- If a relevant file already exists, update it rather than creating a duplicate.

## 3. Name The File

Use the format `YYYY-MM-DD-<slug>.md` where:

- `YYYY-MM-DD` is today's date.
- `<slug>` is a short, lowercase, hyphen-separated description of the topic.

Example: `2026-04-21-unified-agent-base-and-settings.md`

## 4. Write The Content

For **plans**, write clearly structured prose covering:

- What is being decided or designed
- Options considered and trade-offs
- Decision made and rationale
- Open questions if any remain

For **execution handoffs**, use the required sections from `agents/base/HANDOFF_CONTRACT.md`:

- Goal
- Scope
- Files Or Modules
- Constraints
- Checks To Run
- Acceptance Criteria
- Out Of Scope
- Unresolved Questions

## 5. Save The File

Save to the exact path based on document type:

- Plan: `docs/agent-plans/YYYY-MM-DD-<slug>.md`
- Execution handoff: `docs/agent-handoffs/YYYY-MM-DD-<slug>.md`

Do not create intermediate directories unless they are missing.

## 6. Confirm

Report the full file path written and the document type. If any required handoff sections are missing or incomplete, flag them.
