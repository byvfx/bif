//! Property Inspector - USD prim property viewer with editable transforms.
//!
//! Shows properties and metadata for the currently selected USD prim.
//! When a viewport instance is selected, provides editable DragValue fields
//! for translation, rotation (Euler degrees), and scale.

use std::sync::Arc;

use crate::app_event::{AppEvent, EventBus};
use crate::scene_browser::PrimDisplayInfo;
use bif_math::Mat4;

/// Properties for a selected prim to display in the inspector.
#[derive(Clone, Debug, Default)]
pub struct PrimProperties {
    /// Prim path
    pub path: String,

    /// Prim type name
    pub type_name: String,

    /// Whether prim is active
    pub is_active: bool,

    /// World transform matrix (if available)
    pub transform: Option<Mat4>,

    /// Bounding box min (if available)
    pub bounds_min: Option<bif_math::Vec3>,

    /// Bounding box max (if available)
    pub bounds_max: Option<bif_math::Vec3>,

    /// Additional key-value properties
    pub attributes: Vec<(String, String)>,

    /// Bound material (if available)
    pub bound_material: Option<Arc<bif_core::Material>>,
}

/// Event emitted when user edits a transform in the property inspector.
#[derive(Clone, Debug)]
pub struct TransformEdit {
    /// Instance index being edited.
    pub instance_index: usize,
    /// Previous transform (before the drag started).
    pub old_transform: bif_core::Transform,
    /// New transform (current drag value).
    pub new_transform: bif_core::Transform,
    /// True when the drag is released (finalize undo command).
    pub committed: bool,
}

impl PrimProperties {
    /// Create properties from a PrimDisplayInfo.
    pub fn from_display_info(info: &PrimDisplayInfo) -> Self {
        Self {
            path: info.path.clone(),
            type_name: info.type_name.clone(),
            is_active: info.is_active,
            transform: None,
            bounds_min: None,
            bounds_max: None,
            attributes: vec![("Children".to_string(), info.child_count.to_string())],
            bound_material: None,
        }
    }

    /// Set the world transform.
    pub fn with_transform(mut self, transform: Mat4) -> Self {
        self.transform = Some(transform);
        self
    }

    /// Set bounding box.
    pub fn with_bounds(mut self, min: bif_math::Vec3, max: bif_math::Vec3) -> Self {
        self.bounds_min = Some(min);
        self.bounds_max = Some(max);
        self
    }

    /// Add an attribute.
    pub fn with_attribute(mut self, name: &str, value: &str) -> Self {
        self.attributes.push((name.to_string(), value.to_string()));
        self
    }

    /// Set the bound material.
    pub fn with_material(mut self, material: Arc<bif_core::Material>) -> Self {
        self.bound_material = Some(material);
        self
    }
}

