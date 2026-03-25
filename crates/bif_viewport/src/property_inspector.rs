//! Property Inspector - usdview-style property viewer with editable transforms.
//!
//! Top/bottom split: flat property table on top, tabbed detail panel on bottom.
//! When a viewport instance is selected, provides editable DragValue fields
//! for translation, rotation (Euler degrees), and scale.

use std::sync::Arc;

use crate::app_event::{AppEvent, EventBus};
use crate::node_graph::{GraphNodeId, NodeGraphEvent, SceneNode};
use crate::scene_browser::PrimDisplayInfo;
use crate::theme;
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

/// Which detail tab is active in the bottom panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PropertyTab {
    #[default]
    Value,
    MetaData,
}

/// Status indicator for a property row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyStatus {
    /// Green — resolved/bound value.
    Resolved,
    /// Red — unresolved/unbound.
    Unresolved,
    /// Grey-blue — informational.
    Info,
}

/// A single row in the property table.
#[derive(Clone, Debug)]
struct PropertyRow {
    status: PropertyStatus,
    name: String,
    summary: String,
    detail: PropertyDetail,
}

/// Typed detail payload for the Value tab.
#[derive(Clone, Debug)]
enum PropertyDetail {
    Text(String),
    Color(f32, f32, f32),
    Matrix(bif_math::Mat4),
    BoundingBox {
        min: bif_math::Vec3,
        max: bif_math::Vec3,
    },
    TexturePath(String),
    Float(f32),
    Bool(bool),
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

/// Build flat property rows from PrimProperties for the table view.
fn build_property_rows(props: &PrimProperties) -> Vec<PropertyRow> {
    let mut rows = Vec::new();

    // Bounding box
    if let (Some(min), Some(max)) = (&props.bounds_min, &props.bounds_max) {
        rows.push(PropertyRow {
            status: PropertyStatus::Resolved,
            name: "World Bounding Box".to_string(),
            summary: format!(
                "({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1})",
                min.x, min.y, min.z, max.x, max.y, max.z
            ),
            detail: PropertyDetail::BoundingBox {
                min: *min,
                max: *max,
            },
        });
    }

    // Transform
    if let Some(transform) = &props.transform {
        let cols = transform.to_cols_array_2d();
        let t = bif_math::Vec3::new(cols[3][0], cols[3][1], cols[3][2]);
        rows.push(PropertyRow {
            status: PropertyStatus::Resolved,
            name: "Local to World Xform".to_string(),
            summary: format!("({:.3}, {:.3}, {:.3})", t.x, t.y, t.z),
            detail: PropertyDetail::Matrix(*transform),
        });
    }

    // Material section
    if let Some(mat) = &props.bound_material {
        rows.push(PropertyRow {
            status: PropertyStatus::Resolved,
            name: "Resolved Material".to_string(),
            summary: mat.name.to_string(),
            detail: PropertyDetail::Text(format!("Material: {}", mat.name)),
        });

        // Base color
        let c = mat.base_color;
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Base Color".to_string(),
            summary: format!("({:.3}, {:.3}, {:.3})", c.x, c.y, c.z),
            detail: PropertyDetail::Color(c.x, c.y, c.z),
        });

