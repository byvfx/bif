# current bugs as of 2026-03-23

- add performance metrics to the code review process, such as time taken for certain operations, memory usage, etc. This will help identify bottlenecks and areas for optimization. see <https://openusd.org/release/ref_performance_metrics.html> i will want to use usdview and bif to compare times so we can focus on making the code faster and more efficient.
- add a feature where the user can selective load certain prims from the usd file instead of loading everything. lets take a look at katana and houdini to see how they do this and implement something similar. this will help with performance when working with large scenes.
- come up with a SIMPLE way of just working with USD files. there can be two layers, top layer for the artist then a deep layer to the program if they need it. i really want make a novel yet simple scene assembler. lets brainstorm this
- add sub surface scattering support to the renderer, which is important for rendering realistic skin and other materials.
- add support for more complex materials, such as those with multiple layers or complex BRDFs.
- check to make sure subdivision is working correctly, test again self authored assets.
- add camera viewport safe greybox to help artists frame their shots correctly.
❯ ok i was reading over some performance metrics here https://openusd.org/release/ref_performance_metrics.html and i would like to implement this, we would use usdview and bif opening different scenes i have along with the ones they state in their documentation. here is some
performance considerations, actually is might be best to index that site and we can then also do some checks in the code base to make sure everything is implemented as of v25.  they are on v26 we can up grade once we get something more solid here.  so firstly lets get the
performance metrics worked out lets plan this out