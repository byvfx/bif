---
title: Rust Learning Index
type: index
updated: "2026-04-05"
---

# Rust

Learning notes, patterns, and idioms discovered while building BIF. Brandon comes from Go/Python — this section captures Rust-specific knowledge.

## Articles

- [[patterns-discovered|Patterns Discovered]] — Rust patterns learned building BIF
- [[borrow-checker-patterns|Borrow Checker Patterns]] — MutexGuard lifetimes, pre-extraction, as_deref, lock-once patterns

## Topics to Explore

- Ownership & borrowing patterns in scene graph traversal
- Builder pattern usage (wgpu pipelines, renderer config)
- Error handling strategies (thiserror, anyhow, Result chains)
- Trait-based polymorphism vs enum dispatch (node types)
- unsafe usage and safety invariants (C++ FFI bridge)
- Iterator patterns for mesh processing
- Testing patterns (#[cfg(test)], mock strategies)

## See Also

- [[architecture/_index|Architecture]] — How Rust patterns shape BIF's design
