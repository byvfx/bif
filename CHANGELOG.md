# Changelog

All notable changes to BIF will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- CI/CD pipeline: GitHub Actions for fmt/clippy/test on push/PR, release builds on tags
- CHANGELOG.md for tracking release notes

### Changed

### Fixed
- CI: `bif_core` build.rs no longer panics when vcpkg not installed (graceful skip for clippy-only mode)
- CI: removed bif_renderer/bif_viewport test steps that can't link without USD env
- CI: build.rs vcpkg detection checks toolchain file + USD headers (fixes false positive on GH Actions `C:\vcpkg`)
- CI: removed invalid `pxr` port from vcpkg.json (USD not available as standard vcpkg port)
