//! Layer Stack panel (v0.14.0) — displays the USD sublayer tree with mute
//! toggles, working-layer radio buttons, an isolation-mode header toggle,
//! and per-layer offset labels. Read-only in v0.14.0.
//!
//! The panel consumes a `&SceneLayerState` and emits `AppEvent`s via a
//! caller-provided [`EventBus`]. It deliberately does not hold a reference
//! to `Renderer` — the render loop owns state and drives dispatch; this
//! panel just turns user interactions into typed events.

use std::collections::HashSet;

use bif_core::usd::layer::LayerInfo;
use bif_core::SceneLayerState;
use egui::{RichText, Ui};

use crate::app_event::{AppEvent, EventBus};
use crate::theme::{layer_color, TEXT_DISABLED, TEXT_SECONDARY};

/// Stateful panel: tracks UI-only state that doesn't belong in
/// [`SceneLayerState`] (what's visually focused, which branches are open).
#[derive(Default)]
pub struct LayerStackPanel {
    /// Layer index currently focused in the panel — feeds future context
    /// menus and keyboard actions. Distinct from `SceneLayerState::working_layer`.
    pub focused: Option<usize>,
    /// Layer indices whose sublayer children are expanded. Empty = all
    /// collapsed (future: v0.14.5 tree-collapse UI; today we render flat).
    pub expanded: HashSet<usize>,
}

impl LayerStackPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Render the panel into `ui`. Emits events via `events` in response
    /// to clicks and toggles. Safe to call even when `state.stack` is empty.
    pub fn render(&mut self, ui: &mut Ui, state: &SceneLayerState, events: &mut EventBus) {
        ui.horizontal(|ui| {
            ui.heading("Layers");
            ui.separator();

            let mut isolation = state.isolation_mode;
            if ui
                .checkbox(&mut isolation, "Isolate working layer")
                .on_hover_text("Dims opinions from non-working layers. Read-only hint in v0.14.")
                .changed()
            {
                events.emit(AppEvent::IsolationModeToggled(isolation));
            }
        });

        ui.separator();

        if state.stack.layers.is_empty() {
            ui.label(RichText::new("No USD stage loaded.").color(TEXT_SECONDARY));
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, layer) in state.stack.layers.iter().enumerate() {
                self.render_layer_row(ui, state, idx, layer, events);
            }
        });
    }

    fn render_layer_row(
        &mut self,
        ui: &mut Ui,
        state: &SceneLayerState,
        idx: usize,
        layer: &LayerInfo,
        events: &mut EventBus,
    ) {
        ui.horizontal(|ui| {
            // Depth indent — 14px per level makes root + 2 sublayers visually clean.
            let indent = 14.0 * layer.depth as f32;
            if indent > 0.0 {
                ui.add_space(indent);
            }

            // Layer color dot (matches scene-browser dots + property-row borders).
            let dot_color = layer_color(idx);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, dot_color);

            // Working-layer radio. Single-char label gives a larger hit
            // area than an empty-label radio and doubles as a visible cue.
            let is_working = state.working_layer == idx;
            let radio_clicked = ui
                .radio(is_working, "W")
                .on_hover_text("Set as working layer")
                .clicked();
            if radio_clicked && !is_working {
                events.emit(AppEvent::WorkingLayerChanged(idx));
            }

            // Mute checkbox — single-char label for the same reason.
            let mut muted = layer.is_muted;
            let mute_resp = ui
                .checkbox(&mut muted, "M")
                .on_hover_text(if layer.is_muted { "Unmute" } else { "Mute" });
            if mute_resp.changed() {
                events.emit(AppEvent::LayerMuteToggled { index: idx, muted });
            }

            // Display name — muted layers show strikethrough in dimmed color;
            // working layer shows in bold.
            let label_text = if layer.is_muted {
                RichText::new(&layer.display_name)
                    .color(TEXT_DISABLED)
                    .strikethrough()
            } else if is_working {
                RichText::new(&layer.display_name).strong()
            } else {
                RichText::new(&layer.display_name)
            };

            if ui
                .selectable_label(self.focused == Some(idx), label_text)
                .on_hover_text(&layer.identifier)
                .clicked()
            {
                self.focused = Some(idx);
                events.emit(AppEvent::LayerSelected(idx));
            }

            // Layer offset — show when not identity (authored time offset/scale).
            if !layer.offset.is_identity() {
                ui.label(
                    RichText::new(format!(
                        "(+{:.1} \u{00d7}{:.2})",
                        layer.offset.offset, layer.offset.scale
                    ))
                    .color(TEXT_SECONDARY)
                    .small(),
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bif_core::usd::layer::{LayerOffset, LayerStack, PayloadPolicy};

    #[test]
    fn new_panel_is_default() {
        let panel = LayerStackPanel::new();
        assert!(panel.focused.is_none());
        assert!(panel.expanded.is_empty());
    }

    #[test]
    fn panel_default_matches_new() {
        let a = LayerStackPanel::new();
        let b = LayerStackPanel::default();
        assert_eq!(a.focused, b.focused);
        assert_eq!(a.expanded, b.expanded);
    }

    /// Smoke test: panel renders without panicking against a realistic
    /// `SceneLayerState` via egui's headless test context.
    #[test]
    fn render_does_not_panic_on_empty_state() {
        let ctx = egui::Context::default();
        let mut panel = LayerStackPanel::new();
        let state = SceneLayerState::default();
        let mut bus = EventBus::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                panel.render(ui, &state, &mut bus);
            });
        });
    }

    #[test]
    fn render_does_not_panic_with_two_layers() {
        let ctx = egui::Context::default();
        let mut panel = LayerStackPanel::new();
        let state = SceneLayerState {
            stack: LayerStack {
                layers: vec![
                    LayerInfo {
                        identifier: "root.usd".into(),
                        display_name: "root.usd".into(),
                        real_path: Default::default(),
                        is_anonymous: false,
                        is_dirty: false,
                        is_muted: false,
                        permission_to_edit: true,
                        offset: LayerOffset::default(),
                        parent_index: None,
                        depth: 0,
                    },
                    LayerInfo {
                        identifier: "anim.usd".into(),
                        display_name: "anim.usd".into(),
                        real_path: Default::default(),
                        is_anonymous: false,
                        is_dirty: false,
                        is_muted: true,
                        permission_to_edit: true,
                        offset: LayerOffset {
                            offset: 24.0,
                            scale: 1.0,
                        },
                        parent_index: Some(0),
                        depth: 1,
                    },
                ],
                root_index: 0,
            },
            working_layer: 0,
            muted: std::iter::once("anim.usd".to_string()).collect(),
            isolation_mode: false,
            payload_policy: PayloadPolicy::LoadAll,
            layer_for_prim: Default::default(),
        };
        let mut bus = EventBus::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                panel.render(ui, &state, &mut bus);
            });
        });
    }
}
