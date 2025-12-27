//TODO check to se if MIS or NEE is messing up my metallic reflection

package main

import (
	"flag"
	"fmt"
	"go-raytracing/rt"
	"os"
	"os/signal"
	"runtime"
	"strings"
	"syscall"

	"github.com/hajimehoshi/ebiten/v2"
)

func main() {
	// Profiling flags
	enableProfile := flag.Bool("profile", false, "Enable profiling (CPU, memory)")
	cpuProfile := flag.Bool("cpu-profile", true, "Enable CPU profiling (requires -profile)")
	memProfile := flag.Bool("mem-profile", true, "Enable memory profiling (requires -profile)")
	traceProfile := flag.Bool("trace", false, "Enable execution tracing (requires -profile)")
	blockProfile := flag.Bool("block-profile", false, "Enable block profiling (requires -profile)")
	profileDir := flag.String("profile-dir", "profiles", "Directory to save profile files")
	showMemStats := flag.Bool("mem-stats", false, "Show memory statistics after render")
	sceneName := flag.String("scene", "hdri-test", "Scene to render (e.g. hdri-test, random, cornell, cornell-smoke)")

	flag.Parse()

	// Configure profiler
	profileConfig := &rt.ProfileConfig{
		Enabled:      *enableProfile,
		CPUProfile:   *cpuProfile,
		MemProfile:   *memProfile,
		TraceEnabled: *traceProfile,
		BlockProfile: *blockProfile,
		OutputDir:    *profileDir,
		SampleRate:   100,
	}

	profiler := rt.NewProfiler(profileConfig)

	// Start profiling if enabled
	if *enableProfile {
		fmt.Println("🔬 Profiling enabled")
		if err := profiler.Start(); err != nil {
			fmt.Fprintf(os.Stderr, "Failed to start profiler: %v\n", err)
			os.Exit(1)
		}

		// Handle graceful shutdown for profiling
		sigChan := make(chan os.Signal, 1)
		signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)
		go func() {
			<-sigChan
			fmt.Println("\n Interrupt received, saving profiles...")
			profiler.Stop()
			profiler.PrintTimingReport()
			if *showMemStats {
				rt.PrintMemStats()
			}
			os.Exit(0)
		}()
	}

	// Reset render stats
	rt.ResetRenderStats()

	// Time BVH construction
	bvhTimer := rt.NewTimer("BVH Construction")
	world, camera, sceneErr := loadScene(*sceneName)
	if sceneErr != nil {
		fmt.Fprintf(os.Stderr, "Unknown scene '%s'. Use -help for options.\n", *sceneName)
		os.Exit(1)
	}
	bvh := rt.NewBVHNodeFromList(world)
	bvhTime := bvhTimer.Stop()
	rt.GlobalRenderStats.BVHConstructTime = bvhTime

	rt.PrintRenderSettings(camera, len(world.Objects))

	bucketSize := 32
	numWorkers := runtime.NumCPU()

	renderer := rt.NewBucketRenderer(camera, bvh, bucketSize, numWorkers)

	// renderer := rt.NewProgressiveRenderer(camera, bvh)

	ebiten.SetWindowSize(camera.ImageWidth, camera.ImageHeight)
	ebiten.SetWindowTitle("Go Raytracer")

	if err := ebiten.RunGame(renderer); err != nil {
		panic(err)
	}

	// Stop profiling and print reports
	if *enableProfile {
		profiler.Stop()
		profiler.PrintTimingReport()
	}

	if *showMemStats {
		rt.PrintMemStats()
	}
}

func loadScene(name string) (*rt.HittableList, *rt.Camera, error) {
	switch strings.ToLower(name) {
	case "random", "randomscene":
		w, c := rt.RandomScene()
		return w, c, nil
	case "checkered", "checker", "checkered-spheres":
		w, c := rt.CheckeredSpheresScene()
		return w, c, nil
	case "simple", "simple-scene":
		w, c := rt.SimpleScene()
		return w, c, nil
	case "perlin", "perlin-spheres":
		w, c := rt.PerlinSpheresScene()
		return w, c, nil
	case "earth", "earth-scene":
		w, c := rt.EarthScene()
		return w, c, nil
	case "quads", "quads-scene":
		w, c := rt.QuadsScene()
		return w, c, nil
	case "cornell", "cornell-box":
		w, c := rt.CornellBoxScene()
		return w, c, nil
	case "cornell-glossy":
		w, c := rt.CornellBoxGlossy()
		return w, c, nil
	case "cornell-lucy":
		w, c := rt.CornellBoxLucy()
		return w, c, nil
	case "cornell-smoke", "cornell-fog":
		w, c := rt.CornellSmoke()
		return w, c, nil
	case "glossy-metal", "glossy-metal-test":
		w, c := rt.GlossyMetalTest()
		return w, c, nil
	case "primitives", "primitives-scene":
		w, c := rt.PrimitivesScene()
		return w, c, nil
	case "hdri", "hdri-test", "hdr":
		w, c := rt.HDRITestScene()
		return w, c, nil
	default:
		return nil, nil, fmt.Errorf("unknown scene: %s", name)
	}
}
