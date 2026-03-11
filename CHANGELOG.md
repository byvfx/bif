# Changelog

All notable changes to BIF will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Blue noise camera jitter with Cranley-Patterson rotation (256x256 void-and-cluster texture)
- SamplerMode enum (WhiteNoise/BlueNoise) with UI dropdown, default BlueNoise
- Pixel reconstruction filters: Box, Gaussian, Mitchell-Netravali, Blackman-Harris
- Weighted progressive accumulation for non-box filters
- Auto-denoise on render completion (OIDN feature)
- Milestones 30-35 roadmap (project save, lights, materials, shader graph, timeline, render queue)
- CI/CD pipeline: GitHub Actions for fmt/clippy/test on push/PR, release builds on tags
- CHANGELOG.md for tracking release notes

### Changed

### Fixed
- Batch render now uses viewport HDRI rotation/intensity instead of baked-in values
- Camera interaction magic numbers extracted to named constants
- Bucket RNG seed finalization — bit-mixing for uncorrelated seeds between adjacent passes
- ImageBuffer debug_assert bounds checking in get()/set()
- Mesh dedup hash — DefaultHasher with sampled vertices instead of weak XOR
- CI: `bif_core` build.rs no longer panics when vcpkg not installed (graceful skip for clippy-only mode)
- CI: removed bif_renderer/bif_viewport test steps that can't link without USD env
- CI: build.rs vcpkg detection checks toolchain file + USD headers (fixes false positive on GH Actions `C:\vcpkg`)
- CI: removed invalid `pxr` port from vcpkg.json (USD not available as standard vcpkg port)
