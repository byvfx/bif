# current bugs as of 2026-02-04

- [FIXED] when creating  multiple objects the previdous one gets deleted or hidden.  I think we need to start adding in to the USD scene graph

- [FIXED] camera has geo frustum and the drop down doesn't work.  I think we need to start adding in to the USD scene graph as well. that goes with anything.  so this means that we will need
adding in USD prims.  this will be the perfect time since the next milestone is  about adding scattering.

- [FIXED] make an option to hide or show the grid.
- [FIXED] cant delete nodes. we need to add in the delete functionality.  this is a pretty big one since it will be needed for a lot of things.  we can start with just deleting nodes and then move on to deleting connections.

- [FIXED] minimizing the window gives me 2026-02-10T00:00:33Z ERROR bif_viewer] Surface error: Outdated in the console. This is probably because the swapchain is out of date.  We need to handle this error and recreate the swapchain when it happens.  This is a common issue with wgpu and we need to make sure we handle it properly.

- when i transform the cube or sphere, the render doesnt get the updated transform.  this is probably because we are not updating the scene graph when we transform objects.  we need to make sure that when we transform an object, we update the scene graph and then trigger a re-render.

- i dont think  the usd export is working.
- not a big deal but we might need to up the cube map size for the ibl to get better quality in the viewport currently it looks like some artificating is happening.  we can also add in a toggle for the cube map size so that we can test the performance impact of it.

- hdr and .exr ibl has a max of 8192 resolution.  we should add in a warning when the user tries to load an ibl that is too large and maybe even downscale it automatically to prevent performance issues.
- normals are backwards on the cube

i would like a live viewport to mimic karma, octane.. etc.  currently we are saving the render result and i think storing it in a texture buffer
