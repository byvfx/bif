use glam::{Mat4, Vec3};

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
    pub yaw: f32,    // Rotation around Y axis (radians)
    pub pitch: f32,  // Rotation around X axis (radians)
    pub distance: f32, // Distance from target
    pub move_speed: f32, // Movement speed for keyboard
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
        }
    }
    
    /// Get the view matrix (world → camera space)
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }
    
    /// Get the projection matrix (camera → clip space)
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far)
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
        let movement = right_dir * right * speed
            + up_dir * up * speed
            + view_dir * forward * speed;
        
        self.position += movement;
        self.target += movement;
    }
    
    /// Dolly camera (move toward/away from target)
    pub fn dolly(&mut self, delta: f32) {
        self.distance = (self.distance + delta).max(0.1);
        self.update_position_from_angles();
    }
    
    /// Update camera position from spherical coordinates
    pub fn update_position_from_angles(&mut self) {
        let x = self.distance * self.pitch.cos() * self.yaw.cos();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.sin();
        
        self.position = self.target + Vec3::new(x, y, z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_creation() {
        let camera = Camera::new(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            16.0 / 9.0,
        );
        
        assert_eq!(camera.position, Vec3::new(0.0, 0.0, 5.0));
        assert_eq!(camera.target, Vec3::ZERO);
        assert_eq!(camera.aspect, 16.0 / 9.0);
    }
    
    #[test]
    fn test_view_matrix() {
        let camera = Camera::new(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            1.0,
        );
        
        let view = camera.view_matrix();
        // View matrix should translate camera to origin
        assert!(view.w_axis.z < 0.0); // Camera moved back
    }
    
    #[test]
    fn test_projection_matrix() {
        let camera = Camera::new(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, -1.0),
            16.0 / 9.0,
        );
        
        let proj = camera.projection_matrix();
        // Projection matrix should have aspect ratio encoded
        assert!(proj.x_axis.x != 0.0);
        assert!(proj.y_axis.y != 0.0);
    }
    
    #[test]
    fn test_aspect_update() {
        let mut camera = Camera::new(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, -1.0),
            1.0,
        );
        
        camera.set_aspect(16.0 / 9.0);
        assert_eq!(camera.aspect, 16.0 / 9.0);
    }
}
