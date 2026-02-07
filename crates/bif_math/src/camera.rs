use glam::{Mat4, Vec3};

/// Projection mode: perspective or orthographic.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ProjectionMode {
    #[default]
    Perspective,
    Orthographic {
        /// Half-height of the orthographic view volume.
        ortho_size: f32,
    },
}

/// Standard orthographic view presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrthoPreset {
    Top,
    Bottom,
    Front,
    Back,
    Right,
    Left,
}

impl OrthoPreset {
    /// Display name for UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            OrthoPreset::Top => "Top",
            OrthoPreset::Bottom => "Bottom",
            OrthoPreset::Front => "Front",
            OrthoPreset::Back => "Back",
            OrthoPreset::Right => "Right",
            OrthoPreset::Left => "Left",
        }
    }

    /// All presets for iteration.
    pub fn all() -> &'static [OrthoPreset] {
        &[
            OrthoPreset::Top,
            OrthoPreset::Bottom,
            OrthoPreset::Front,
            OrthoPreset::Back,
            OrthoPreset::Right,
            OrthoPreset::Left,
        ]
    }

    /// Camera direction vector (where the camera looks from, relative to target).
    pub fn direction(&self) -> Vec3 {
        match self {
            OrthoPreset::Top => Vec3::Y,
            OrthoPreset::Bottom => Vec3::NEG_Y,
            OrthoPreset::Front => Vec3::Z,
            OrthoPreset::Back => Vec3::NEG_Z,
            OrthoPreset::Right => Vec3::X,
            OrthoPreset::Left => Vec3::NEG_X,
        }
    }

    /// Up vector for this view.
    pub fn up(&self) -> Vec3 {
        match self {
            OrthoPreset::Top => Vec3::NEG_Z,
            OrthoPreset::Bottom => Vec3::Z,
            _ => Vec3::Y,
        }
    }
}

/// Camera for 3D rendering with orbit controls
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,

    // Orbit controls
    pub yaw: f32,        // Rotation around Y axis (radians)
    pub pitch: f32,      // Rotation around X axis (radians)
    pub distance: f32,   // Distance from target
    pub move_speed: f32, // Movement speed for keyboard

    /// Projection mode (perspective or orthographic).
    pub projection: ProjectionMode,
}

impl Camera {
    /// Create a new camera
    pub fn new(position: Vec3, target: Vec3, aspect: f32) -> Self {
        let distance = (position - target).length();
        let direction = (position - target).normalize();

        // Calculate initial yaw and pitch from position
        let yaw = direction.z.atan2(direction.x);
        let pitch = direction.y.asin();

        Self {
            position,
            target,
            up: Vec3::Y,
            fov_y: 45.0_f32.to_radians(),
            aspect,
            near: 0.1,
            far: 100.0,
            yaw,
            pitch,
            distance,
            move_speed: 2.0,
            projection: ProjectionMode::Perspective,
        }
    }

