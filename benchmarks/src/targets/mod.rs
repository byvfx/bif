//! Target abstraction — BIF native, usdview, Houdini.
//!
//! Phase 1 implements BIF-native only. usdview and Houdini targets
//! are deferred to Phase 4.

/// List available target names.
pub fn available_targets() -> Vec<&'static str> {
    vec!["bif"]
}