/// Render the property inspector panel.
///
/// `editable_transform` is `Some((instance_index, Transform))` when a viewport
/// instance is selected and its transform can be edited.
pub fn render_property_inspector(
    ui: &mut egui::Ui,
    event_bus: &mut EventBus,
    properties: Option<&PrimProperties>,
    editable_transform: Option<(usize, &bif_core::Transform)>,
) {
    ui.heading("Properties");
    ui.separator();

    match properties {
        None => {
            ui.label("No prim selected");
            ui.label("Select a prim in the Scene Browser");
        }
        Some(props) => {
            // Path
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.label(egui::RichText::new(&props.path).monospace());
            });

            // Type
            ui.horizontal(|ui| {
                ui.label("Type:");
                ui.label(&props.type_name);
            });

            // Active status
            ui.horizontal(|ui| {
                ui.label("Active:");
                if props.is_active {
                    ui.colored_label(egui::Color32::GREEN, "Yes");
                } else {
                    ui.colored_label(egui::Color32::RED, "No");
                }
            });

            ui.separator();

            // Transform (if available)
            if let Some(transform) = &props.transform {
                ui.collapsing("Transform", |ui| {
                    render_matrix(ui, transform);
                });
                ui.separator();
            }

            // Bounding box (if available)
            if let (Some(min), Some(max)) = (&props.bounds_min, &props.bounds_max) {
                ui.collapsing("Bounding Box", |ui| {
                    ui.label(format!("Min: ({:.3}, {:.3}, {:.3})", min.x, min.y, min.z));
                    ui.label(format!("Max: ({:.3}, {:.3}, {:.3})", max.x, max.y, max.z));

                    let size = *max - *min;
                    ui.label(format!(
                        "Size: ({:.3}, {:.3}, {:.3})",
                        size.x, size.y, size.z
                    ));

                    let center = (*min + *max) * 0.5;
                    ui.label(format!(
                        "Center: ({:.3}, {:.3}, {:.3})",
                        center.x, center.y, center.z
                    ));
                });
                ui.separator();
            }

            // Attributes
            if !props.attributes.is_empty() {
                ui.collapsing("Attributes", |ui| {
                    egui::Grid::new("attributes_grid")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (name, value) in &props.attributes {
                                ui.label(name);
                                ui.label(egui::RichText::new(value).monospace());
                                ui.end_row();
                            }
                        });
                });
            }

            // Bound Material
            if let Some(mat) = &props.bound_material {
                ui.separator();
                ui.collapsing("Bound Material", |ui| {
                    render_material_properties(ui, mat);
                });
            }
        }
    }

    // Editable transform section (when viewport instance is selected)
    if let Some((instance_index, transform)) = editable_transform {
        ui.separator();
        render_editable_transform(ui, event_bus, instance_index, transform);

        ui.add_space(8.0);
        if ui.button("Set Key (K)").clicked() {
            event_bus.emit(AppEvent::SetKeyframe(instance_index as u64));
        }
    }
}

/// Render editable DragValue fields for an instance transform.
///
/// Uses egui temporary data to pass `TransformEdit` events back to the renderer.
fn render_editable_transform(
    ui: &mut egui::Ui,
    event_bus: &mut EventBus,
    instance_index: usize,
    transform: &bif_core::Transform,
) {
    ui.heading("Instance Transform");

    // Read current editing values from egui temp data, or init from transform
    let edit_id = egui::Id::new("transform_edit_values");
    let drag_start_id = egui::Id::new("transform_drag_start");

    // Current editable values [tx,ty,tz, rx,ry,rz, sx,sy,sz]
    let mut values: [f32; 9] = ui.data(|d| {
        d.get_temp(edit_id).unwrap_or_else(|| {
            let euler = quat_to_euler_degrees(transform.rotation);
            [
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
                euler.0,
                euler.1,
                euler.2,
                transform.scale.x,
                transform.scale.y,
                transform.scale.z,
            ]
        })
    });

    let mut any_changed = false;
    let mut any_released = false;

    ui.label("Translation");
    egui::Grid::new("translate_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, r) = drag_value_row(ui, &["X", "Y", "Z"], &mut values[0..3], 0.1);
            any_changed |= c;
            any_released |= r;
        });

    ui.add_space(4.0);
    ui.label("Rotation");
    egui::Grid::new("rotation_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, r) = drag_value_row(ui, &["X", "Y", "Z"], &mut values[3..6], 1.0);
            any_changed |= c;
            any_released |= r;
        });

    ui.add_space(4.0);
    ui.label("Scale");
    egui::Grid::new("scale_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, r) = drag_value_row(ui, &["X", "Y", "Z"], &mut values[6..9], 0.01);
            any_changed |= c;
            any_released |= r;
        });

    // Store current values for next frame
    ui.data_mut(|d| d.insert_temp(edit_id, values));

    if any_changed {
        // Store drag start transform if not already stored
        let drag_start: Option<bif_core::Transform> = ui.data(|d| d.get_temp(drag_start_id));
        if drag_start.is_none() {
            ui.data_mut(|d| d.insert_temp(drag_start_id, transform.clone()));
        }

        let new_transform = values_to_transform(&values);

        // Emit live preview edit (not committed yet)
        event_bus.emit(AppEvent::TransformEdit(TransformEdit {
            instance_index,
            old_transform: transform.clone(),
            new_transform,
            committed: false,
        }));
    }

    if any_released {
        // Finalize: emit committed edit with drag start as old_transform
        let drag_start: Option<bif_core::Transform> = ui.data(|d| d.get_temp(drag_start_id));
        if let Some(old_transform) = drag_start {
            let new_transform = values_to_transform(&values);
            event_bus.emit(AppEvent::TransformEdit(TransformEdit {
                instance_index,
                old_transform,
                new_transform,
                committed: true,
            }));
            // Clear drag start
            ui.data_mut(|d| d.remove::<bif_core::Transform>(drag_start_id));
        }
    }
}

