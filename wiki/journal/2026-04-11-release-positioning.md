---
title: Release positioning under the Qt migration constraint
type: journal
tags: [marketing, positioning, release, journal]
created: 2026-04-11
updated: 2026-04-11
---

# Release positioning under the Qt migration constraint

## Context

Drafted the v0.13.0 and v0.13.5 announcement posts today (see [v0.13.0 announcement](../../devlog/2026-04/v0.13.0_announcement.md), [v0.13.5 announcement](../../devlog/2026-04/v0.13.5_announcement.md)). Ran into a real conflict that's going to shape every public post between now and v0.15.0.

## The conflict

v0.13.0 shipped M29.5 — a major egui UI overhaul (centralized theme, property inspector, menu bar, panel restructure). It's a legitimate release highlight and on the surface the most "flex-able" visual thing we shipped.

**But** v0.15.0 is the Qt migration — the entire frontend gets replaced. Any post leaning on egui UI screenshots becomes visually stale in ~3 months. Every "look at my cool UI" shot dates itself the moment Qt lands.

## The decision

Don't lead with UI chrome. Lead with engine-agnostic assets.

**Hero visuals now favor (in priority order):**

1. Rendered output — Ivar path tracer frames, subdiv + displacement, HDRI-lit hero renders. Engine-agnostic, reusable across every release between now and 1.0.
2. Code + terminal — `cargo test`, `git diff --stat`, source snippets of key traits. Timeless.
3. Architecture diagrams — block diagrams of the FFI split, SceneQuery trait layering. Typography-driven, no screenshots.
4. **For v0.13.5 specifically:** the walk cycle video. A walking character is engine-agnostic — it's character animation, not UI.

**Hero visuals now avoid:**

- egui-themed UI chrome (buttons, tabs, sliders, the property inspector panel layout)
- Anything that requires the viewer to know "this is BIF's current UI" to understand the shot
- Screen-recordings where egui widgets dominate the frame

## The honest telegraph

Both posts include an explicit line about the Qt migration:

> "UI is getting replaced by Qt in v0.15.0. The architecture underneath survives the port."

This earns trust instead of eroding it. When v0.15 lands and the UI looks completely different, anyone who saw the v0.13.x posts is primed for the change and reads it as progress, not instability.

## What survives the Qt port

Listing the architectural work that is UI-agnostic and will look identical after the migration — this is what's safe to flex:

- `SceneQuery` trait — decoupled scene providers
- FFI bridge split (`ffi_raw` / `ffi_convert` / `cpp_bridge`)
- `UsdStage` `Sync` fix (`Arc<Mutex>` wrap, unsafe removed)
- Subdivision surfaces + CPU displacement pipeline
- faceVarying UVs handling
- Native MaterialX displacement
- DomeLight_1 schema support
- `bif_core::skinning` module + CPU linear blend skinning
- `UsdSkelCache` + `UsdSkelSkeletonQuery` wiring
- The Embree feature gate

## Applies to: every release until v0.15.0

This is the default until Qt lands. After v0.15.0, the UI itself becomes a legitimate hero visual again — but with the understanding that Qt chrome is the new look for the long haul.

## See also

- [[../../MILESTONES|MILESTONES.md]] — v0.15.0 Qt migration is on the roadmap
- [[../../CHANGELOG|CHANGELOG.md]] — v0.13.0 and v0.13.5 release notes
- [[../../devlog/2026-04/v0.13.0_announcement|v0.13.0 announcement drafts]]
- [[../../devlog/2026-04/v0.13.5_announcement|v0.13.5 announcement drafts]]
