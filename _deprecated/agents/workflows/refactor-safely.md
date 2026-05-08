# Refactor Safely

Refactor with explicit blast-radius awareness.

## Workflow

1. Define the refactor goal and non-goals.
2. Identify the owning modules and impacted call paths.
3. Check for tests that already cover the area.
4. Make the smallest structural change that improves the target problem.
5. Re-run focused validation and compare the before-and-after behavior.

## Safety Rules

- Do not mix behavioral changes into a structural refactor unless necessary.
- Prefer incremental commits for large refactors.
- Check dependent flows before renames or interface changes.
- When behavior must change, document that clearly and test it directly.
