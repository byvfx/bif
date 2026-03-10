# Development Log - February 2, 2026

## Session Focus
M19.3: Viewport Camera Selection & Locking

## Goals
- Add camera dropdown to timeline panel
- Lock camera controls when USD camera selected
- Animate camera during playback

## What I Did

### Viewport Camera Selection
Added new state to Renderer:
- `viewport_camera_source: CameraSource` - tracks selected camera
- `camera_locked: bool` - locks controls when USD cam active
- `selected_usd_camera: Option<String>` - stores path for animation

### Timeline Panel UI Updates
- Camera dropdown showing "Viewport" + USD cameras from stage
- Lock/Free toggle button (only visible when USD camera selected)
- Improved slider with `.show_value(true).integer()` for frame numbers
- Added start/end frame labels next to slider
- FPS display as `@24fps` format

### Camera Control Locking
- Added `is_camera_locked()` public getter
- main.rs: Mouse controls (orbit, pan, dolly, wheel) check lock state
- main.rs: WASD keyboard movement checks lock state

### Animation Integration
- Camera sync moved BEFORE mesh animation check in `update_animation()`
- Camera can now animate alone even without mesh animations

### Bug Fixes
- Ivar cache invalidation now only happens in Ivar mode (not during viewport playback)
- Frame tolerance increased from 0.001 to 0.5 to handle rapid redraws

## Issues Found
- Play button animation not working reliably
- Rapid redraws causing tiny delta_time values (~1.4ms instead of ~16ms)
- Frame tolerance was too strict (0.001) causing updates to be skipped
- Still debugging playback - scrubbing works, play doesn't advance properly

## Files Modified
| File | Changes |
|------|---------|
| `crates/bif_viewport/src/lib.rs` | Camera state, dropdown UI, animation sync |
| `crates/bif_viewer/src/main.rs` | Lock check for camera controls |

## Next Session
- Continue debugging play button animation
- Investigate rapid redraw causing tiny delta_time
- Consider accumulating delta or different timing approach
