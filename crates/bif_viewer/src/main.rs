use anyhow::Result;
use bif_viewport::Renderer;
use winit::{
    application::ApplicationHandler,
    event::{WindowEvent, MouseButton, ElementState, KeyEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
    keyboard::{PhysicalKey, KeyCode},
};
use std::time::Instant;

/// CLI options
#[derive(Default)]
struct CliOptions {
    usda_path: Option<String>,
}

fn parse_args() -> CliOptions {
    let args: Vec<String> = std::env::args().collect();
    let mut opts = CliOptions::default();
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--usda" | "-u" => {
                if i + 1 < args.len() {
                    opts.usda_path = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            arg if !arg.starts_with('-') && opts.usda_path.is_none() => {
                // Positional argument - treat as usda path if it ends with .usda
                if arg.ends_with(".usda") || arg.ends_with(".usd") {
                    opts.usda_path = Some(arg.to_string());
                }
            }
            "--help" | "-h" => {
                println!("BIF Viewer - VFX Renderer");
                println!();
                println!("Usage: bif_viewer [OPTIONS] [FILE]");
                println!();
                println!("Options:");
                println!("  --usda, -u <FILE>  Load a USDA scene file");
                println!("  --help, -h         Show this help message");
                println!();
                println!("Controls:");
                println!("  Left Mouse Drag    Orbit camera");
                println!("  Middle Mouse Drag  Pan camera");
                println!("  Scroll Wheel       Zoom in/out");
                println!("  WASD               Move camera");
                println!("  Tab                Toggle UI");
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }
    
    opts
}

/// Application state
struct App {
    window: Option<std::sync::Arc<Window>>,
    renderer: Option<Renderer>,
    usda_path: Option<String>,
    
    // Input state
    left_mouse_pressed: bool,
    middle_mouse_pressed: bool,
    last_mouse_pos: Option<(f64, f64)>,
    keys_pressed: std::collections::HashSet<KeyCode>,
    last_frame_time: Instant,
}

impl App {
    fn new(usda_path: Option<String>) -> Self {
        Self {
            window: None,
            renderer: None,
            usda_path,
            left_mouse_pressed: false,
            middle_mouse_pressed: false,
            last_mouse_pos: None,
            keys_pressed: std::collections::HashSet::new(),
            last_frame_time: Instant::now(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attrs = Window::default_attributes()
                .with_title("BIF Viewer")
                .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720));
            
            let window = std::sync::Arc::new(
                event_loop
                    .create_window(window_attrs)
                    .expect("Failed to create window"),
            );
            
            // Initialize renderer based on CLI args
            let renderer = if let Some(ref usda_path) = self.usda_path {
                log::info!("Loading USDA scene: {}", usda_path);
                match bif_core::load_usda(usda_path) {
                    Ok(scene) => {
                        log::info!("Scene loaded: {} prototypes, {} instances", 
                                   scene.prototype_count(), scene.instance_count());
                        pollster::block_on(Renderer::new_with_scene(window.clone(), &scene))
                            .expect("Failed to initialize renderer with scene")
                    }
                    Err(e) => {
                        log::error!("Failed to load USDA file: {}", e);
                        log::info!("Falling back to default OBJ loader");
                        pollster::block_on(Renderer::new(window.clone()))
                            .expect("Failed to initialize renderer")
                    }
                }
            } else {
                pollster::block_on(Renderer::new(window.clone()))
                    .expect("Failed to initialize renderer")
            };
            
            self.window = Some(window);
            self.renderer = Some(renderer);
            
            log::info!("Window and renderer initialized");
        }
    }
    
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Let egui handle the event first
        if let Some(renderer) = &mut self.renderer {
            if let Some(window) = &self.window {
                if renderer.handle_egui_event(window, &event) {
                    // Event was consumed by egui, don't process it further
                    return;
                }
            }
        }
        
        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested");
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize((physical_size.width, physical_size.height));
                    log::info!("Resized to {}x{}", physical_size.width, physical_size.height);
                }
            }
            WindowEvent::MouseInput { button, state, .. } => {
                match button {
                    MouseButton::Left => {
                        self.left_mouse_pressed = state == ElementState::Pressed;
                        if !self.left_mouse_pressed {
                            self.last_mouse_pos = None;
                        }
                    }
                    MouseButton::Middle => {
                        self.middle_mouse_pressed = state == ElementState::Pressed;
                        if !self.middle_mouse_pressed {
                            self.last_mouse_pos = None;
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.left_mouse_pressed || self.middle_mouse_pressed {
                    if let Some(last_pos) = self.last_mouse_pos {
                        let delta_x = position.x - last_pos.0;
                        let delta_y = position.y - last_pos.1;
                        
                        if let Some(renderer) = &mut self.renderer {
                            if self.left_mouse_pressed {
                                // Orbit camera with left mouse
                                let sensitivity = 0.005;
                                renderer.camera.orbit(
                                    -delta_x as f32 * sensitivity,
                                    -delta_y as f32 * sensitivity,
                                );
                            } else if self.middle_mouse_pressed {
                                // Pan camera with middle mouse (scaled with distance)
                                let sensitivity = 0.1;
                                let distance_scale = renderer.camera.distance * 0.0001;
                                renderer.camera.pan(
                                    -delta_x as f32 * sensitivity * distance_scale,
                                    delta_y as f32 * sensitivity * distance_scale,
                                    0.0,
                                    1.0,  // delta_time = 1.0 for mouse pan (direct control)
                                );
                            }
                            renderer.update_camera();
                        }
                    }
                    self.last_mouse_pos = Some((position.x, position.y));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(renderer) = &mut self.renderer {
                    // Handle mouse wheel for dolly (zoom in/out)
                    let scroll_amount = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y * 100.0,
                        winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                    };
                    
                    renderer.camera.dolly(-scroll_amount);
                    renderer.update_camera();
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, .. }, .. } => {
                if let PhysicalKey::Code(keycode) = physical_key {
                    match state {
                        ElementState::Pressed => {
                            self.keys_pressed.insert(keycode);
                            
                            // Handle single-press keys
                            if keycode == KeyCode::KeyF {
                                // Frame mesh
                                if let Some(renderer) = &mut self.renderer {
                                    renderer.frame_mesh();
                                }
                            }
                        }
                        ElementState::Released => {
                            self.keys_pressed.remove(&keycode);
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Calculate delta time
                let now = Instant::now();
                let delta_time = (now - self.last_frame_time).as_secs_f32();
                self.last_frame_time = now;
                
                // Update FPS counter
                if let Some(renderer) = &mut self.renderer {
                    renderer.update_fps(delta_time);
                }
                
                // Handle keyboard movement
                if let Some(renderer) = &mut self.renderer {
                    let mut right = 0.0;
                    let mut up = 0.0;
                    let mut forward = 0.0;
                    
                    if self.keys_pressed.contains(&KeyCode::KeyW) {
                        forward += 1.0;
                    }
                    if self.keys_pressed.contains(&KeyCode::KeyS) {
                        forward -= 1.0;
                    }
                    if self.keys_pressed.contains(&KeyCode::KeyA) {
                        right -= 1.0;
                    }
                    if self.keys_pressed.contains(&KeyCode::KeyD) {
                        right += 1.0;
                    }
                    if self.keys_pressed.contains(&KeyCode::KeyE) {
                        up += 1.0;
                    }
                    if self.keys_pressed.contains(&KeyCode::KeyQ) {
                        up -= 1.0;
                    }
                    
                    if right != 0.0 || up != 0.0 || forward != 0.0 {
                        renderer.camera.pan(right, up, forward, delta_time);
                        renderer.update_camera();
                    }
                }
                
                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    // Clear to dark blue
                    let clear_color = wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    };
                    
                    if let Err(e) = renderer.render(clear_color, window) {
                        // Check if it's a surface error we can handle
                        if let Some(surface_err) = e.downcast_ref::<wgpu::SurfaceError>() {
                            match surface_err {
                                wgpu::SurfaceError::Lost => {
                                    // Surface lost, reconfigure
                                    if let Some(renderer) = &mut self.renderer {
                                        renderer.resize(renderer.size);
                                    }
                                }
                                wgpu::SurfaceError::OutOfMemory => {
                                    log::error!("Out of memory!");
                                    event_loop.exit();
                                }
                                _ => {
                                    log::error!("Surface error: {:?}", surface_err);
                                }
                            }
                        } else {
                            log::error!("Render error: {:?}", e);
                        }
                    }
                }
                
                // Request next frame
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
    
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Request continuous redraw when keys are pressed for smooth movement
        if !self.keys_pressed.is_empty() {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    
    let opts = parse_args();
    
    if let Some(ref path) = opts.usda_path {
        log::info!("Starting BIF Viewer with USDA: {}", path);
    } else {
        log::info!("Starting BIF Viewer (default scene)");
    }
    
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    
    let mut app = App::new(opts.usda_path);
    
    log::info!("Running event loop");
    event_loop.run_app(&mut app)?;
    
    Ok(())
}