        // Metalness
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Metalness".to_string(),
            summary: format!("{:.3}", mat.base_metalness),
            detail: PropertyDetail::Float(mat.base_metalness),
        });

        // Roughness
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Roughness".to_string(),
            summary: format!("{:.3}", mat.specular_roughness),
            detail: PropertyDetail::Float(mat.specular_roughness),
        });

        // Specular Weight
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Specular Weight".to_string(),
            summary: format!("{:.3}", mat.specular_weight),
            detail: PropertyDetail::Float(mat.specular_weight),
        });

        // Specular IOR
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Specular IOR".to_string(),
            summary: format!("{:.3}", mat.specular_ior),
            detail: PropertyDetail::Float(mat.specular_ior),
        });

        // Transmission
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Transmission".to_string(),
            summary: format!("{:.3}", mat.transmission_weight),
            detail: PropertyDetail::Float(mat.transmission_weight),
        });

        // Opacity
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Opacity".to_string(),
            summary: format!("{:.3}", mat.geometry_opacity),
            detail: PropertyDetail::Float(mat.geometry_opacity),
        });

        // Emission (only if luminance > 0)
        if mat.emission_luminance > 0.0 {
            rows.push(PropertyRow {
                status: PropertyStatus::Info,
                name: "Emission Luminance".to_string(),
                summary: format!("{:.1} nits", mat.emission_luminance),
                detail: PropertyDetail::Float(mat.emission_luminance),
            });

            let ec = mat.emission_color;
            rows.push(PropertyRow {
                status: PropertyStatus::Info,
                name: "Emission Color".to_string(),
                summary: format!("({:.3}, {:.3}, {:.3})", ec.x, ec.y, ec.z),
                detail: PropertyDetail::Color(ec.x, ec.y, ec.z),
            });
        }

        // Double-sided
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: "Double-Sided".to_string(),
            summary: if mat.double_sided {
                "Yes".to_string()
            } else {
                "No".to_string()
            },
            detail: PropertyDetail::Bool(mat.double_sided),
        });

        // Textures (only non-None)
        let textures: &[(&str, &Option<Arc<str>>)] = &[
            ("base_color_texture", &mat.base_color_texture),
            ("roughness_texture", &mat.specular_roughness_texture),
            ("metalness_texture", &mat.base_metalness_texture),
            ("normal_texture", &mat.normal_texture),
            ("emission_texture", &mat.emission_texture),
            ("opacity_texture", &mat.geometry_opacity_texture),
        ];
        for (name, tex) in textures {
            if let Some(path) = tex {
                // Extract filename for summary
                let filename = path.rsplit(['/', '\\']).next().unwrap_or(path.as_ref());
                rows.push(PropertyRow {
                    status: PropertyStatus::Resolved,
                    name: name.to_string(),
                    summary: filename.to_string(),
                    detail: PropertyDetail::TexturePath(path.to_string()),
                });
            }
        }
    } else {
        // No material bound
        rows.push(PropertyRow {
            status: PropertyStatus::Unresolved,
            name: "Resolved Material".to_string(),
            summary: "<unbound>".to_string(),
            detail: PropertyDetail::Text("No material bound to this prim.".to_string()),
        });
    }

    // Extra attributes
    for (name, value) in &props.attributes {
        rows.push(PropertyRow {
            status: PropertyStatus::Info,
            name: name.clone(),
            summary: value.clone(),
            detail: PropertyDetail::Text(value.clone()),
        });
    }

    rows
}

/// Render the property inspector panel (usdview-style).
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

    match properties {
        None => {
            ui.label("No prim selected");
            ui.label("Select a prim in the Scene Browser");
        }
        Some(props) => {
            // Header: path, type, active
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.label(egui::RichText::new(&props.path).monospace());
            });
            ui.horizontal(|ui| {
                ui.label("Type:");
                ui.label(&props.type_name);
                ui.separator();
                ui.label("Active:");
                if props.is_active {
                    ui.colored_label(theme::STATUS_OK, "Yes");
                } else {
                    ui.colored_label(theme::STATUS_ERROR, "No");
                }
            });

            // Editable transform section (when viewport instance is selected)
            if let Some((instance_index, transform)) = editable_transform {
                ui.separator();
                render_editable_transform(ui, event_bus, instance_index, transform);
                ui.add_space(4.0);
                if ui
                    .button("Set Key (K)")
                    .on_hover_text(
                        "Set a keyframe for this instance's transform at the current frame",
                    )
                    .clicked()
                {
                    event_bus.emit(AppEvent::SetKeyframe(instance_index as u64));
                }
            }

            ui.separator();

            // Build property rows
            let rows = build_property_rows(props);

            // Read persisted state from egui temp data
            let selected_id = egui::Id::new("prop_selected_row");
            let tab_id = egui::Id::new("property_detail_tab");

            let mut selected_row: Option<usize> = ui.data(|d| d.get_temp(selected_id));
            let mut active_tab: PropertyTab = ui.data(|d| d.get_temp(tab_id).unwrap_or_default());

            // -- Top: scrollable property table --
            let table_height = ui.available_height() * 0.55;
            egui::ScrollArea::vertical()
                .id_salt("property_table_scroll")
                .max_height(table_height)
                .show(ui, |ui| {
                    egui::Grid::new("property_table_grid")
                        .num_columns(3)
                        .striped(true)
                        .spacing([6.0, 3.0])
                        .show(ui, |ui| {
                            for (idx, row) in rows.iter().enumerate() {
                                // Col 1: status circle
                                let circle_color = match row.status {
                                    PropertyStatus::Resolved => theme::STATUS_OK,
                                    PropertyStatus::Unresolved => theme::STATUS_ERROR,
                                    PropertyStatus::Info => theme::STATUS_INFO,
                                };
                                let (circle_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(10.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter()
                                    .circle_filled(circle_rect.center(), 4.0, circle_color);

                                // Col 2: property name
                                ui.label(&row.name);

                                // Col 3: summary (monospace, truncated)
                                let summary_resp =
                                    ui.label(egui::RichText::new(&row.summary).monospace());

                                // Make entire row clickable
                                let row_rect = circle_rect.union(summary_resp.rect);
                                let row_id = ui.id().with(idx);
                                let interact = ui.interact(row_rect, row_id, egui::Sense::click());

                                if interact.clicked() {
                                    selected_row = Some(idx);
                                }

                                // Highlight selected row
                                if selected_row == Some(idx) {
                                    ui.painter().rect_filled(
                                        row_rect.expand(1.0),
                                        2.0,
                                        ui.visuals().selection.bg_fill.linear_multiply(0.3),
                                    );
                                }

                                ui.end_row();
                            }
                        });
                });

            ui.separator();

            // -- Bottom: tabbed detail panel --
            ui.horizontal(|ui| {
                for tab in [PropertyTab::Value, PropertyTab::MetaData] {
                    let label = match tab {
                        PropertyTab::Value => "Value",
                        PropertyTab::MetaData => "Meta Data",
                    };
                    if ui.selectable_label(active_tab == tab, label).clicked() {
                        active_tab = tab;
                    }
                }
            });

            egui::ScrollArea::vertical()
                .id_salt("property_detail_scroll")
                .show(ui, |ui| match active_tab {
                    PropertyTab::Value => {
                        if let Some(idx) = selected_row {
                            if let Some(row) = rows.get(idx) {
                                render_property_detail(ui, row);
                            } else {
                                ui.label("Select a property above");
                            }
                        } else {
                            ui.label("Select a property above");
                        }
                    }
                    PropertyTab::MetaData => {
                        render_metadata_tab(ui, props);
                    }
                });

            // Store state
            ui.data_mut(|d| {
                d.insert_temp(selected_id, selected_row);
                d.insert_temp(tab_id, active_tab);
            });
        }
    }
}

