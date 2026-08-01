//! Translate gizmo math shared by the old egui overlay and the Qt viewport.
//!
//! Projects 3D axis endpoints to screen space and draws colored lines/arrows.
//! Handles mouse hit testing and drag-to-translate along a single axis.

use crate::theme;
use bif_math::{Camera, Mat4, Vec3, Vec4};

/// Which axis the gizmo is interacting with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GizmoAxis {
    None = 0,
    X = 1,
    Y = 2,
    Z = 3,
}

impl GizmoAxis {
    /// Convert from u8 discriminant.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::X,
            2 => Self::Y,
            3 => Self::Z,
            _ => Self::None,
        }
    }
}

/// Gizmo interaction state.
#[derive(Debug, Clone, Copy)]
pub struct GizmoState {
    /// Currently hovered axis (for visual feedback).
    pub hovered_axis: GizmoAxis,
    /// Currently active (dragging) axis.
    pub active_axis: GizmoAxis,
    /// Whether a drag is in progress.
    pub is_dragging: bool,
    /// Screen position where drag started.
    pub drag_start_screen: (f32, f32),
    /// World position of the gizmo center at drag start.
    pub drag_start_world: Vec3,
    /// Accumulated drag delta in world units along the active axis.
    pub drag_world_delta: f32,
    /// Transform at drag start, used to commit one undoable edit on release.
    pub drag_start_transform: Option<bif_core::Transform>,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self::new()
    }
}

impl GizmoState {
    pub fn new() -> Self {
        Self {
            hovered_axis: GizmoAxis::None,
            active_axis: GizmoAxis::None,
            is_dragging: false,
            drag_start_screen: (0.0, 0.0),
            drag_start_world: Vec3::ZERO,
            drag_world_delta: 0.0,
            drag_start_transform: None,
        }
    }

    /// Reset gizmo state (e.g. on selection change).
    pub fn reset(&mut self) {
        self.hovered_axis = GizmoAxis::None;
        self.active_axis = GizmoAxis::None;
        self.is_dragging = false;
        self.drag_world_delta = 0.0;
        self.drag_start_transform = None;
    }
}

pub fn axis_direction(axis: GizmoAxis) -> Option<Vec3> {
    match axis {
        GizmoAxis::X => Some(Vec3::X),
        GizmoAxis::Y => Some(Vec3::Y),
        GizmoAxis::Z => Some(Vec3::Z),
        GizmoAxis::None => None,
    }
}

pub fn axis_name(axis: GizmoAxis) -> &'static str {
    match axis {
        GizmoAxis::X => "X",
        GizmoAxis::Y => "Y",
        GizmoAxis::Z => "Z",
        GizmoAxis::None => "",
    }
}

/// Project a world-space point to screen-space (pixels).
///
/// Returns `None` if the point is behind the camera.
pub(crate) fn project_to_screen(
    point: Vec3,
    view_proj: &Mat4,
    viewport_rect: (f32, f32, f32, f32), // (x, y, w, h) in pixels
) -> Option<(f32, f32)> {
    let clip = *view_proj * Vec4::new(point.x, point.y, point.z, 1.0);
    if clip.w <= 0.0 {
        return None; // Behind camera
    }
    let ndc_x = clip.x / clip.w;
    let ndc_y = clip.y / clip.w;

    // NDC [-1,1] -> viewport pixels
    let (vp_x, vp_y, vp_w, vp_h) = viewport_rect;
    let screen_x = vp_x + (ndc_x + 1.0) * 0.5 * vp_w;
    let screen_y = vp_y + (1.0 - ndc_y) * 0.5 * vp_h; // Y-down in screen

    Some((screen_x, screen_y))
}

