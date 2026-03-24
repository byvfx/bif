use anyhow::Result;
use bif_viewport::Renderer;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

// Camera interaction constants
const ORBIT_SENSITIVITY: f32 = 0.005;
const PAN_SENSITIVITY: f32 = 0.1;
const PAN_DISTANCE_SCALE: f32 = 0.0001;
const SCROLL_DOLLY_SCALE: f32 = 0.1;
const SCROLL_PIXEL_TO_LINES: f32 = 50.0;
const CLICK_THRESHOLD: f64 = 3.0;

/// CLI options
#[derive(Debug, Default)]
struct CliOptions {
    usda_path: Option<String>,
    usd_path: Option<String>, // Uses C++ bridge (USDC, references)
}

/// Parse CLI arguments from a slice of strings (first element = program name).
///
/// Returns `Ok(CliOptions)` on success, `Err(message)` on parse errors.
/// Help requests (`--help`/`-h`) return `Err` with the help text.
fn parse_args_from(args: &[String]) -> Result<CliOptions, String> {
    let mut opts = CliOptions::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--usda" | "-u" => {
                if i + 1 < args.len() {
                    opts.usda_path = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    return Err(format!("Error: {} requires a file path", args[i]));
                }
            }
            "--usd" => {
                if i + 1 < args.len() {
                    opts.usd_path = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    return Err("Error: --usd requires a file path".to_string());
                }
            }
            arg if !arg.starts_with('-') && opts.usda_path.is_none() && opts.usd_path.is_none() => {
                // Positional argument - auto-detect based on extension
                if arg.ends_with(".usda") {
                    opts.usda_path = Some(arg.to_string());
                } else if arg.ends_with(".usd") || arg.ends_with(".usdc") {
                    opts.usd_path = Some(arg.to_string());
                }
            }
            "--help" | "-h" => {
                return Err("help".to_string());
            }
            unknown if unknown.starts_with('-') => {
                eprintln!("Warning: unknown flag '{}' (use --help for usage)", unknown);
            }
            _ => {}
        }
        i += 1;
    }

    Ok(opts)
}

