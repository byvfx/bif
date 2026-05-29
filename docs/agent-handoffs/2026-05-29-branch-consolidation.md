# Branch Consolidation + #5 Landing — Execution Handoff

**Date:** 2026-05-29
**Branch:** start from `refactor/deepen-modules` (current), operate on `main`

---

## Goal

Collapse the unmerged branch tail into `main` so the trunk reflects reality, land the completed RFC #5 (`PathTracer`) work, tag the release, and prune dead branches. After this: `main` is current through v0.16.8 + the path-trace deepening, MILESTONES "Released" matches the tags, and the local branch list is clean. Then pick the next real work (re-scoped #6 or v0.17.0).

**Decided:** v0.16.5–v0.16.8 were working labels on one in-progress line, not separate tagged releases → consolidate as one tranche (squash acceptable). The user confirmed squashing the working-label commits is fine.

---

## Current topology (as of 2026-05-29, verify before acting)

```
main                       baseline; latest tag v0.16.6
  └─ v0.16.8-dogfood-polish   4 ahead of main (PUSHED to origin)
       └─ refactor/deepen-modules   +5 (#5 work; NOT pushed)
```

- **`main..v0.16.8-dogfood-polish` (4):** `07b15a1` v0.16.7 keybinding editor · `0d9f4c7` v0.16.8 crash chain · `84ee453` docs(handoff) · `8b140ed` docs(devlog s2)
- **`v0.16.8-dogfood-polish..refactor/deepen-modules` (5):** `3196226` helpers · `546e6d4` PathTracer · `dd61539` docs · `cbdeda7` NEE/MIS tests · `0a7eec5` docs
- All green: `bif_renderer` 119, `bif_viewport` 177; clippy clean; fmt clean; workspace builds.

---

## Scope

### In scope

1. Land the v0.16.7/v0.16.8 tail into `main`.
2. Reconcile `MILESTONES.md` "Released" table + "Latest release" line (currently stops at v0.16.2; tag is v0.16.6).
3. Tag the consolidated release (`v0.16.8`).
4. Land RFC #5 (`refactor/deepen-modules`) onto `main`.
5. Prune branches confirmed merged into `main`.
6. Decide fate of the 4 unmerged-but-stale branches.

### Out of scope

- RFC #6 implementation (paused; re-scope recorded on issue #6 — `NodeOutputs` + `CookNode` routing, drop global `SceneCmd`).
- v0.17.0 work.
- Any behavior changes — this is pure integration + hygiene.

---

## Constraints

- **Re-verify the topology first** — `git log --oneline main..v0.16.8-dogfood-polish` and `...dogfood..refactor/deepen-modules`. Don't act on the snapshot above blindly.
- **`refactor/deepen-modules` is NOT pushed** — push it before/with the PR so the #5 work is backed up.
- Outward git ops (push, merge to main, tag, branch -D on origin) were **blocked by the auto-mode classifier** in the authoring session — expect to run these yourself via `!` or grant a permission rule. Don't fight the classifier.
- Run the full gate before tagging: `. .\setup_usd_env.ps1` then `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt --check`, `cargo test -p bif_renderer -p bif_viewport`. (`bif_core`/`bif_qt` tests need the USD env + `--test-threads=1`.)

---

## Implementation Detail

### Phase 1 — Land the v0.16.x tail to `main`

```bash
git switch main && git pull --ff-only
# Squash-consolidate the tail (working labels → one release commit):
git merge --squash v0.16.8-dogfood-polish
git commit -m "release: v0.16.7 keybinding editor + v0.16.8 crash-chain hardening"
# (or `git merge --no-ff v0.16.8-dogfood-polish` if you'd rather keep the 4 commits)
```

Then update `MILESTONES.md`:
- Move **v0.16.7** (keybinding editor) and **v0.16.8** (rt_010 crash chain — OOM + device-lost) into the **Released** table.
- Fix the "Latest release: v0.16.2…" line (stale).
- The CHANGELOG `[Unreleased]` block (crash chain + keybinding + PathTracer entries) → cut a `## [0.16.8] - 2026-05-29` section.

```bash
git tag v0.16.8
git push origin main --tags
```

### Phase 2 — Land RFC #5

```bash
git switch refactor/deepen-modules
git rebase main          # the 4 dogfood commits drop (now in main); leaves the 5 #5 commits
git push -u origin refactor/deepen-modules
gh pr create --base main --title "refactor: deepen path-trace core (PathTracer) — RFC #5" --body "Closes #5. …"
# self-review, then merge
```

Note: if Phase 1 squashed, the rebase will replay only the 5 #5 commits cleanly (their dogfood ancestors are absorbed). Resolve the CHANGELOG conflict by keeping main's `[0.16.8]` section + the PathTracer entry.

### Phase 3 — Prune merged branches

Confirmed merged into `main` (safe `git branch -d`): `agent-config-unification`, `finish-qt-ui`, `v0.15-qt`, `v0.16.2-bugfixes`, `v0.16.3-followups`, `v0.16.5-hotfix`, `v0.16.6-review-fixes`, `v0.16.6-ui-polish`, `worktree-update-milestones`. After Phase 1, also `v0.16.8-dogfood-polish` (local + `origin`). After Phase 2, `refactor/deepen-modules`.

### Phase 4 — Decide the 4 stale unmerged branches

`git log --oneline main..<branch>` each, then keep or `-D`:
- `v0.16.7-keybindings` — almost certainly superseded by the squashed tail → delete.
- `v0.16.1-followups` — check; likely superseded.
- `skills-split-save-handoff` — check; may hold uncommitted skills/docs work.
- `v0.17.0` — likely an empty/stale scaffold for the next milestone → delete or reset.

---

## Done When

- `git rev-list --count main..origin/main` is 0; `main` builds + all tests green.
- `v0.16.8` tag pushed; MILESTONES "Released" + "Latest release" match the tags.
- #5 merged to `main` (or in an open PR if you want review first).
- `git branch` shows only `main` + any genuinely-active work; the 9 merged branches pruned.

---

## Then — Next Work (pick one)

- **Re-scoped #6** (issue #6): `NodeOutputs` merge (collapse `node_proto_map` + `node_cloud_map`, 5 files) → then `CookNode`/node-routing consolidation. ~1 session. Drop the global `SceneCmd` idea.
- **v0.17.0 Context System** (roadmap, 30–40h, highest arch risk — touches `scene_loader`/`render`/`property_inspector`). The architecture review in `devlog/2026-05/DEVLOG_2026-05-28.md` (session 2) is direct prep; consider deepening those three before building contexts on them.

---

## Open Questions

- Squash vs `--no-ff` for the tail merge (leaning squash per the decision; `--no-ff` preserves the keybinding/crash commits if you want them in history).
- Tag as single `v0.16.8`, or `v0.16.7` + `v0.16.8` separately? (Single is simpler given the squash.)
- Land #5 straight to `main`, or keep an open PR for a `/code-review` pass first?
