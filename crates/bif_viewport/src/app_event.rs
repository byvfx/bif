//! Typed event bus replacing egui temp-data ad-hoc event passing.
//!
//! UI panels push `AppEvent` variants; the render loop drains and dispatches.
//! When Qt replaces egui, the same `AppEvent` variants are emitted — zero
//! coupling to any UI framework.

use std::path::PathBuf;

use crate::node_graph::NodeGraphEvent;
use crate::property_inspector::TransformEdit;

/// Camera projection mode — typed replacement for string matching.
#[derive(Debug, Clone)]
pub enum CameraProjection {
    Perspective,
    Ortho(String),
}

/// Typed event emitted by UI code, consumed by the render loop each frame.
#[derive(Debug)]
pub enum AppEvent {
    /// Render mode toggled (Vulkan ↔ Ivar).
    RenderModeChanged,
    /// User requested manual scene rebuild.
    RebuildScene,
    /// User requested OIDN denoising.
    DenoiseRequested,
    /// Pixel filter or sampler changed — restart progressive render.
    FilterChanged,
    /// Start batch render.
    StartBatchRender,
    /// Cancel in-progress batch render.
    CancelBatchRender,
    /// Sync viewport camera to a USD camera by path (batch panel or timeline).
    SyncUsdCamera(String),
    /// Sync viewport to a scene camera by index.
    SyncSceneCamera(u64),
    /// Prim selected in scene browser.
    PrimSelected(String),
    /// Transform edit from property inspector (live preview or committed).
    TransformEdit(TransformEdit),
    /// Stage axis/unit correction toggle changed.
    StageCorrectionsChanged,
    /// Set animation keyframe at instance index.
    SetKeyframe(u64),
    /// Camera projection changed.
    CameraProjectionChange(CameraProjection),
    /// Export edit layer to USD file.
    ExportEditLayer(PathBuf),
    /// One or more node graph events.
    NodeGraph(Vec<NodeGraphEvent>),
}

/// Frame-scoped event bus. UI pushes events, render loop drains them.
#[derive(Default)]
pub struct EventBus {
    pending: Vec<AppEvent>,
}

impl EventBus {
    /// Push an event to be processed this frame.
    pub fn emit(&mut self, event: AppEvent) {
        self.pending.push(event);
    }

    /// Drain all pending events. Preserves vec capacity for next frame.
    pub fn drain(&mut self) -> Vec<AppEvent> {
        let mut events = Vec::with_capacity(self.pending.len());
        std::mem::swap(&mut events, &mut self.pending);
        events
    }

    /// Check if any events are pending.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_and_drain() {
        let mut bus = EventBus::default();
        assert!(bus.is_empty());

        bus.emit(AppEvent::RebuildScene);
        bus.emit(AppEvent::FilterChanged);
        assert!(!bus.is_empty());

        let events = bus.drain();
        assert_eq!(events.len(), 2);
        assert!(bus.is_empty());

        // Second drain returns empty
        assert!(bus.drain().is_empty());
    }

    #[test]
    fn drain_preserves_order() {
        let mut bus = EventBus::default();
        bus.emit(AppEvent::StartBatchRender);
        bus.emit(AppEvent::CancelBatchRender);
        bus.emit(AppEvent::DenoiseRequested);

        let events = bus.drain();
        assert!(matches!(events[0], AppEvent::StartBatchRender));
        assert!(matches!(events[1], AppEvent::CancelBatchRender));
        assert!(matches!(events[2], AppEvent::DenoiseRequested));
    }
}