/// Hit-test the screen-space translate axes without drawing them.
pub fn hit_test_axis(
    camera: &Camera,
    world_pos: Vec3,
    viewport_rect: (f32, f32, f32, f32),
    mouse_pos: (f32, f32),
) -> GizmoAxis {
    let vp = camera.view_projection_matrix();
    let Some(origin_screen) = project_to_screen(world_pos, &vp, viewport_rect) else {
        return GizmoAxis::None;
    };

    let cam_dist = (camera.position - world_pos).length();
    let axis_length = cam_dist * 0.14;
    let mut closest_axis = GizmoAxis::None;
    let mut closest_dist = f32::MAX;
    let hit_threshold = 8.0;

    for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
        let Some(axis_dir) = axis_direction(axis) else {
            continue;
        };
        let Some(end_screen) =
            project_to_screen(world_pos + axis_dir * axis_length, &vp, viewport_rect)
        else {
            continue;
        };
        let dist = point_to_segment_dist(
            mouse_pos.0,
            mouse_pos.1,
            origin_screen.0,
            origin_screen.1,
            end_screen.0,
            end_screen.1,
        );
        if dist < hit_threshold && dist < closest_dist {
            closest_dist = dist;
            closest_axis = axis;
        }
    }

    closest_axis
}

/// Distance from a point to a line segment in 2D.
fn point_to_segment_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-6 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let proj_x = ax + t * dx;
    let proj_y = ay + t * dy;
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

/// Draw the translate gizmo and return the hovered axis.
///
/// Call this inside the egui frame after panels but before tessellation.
pub fn draw_gizmo(
    painter: &egui::Painter,
    camera: &Camera,
    world_pos: Vec3,
    viewport_rect: (f32, f32, f32, f32),
    gizmo_state: &GizmoState,
    mouse_pos: Option<(f32, f32)>,
) -> GizmoAxis {
    let vp = camera.view_projection_matrix();

    // Axis length scales with camera distance for consistent screen size
    let cam_dist = (camera.position - world_pos).length();
    let axis_length = cam_dist * 0.14;

    let origin = world_pos;
    let x_end = origin + Vec3::X * axis_length;
    let y_end = origin + Vec3::Y * axis_length;
    let z_end = origin + Vec3::Z * axis_length;

    let Some(origin_screen) = project_to_screen(origin, &vp, viewport_rect) else {
        return GizmoAxis::None;
    };

    let axes = [
        (x_end, theme::AXIS_X, GizmoAxis::X),
        (y_end, theme::AXIS_Y, GizmoAxis::Y),
        (z_end, theme::AXIS_Z, GizmoAxis::Z),
    ];

    let mut closest_axis = GizmoAxis::None;
    let mut closest_dist = f32::MAX;
    let hit_threshold = 8.0; // pixels

    for (end_world, color, axis) in &axes {
        let Some(end_screen) = project_to_screen(*end_world, &vp, viewport_rect) else {
            continue;
        };

        // Determine line thickness
        let is_hovered = gizmo_state.hovered_axis == *axis;
        let is_active = gizmo_state.active_axis == *axis && gizmo_state.is_dragging;
        let thickness: f32 = if is_active {
            3.5
        } else if is_hovered {
            2.5
        } else {
            1.5
        };

        let draw_color = if is_active || is_hovered {
            egui::Color32::from_rgb(
                (color.r() as u16 + 60).min(255) as u8,
                (color.g() as u16 + 60).min(255) as u8,
                (color.b() as u16 + 60).min(255) as u8,
            )
        } else {
            *color
        };

        // Draw axis line
        painter.line_segment(
            [
                egui::pos2(origin_screen.0, origin_screen.1),
                egui::pos2(end_screen.0, end_screen.1),
            ],
            egui::Stroke::new(thickness, draw_color),
        );

        // Draw arrowhead (small filled triangle)
        let dir_x = end_screen.0 - origin_screen.0;
        let dir_y = end_screen.1 - origin_screen.1;
        let dir_len = (dir_x * dir_x + dir_y * dir_y).sqrt();
        if dir_len > 1.0 {
            let nx = dir_x / dir_len;
            let ny = dir_y / dir_len;
            let arrow_size = 8.0;
            let tip = egui::pos2(end_screen.0, end_screen.1);
            let left = egui::pos2(
                end_screen.0 - nx * arrow_size + ny * arrow_size * 0.4,
                end_screen.1 - ny * arrow_size - nx * arrow_size * 0.4,
            );
            let right = egui::pos2(
                end_screen.0 - nx * arrow_size - ny * arrow_size * 0.4,
                end_screen.1 - ny * arrow_size + nx * arrow_size * 0.4,
            );
            painter.add(egui::Shape::convex_polygon(
                vec![tip, left, right],
                draw_color,
                egui::Stroke::NONE,
            ));
        }

        // Hit test against mouse position
        if let Some((mx, my)) = mouse_pos {
            let dist = point_to_segment_dist(
                mx,
                my,
                origin_screen.0,
                origin_screen.1,
                end_screen.0,
                end_screen.1,
            );
            if dist < hit_threshold && dist < closest_dist {
                closest_dist = dist;
                closest_axis = *axis;
            }
        }
    }

    // Draw center dot
    painter.circle_filled(
        egui::pos2(origin_screen.0, origin_screen.1),
        3.0,
        theme::TEXT_PRIMARY,
    );

    closest_axis
}

