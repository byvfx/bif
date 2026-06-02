//! Regression test for the 24 GiB OOM panic loading a production shot file.
//!
//! On 2026-05-23 `bif_viewer` aborted with
//! `memory allocation of 25769803776 bytes failed`
//! when opening `rt_010_base.usda` — the assembly/anim sublayers fed a
//! `PointInstancer` with ~25k instances × ~16k time samples into
//! `convert_instancer_animation`, which called `Vec::with_capacity` on the
//! unchecked `time_sample_count * instance_count` product. The fix is in
//! `usd::ffi_guard` + the guarded `convert_instancer_animation`.
//!
//! This test is `#[ignore]` because it depends on an absolute path that only
//! exists on the developer's machine. Run with:
//!     cargo test -p bif_core --test rt_010_base_oom_regression -- --ignored --nocapture

use std::path::Path;

#[test]
#[ignore]
fn rt_010_base_loads_without_aborting() {
    let path = Path::new("G:/USDs/Distributable_2023_Davinci/shot/rt/rt_010/rt_010_base.usda");
    if !path.exists() {
        eprintln!("fixture not present at {} — skipping", path.display());
        return;
    }

    // Before the fix this aborted the process with STATUS_STACK_BUFFER_OVERRUN.
    // After the fix it must return a `Result` (Ok or Err) without aborting.
    match bif_core::usd::load_usd(path) {
        Ok(scene) => {
            eprintln!(
                "loaded ok: {} prototypes, {} instances",
                scene.prototype_count(),
                scene.instance_count()
            );
        }
        Err(err) => {
            // A user-visible error is acceptable; a process abort is not.
            eprintln!("loaded with error (no abort, OK): {err}");
        }
    }
}