/// Render a row of 3 labeled DragValue fields. Returns (any_changed, any_released).
fn drag_value_row(
    ui: &mut egui::Ui,
    labels: &[&str; 3],
    values: &mut [f32],
    speed: f32,
) -> (bool, bool) {
    let mut changed = false;
    let mut released = false;

    let colors = [
        egui::Color32::from_rgb(200, 60, 60),  // R for X
        egui::Color32::from_rgb(60, 180, 60),  // G for Y
        egui::Color32::from_rgb(60, 100, 220), // B for Z
    ];

    for i in 0..3 {
        ui.colored_label(colors[i], labels[i]);
        let resp = ui.add(
            egui::DragValue::new(&mut values[i])
                .speed(speed)
                .fixed_decimals(3),
        );
        if resp.changed() {
            changed = true;
        }
        if resp.drag_stopped() || resp.lost_focus() {
            released = true;
        }
    }
    ui.end_row();

    (changed, released)
}

/// Convert quaternion to Euler angles in degrees (XYZ order).
fn quat_to_euler_degrees(q: bif_math::Quat) -> (f32, f32, f32) {
    let (yaw, pitch, roll) = q.to_euler(bif_math::EulerRot::XYZ);
    (yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees())
}

/// Convert editing values [tx,ty,tz, rx,ry,rz, sx,sy,sz] to a Transform.
fn values_to_transform(values: &[f32; 9]) -> bif_core::Transform {
    let translation = bif_math::Vec3::new(values[0], values[1], values[2]);
    let rotation = bif_math::Quat::from_euler(
        bif_math::EulerRot::XYZ,
        values[3].to_radians(),
        values[4].to_radians(),
        values[5].to_radians(),
    );
    let scale = bif_math::Vec3::new(values[6], values[7], values[8]);

    bif_core::Transform {
        translation,
        rotation,
        scale,
    }
}

