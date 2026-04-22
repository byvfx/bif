# Explore Codebase

Explore from broad structure to specific implementation details.

## Workflow

1. Identify the likely crate, module, or surface.
2. Read the highest-signal entry points first.
3. Trace related types, functions, and call paths.
4. Cross-check with project docs when behavior is milestone- or domain-specific.
5. Summarize the relevant architecture before proposing changes.

## Tooling Guidance

- If a graph or code-intel tool is available, start with the smallest context and then request architecture or dependency views.
- Otherwise use `rg`, file listings, and targeted reads.
- Avoid bulk-reading large files when a narrower search can answer the question.

## Output

- relevant modules and responsibilities
- important flows and boundaries
- likely edit points
- risks or unknowns
