//! Render event dispatch — render mode, batch render, filter, denoise.

use std::sync::atomic::Ordering;

use crate::ivar_state::RenderMode;
use crate::Renderer;

impl Renderer {
    pub(crate) fn handle_render_mode_changed(&mut self) {
        if self.ivar.ivar_state.mode == RenderMode::Ivar {
            log::info!("Switched to Ivar mode - starting render");
            self.ivar.ivar_state.current_scale = 1;
            self.ivar.ivar_state.last_interaction_time = None;
            self.start_ivar_render();
        }
    }

    pub(crate) fn handle_rebuild_scene(&mut self) {
        log::info!("Manual scene rebuild requested");
        self.invalidate_ivar_scene();
    }

    pub(crate) fn handle_denoise_requested(&mut self) {
        self.denoise_ivar_result();
    }

    pub(crate) fn handle_filter_changed(&mut self) {
        if self.ivar.ivar_state.mode == RenderMode::Ivar {
            self.ivar.ivar_state.current_scale = 1;
            self.ivar.ivar_state.last_interaction_time = None;
            self.start_ivar_render();
        }
    }

    pub(crate) fn handle_start_batch_render(&mut self) {
        self.start_batch_render();
    }

    pub(crate) fn handle_cancel_batch_render(&mut self) {
        if let Some(ref flag) = self.async_channels.batch_cancel_flag {
            flag.store(true, Ordering::Relaxed);
        }
    }
}
