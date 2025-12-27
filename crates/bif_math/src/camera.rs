use glam::{Mat4, Vec3};

/// Camera for 3D rendering
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    /// Create a new camera
    pub fn new(position: Vec3, target: Vec3, aspect: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            fov_y: 45.0_f32.to_radians(),
            aspect,
            near: 0.1,
            far: 100.0,
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
