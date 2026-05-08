---
title: BIF Knowledge Base
type: index
updated: "2026-05-01"
---

# BIF Knowledge Base

> LLM entry point — read this first to navigate the wiki.

BIF is a VFX scene assembler and renderer built in Rust with wgpu, USD, and MaterialX. This knowledge base documents architecture decisions, domain concepts, learning notes, and reference material.

## Sections

| Section | Articles | Description |
|---------|----------|-------------|
| [[architecture/_index\|Architecture]] | 9 + 8 ADRs | Crate structure, node graph, scene browser, Qt migration, key decisions |
| [[usd/_index\|USD]] | 5 | Composition, schemas, shading, BIF integration |
| [[rendering/_index\|Rendering]] | 3 | wgpu pipeline, OpenPBR, MaterialX bridge |
| [[concepts/_index\|Concepts]] | 13 | Atomic notes on key technical concepts |
| [[rust/_index\|Rust]] | 1 | Learning notes, patterns, idioms |
| [[ui-ux/_index\|UI/UX]] | 2 | Design philosophy, material editor |
| [[journal/_index\|Journal]] | 6 | Reflective learning entries |
| [[raw/_index\|Raw]] | — | Ingested source material |

## Source Documents

Existing project docs live outside this vault and are referenced by wiki articles:

- `../docs/usd/` — 11 curated USD reference docs
- `../docs/ux/` — 5 UX research/design docs
- `../devlog/` — 93 development journal entries
- `../ARCHITECTURE.md`, `../MILESTONES.md`, `../FEATURES.md` — project docs

## Current ADRs

- [[architecture/adr/008-edit-operation-architecture|ADR-008 — Edit operation architecture]]

## Conventions

- **Frontmatter:** Every article has title, type, tags, created/updated dates
- **Types:** `concept` (atomic note), `article` (longer), `adr` (decision record), `journal`, `reference`
- **Cross-links:** Use `[[wikilinks]]` for internal links, relative paths for source docs
- **Indexes:** Each section has `_index.md` with one-line summaries
- **Templates:** In `templates/` — use Obsidian's template picker (Ctrl+T)
