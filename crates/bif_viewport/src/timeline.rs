/// Timeline state for animation playback.
/// Timeline playback state for animation control.
#[derive(Clone, Debug)]
pub struct TimelineState {
    /// Whether animation is playing
    pub is_playing: bool,
    /// Current frame
    pub current_frame: f64,
    /// Start frame from USD stage
    pub start_frame: f64,
    /// End frame from USD stage
    pub end_frame: f64,
    /// Frames per second
    pub fps: f64,
    /// Loop playback when reaching end
    pub loop_playback: bool,
    /// Use USD camera instead of viewport camera
    pub use_usd_camera: bool,
    /// Snap to integer frames (no sub-frame interpolation)
    pub snap_to_frames: bool,
    /// Wall-clock instant when playback started (for realtime mode)
    playback_start_instant: Option<std::time::Instant>,
    /// Frame at which playback started (for realtime mode)
    playback_start_frame: f64,
    /// Realtime mode: ON = wall-clock accurate, OFF = every frame as fast as possible
    pub realtime: bool,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            is_playing: false,
            current_frame: 0.0,
            start_frame: 0.0,
            end_frame: 0.0,
            fps: 24.0,
            loop_playback: true,
            use_usd_camera: false,
            snap_to_frames: true, // Default to integer frames for predictable playback
            playback_start_instant: None,
            playback_start_frame: 0.0,
            realtime: true, // Default to wall-clock accurate playback
        }
    }
}

impl TimelineState {
    /// Check if the timeline has a valid frame range.
    pub fn has_range(&self) -> bool {
        self.end_frame > self.start_frame
    }

    /// Start playback, recording wall-clock start time.
    pub fn play(&mut self) {
        if !self.has_range() {
            return;
        }
        self.is_playing = true;
        self.playback_start_instant = Some(std::time::Instant::now());
        self.playback_start_frame = self.current_frame;
        log::debug!(
            "play(): started at frame {:.2}, realtime={}",
            self.playback_start_frame,
            self.realtime
        );
    }

    /// Pause playback, clearing wall-clock state.
    pub fn pause(&mut self) {
        self.is_playing = false;
        self.playback_start_instant = None;
        log::debug!("pause(): stopped at frame {:.2}", self.current_frame);
    }

    /// Toggle between play and pause.
    pub fn toggle_playback(&mut self) {
        if self.is_playing {
            self.pause();
        } else {
            self.play();
        }
    }

    /// Update timeline based on wall-clock time (for realtime mode) or advance one frame (for non-realtime).
    /// Call this each frame instead of advance().
    pub fn update(&mut self) {
        if !self.is_playing || !self.has_range() {
            return;
        }

        let old_frame = self.current_frame;

        if self.realtime {
            // Wall-clock accurate: calculate frame from elapsed time since play started
            if let Some(start_instant) = self.playback_start_instant {
                let elapsed_secs = start_instant.elapsed().as_secs_f64();
                let frame_offset = elapsed_secs * self.fps;
                let new_frame = self.playback_start_frame + frame_offset;

                // Handle loop/stop at end
                let range = self.end_frame - self.start_frame;
                if new_frame > self.end_frame {
                    if self.loop_playback {
                        // Calculate wrapped frame position
                        let overshoot = new_frame - self.start_frame;
                        let loops = (overshoot / range).floor();
                        self.current_frame = self.start_frame + (overshoot - loops * range);
                        // Reset anchor for next loop cycle
                        self.playback_start_instant = Some(std::time::Instant::now());
                        self.playback_start_frame = self.current_frame;
                        log::debug!("loop(): wrapped to frame {:.2}", self.current_frame);
                    } else {
                        self.current_frame = self.end_frame;
                        self.pause();
                    }
                } else {
                    self.current_frame = new_frame;
                }
            }
        } else {
            // Non-realtime: advance exactly one frame per call (plays every frame, fast as possible)
            self.current_frame += 1.0;
            if self.current_frame > self.end_frame {
                if self.loop_playback {
                    self.current_frame = self.start_frame;
                } else {
                    self.current_frame = self.end_frame;
                    self.pause();
                }
            }
        }

        // Debug log only when frame changes significantly
        if (self.current_frame - old_frame).abs() >= 0.5 {
            log::debug!(
                "update(): {:.2} -> {:.2} (realtime={})",
                old_frame,
                self.current_frame,
                self.realtime
            );
        }
    }