/// Render material properties in a grid layout.
fn render_material_properties(ui: &mut egui::Ui, mat: &bif_core::Material) {
    // Material name
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.label(egui::RichText::new(mat.name.as_ref()).monospace());
    });
    ui.add_space(4.0);

    egui::Grid::new("material_props_grid")
        .num_columns(2)
        .striped(true)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            // Base color with swatch
            ui.label("Base Color");
            ui.horizontal(|ui| {
                let c = mat.base_color;
                let color = egui::Color32::from_rgb(
                    (c.x.clamp(0.0, 1.0) * 255.0) as u8,
                    (c.y.clamp(0.0, 1.0) * 255.0) as u8,
                    (c.z.clamp(0.0, 1.0) * 255.0) as u8,
                );
                show_color_swatch(ui, color);
                ui.label(
                    egui::RichText::new(format!("({:.3}, {:.3}, {:.3})", c.x, c.y, c.z))
                        .monospace(),
                );
            });
            ui.end_row();

            // Metalness
            ui.label("Metalness");
            ui.label(egui::RichText::new(format!("{:.3}", mat.base_metalness)).monospace());
            ui.end_row();

            // Roughness
            ui.label("Roughness");
            ui.label(egui::RichText::new(format!("{:.3}", mat.specular_roughness)).monospace());
            ui.end_row();

            // Specular Weight
            ui.label("Specular Weight");
            ui.label(egui::RichText::new(format!("{:.3}", mat.specular_weight)).monospace());
            ui.end_row();

            // Specular IOR
            ui.label("Specular IOR");
            ui.label(egui::RichText::new(format!("{:.3}", mat.specular_ior)).monospace());
            ui.end_row();

            // Transmission
            ui.label("Transmission");
            ui.label(egui::RichText::new(format!("{:.3}", mat.transmission_weight)).monospace());
            ui.end_row();

            // Opacity
            ui.label("Opacity");
            ui.label(egui::RichText::new(format!("{:.3}", mat.geometry_opacity)).monospace());
            ui.end_row();

            // Emission (only if luminance > 0)
            if mat.emission_luminance > 0.0 {
                ui.label("Emission Color");
                ui.horizontal(|ui| {
                    let c = mat.emission_color;
                    let color = egui::Color32::from_rgb(
                        (c.x.clamp(0.0, 1.0) * 255.0) as u8,
                        (c.y.clamp(0.0, 1.0) * 255.0) as u8,
                        (c.z.clamp(0.0, 1.0) * 255.0) as u8,
                    );
                    show_color_swatch(ui, color);
                    ui.label(
                        egui::RichText::new(format!("({:.3}, {:.3}, {:.3})", c.x, c.y, c.z))
                            .monospace(),
                    );
                });
                ui.end_row();

                ui.label("Emission Luminance");
                ui.label(
                    egui::RichText::new(format!("{:.1} nits", mat.emission_luminance)).monospace(),
                );
                ui.end_row();
            }

            // Double-sided
            ui.label("Double-Sided");
            ui.label(if mat.double_sided { "Yes" } else { "No" });
            ui.end_row();
        });

    // Textures (only show non-None)
    let textures = [
        ("Base Color", &mat.base_color_texture),
        ("Roughness", &mat.specular_roughness_texture),
        ("Metalness", &mat.base_metalness_texture),
        ("Normal", &mat.normal_texture),
        ("Emission", &mat.emission_texture),
        ("Opacity", &mat.geometry_opacity_texture),
    ];
    let has_textures = textures.iter().any(|(_, t)| t.is_some());

    if has_textures {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Textures").strong());
        egui::Grid::new("material_textures_grid")
            .num_columns(2)
            .striped(true)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for (name, tex) in &textures {
                    if let Some(path) = tex {
                        ui.label(*name);
                        ui.label(egui::RichText::new(path.as_ref()).monospace().small());
                        ui.end_row();
                    }
                }
            });
    }
}

/// Draw a small color swatch.
fn show_color_swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

/// Render a 4x4 matrix in a collapsible grid.
fn render_matrix(ui: &mut egui::Ui, matrix: &Mat4) {
    // Extract columns (Mat4 is column-major)
    let cols = matrix.to_cols_array_2d();

    egui::Grid::new("matrix_grid")
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for row in 0..4 {
                for col_data in cols.iter().take(4) {
                    // cols[col][row] because column-major
                    ui.label(
                        egui::RichText::new(format!("{:.3}", col_data[row]))
                            .monospace()
                            .small(),
                    );
                }
                ui.end_row();
            }
        });

    // Also show decomposed TRS if it's a typical transform
    // (translation in last column, rotation/scale in upper-left 3x3)
    let translation = bif_math::Vec3::new(cols[3][0], cols[3][1], cols[3][2]);

    ui.separator();
    ui.label("Translation:");
    ui.label(
        egui::RichText::new(format!(
            "({:.3}, {:.3}, {:.3})",
            translation.x, translation.y, translation.z
        ))
        .monospace(),
    );
}