/// Compute drag delta in world units along the given axis.
///
/// Projects the mouse movement onto the screen-space direction of the axis.
pub fn compute_drag_delta(
    mouse_pos: (f32, f32),
    drag_start_screen: (f32, f32),
    camera: &Camera,
    axis: GizmoAxis,
    world_pos: Vec3,
    viewport_rect: (f32, f32, f32, f32),
) -> f32 {
    let vp = camera.view_projection_matrix();

    let Some(axis_dir) = axis_direction(axis) else {
        return 0.0;
    };

    // Project origin and origin+axis to screen
    let cam_dist = (camera.position - world_pos).length();
    let axis_length = cam_dist * 0.14;

    let Some(origin_screen) = project_to_screen(world_pos, &vp, viewport_rect) else {
        return 0.0;
    };
    let Some(tip_screen) =
        project_to_screen(world_pos + axis_dir * axis_length, &vp, viewport_rect)
    else {
        return 0.0;
    };

    // Screen-space axis direction
    let screen_dir_x = tip_screen.0 - origin_screen.0;
    let screen_dir_y = tip_screen.1 - origin_screen.1;
    let screen_dir_len = (screen_dir_x * screen_dir_x + screen_dir_y * screen_dir_y).sqrt();
    if screen_dir_len < 1.0 {
        return 0.0;
    }

    let screen_nx = screen_dir_x / screen_dir_len;
    let screen_ny = screen_dir_y / screen_dir_len;

    // Project mouse delta onto screen axis direction
    let mouse_dx = mouse_pos.0 - drag_start_screen.0;
    let mouse_dy = mouse_pos.1 - drag_start_screen.1;
    let screen_proj = mouse_dx * screen_nx + mouse_dy * screen_ny;

    // Convert screen pixels to world units
    // screen_dir_len pixels = axis_length world units
    screen_proj * axis_length / screen_dir_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_to_segment_dist_on_segment() {
        let dist = point_to_segment_dist(5.0, 0.0, 0.0, 0.0, 10.0, 0.0);
        assert!(dist.abs() < 0.001);
    }

    #[test]
    fn test_point_to_segment_dist_perpendicular() {
        let dist = point_to_segment_dist(5.0, 3.0, 0.0, 0.0, 10.0, 0.0);
        assert!((dist - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_gizmo_axis_default() {
        let state = GizmoState::new();
        assert_eq!(state.hovered_axis, GizmoAxis::None);
        assert_eq!(state.active_axis, GizmoAxis::None);
        assert!(!state.is_dragging);
        assert!(state.drag_start_transform.is_none());
    }

    #[test]
    fn test_project_behind_camera() {
        // A point behind the camera should return None
        let vp = Mat4::perspective_rh(1.0, 1.0, 0.1, 100.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let result = project_to_screen(Vec3::new(0.0, 0.0, 10.0), &vp, (0.0, 0.0, 800.0, 600.0));
        assert!(result.is_none());
    }

    #[test]
    fn test_project_center_of_view() {
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let target = Vec3::ZERO;
        let vp = Mat4::perspective_rh(1.0, 800.0 / 600.0, 0.1, 100.0)
            * Mat4::look_at_rh(eye, target, Vec3::Y);
        let result = project_to_screen(Vec3::ZERO, &vp, (0.0, 0.0, 800.0, 600.0));
        assert!(result.is_some());
        let (sx, sy) = result.unwrap();
        // Should be near center of viewport
        assert!((sx - 400.0).abs() < 1.0);
        assert!((sy - 300.0).abs() < 1.0);
    }
}
