# Debug Issue

Use a structured narrowing process instead of random code spelunking.

## Workflow

1. Define the failing behavior precisely.
2. Find the entry points, likely owners, and recent changes.
3. Trace callers, callees, and state transitions through the affected path.
4. Form one or two concrete hypotheses.
5. Validate the hypotheses with the smallest useful inspection or test.
6. Report the likely root cause, affected scope, and next fix path.

## Tooling Guidance

- Prefer minimal-context queries first.
- If a graph or code-intel tool is available, use it to trace call paths and impact radius.
- If not, fall back to `rg`, focused file reads, and targeted tests.

## Output

- observed failure
- likely root cause
- evidence
- affected code paths
- recommended fix direction
