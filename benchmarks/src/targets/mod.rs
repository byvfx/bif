//! Target abstraction — BIF native, usdview, Houdini.

pub mod houdini;
pub mod usdview;

/// List available target names.
pub fn available_targets() -> Vec<&'static str> {
    vec!["bif", "usdview", "houdini"]
}