/// Render the detail view for a selected property row (Value tab).
fn render_property_detail(ui: &mut egui::Ui, row: &PropertyRow) {
    ui.label(egui::RichText::new(&row.name).strong());
    ui.add_space(4.0);

    match &row.detail {
        PropertyDetail::Text(text) => {
            ui.label(egui::RichText::new(text).monospace());
        }
        PropertyDetail::Color(r, g, b) => {
            ui.horizontal(|ui| {
                let color = egui::Color32::from_rgb(
                    (r.clamp(0.0, 1.0) * 255.0) as u8,
                    (g.clamp(0.0, 1.0) * 255.0) as u8,
                    (b.clamp(0.0, 1.0) * 255.0) as u8,
                );
                show_color_swatch(ui, color, 24.0);
                ui.label(
                    egui::RichText::new(format!("({:.3}, {:.3}, {:.3})", r, g, b)).monospace(),
                );
            });
        }
        PropertyDetail::Matrix(matrix) => {
            render_matrix(ui, matrix);
        }
        PropertyDetail::BoundingBox { min, max } => {
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
        }
        PropertyDetail::TexturePath(path) => {
            ui.label(egui::RichText::new(path).monospace().small());
        }
        PropertyDetail::Float(val) => {
            ui.label(egui::RichText::new(format!("{:.6}", val)).monospace());
        }
        PropertyDetail::Bool(val) => {
            if *val {
                ui.colored_label(theme::STATUS_OK, "Yes");
            } else {
                ui.colored_label(theme::STATUS_ERROR, "No");
            }
        }
    }
}

