# Development Log - 2025-12-27

## Session Duration

~5 hours (full productive session - Milestone 0 and Milestone 1 complete!)

## Goals

- ✅ Complete Milestone 0: Environment Setup
- ✅ Complete Milestone 1: Math Library port (Ray, Interval, AABB)
- ⏳ Start Milestone 2: wgpu Window (next session)

## What I Did

### Environment Setup (Milestone 0 Complete!)

- ✅ Verified Rust 1.86.0 installation
- ✅ Created Cargo workspace with 4 crates:
  - `bif_math` - Math library (wraps glam)
  - `bif_core` - Core scene/primitive types
  - `bif_render` - Rendering engine
  - `bif_viewer` - Application entry point

### Workspace Configuration

- ✅ Configured workspace dependencies in root `Cargo.toml`
- ✅ Set up each crate with proper dependencies
- ✅ Used glam 0.29, wgpu 22.1, egui 0.29 (compatible with Rust 1.86)
- ✅ Added development profile with opt-level=1 for faster iteration
- ✅ Created basic tests for `bif_math` crate

### Git Repository Issue - Fixed

- **Problem:** `cargo new` created nested `.git` directories in crates
- **Solution:** Manually created crate structure without `cargo new`
- **Verification:** Only ONE `.git` directory at workspace root
- ✅ Committed workspace setup to git

### Legacy Code Setup

- ✅ Copied Go raytracer to `legacy/go-raytracing/`
- ✅ Set up Git LFS for large files (.obj models, .hdr, .pprof, .usd files)
- ✅ Created `.gitattributes` with LFS tracking rules
- ✅ Committed legacy code as reference for porting

### Milestone 1: Math Library Porting

#### Ray Struct - Complete! ✅

- ✅ Ported `Ray` from Go to Rust
- ✅ Added constructor: `Ray::new(origin, direction, time)`
- ✅ Added getter methods: `origin()`, `direction()`, `time()`
- ✅ Added `at(t)` method for point-along-ray calculation
- ✅ Made Ray `Copy` trait (small, cheap to copy)
- ✅ Wrote comprehensive tests (6 tests, all passing)
- ✅ Documented with doc comments

**Key learning:** Direct field access vs getters - Rust allows both!

#### Interval Struct - Complete! ✅

- ✅ Ported `Interval` from Go to Rust
- ✅ Implemented all methods:
  - `new(min, max)` - Constructor
  - `size()` - Returns max - min
  - `contains(x)` - Inclusive check [min, max]
  - `surrounds(x)` - Exclusive check (min, max)
  - `clamp(x)` - Clamps value to interval
  - `expand(delta)` - Expands by delta/2 on each side
  - `add(other)` - Component-wise addition
  - `add_scalar(displacement)` - Adds scalar to both bounds
  - `surrounding(a, b)` - Creates interval surrounding two others
- ✅ Added constants: `Interval::EMPTY`, `Interval::UNIVERSE`
- ✅ Wrote comprehensive tests (10 tests, all passing)
- ✅ Hand-typed code for learning (ask mode)

**Key learning:** Rust's `f32::clamp()` is cleaner than Go's if/else chain!

#### AABB (Axis-Aligned Bounding Box) - Complete! ✅

- ✅ Ported `AABB` from Go to Rust
- ✅ Multiple constructors:
  - `new(x, y, z)` - From three intervals
  - `empty()` - Empty bounding box
  - `universe()` - Infinite bounding box
  - `from_points(a, b)` - From two corner points
  - `surrounding(box0, box1)` - Union of two boxes
- ✅ Implemented core methods:
  - `hit(ray, interval)` - **Ray-box intersection test** (critical for BVH!)
  - `axis_interval(n)` - Get interval for specific axis
  - `centroid()` - Center point of box
  - `longest_axis()` - Index of longest dimension
  - `translate(offset)` - Move box by offset
  - `pad_to_minimums()` - Prevent degenerate zero-width boxes
- ✅ Added constants: `Aabb::EMPTY`, `Aabb::UNIVERSE`
- ✅ Wrote comprehensive tests (6 tests, all passing)
- ✅ Optimized hit test with slab method (unrolled loop for X/Y/Z)

**Key learning:** `std::mem::swap()` for efficient in-place swaps!

### Documentation

- ✅ Added devlog instructions to `CLAUDE.md`
- ✅ Created `devlog/README.md` with format guidelines

## Learnings

### Rust Concepts Mastered Today

1. **Workspace structure** - `resolver = "2"`, `members = [...]`
2. **Workspace inheritance** - `version.workspace = true` to share config
3. **Dependency management** - Centralized versions in `[workspace.dependencies]`
4. **Profile optimization** - `opt-level = 1` for dev builds speeds up iteration
5. **Method receivers:**
   - `&self` - Borrow (read-only, most common)
   - `self` - Take ownership (consumes the value)
   - `&mut self` - Mutable borrow (can modify fields)
