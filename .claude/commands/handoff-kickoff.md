---
allowed-tools: Read, Glob
description: Emit a ready-to-paste kickoff message for a saved execution handoff
---

Follow the canonical workflow in `agents/workflows/kickoff-handoff.md`.

Accept either a full repo-relative path or a bare slug. Never modify the filesystem — only read, resolve, and print the kickoff message block.