/// Render Xform node properties in the property panel (like Nuke/Houdini).
///
/// Returns `true` if any value was changed (caller should emit XformChanged).
pub fn render_xform_properties(
    ui: &mut egui::Ui,
    translate: &mut [f32; 3],
    rotate: &mut [f32; 3],
    scale: &mut [f32; 3],
    prim_filter: &mut String,
) -> bool {
    ui.heading("Xform");
    ui.separator();

    let mut changed = false;

    ui.label("Translation");
    egui::Grid::new("xform_translate_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, _) = drag_value_row(ui, &["X", "Y", "Z"], translate.as_mut_slice(), 0.1);
            changed |= c;
        });

    ui.add_space(4.0);
    ui.label("Rotation");
    egui::Grid::new("xform_rotation_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, _) = drag_value_row(ui, &["X", "Y", "Z"], rotate.as_mut_slice(), 1.0);
            changed |= c;
        });

    ui.add_space(4.0);
    ui.label("Scale");
    egui::Grid::new("xform_scale_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, _) = drag_value_row(ui, &["X", "Y", "Z"], scale.as_mut_slice(), 0.01);
            changed |= c;
        });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add_enabled(
            false,
            egui::TextEdit::singleline(prim_filter)
                .hint_text("all prims (future)")
                .desired_width(ui.available_width()),
        );
    });

    changed
}

/// Reset cached transform edit values (call when selection changes).
pub fn reset_transform_edit_cache(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.remove::<[f32; 9]>(egui::Id::new("transform_edit_values"));
        d.remove::<bif_core::Transform>(egui::Id::new("transform_drag_start"));
        d.remove::<TransformEdit>(egui::Id::new("transform_edit_event"));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_browser::PrimDisplayInfo;

    #[test]
    fn test_prim_properties_from_display_info() {
        let info =
            PrimDisplayInfo::new("/World/Mesh".to_string(), "Mesh".to_string(), true, true, 3);

        let props = PrimProperties::from_display_info(&info);

        assert_eq!(props.path, "/World/Mesh");
        assert_eq!(props.type_name, "Mesh");
        assert!(props.is_active);
        assert!(props.transform.is_none());
    }

    #[test]
    fn test_prim_properties_builder() {
        let info = PrimDisplayInfo::new(
            "/World/Mesh".to_string(),
            "Mesh".to_string(),
            true,
            false,
            0,
        );

        let props = PrimProperties::from_display_info(&info)
            .with_transform(Mat4::IDENTITY)
            .with_bounds(
                bif_math::Vec3::new(-1.0, -1.0, -1.0),
                bif_math::Vec3::new(1.0, 1.0, 1.0),
            )
            .with_attribute("vertices", "1024");

        assert!(props.transform.is_some());
        assert!(props.bounds_min.is_some());
        assert!(props.bounds_max.is_some());
        assert!(props.attributes.iter().any(|(k, _)| k == "vertices"));
    }

    #[test]
    fn test_quat_to_euler_roundtrip() {
        let original = bif_math::Quat::from_euler(
            bif_math::EulerRot::XYZ,
            30.0_f32.to_radians(),
            45.0_f32.to_radians(),
            0.0_f32.to_radians(),
        );

        let (rx, ry, rz) = quat_to_euler_degrees(original);

        assert!((rx - 30.0).abs() < 0.1);
        assert!((ry - 45.0).abs() < 0.1);
        assert!((rz - 0.0).abs() < 0.1);
    }

    #[test]
    fn test_values_to_transform() {
        let values = [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let t = values_to_transform(&values);

        assert!((t.translation.x - 1.0).abs() < 0.001);
        assert!((t.translation.y - 2.0).abs() < 0.001);
        assert!((t.translation.z - 3.0).abs() < 0.001);
        assert!((t.scale.x - 1.0).abs() < 0.001);
    }
}
