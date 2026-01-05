//! Property Inspector - USD prim property viewer.
//!
//! Shows properties and metadata for the currently selected USD prim.
//!
//! # Features
//! - Path and type display
//! - Transform matrix visualization
//! - Bounding box info (when available)
//! - Key USD attributes
//!
//! # TODOs
//! - [ ] Fetch actual USD attributes from C++ bridge
//! - [ ] Time-sampled attribute visualization
//! - [ ] Editable attributes (write-back to USD)

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
            attributes: vec![
                ("Children".to_string(), info.child_count.to_string()),
            ],
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
}

/// Render the property inspector panel.
pub fn render_property_inspector(ui: &mut egui::Ui, properties: Option<&PrimProperties>) {
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
                    ui.colored_label(egui::Color32::GREEN, "✓ Yes");
                } else {
                    ui.colored_label(egui::Color32::RED, "✗ No");
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
                    ui.label(format!(
                        "Min: ({:.3}, {:.3}, {:.3})",
                        min.x, min.y, min.z
                    ));
                    ui.label(format!(
                        "Max: ({:.3}, {:.3}, {:.3})",
                        max.x, max.y, max.z
                    ));

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
        }
    }
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
                for col in 0..4 {
                    // cols[col][row] because column-major
                    ui.label(
                        egui::RichText::new(format!("{:.3}", cols[col][row]))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_browser::PrimDisplayInfo;

    #[test]
    fn test_prim_properties_from_display_info() {
        let info = PrimDisplayInfo::new(
            "/World/Mesh".to_string(),
            "Mesh".to_string(),
            true,
            true,
            3,
        );

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
}
