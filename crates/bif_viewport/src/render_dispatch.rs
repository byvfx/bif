//! Render event dispatch — render mode, batch render, filter, denoise.

use std::sync::atomic::Ordering;

use crate::ivar_state::{BuildStatus, RenderMode};
use crate::Renderer;

impl Renderer {
    pub fn trigger_ivar_render(&mut self) -> bool {
        self.ivar.ivar_state.mode = RenderMode::Ivar;
        self.ivar.ivar_state.current_scale = 1;
        self.ivar.ivar_state.last_interaction_time = None;
        self.start_ivar_render();
        matches!(self.ivar.ivar_state.build_status, BuildStatus::Building)
            || self.ivar.ivar_state.world.is_some()
    }

    pub fn ivar_status_line(&self) -> String {
        let ivar = &self.ivar.ivar_state;
        if matches!(
            ivar.batch_status,
            crate::BatchRenderStatus::Rendering { .. }
        ) {
            return format!("Batch render: {:.0}%", ivar.batch_status.overall_progress());
        }
        match ivar.build_status {
            BuildStatus::Building => "Ivar: building scene...".to_string(),
            BuildStatus::Complete => {
                if ivar.render_complete {
                    format!(
                        "Ivar: complete  {}/{} spp  {:.1}s",
                        ivar.accumulated_samples,
                        ivar.target_spp,
                        ivar.elapsed_secs()
                    )
                } else if ivar.is_pass_in_flight() || ivar.accumulated_samples > 0 {
                    format!(
                        "Ivar: {:.0}%  {}/{} spp  {:.1}s",
                        ivar.progress(),
                        ivar.accumulated_samples,
                        ivar.target_spp,
                        ivar.elapsed_secs()
                    )
                } else {
                    String::new()
                }
            }
            BuildStatus::NotStarted => String::new(),
            BuildStatus::Failed => "Ivar: scene build failed".to_string(),
        }
    }

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