    /// Get the view matrix (world → camera space)
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    /// Get the projection matrix (camera → clip space)
    pub fn projection_matrix(&self) -> Mat4 {
        match self.projection {
            ProjectionMode::Perspective => {
                Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far)
            }
            ProjectionMode::Orthographic { ortho_size } => {
                let half_h = ortho_size;
                let half_w = half_h * self.aspect;
                Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, self.near, self.far)
            }
        }
    }

    /// Get the combined view-projection matrix
    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Update aspect ratio (e.g., on window resize)
    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    /// Orbit camera by delta angles (in radians)
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch += delta_pitch;

        // Clamp pitch to avoid gimbal lock
        const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);

        // Update position from angles
        self.update_position_from_angles();
    }

    /// Move camera and target together (in view space)
    pub fn pan(&mut self, right: f32, up: f32, forward: f32, delta_time: f32) {
        // Scale speed with distance for consistent movement feel at any zoom level
        let speed = self.move_speed * self.distance * delta_time;

        // Get camera axes
        let view_dir = (self.target - self.position).normalize();
        let right_dir = view_dir.cross(self.up).normalize();
        let up_dir = right_dir.cross(view_dir).normalize();

        // Move camera and target together
        let movement = right_dir * right * speed + up_dir * up * speed + view_dir * forward * speed;

        self.position += movement;
        self.target += movement;
    }

    /// Dolly camera (move toward/away from target).
    ///
    /// In orthographic mode, adjusts `ortho_size` instead of distance.
    pub fn dolly(&mut self, delta: f32) {
        if let ProjectionMode::Orthographic { ref mut ortho_size } = self.projection {
            *ortho_size = (*ortho_size + delta * 0.5).max(0.01);
        } else {
            self.distance = (self.distance + delta).max(0.1);
            self.update_position_from_angles();
        }
    }

    /// Check if camera is in orthographic mode.
    pub fn is_ortho(&self) -> bool {
        matches!(self.projection, ProjectionMode::Orthographic { .. })
    }

    /// Set camera from an orthographic preset, preserving the current target.
    pub fn set_ortho_preset(&mut self, preset: OrthoPreset) {
        let ortho_size = match self.projection {
            ProjectionMode::Orthographic { ortho_size } => ortho_size,
            ProjectionMode::Perspective => self.distance * (self.fov_y * 0.5).tan(),
        };
        self.projection = ProjectionMode::Orthographic { ortho_size };
        self.up = preset.up();
        self.position = self.target + preset.direction() * self.distance;

        // Recalculate yaw/pitch from new direction
        let direction = (self.position - self.target).normalize();
        self.yaw = direction.z.atan2(direction.x);
        self.pitch = direction.y.asin();
    }

    /// Switch back to perspective projection.
    pub fn set_perspective(&mut self) {
        if let ProjectionMode::Orthographic { ortho_size } = self.projection {
            // Recover distance from ortho_size
            let tan_half = (self.fov_y * 0.5).tan();
            if tan_half > 0.0 {
                self.distance = ortho_size / tan_half;
            }
        }
        self.projection = ProjectionMode::Perspective;
        self.up = Vec3::Y;
        self.update_position_from_angles();
    }

    /// Update camera position from spherical coordinates
    pub fn update_position_from_angles(&mut self) {
        let x = self.distance * self.pitch.cos() * self.yaw.cos();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.sin();

        self.position = self.target + Vec3::new(x, y, z);
    }

    /// Set camera from a world transform matrix (e.g., from USD camera animation).
    ///
    /// The matrix is treated as the camera's world transform, where:
    /// - Translation gives camera position
    /// - -Z axis gives view direction (camera looks down -Z in its local space)
    pub fn set_from_matrix(&mut self, matrix: Mat4) {
        // Extract position from translation column
        self.position = Vec3::new(matrix.w_axis.x, matrix.w_axis.y, matrix.w_axis.z);

        // Camera looks down -Z in its local space
        let forward = -Vec3::new(matrix.z_axis.x, matrix.z_axis.y, matrix.z_axis.z).normalize();
        let up = Vec3::new(matrix.y_axis.x, matrix.y_axis.y, matrix.y_axis.z).normalize();

        // Set target along view direction
        self.target = self.position + forward * self.distance;
        self.up = up;

        // Recalculate yaw/pitch from new direction
        let direction = (self.position - self.target).normalize();
        self.yaw = direction.z.atan2(direction.x);
        self.pitch = direction.y.asin();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_creation() {
        let camera = Camera::new(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 16.0 / 9.0);

        assert_eq!(camera.position, Vec3::new(0.0, 0.0, 5.0));
        assert_eq!(camera.target, Vec3::ZERO);
        assert_eq!(camera.aspect, 16.0 / 9.0);
    }

    #[test]
    fn test_view_matrix() {
        let camera = Camera::new(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, 1.0);

        let view = camera.view_matrix();
        // View matrix should translate camera to origin
        assert!(view.w_axis.z < 0.0); // Camera moved back
    }

    #[test]
    fn test_projection_matrix() {
        let camera = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 16.0 / 9.0);

        let proj = camera.projection_matrix();
        // Projection matrix should have aspect ratio encoded
        assert!(proj.x_axis.x != 0.0);
        assert!(proj.y_axis.y != 0.0);
    }

    #[test]
    fn test_aspect_update() {
        let mut camera = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 1.0);

        camera.set_aspect(16.0 / 9.0);
        assert_eq!(camera.aspect, 16.0 / 9.0);
    }
}