/// Render the Meta Data tab content.
fn render_metadata_tab(ui: &mut egui::Ui, props: &PrimProperties) {
    egui::Grid::new("metadata_grid")
        .num_columns(2)
        .striped(true)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label("Path");
            ui.label(egui::RichText::new(&props.path).monospace());
            ui.end_row();

            ui.label("Type");
            ui.label(&props.type_name);
            ui.end_row();

            ui.label("Active");
            ui.label(if props.is_active { "Yes" } else { "No" });
            ui.end_row();

            // Child count from attributes
            if let Some((_, count)) = props.attributes.iter().find(|(k, _)| k == "Children") {
                ui.label("Children");
                ui.label(count);
                ui.end_row();
            }
        });
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
            let (c, r) = drag_value_row(
                ui,
                &["X", "Y", "Z"],
                &mut values[0..3],
                0.1,
                "Translation",
                "",
            );
            any_changed |= c;
            any_released |= r;
        });

    ui.add_space(4.0);
    ui.label("Rotation");
    egui::Grid::new("rotation_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, r) = drag_value_row(
                ui,
                &["X", "Y", "Z"],
                &mut values[3..6],
                1.0,
                "Rotation",
                "degrees",
            );
            any_changed |= c;
            any_released |= r;
        });

    ui.add_space(4.0);
    ui.label("Scale");
    egui::Grid::new("scale_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, r) = drag_value_row(ui, &["X", "Y", "Z"], &mut values[6..9], 0.01, "Scale", "");
            any_changed |= c;
            any_released |= r;
        });

    // Store current values for next frame
    ui.data_mut(|d| d.insert_temp(edit_id, values));

    if any_changed {
        // Store drag start transform if not already stored
        let drag_start: Option<bif_core::Transform> = ui.data(|d| d.get_temp(drag_start_id));
        if drag_start.is_none() {
            ui.data_mut(|d| d.insert_temp(drag_start_id, *transform));
        }

        let new_transform = values_to_transform(&values);

        // Emit live preview edit (not committed yet)
        event_bus.emit(AppEvent::TransformEdit(TransformEdit {
            instance_index,
            old_transform: *transform,
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
///
/// `section` is the transform section name ("Translation", "Rotation", "Scale") used
/// for tooltip text on each DragValue.
fn drag_value_row(
    ui: &mut egui::Ui,
    labels: &[&str; 3],
    values: &mut [f32],
    speed: f32,
    section: &str,
    unit: &str,
) -> (bool, bool) {
    let mut changed = false;
    let mut released = false;

    let colors = [
        theme::AXIS_X, // R for X
        theme::AXIS_Y, // G for Y
        theme::AXIS_Z, // B for Z
    ];

    for i in 0..3 {
        ui.colored_label(colors[i], labels[i]);
        let tooltip = if unit.is_empty() {
            format!("{} {}", section, labels[i])
        } else {
            format!("{} {} ({})", section, labels[i], unit)
        };
        let resp = ui
            .add(
                egui::DragValue::new(&mut values[i])
                    .speed(speed)
                    .fixed_decimals(3),
            )
            .on_hover_text(tooltip);
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

/// Draw a color swatch of given size.
fn show_color_swatch(ui: &mut egui::Ui, color: egui::Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

/// Render a 4x4 matrix in a grid.
fn render_matrix(ui: &mut egui::Ui, matrix: &Mat4) {
    let cols = matrix.to_cols_array_2d();

    egui::Grid::new("matrix_grid")
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for row in 0..4 {
                for col_data in cols.iter().take(4) {
                    ui.label(
                        egui::RichText::new(format!("{:.3}", col_data[row]))
                            .monospace()
                            .small(),
                    );
                }
                ui.end_row();
            }
        });

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
            let (c, _) = drag_value_row(
                ui,
                &["X", "Y", "Z"],
                translate.as_mut_slice(),
                0.1,
                "Translation",
                "",
            );
            changed |= c;
        });

    ui.add_space(4.0);
    ui.label("Rotation");
    egui::Grid::new("xform_rotation_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, _) = drag_value_row(
                ui,
                &["X", "Y", "Z"],
                rotate.as_mut_slice(),
                1.0,
                "Rotation",
                "degrees",
            );
            changed |= c;
        });

    ui.add_space(4.0);
    ui.label("Scale");
    egui::Grid::new("xform_scale_grid")
        .num_columns(4)
        .spacing([4.0, 2.0])
        .show(ui, |ui| {
            let (c, _) = drag_value_row(
                ui,
                &["X", "Y", "Z"],
                scale.as_mut_slice(),
                0.01,
                "Scale",
                "",
            );
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

/// Render parameters for the selected node in the property inspector.
/// Returns a list of node graph events to emit.
pub(crate) fn render_node_properties(
    ui: &mut egui::Ui,
    node: &mut SceneNode,
    node_id: GraphNodeId,
) -> Vec<NodeGraphEvent> {
    let mut events = Vec::new();

    match node {
        SceneNode::UsdRead {
            file_path,
            is_loaded,
            error,
        } => {
            ui.heading("USD Read");
            ui.horizontal(|ui| {
                ui.label("File:");
                if ui.text_edit_singleline(file_path).changed() {
                    *is_loaded = false;
                    *error = None;
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("USD Files", &["usda", "usdc", "usd"])
                        .add_filter("All Files", &["*"])
                        .pick_file()
                    {
                        *file_path = path.display().to_string();
                        *is_loaded = false;
                        *error = None;
                        events.push(NodeGraphEvent::LoadUsdFile {
                            path: file_path.clone(),
                            node_id,
                        });
                    }
                }
                if ui.button("Load").clicked() && !file_path.is_empty() {
                    events.push(NodeGraphEvent::LoadUsdFile {
                        path: file_path.clone(),
                        node_id,
                    });
                }
            });
            if *is_loaded {
                ui.colored_label(theme::STATUS_OK, "\u{2713} Loaded");
            } else if let Some(err) = error {
                ui.colored_label(theme::STATUS_ERROR, format!("\u{2717} {}", err));
            }
        }
        SceneNode::IvarRender {
            spp,
            is_rendering,
            is_converting_tx,
            tx_status,
        } => {
            ui.heading("Ivar Render");
            ui.horizontal(|ui| {
                ui.label("SPP:");
                ui.add(egui::DragValue::new(spp).range(1..=1024))
                    .on_hover_text("Samples per pixel — higher = less noise, slower");
            });
            if *is_rendering {
                ui.colored_label(theme::STATUS_WARNING, "Rendering...");
            } else if ui
                .button("Render")
                .on_hover_text("Start Ivar path tracer render")
                .clicked()
            {
                events.push(NodeGraphEvent::StartRender { spp: *spp });
                *is_rendering = true;
            }
            ui.separator();
            if *is_converting_tx {
                ui.colored_label(theme::STATUS_WARNING, "Converting .tx...");
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .button("Convert to .tx")
                        .on_hover_text(
                            "Convert scene textures to tiled .tx format for faster rendering",
                        )
                        .clicked()
                    {
                        events.push(NodeGraphEvent::ConvertTexturesToTx);
                        *is_converting_tx = true;
                        *tx_status = None;
                    }
                    if ui
                        .button("Clear .tx")
                        .on_hover_text("Delete all cached .tx texture files")
                        .clicked()
                    {
                        events.push(NodeGraphEvent::ClearTxCache);
                    }
                });
            }
            if let Some(status) = tx_status {
                ui.colored_label(theme::STATUS_OK, status.as_str());
            }
        }
        SceneNode::Primitive {
            kind: _,
            size,
            is_created,
            prim_path,
        } => {
            ui.heading("Primitive");
            ui.horizontal(|ui| {
                ui.label("Size:");
                if ui
                    .add(egui::DragValue::new(size).speed(0.01).range(0.01..=100.0))
                    .changed()
                {
                    *is_created = false;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(prim_path);
            });
        }
        SceneNode::ScatterPoints {
            source,
            count,
            max_point_limit,
            seed,
            scatter_mode,
            min_distance,
            align_to_normal,
            grid_size,
            grid_spacing,
            sphere_radius,
            sphere_on_surface,
            relax_iterations,
            scale_radii,
            max_relax_radius,
            scale_min,
            scale_max,
            rotation_range,
            target_proto_id: _,
            is_computed,
            point_size,
            point_color,
        } => {
            ui.heading("Scatter Points");

            // Source dropdown
            ui.horizontal(|ui| {
                ui.label("Source:");
                egui::ComboBox::from_id_salt("scatter_source_inspector")
                    .selected_text(match source {
                        bif_core::PointSource::Surface => "Surface",
                        bif_core::PointSource::Grid => "Grid",
                        bif_core::PointSource::Sphere => "Sphere",
                    })
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(source, bif_core::PointSource::Surface, "Surface")
                            .changed()
                        {
                            *is_computed = false;
                        }
                        if ui
                            .selectable_value(source, bif_core::PointSource::Grid, "Grid")
                            .changed()
                        {
                            *is_computed = false;
                        }
                        if ui
                            .selectable_value(source, bif_core::PointSource::Sphere, "Sphere")
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
            });

            // Source-specific params
            match source {
                bif_core::PointSource::Surface => {
                    ui.horizontal(|ui| {
                        ui.label("Count:");
                        if ui
                            .add(egui::DragValue::new(count).range(1..=100_000))
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        let mut is_poisson =
                            *scatter_mode == bif_core::scatter::ScatterMode::PoissonDisk;
                        if ui
                            .checkbox(&mut is_poisson, "Poisson Disk")
                            .on_hover_text(
                                "Poisson Disk: even distribution with minimum spacing; unchecked = random",
                            )
                            .changed()
                        {
                            *scatter_mode = if is_poisson {
                                bif_core::scatter::ScatterMode::PoissonDisk
                            } else {
                                bif_core::scatter::ScatterMode::Random
                            };
                            *is_computed = false;
                        }
                    });

                    if *scatter_mode == bif_core::scatter::ScatterMode::PoissonDisk {
                        ui.horizontal(|ui| {
                            ui.label("Min Dist:");
                            if ui
                                .add(
                                    egui::DragValue::new(min_distance)
                                        .speed(0.01)
                                        .range(0.01..=100.0),
                                )
                                .changed()
                            {
                                *is_computed = false;
                            }
                        });
                    }

                    ui.horizontal(|ui| {
                        ui.label("Seed:");
                        let mut seed_val = *seed as i64;
                        if ui
                            .add(egui::DragValue::new(&mut seed_val).range(0..=999_999))
                            .changed()
                        {
                            *seed = seed_val as u64;
                            *is_computed = false;
                        }
                    });

                    if ui
                        .checkbox(align_to_normal, "Align to Normal")
                        .on_hover_text(
                            "Orient instances to the surface normal at their scatter point",
                        )
                        .changed()
                    {
                        *is_computed = false;
                    }
                }
                bif_core::PointSource::Grid => {
                    ui.horizontal(|ui| {
                        ui.label("Size X:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut grid_size[0])
                                    .speed(0.1)
                                    .range(0.0..=1000.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Size Y:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut grid_size[1])
                                    .speed(0.1)
                                    .range(0.0..=1000.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Size Z:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut grid_size[2])
                                    .speed(0.1)
                                    .range(0.0..=1000.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Spacing:");
                        if ui
                            .add(
                                egui::DragValue::new(grid_spacing)
                                    .speed(0.01)
                                    .range(0.01..=100.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                }
                bif_core::PointSource::Sphere => {
                    ui.horizontal(|ui| {
                        ui.label("Count:");
                        if ui
                            .add(egui::DragValue::new(count).range(1..=100_000))
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Radius:");
                        if ui
                            .add(
                                egui::DragValue::new(sphere_radius)
                                    .speed(0.1)
                                    .range(0.01..=1000.0),
                            )
                            .changed()
                        {
                            *is_computed = false;
                        }
                    });
                    if ui
                        .checkbox(sphere_on_surface, "Surface Only")
                        .on_hover_text("Scatter only on the sphere surface, not inside the volume")
                        .changed()
                    {
                        *is_computed = false;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Seed:");
                        let mut seed_val = *seed as i64;
                        if ui
                            .add(egui::DragValue::new(&mut seed_val).range(0..=999_999))
                            .changed()
                        {
                            *seed = seed_val as u64;
                            *is_computed = false;
                        }
                    });
                }
            }

            // Common: max point limit
            ui.horizontal(|ui| {
                ui.label("Max Pts:");
                if ui
                    .add(egui::DragValue::new(max_point_limit).range(1..=1_000_000))
                    .on_hover_text(
                        "Hard cap on scatter points regardless of count/density settings",
                    )
                    .changed()
                {
                    *is_computed = false;
                }
            });

            // Relax section
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Relax Iters:");
                if ui
                    .add(egui::DragValue::new(relax_iterations).range(0..=100))
                    .on_hover_text("Lloyd relaxation passes to improve point distribution evenness")
                    .changed()
                {
                    *is_computed = false;
                }
            });
            if *relax_iterations > 0 {
                ui.horizontal(|ui| {
                    ui.label("Scale Radii:");
                    if ui
                        .add(
                            egui::DragValue::new(scale_radii)
                                .speed(0.01)
                                .range(0.01..=10.0),
                        )
                        .changed()
                    {
                        *is_computed = false;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Max Radius:");
                    if ui
                        .add(
                            egui::DragValue::new(max_relax_radius)
                                .speed(0.01)
                                .range(0.01..=100.0),
                        )
                        .changed()
                    {
                        *is_computed = false;
                    }
                });
            }

            // Per-point attributes
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Scale:");
                let changed_min = ui
                    .add(
                        egui::DragValue::new(scale_min)
                            .speed(0.01)
                            .range(0.01..=10.0),
                    )
                    .changed();
                ui.label("-");
                let changed_max = ui
                    .add(
                        egui::DragValue::new(scale_max)
                            .speed(0.01)
                            .range(0.01..=10.0),
                    )
                    .changed();
                if changed_min || changed_max {
                    *is_computed = false;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Rotation:");
                if ui
                    .add(
                        egui::DragValue::new(rotation_range)
                            .speed(1.0)
                            .suffix("deg")
                            .range(0.0..=360.0),
                    )
                    .changed()
                {
                    *is_computed = false;
                }
            });

            // Status display
            if *is_computed {
                ui.colored_label(theme::STATUS_OK, "Computed");
            }

            // Point preview controls (cosmetic only, no recompute)
            ui.separator();
            let mut preview_changed = false;
            ui.horizontal(|ui| {
                ui.label("Pt Size:");
                if ui
                    .add(
                        egui::DragValue::new(point_size)
                            .speed(0.5)
                            .range(1.0..=20.0),
                    )
                    .changed()
                {
                    preview_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Color:");
                if ui
                    .color_edit_button_rgba_unmultiplied(point_color)
                    .changed()
                {
                    preview_changed = true;
                }
            });
            if preview_changed {
                events.push(NodeGraphEvent::PointPreviewUpdate {
                    node_id,
                    point_size: *point_size,
                    point_color: *point_color,
                });
            }
        }
        SceneNode::PointInstancer {
            prim_path,
            instance_count,
            is_instanced,
            ..
        } => {
            ui.heading("Point Instancer");
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(prim_path);
            });
            if *is_instanced {
                ui.colored_label(theme::STATUS_OK, format!("{} instances", instance_count));
            }
        }
        SceneNode::UsdExport {
            output_path,
            as_sublayer,
            export_root,
            is_exported,
            last_result,
        } => {
            ui.heading("USD Export");
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(output_path);
            });
            ui.horizontal(|ui| {
                if ui.button("Browse...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("USD Files", &["usda", "usdc"])
                        .set_file_name("export.usda")
                        .save_file()
                    {
                        *output_path = path.display().to_string();
                        *is_exported = false;
                        *last_result = None;
                    }
                }
            });
            ui.checkbox(as_sublayer, "As Sublayer")
                .on_hover_text("Export as a sublayer reference instead of a flattened stage");
            ui.horizontal(|ui| {
                ui.label("Root:");
                ui.text_edit_singleline(export_root);
            });
            if !output_path.is_empty() && ui.button("Export").clicked() {
                events.push(NodeGraphEvent::ExportUsd {
                    node_id,
                    output_path: output_path.clone(),
                    as_sublayer: *as_sublayer,
                    export_root: export_root.clone(),
                });
            }
            if *is_exported {
                if let Some(ref result) = last_result {
                    ui.colored_label(theme::STATUS_OK, result.as_str());
                }
            } else if let Some(ref result) = last_result {
                ui.colored_label(theme::STATUS_ERROR, result.as_str());
            }
        }
        SceneNode::Xform {
            translate,
            rotate,
            scale,
            prim_filter,
        } => {
            let changed = render_xform_properties(ui, translate, rotate, scale, prim_filter);
            if changed {
                events.push(NodeGraphEvent::XformChanged { node_id });
            }
        }
        SceneNode::UsdPrim {
            prim_path,
            prim_type,
            kind,
            specifier,
        } => {
            ui.heading("USD Prim");
            ui.horizontal(|ui| {
                ui.label("Path:");
                ui.text_edit_singleline(prim_path);
            });
            ui.horizontal(|ui| {
                ui.label("Type:");
                egui::ComboBox::from_id_salt("prim_type_inspector")
                    .selected_text(format!("{}", prim_type))
                    .show_ui(ui, |ui| {
                        for t in bif_core::usd::UsdPrimType::ALL {
                            ui.selectable_value(prim_type, t, format!("{}", t));
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Kind:");
                egui::ComboBox::from_id_salt("prim_kind_inspector")
                    .selected_text(format!("{}", kind))
                    .show_ui(ui, |ui| {
                        for k in bif_core::usd::UsdKind::ALL {
                            ui.selectable_value(kind, k, format!("{}", k));
                        }
                    })
                    .response
                    .on_hover_text(
                        "USD prim kind metadata (component, assembly, group, subcomponent)",
                    );
            });
            ui.horizontal(|ui| {
                ui.label("Spec:");
                egui::ComboBox::from_id_salt("prim_spec_inspector")
                    .selected_text(format!("{}", specifier))
                    .show_ui(ui, |ui| {
                        for s in bif_core::usd::UsdSpecifier::ALL {
                            ui.selectable_value(specifier, s, format!("{}", s));
                        }
                    })
                    .response
                    .on_hover_text("USD prim specifier (def = concrete prim, over = opinion, class = abstract)");
            });
            ui.colored_label(theme::PIN_SCENE, prim_path.as_str());
        }
        SceneNode::GraftBranches { destination_path } => {
            ui.heading("Graft Branches");
            ui.horizontal(|ui| {
                ui.label("Dest:");
                ui.text_edit_singleline(destination_path);
            });
            ui.colored_label(theme::PIN_SCENE, destination_path.as_str());
        }
        SceneNode::HdriEnvironment {
            file_path,
            is_loaded,
            is_loading,
            rotation,
            intensity,
            show_background,
            error,
            last_load_secs,
            last_compute_secs,
        } => {
            ui.heading("HDRI Environment");
            ui.horizontal(|ui| {
                ui.label("File:");
                ui.add_enabled_ui(!*is_loading, |ui| {
                    if ui.text_edit_singleline(file_path).changed() {
                        *is_loaded = false;
                        *error = None;
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!*is_loading, |ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("HDR Files", &["hdr", "exr"])
                            .add_filter("All Files", &["*"])
                            .pick_file()
                        {
                            *file_path = path.display().to_string();
                            *is_loaded = false;
                            *error = None;
                            events.push(NodeGraphEvent::LoadHdri {
                                path: file_path.clone(),
                                rotation: *rotation,
                                intensity: *intensity,
                                show_background: *show_background,
                            });
                        }
                    }
                    if ui.button("Load").clicked() && !file_path.is_empty() {
                        events.push(NodeGraphEvent::LoadHdri {
                            path: file_path.clone(),
                            rotation: *rotation,
                            intensity: *intensity,
                            show_background: *show_background,
                        });
                    }
                });
            });

            let mut params_changed = false;

            ui.horizontal(|ui| {
                ui.label("Rotation:");
                if ui
                    .add(egui::DragValue::new(rotation).speed(1.0).suffix("deg"))
                    .changed()
                {
                    params_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Intensity:");
                if ui
                    .add(
                        egui::DragValue::new(intensity)
                            .speed(0.01)
                            .range(0.0..=10.0),
                    )
                    .changed()
                {
                    params_changed = true;
                }
            });

            if let Some(load_secs) = last_load_secs {
                ui.label(format!("Load: {:.2}s", load_secs));
            }
            if let Some(compute_secs) = last_compute_secs {
                ui.label(format!("IBL: {:.2}s", compute_secs));
            }

            if ui
                .checkbox(show_background, "Show Background")
                .on_hover_text("Show HDRI image as the viewport background")
                .changed()
            {
                params_changed = true;
            }

            if params_changed && *is_loaded {
                events.push(NodeGraphEvent::UpdateHdriParams {
                    rotation: *rotation,
                    intensity: *intensity,
                    show_background: *show_background,
                });
            }

            if *is_loading {
                ui.colored_label(theme::STATUS_WARNING, "Generating IBL...");
            } else if *is_loaded {
                ui.colored_label(theme::STATUS_OK, "Loaded");
            } else if let Some(err) = error {
                ui.colored_label(theme::STATUS_ERROR, format!("Error: {}", err));
            }
        }
        SceneNode::Cache {
            bypassed,
            label,
            is_cached,
            cache_key,
        } => {
            ui.heading("Cache");
            ui.horizontal(|ui| {
                ui.label("Label:");
                ui.text_edit_singleline(label);
            });
            if ui.checkbox(bypassed, "Bypass").changed() {
                events.push(NodeGraphEvent::CacheToggleBypass { node_id });
            }
            if ui.button("Clear Cache").clicked() {
                *is_cached = false;
                *cache_key = None;
                events.push(NodeGraphEvent::CacheClear { node_id });
            }
            ui.separator();
            if *bypassed {
                ui.colored_label(theme::TEXT_SECONDARY, "Bypassed — pass-through");
            } else if *is_cached {
                ui.colored_label(theme::STATUS_OK, "Cached");
                if let Some(key) = cache_key {
                    ui.colored_label(theme::TEXT_SECONDARY, format!("Key: {:016x}", key));
                }
            } else {
                ui.colored_label(theme::STATUS_WARNING, "Stale — needs evaluation");
            }
        }
    }

    events
}

/// Reset cached property inspector state (call when selection changes).
pub fn reset_property_inspector_cache(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.remove::<[f32; 9]>(egui::Id::new("transform_edit_values"));
        d.remove::<bif_core::Transform>(egui::Id::new("transform_drag_start"));
        d.remove::<TransformEdit>(egui::Id::new("transform_edit_event"));
        d.remove::<Option<usize>>(egui::Id::new("prop_selected_row"));
        d.remove::<PropertyTab>(egui::Id::new("property_detail_tab"));
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

    #[test]
    fn test_build_property_rows_minimal() {
        let props = PrimProperties::default();
        let rows = build_property_rows(&props);
        // Should have unresolved material + Children attribute
        assert!(rows
            .iter()
            .any(|r| r.name == "Resolved Material" && r.status == PropertyStatus::Unresolved));
    }

    #[test]
    fn test_build_property_rows_with_material() {
        let mat = bif_core::Material::default();
        let props = PrimProperties::default().with_material(Arc::new(mat));
        let rows = build_property_rows(&props);
        // Material + base color + metalness + roughness + specular weight + specular IOR
        // + transmission + opacity + double-sided + Children attribute = 10+
        assert!(rows.len() >= 10);
        assert!(rows
            .iter()
            .any(|r| r.name == "Resolved Material" && r.status == PropertyStatus::Resolved));
        assert!(rows.iter().any(|r| r.name == "Base Color"));
        assert!(rows.iter().any(|r| r.name == "Roughness"));
    }

    #[test]
    fn test_build_property_rows_with_bounds() {
        let props = PrimProperties::default().with_bounds(
            bif_math::Vec3::new(-1.0, -2.0, -3.0),
            bif_math::Vec3::new(1.0, 2.0, 3.0),
        );
        let rows = build_property_rows(&props);
        assert!(rows.iter().any(|r| r.name == "World Bounding Box"));
    }

    #[test]
    fn test_property_tab_default() {
        assert_eq!(PropertyTab::default(), PropertyTab::Value);
    }
}