    /// Reset playback anchor when scrubbing during playback.
    /// Call this when the user manually changes current_frame while playing.
    pub fn reset_playback_anchor(&mut self) {
        if self.is_playing {
            self.playback_start_instant = Some(std::time::Instant::now());
            self.playback_start_frame = self.current_frame;
        }
    }

    /// Go to start frame.
    pub fn go_to_start(&mut self) {
        self.current_frame = self.start_frame;
        self.reset_playback_anchor();
    }

    /// Go to end frame.
    pub fn go_to_end(&mut self) {
        self.current_frame = self.end_frame;
        self.reset_playback_anchor();
    }

    /// Set timeline from scene info.
    pub fn set_from_scene(&mut self, start: f64, end: f64, fps: f64) {
        self.start_frame = start;
        self.end_frame = end;
        self.fps = fps;
        self.current_frame = start;
    }

    /// Get the effective frame for animation evaluation.
    /// If snap_to_frames is true, returns the nearest integer frame.
    pub fn effective_frame(&self) -> f64 {
        if self.snap_to_frames {
            self.current_frame.round()
        } else {
            self.current_frame
        }
    }
}

#[cfg(test)]
mod timeline_tests {
    use super::TimelineState;

    fn setup_timeline() -> TimelineState {
        let mut ts = TimelineState::default();
        ts.set_from_scene(1.0, 48.0, 24.0); // 1-48 at 24fps = 2 seconds
        ts
    }

    #[test]
    fn test_wall_clock_playback() {
        let mut ts = setup_timeline();

        // Start playback
        ts.play();
        assert!(ts.is_playing);
        assert!(ts.playback_start_instant.is_some());
        assert!((ts.playback_start_frame - 1.0).abs() < 0.001);

        // Simulate time passing (sleep briefly then update)
        std::thread::sleep(std::time::Duration::from_millis(50));
        ts.update();

        // Frame should have advanced (50ms at 24fps = ~1.2 frames)
        assert!(
            ts.current_frame > 1.0,
            "Frame should advance: {}",
            ts.current_frame
        );

        // Pause should stop playback
        ts.pause();
        assert!(!ts.is_playing);
        assert!(ts.playback_start_instant.is_none());
    }

    #[test]
    fn test_loop_playback() {
        let mut ts = setup_timeline();
        ts.loop_playback = true;
        ts.realtime = false; // Use non-realtime for predictable testing

        // Start near end
        ts.current_frame = 47.0;
        ts.play();

        // Advance past end
        ts.update(); // 47 -> 48
        ts.update(); // 48 -> wraps to 1

        // Should have looped back to start
        assert!(
            ts.current_frame >= 1.0 && ts.current_frame < 3.0,
            "Should loop to start: {}",
            ts.current_frame
        );
        assert!(ts.is_playing, "Should still be playing after loop");
    }

    #[test]
    fn test_pause_stops_time() {
        let mut ts = setup_timeline();
        ts.play();

        // Let some time pass
        std::thread::sleep(std::time::Duration::from_millis(20));
        ts.update();
        let frame_after_play = ts.current_frame;

        // Pause
        ts.pause();
        let frame_at_pause = ts.current_frame;

        // More time passes but update shouldn't change frame
        std::thread::sleep(std::time::Duration::from_millis(50));
        ts.update();

        assert!(
            (ts.current_frame - frame_at_pause).abs() < 0.001,
            "Frame should not change while paused: {} vs {}",
            ts.current_frame,
            frame_at_pause
        );
        assert!(
            frame_after_play > 1.0,
            "Frame should have advanced before pause"
        );
    }

    #[test]
    fn test_non_realtime_mode() {
        let mut ts = setup_timeline();
        ts.realtime = false;
        ts.current_frame = 10.0;
        ts.play();

        // Each update should advance exactly 1 frame
        ts.update();
        assert!((ts.current_frame - 11.0).abs() < 0.001);

        ts.update();
        assert!((ts.current_frame - 12.0).abs() < 0.001);
    }

    #[test]
    fn test_scrub_during_playback() {
        let mut ts = setup_timeline();
        ts.play();

        // Let playback run
        std::thread::sleep(std::time::Duration::from_millis(20));
        ts.update();

        // Scrub to new position
        ts.current_frame = 30.0;
        ts.reset_playback_anchor();

        // Continue playback from new position
        std::thread::sleep(std::time::Duration::from_millis(20));
        ts.update();

        // Should be advancing from frame 30, not original start
        assert!(
            ts.current_frame >= 30.0,
            "Should continue from scrubbed position: {}",
            ts.current_frame
        );
    }
}
