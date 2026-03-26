# current bugs as of 2026-03-23

- add performance metrics to the code review process, such as time taken for certain operations, memory usage, etc. This will help identify bottlenecks and areas for optimization. see https://openusd.org/release/ref_performance_metrics.html
- add a feature where the user can selective load certain prims from the usd file instead of loading everything.
- improve error handling and logging to provide more detailed information when something goes wrong.
- checkt to see if the denoising is using the shading normals instead of the geometry normals, which can cause artifacts in the final render.
- dirty flag missing for camera, render settings, display settings changes — closing after changing these won't prompt to save (M30 persistence)
- undo/redo dirty tracking is approximate — undoing back to saved state still shows dirty. Needs saved-state index in undo stack for true tracking (M30 persistence)