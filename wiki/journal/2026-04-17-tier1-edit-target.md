---
title: "Tier 1 — Edit target visible everywhere + schema labels"
type: journal
tags: [tier-1, ux, edit-target, schema-labels, qt]
created: 2026-04-17
updated: 2026-04-17
---

# Tier 1 — Edit target visible everywhere + schema labels

Follow-on to [[2026-04-16-tier0-phase-f|Tier 0 + Phase F]]. The Qt shell now gives a persistent, glanceable answer to *"which layer am I editing?"* across four surfaces simultaneously, and the property inspector speaks an artist's dialect instead of raw USD schema names.

## The edit-target fanout

One fact (`scene_layer_state.working_layer`) drives four independent UI surfaces: the **edit-target pill** in the breadcrumb row, the **compact chip** in the status bar, the **2px viewport edge tint**, and the **breadcrumb layer segment**. All four read the same 4 cxx-qt invokables and refresh on the same existing signal (`layer_state_revisionChanged`). No new signal minted; the dependency graph stays small.

This fanout pattern recurs in pro DCC UIs for good reason — *one* source of truth updating *many* surfaces at once is how you kill the "where am I, really?" anxiety. Borrowing from Solaris's edit-context lamp but making it redundantly encoded three places out of the box.

## Auto-pick heuristic

The audit asked for "strongest writable sublayer". The pragmatic version that shipped treats **writable = `!is_anonymous && !is_muted`**, walked top-of-stack first. The real USD answer is `SdfLayer::PermissionToEdit()` — it's the one that catches read-only-baked studio assets where the root layer can't actually accept opinions. That's Tier 1.5 FFI work.

For 95% of cases (the root is a normal `.usda` file), the pragmatic check returns the same answer as the real one. The 5% where it diverges — published assets with read-only roots — is where Solaris users would also expect the edit target to skip to the next writable sublayer. Documenting that the gap exists so future-me can spot the symptom.

## Schema labels: one table, 180+ entries, massive UX leverage

`friendly_attribute_name("xformOp:translate") → "Position"`.

Every artist who's ever squinted at a property inspector knows what this fixes. USD's schema names are *descriptive* (good for pipeline) but *cryptic* (bad for artists in the middle of a lookdev session). Having both — friendly as the displayed text, raw as the tooltip — keeps both audiences fed. The tooltip is free insurance; anyone who needs to grep for `xformOp:translate` still can.

The table is `match` on `&str → &'static str`. No hashmap, no allocation on the hot path. Unknown names pass through unchanged — important for two reasons:

1. Procedural / custom schemas don't break (they show their raw name instead of an empty string).
2. Artists who've *learned* the USD name still see it, so the inspector doesn't gaslight experienced users.

Growth policy captured in the module docstring: add entries conservatively, only when the friendly name is clearer than USD's own. The list is curated, not kitchen-sink.

## Composition pitfall: the "clear all" widget

`breadcrumb_set_path` calls `bar->clear()` on every selection change. I almost put the edit-target pill inside the toolbar before realizing it'd get nuked on every click. Fix: wrap [breadcrumb + pill] in a sibling `QHBoxLayout` row so the pill is beside the toolbar, not inside it. Writing this down because it'll happen again — when any widget's refresh pattern is "destroy everything and rebuild", permanent chrome has to live *next to* it, not inside.

## Pragmatic deviations from the plan

The UX audit was specific about the styling expectations — Graphite design system from `assets/stitch_bif_ui/obsidian_graphite/DESIGN.md` ("Quiet Confidence / Technical Atelier"). What shipped matches the *structure* but not the *skin*: generic dark QSS borders + palette, not the specified tonal surface hierarchy or JetBrains-Mono data columns. That's the user's explicit preference ([[../../memory/feedback_function_before_form.md|function before form, skin last]]) — styling passes come after all widgets are in tree.

Also deferred: viewport toolbar (M effort, FEATURES.md expanded the spec to 9+ display modes — separate session) and first-opinion guard (needs Tier 1.5's per-attribute opinion-resolution FFI to color the right things).

## Residuals

- **Auto-pick false positives**: pragmatic `!is_anonymous && !is_muted` check doesn't catch read-only-baked studio layers. Tier 1.5 fix.
- **Dirty asterisk is latent**: `compose_title` reads `LayerInfo::is_dirty` but that flag only flips on real USD writes (v0.16). Plumbing is ready; signal won't fire until then.
- **Cheaper layer palette**: the `EDIT_TARGET_PALETTE` in `window_builder.cpp` is the fourth copy of the same 8 colors across `bif_qt/cpp/*`. Noted as a Tier 2 cleanup — consolidate into a shared header.

## Next

[[primdataprovider-trait|PrimDataProvider]] territory → Tier 1.5: `UsdAttribute::GetPropertyStack` FFI so per-attribute opinion colors become honest. Gates first-opinion guard + the inspector left-border coloring from Tier 2 #9.
