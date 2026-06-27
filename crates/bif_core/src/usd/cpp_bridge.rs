//! USD C++ Bridge — thin re-export shim.
//!
//! All implementation has moved to `usd/ffi/`. This file exists only
//! so that existing `use bif_core::usd::cpp_bridge::…` paths keep working.

pub use super::ffi::*;