fn parse_args() -> CliOptions {
    let args: Vec<String> = std::env::args().collect();
    match parse_args_from(&args) {
        Ok(opts) => opts,
        Err(msg) if msg == "help" => {
            println!("BIF Viewer - VFX Renderer");
            println!();
            println!("Usage: bif_viewer [OPTIONS] [FILE]");
            println!();
            println!("Options:");
            println!("  --usda, -u <FILE>  Load a USDA scene file (pure Rust parser)");
            println!(
                "  --usd <FILE>       Load USD/USDA/USDC file (C++ bridge, supports references)"
            );
            println!("  --help, -h         Show this help message");
            println!();
            println!("Note: --usd requires PXR_PLUGINPATH_NAME environment variable.");
            println!("      Run: . .\\setup_usd_env.ps1");
            println!();
            println!("Controls:");
            println!("  Left Mouse Drag    Orbit camera");
            println!("  Middle Mouse Drag  Pan camera");
            println!("  Scroll Wheel       Zoom in/out");
            println!("  WASD               Move camera");
            println!("  Tab                Toggle UI");
            std::process::exit(0);
        }
        Err(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
    }
}

/// Returns `true` if accumulated drag distance is below the click threshold.
///
/// Used to distinguish a click (selection) from a drag (orbit/pan).
fn is_click(drag_distance: f64) -> bool {
    drag_distance < CLICK_THRESHOLD
}

/// Application state
struct App {
    window: Option<std::sync::Arc<Window>>,
    renderer: Option<Renderer>,
    usda_path: Option<String>,
    usd_path: Option<String>, // C++ bridge path

    // Input state
    left_mouse_pressed: bool,
    middle_mouse_pressed: bool,
    right_mouse_pressed: bool,
    last_mouse_pos: Option<(f64, f64)>,
    keys_pressed: std::collections::HashSet<KeyCode>,
    last_frame_time: Instant,

    // Click detection (distinguish click from drag)
    current_mouse_pos: (f64, f64),
    mouse_press_pos: Option<(f64, f64)>,
    mouse_drag_distance: f64,

    // Redraw tracking — set when user interaction or state change needs a frame
    needs_redraw: bool,
}

impl App {
    fn new(usda_path: Option<String>, usd_path: Option<String>) -> Self {
        Self {
            window: None,
            renderer: None,
            usda_path,
            usd_path,
            left_mouse_pressed: false,
            middle_mouse_pressed: false,
            right_mouse_pressed: false,
            last_mouse_pos: None,
            keys_pressed: std::collections::HashSet::new(),
            last_frame_time: Instant::now(),
            current_mouse_pos: (0.0, 0.0),
            mouse_press_pos: None,
            mouse_drag_distance: 0.0,
            needs_redraw: true,
        }
    }

    /// Run a closure on the renderer if it exists.
    /// Only use for simple cases that don't need other App fields.
    fn with_renderer(&mut self, f: impl FnOnce(&mut Renderer)) {
        if let Some(renderer) = &mut self.renderer {
            f(renderer);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attrs = Window::default_attributes()
                .with_title("BIF Viewer")
                .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720));

            let window = match event_loop.create_window(window_attrs) {
                Ok(w) => std::sync::Arc::new(w),
                Err(e) => {
                    eprintln!("Failed to create window: {}", e);
                    std::process::exit(1);
                }
            };

            // Load scene based on CLI options
            let renderer = if let Some(usd_path) = &self.usd_path {
                // Use C++ bridge for --usd flag (supports USDC and references)
                log::info!("Loading USD scene via C++ bridge: {}", usd_path);
                let mut r = match pollster::block_on(Renderer::new(window.clone())) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to initialize renderer: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = r.load_usd_scene(usd_path) {
                    log::error!("Failed to load USD file '{}': {:?}", usd_path, e);
                    log::error!(
                        "Hint: Make sure PXR_PLUGINPATH_NAME is set. Run: . .\\setup_usd_env.ps1"
                    );
                    log::info!("Continuing with empty viewport");
                }
                r
            } else if let Some(usda_path) = self.usda_path.as_deref() {
                log::info!("Loading USDA scene: {}", usda_path);
                let mut r = match pollster::block_on(Renderer::new(window.clone())) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to initialize renderer: {}", e);
                        std::process::exit(1);
                    }
                };
                // Try C++ bridge first to get scene browser support
                if let Err(e) = r.load_usd_scene(usda_path) {
                    // Fall back to pure Rust parser (no scene browser)
                    log::warn!(
                        "C++ bridge failed ({}), using pure Rust parser (no scene browser)",
                        e
                    );
                    match bif_core::load_usda(usda_path) {
                        Ok(scene) => {
                            log::info!(
                                "Scene loaded: {} prototypes, {} instances",
                                scene.prototype_count(),
                                scene.instance_count()
                            );
                            if let Err(e) = r.load_scene_data(&scene) {
                                log::error!("Failed to load scene data: {}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to load USDA file '{}': {}", usda_path, e);
                            log::info!("Continuing with empty viewport");
                        }
                    }
                }
                r
            } else {
                // No file specified - start with blank scene
                log::info!("Starting with blank scene (load USD via node graph)");
                match pollster::block_on(Renderer::new(window.clone())) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Failed to initialize renderer: {}", e);
                        std::process::exit(1);
                    }
                }
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
        // Any window event means we need at least one redraw
        self.needs_redraw = true;

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
                self.with_renderer(|r| {
                    r.resize((physical_size.width, physical_size.height));
                    log::info!(
                        "Resized to {}x{}",
                        physical_size.width,
                        physical_size.height
                    );
                });
            }
            WindowEvent::MouseInput { button, state, .. } => match button {
                MouseButton::Left => {
                    if state == ElementState::Pressed {
                        self.left_mouse_pressed = true;
                        self.mouse_press_pos = Some(self.current_mouse_pos);
                        self.mouse_drag_distance = 0.0;

                        // Check if gizmo should start dragging
                        if let Some(renderer) = &mut self.renderer {
                            let hovered = renderer.selection.gizmo_state.hovered_axis;
                            if hovered != bif_viewport::gizmo::GizmoAxis::None {
                                if let Some(sel_idx) = renderer.selection.selected_instance_index {
                                    if let Some(transform) =
                                        renderer.get_instance_transform(sel_idx)
                                    {
                                        renderer.selection.gizmo_state.active_axis = hovered;
                                        renderer.selection.gizmo_state.is_dragging = true;
                                        renderer.selection.gizmo_state.drag_start_screen = (
                                            self.current_mouse_pos.0 as f32,
                                            self.current_mouse_pos.1 as f32,
                                        );
                                        renderer.selection.gizmo_state.drag_start_world =
                                            transform.translation;
                                        renderer.selection.gizmo_state.drag_world_delta = 0.0;
                                    }
                                }
                            }
                        }
                    } else {
                        self.left_mouse_pressed = false;

                        // Finalize gizmo drag
                        if let Some(renderer) = &mut self.renderer {
                            if renderer.selection.gizmo_state.is_dragging {
                                if let Some(sel_idx) = renderer.selection.selected_instance_index {
                                    if let Some(current) = renderer.get_instance_transform(sel_idx)
                                    {
                                        // Build old_transform from drag_start_world
                                        let mut old_transform = current;
                                        old_transform.translation =
                                            renderer.selection.gizmo_state.drag_start_world;

                                        renderer.push_transform_command(
                                            sel_idx,
                                            old_transform,
                                            current,
                                        );
                                    }
                                }
                                renderer.selection.gizmo_state.is_dragging = false;
                                renderer.selection.gizmo_state.active_axis =
                                    bif_viewport::gizmo::GizmoAxis::None;
                            } else if is_click(self.mouse_drag_distance) {
                                // Click detection: pick instance
                                if let Some(pos) = self.mouse_press_pos {
                                    let picked =
                                        renderer.pick_instance_at(pos.0 as f32, pos.1 as f32);
                                    renderer.selection.selected_instance_index = picked;
                                    renderer.selection.gizmo_state.reset();
                                }
                            }
                        }
                        self.last_mouse_pos = None;
                        self.mouse_press_pos = None;
                    }
                }
                MouseButton::Middle => {
                    self.middle_mouse_pressed = state == ElementState::Pressed;
                    if !self.middle_mouse_pressed {
                        self.last_mouse_pos = None;
                    }
                }
                MouseButton::Right => {
                    self.right_mouse_pressed = state == ElementState::Pressed;
                    if !self.right_mouse_pressed {
                        self.last_mouse_pos = None;
                    }
                }
                _ => {}
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.current_mouse_pos = (position.x, position.y);
                if self.left_mouse_pressed || self.middle_mouse_pressed || self.right_mouse_pressed
                {
                    if let Some(last_pos) = self.last_mouse_pos {
                        let delta_x = position.x - last_pos.0;
                        let delta_y = position.y - last_pos.1;

                        // Accumulate drag distance for click detection
                        self.mouse_drag_distance += (delta_x * delta_x + delta_y * delta_y).sqrt();

                        if let Some(renderer) = &mut self.renderer {
                            // Gizmo drag takes priority over camera orbit
                            if renderer.selection.gizmo_state.is_dragging && self.left_mouse_pressed
                            {
                                if let Some(sel_idx) = renderer.selection.selected_instance_index {
                                    let vp_rect = renderer.viewport_rect();
                                    let delta = bif_viewport::gizmo::compute_drag_delta(
                                        (position.x as f32, position.y as f32),
                                        renderer.selection.gizmo_state.drag_start_screen,
                                        &renderer.cam.camera,
                                        renderer.selection.gizmo_state.active_axis,
                                        renderer.selection.gizmo_state.drag_start_world,
                                        vp_rect,
                                    );

                                    let axis_dir = match renderer.selection.gizmo_state.active_axis
                                    {
                                        bif_viewport::gizmo::GizmoAxis::X => bif_math::Vec3::X,
                                        bif_viewport::gizmo::GizmoAxis::Y => bif_math::Vec3::Y,
                                        bif_viewport::gizmo::GizmoAxis::Z => bif_math::Vec3::Z,
                                        _ => bif_math::Vec3::ZERO,
                                    };

                                    let new_pos = renderer.selection.gizmo_state.drag_start_world
                                        + axis_dir * delta;

                                    // Build live transform and update GPU
                                    if let Some(mut live_transform) =
                                        renderer.get_instance_transform(sel_idx)
                                    {
                                        live_transform.translation = new_pos;
                                        renderer.set_live_transform(sel_idx, live_transform);
                                        renderer.reset_property_inspector_cache();
                                    }
                                }
                            } else if !renderer.is_camera_locked() {
                                // Normal camera controls
                                if self.left_mouse_pressed && !renderer.cam.camera.is_ortho() {
                                    renderer.cam.camera.orbit(
                                        -delta_x as f32 * ORBIT_SENSITIVITY,
                                        -delta_y as f32 * ORBIT_SENSITIVITY,
                                    );
                                } else if self.middle_mouse_pressed {
                                    let distance_scale =
                                        renderer.cam.camera.distance * PAN_DISTANCE_SCALE;
                                    renderer.cam.camera.pan(
                                        -delta_x as f32 * PAN_SENSITIVITY * distance_scale,
                                        delta_y as f32 * PAN_SENSITIVITY * distance_scale,
                                        0.0,
                                        1.0,
                                    );
                                } else if self.right_mouse_pressed {
                                    let dolly_amount = delta_y as f32
                                        * ORBIT_SENSITIVITY
                                        * renderer.cam.camera.distance;
                                    renderer.cam.camera.dolly(dolly_amount);
                                }
                                renderer.update_camera();
                            }
                        }
                    }
                    self.last_mouse_pos = Some((position.x, position.y));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(renderer) = &mut self.renderer {
                    // Skip camera controls if locked (USD camera active)
                    if !renderer.is_camera_locked() {
                        // Handle mouse wheel for dolly (zoom in/out) - scaled with distance
                        let scroll_lines = match delta {
                            winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                                pos.y as f32 / SCROLL_PIXEL_TO_LINES
                            }
                        };
                        // Scale dolly with distance for consistent feel
                        let dolly_amount =
                            -scroll_lines * renderer.cam.camera.distance * SCROLL_DOLLY_SCALE;
                        renderer.cam.camera.dolly(dolly_amount);
                        renderer.update_camera();
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(keycode),
                        state,
                        ..
                    },
                ..
            } => {
                match state {
                    ElementState::Pressed => {
                        self.keys_pressed.insert(keycode);

                        // Handle single-press keys
                        let ctrl = self.keys_pressed.contains(&KeyCode::ControlLeft)
                            || self.keys_pressed.contains(&KeyCode::ControlRight);
                        let shift = self.keys_pressed.contains(&KeyCode::ShiftLeft)
                            || self.keys_pressed.contains(&KeyCode::ShiftRight);

                        if keycode == KeyCode::KeyF {
                            self.with_renderer(|r| r.frame_mesh());
                        } else if ctrl && shift && keycode == KeyCode::KeyZ {
                            self.with_renderer(|r| {
                                if let Some(desc) = r.redo() {
                                    log::info!("Redo: {}", desc);
                                }
                            });
                        } else if ctrl && keycode == KeyCode::KeyZ {
                            self.with_renderer(|r| {
                                if let Some(desc) = r.undo() {
                                    log::info!("Undo: {}", desc);
                                }
                            });
                        } else if keycode == KeyCode::KeyK {
                            self.with_renderer(|r| {
                                if let Some(idx) = r.selection.selected_instance_index {
                                    r.set_keyframe(idx);
                                }
                            });
                        }
                    }
                    ElementState::Released => {
                        self.keys_pressed.remove(&keycode);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Calculate delta time
                let now = Instant::now();
                let delta_time = (now - self.last_frame_time).as_secs_f32();
                self.last_frame_time = now;

                // Update FPS counter and animation
                self.with_renderer(|r| {
                    r.update_fps(delta_time);
                    r.update_animation(delta_time);
                });

                // Handle keyboard movement (skip if camera locked)
                if let Some(renderer) = &mut self.renderer {
                    if !renderer.is_camera_locked() {
                        let mut right = 0.0;
                        let mut up = 0.0;
                        let mut forward = 0.0;

                        // In ortho mode, only allow pan (A/D/E/Q), no forward/back
                        let allow_forward = !renderer.cam.camera.is_ortho();
                        if allow_forward && self.keys_pressed.contains(&KeyCode::KeyW) {
                            forward += 1.0;
                        }
                        if allow_forward && self.keys_pressed.contains(&KeyCode::KeyS) {
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
                            renderer.cam.camera.pan(right, up, forward, delta_time);
                            renderer.update_camera();
                        }
                    }
                }

                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    // Skip frame if minimized (0x0 surface on Windows)
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        let clear_color = wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        };

                        if let Err(e) = renderer.render(clear_color, window) {
                            if let Some(surface_err) = e.downcast_ref::<wgpu::SurfaceError>() {
                                match surface_err {
                                    wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                                        renderer.resize(renderer.size);
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
                }

                // Redraw requested by about_to_wait when needed
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Only request redraw when something actually needs updating
        let renderer_busy = self.renderer.as_ref().is_some_and(|r| r.needs_redraw());
        let user_interacting = self.left_mouse_pressed
            || self.middle_mouse_pressed
            || self.right_mouse_pressed
            || !self.keys_pressed.is_empty();

        if self.needs_redraw || renderer_busy || user_interacting {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.needs_redraw = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build args vec from string slices (first element is program name).
    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| s.to_string()).collect()
    }

    // ── parse_args_from ──────────────────────────────────────────

    #[test]
    fn parse_args_no_arguments_returns_defaults() {
        let opts = parse_args_from(&args(&["bif_viewer"])).unwrap();
        assert!(opts.usda_path.is_none());
        assert!(opts.usd_path.is_none());
    }

    #[test]
    fn parse_args_usda_long_flag() {
        let opts = parse_args_from(&args(&["bif_viewer", "--usda", "scene.usda"])).unwrap();
        assert_eq!(opts.usda_path.as_deref(), Some("scene.usda"));
        assert!(opts.usd_path.is_none());
    }

    #[test]
    fn parse_args_usda_short_flag() {
        let opts = parse_args_from(&args(&["bif_viewer", "-u", "scene.usda"])).unwrap();
        assert_eq!(opts.usda_path.as_deref(), Some("scene.usda"));
    }

    #[test]
    fn parse_args_usd_flag() {
        let opts = parse_args_from(&args(&["bif_viewer", "--usd", "scene.usdc"])).unwrap();
        assert_eq!(opts.usd_path.as_deref(), Some("scene.usdc"));
        assert!(opts.usda_path.is_none());
    }

    #[test]
    fn parse_args_positional_usda() {
        let opts = parse_args_from(&args(&["bif_viewer", "kitchen.usda"])).unwrap();
        assert_eq!(opts.usda_path.as_deref(), Some("kitchen.usda"));
        assert!(opts.usd_path.is_none());
    }

    #[test]
    fn parse_args_positional_usdc() {
        let opts = parse_args_from(&args(&["bif_viewer", "kitchen.usdc"])).unwrap();
        assert!(opts.usda_path.is_none());
        assert_eq!(opts.usd_path.as_deref(), Some("kitchen.usdc"));
    }

    #[test]
    fn parse_args_positional_usd() {
        let opts = parse_args_from(&args(&["bif_viewer", "kitchen.usd"])).unwrap();
        assert_eq!(opts.usd_path.as_deref(), Some("kitchen.usd"));
    }

    #[test]
    fn parse_args_missing_value_after_usda() {
        let result = parse_args_from(&args(&["bif_viewer", "--usda"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires a file path"));
    }

    #[test]
    fn parse_args_missing_value_after_usd() {
        let result = parse_args_from(&args(&["bif_viewer", "--usd"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires a file path"));
    }

    #[test]
    fn parse_args_missing_value_after_short_u() {
        let result = parse_args_from(&args(&["bif_viewer", "-u"]));
        assert!(result.is_err());
    }

    #[test]
    fn parse_args_help_returns_err() {
        let result = parse_args_from(&args(&["bif_viewer", "--help"]));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "help");
    }

    #[test]
    fn parse_args_short_help_returns_err() {
        let result = parse_args_from(&args(&["bif_viewer", "-h"]));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "help");
    }

    #[test]
    fn parse_args_unknown_flag_still_succeeds() {
        // Unknown flags print a warning but don't fail
        let opts = parse_args_from(&args(&["bif_viewer", "--foo", "scene.usda"])).unwrap();
        // "scene.usda" is a positional arg after unknown flag is skipped
        assert_eq!(opts.usda_path.as_deref(), Some("scene.usda"));
    }

    #[test]
    fn parse_args_unknown_flag_alone() {
        let opts = parse_args_from(&args(&["bif_viewer", "--bogus"])).unwrap();
        assert!(opts.usda_path.is_none());
        assert!(opts.usd_path.is_none());
    }

    // ── is_click (click vs drag) ─────────────────────────────────

    #[test]
    fn is_click_zero_distance() {
        assert!(is_click(0.0));
    }

    #[test]
    fn is_click_small_movement() {
        assert!(is_click(1.5));
    }

    #[test]
    fn is_click_at_threshold_is_drag() {
        // Exactly at threshold → not a click (uses strict <)
        assert!(!is_click(CLICK_THRESHOLD));
    }

    #[test]
    fn is_click_just_below_threshold() {
        assert!(is_click(CLICK_THRESHOLD - 0.01));
    }

    #[test]
    fn is_click_large_movement_is_drag() {
        assert!(!is_click(50.0));
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        // Suppress noisy wgpu logs
        .filter_module("wgpu_core", log::LevelFilter::Warn)
        .filter_module("wgpu_hal", log::LevelFilter::Warn)
        .filter_module("naga", log::LevelFilter::Warn)
        .init();

    let opts = parse_args();

    if let Some(ref path) = opts.usd_path {
        log::info!("Starting BIF Viewer with USD (C++ bridge): {}", path);
    } else if let Some(ref path) = opts.usda_path {
        log::info!("Starting BIF Viewer with USDA: {}", path);
    } else {
        log::info!("Starting BIF Viewer (default scene)");
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new(opts.usda_path, opts.usd_path);

    log::info!("Running event loop");
    event_loop.run_app(&mut app)?;

    Ok(())
}
