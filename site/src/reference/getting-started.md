# Getting Started

## Prerequisites

- **Rust 1.86+** — [rustup.rs](https://rustup.rs/)
- **Visual Studio 2022** C++ workload (Windows) or cmake + pkg-config (Linux)
- **Pixar USD 25.11** — required for USD scene loading

## Build

```bash
git clone https://github.com/byvfx/bif.git
cd bif
cargo build
```

### Optional Features

```bash
cargo build --features oidn    # Intel OIDN denoising
cargo build --features oiio    # OpenImageIO .tx conversion
```

Both features are off by default. The UI gracefully degrades without them.

## Run

```bash
# Basic launch
cargo run -p bif_viewer

# With USD support (needs USD env)
. .\setup_usd_env.ps1
cargo run -p bif_viewer

# With denoising
cargo run -p bif_viewer --features oidn
```

## Viewport Controls

| Input | Action |
|-------|--------|
| Left drag | Orbit |
| Middle drag | Pan |
| Scroll | Dolly |
| WASD + QE | Fly |
| F | Frame selection |

## Tests

```bash
cargo test -p bif_math         # 41 tests (no deps)
cargo test -p bif_renderer     # 68+ tests
cargo test -p bif_viewport     # 24 tests

# USD tests require env setup
. .\setup_usd_env.ps1
cargo test -p bif_core -- --test-threads=1
```

Note: `bif_core` tests must run single-threaded — the USD C++ bridge is not thread-safe.

## Checks

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

## Environment Setup

### USD

The `setup_usd_env.ps1` script sets `PATH` and library paths for USD DLLs. Must be sourced before running anything that loads USD scenes.

### OIDN

Set `OIDN_DIR` to your Intel OIDN installation and add `$OIDN_DIR/bin` to `PATH`.

## Performance Benchmarking

BIF includes a performance harness (`bif_perf`):

```bash
. .\setup_usd_env.ps1
cargo run -p bif_perf -- list                     # show available metrics/scenes
cargo run -p bif_perf -- run all -n 10            # quick benchmark
cargo run -p bif_perf -- run medium -n 50 --save  # save results
cargo run -p bif_perf -- audit scene.usdc         # best-practice checks
cargo run -p bif_perf -- compare base.yaml new.yaml  # regression detection
```
