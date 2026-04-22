---
name: Refactor Safely
description: Plan and execute safe refactoring using dependency analysis
---

Use the canonical workflow in `agents/workflows/refactor-safely.md`.

Tool-specific notes:

- Prefer graph-backed dependency and impact tools when they are available.
- Start with `get_minimal_context(...)` before larger graph queries.