6. **Copy trait** - Small types (Ray, Interval, Aabb) can be copied implicitly
7. **Associated constants** - `pub const EMPTY: Interval = ...` in impl block
8. **Inline hints** - `#[inline]` for simple getters
9. **Doc comments** - `///` generates documentation
10. **Git LFS** - Tracking large binary files
11. **Pattern matching** - `match` for `axis_interval()` method
12. **std::mem::swap** - Efficient in-place value swapping

### Go → Rust Translation Patterns

| Go | Rust | Notes |
|---|---|---|
| `type Ray struct { ... }` | `pub struct Ray { ... }` | Explicit pub |
| `func NewRay(...)` | `pub fn new(...) -> Self` | Constructor in impl |
| `func (r Ray) Method()` | `pub fn method(&self)` | Borrow by default |
| `r.Field` | `self.field` | Lowercase convention |
| `float64` | `f32` | Graphics standard |
| `math.Inf(1)` | `f32::INFINITY` | Built-in constant |
| `v.Add(u)` | `v + u` | Operator overloading |

### Common Pitfalls Avoided

- ❌ **Nested git repos** - `cargo new` creates `.git` in each crate
  - ✅ Fixed by manually creating directory structure
- ❌ **Version compatibility** - Rust 1.86 doesn't support latest egui/wgpu
  - ✅ Used compatible versions (egui 0.29, wgpu 22.1)
- ❌ **PascalCase method names** - Rust uses snake_case
  - ✅ Used `add()` not `Add()`
- ❌ **Missing doc comments** - Easy to forget
  - ✅ Added `///` comments for all public APIs

### Rust vs Go Observations

- Cargo workspaces are like Go modules with nested `go.mod` files
- Dependency version pinning is more explicit than Go's `go.mod`
- Tests live in the same file (`#[cfg(test)]` mod) vs separate `_test.go` files
- Rust's operator overloading makes math code cleaner (`v + u` vs `v.Add(u)`)
- Constants must be compile-time known (no dynamic initialization)
- Field shorthand: `Self { min, max }` like Go's `Ray{orig: orig}`

## Architecture Decisions

### Why glam instead of custom Vec3?

- SIMD-optimized (4x faster than naive implementation)
- Industry standard in Rust gamedev/graphics
- Proven in production (Bevy, Fyrox engines use it)
- Can always add custom wrappers if needed

### Why egui for prototyping?

- Pure Rust (no C++ FFI complexity)
- Immediate mode = fast iteration
- Will keep it as dev UI even when adding Qt later

### Crate Structure Rationale

- `bif_math` - Pure math, no dependencies → fastest compile times
- `bif_core` - Depends on math, used by all → stable API layer
- `bif_render` - Heavy wgpu dependencies → isolates GPU code
- `bif_viewer` - Application glue → can swap UI without touching core

### Why Public Fields on Ray/Interval?

- Simple data types (no invariants to maintain)
- Direct access is idiomatic for simple structs
- Added getter methods for API compatibility with Go
- Rust's type system prevents misuse anyway

## Next Session

### ✅ Milestone 1 Complete!

All math primitives ported from Go:
- ✅ Ray struct (6 tests)
- ✅ Interval helper (10 tests)
- ✅ AABB struct (6 tests)

**Total: 22 tests passing, ~560 LOC, 5 commits** 🎉

### Immediate Next: Milestone 2 - wgpu Window

1. **Set up wgpu context** in `bif_render` crate
   - Initialize device, queue, surface
   - Create swap chain
   - Handle window resize
   - Estimated: 1 hour

2. **Create window** in `bif_viewer` crate
   - Use `winit` for windowing
   - Integrate wgpu renderer
   - Render solid color clear screen
   - Estimated: 30-45 minutes

**Total estimated: 1.5-2 hours**

### Future Milestones (Reference)

- **Milestone 3:** Basic rendering (triangle, camera, shader)
- **Milestone 4:** BVH acceleration structure
- **Milestone 5:** Material system
- **Milestone 6:** USD scene loading

## Blockers/Issues

- None! Everything compiling and tests passing (22/22 tests green ✅)

## Notes

- Rust 1.86 is sufficient for now, can upgrade to 1.88+ later for latest deps
- devlog format working well - keeping this habit!
- Side project pace is realistic - no rush, steady progress
- Hand-typing code (ask mode) really helps internalize Rust patterns
- Git LFS setup will be useful for USD files later
- **Milestone 1 complete!** Math foundation is solid, ready for rendering

## Stats

- **Files Created:** 4 (ray.rs, interval.rs, aabb.rs, .gitattributes)
- **Lines of Code:** ~560 production code (excluding tests)
- **Tests Written:** 22 (all passing)
- **Commits:** 5
- **Time Spent:** ~5 hours

---

**Next session starts with:** Milestone 2 - Setting up wgpu window and basic rendering context.

---

**Next session starts with:** Copying Go code to legacy/ and porting Ray struct.
