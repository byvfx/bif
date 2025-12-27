package rt

import (
	"fmt"
	"runtime"
	"testing"
	"time"
)

// Benchmark utilities for performance testing

// BenchmarkConfig holds configuration for benchmarks
type BenchmarkConfig struct {
	Width           int
	Height          int
	SamplesPerPixel int
	MaxDepth        int
	Iterations      int
}

// DefaultBenchmarkConfig returns a default benchmark configuration
func DefaultBenchmarkConfig() *BenchmarkConfig {
	return &BenchmarkConfig{
		Width:           320,
		Height:          180,
		SamplesPerPixel: 4,
		MaxDepth:        10,
		Iterations:      1,
	}
}

// BenchmarkResult stores benchmark results
type BenchmarkResult struct {
	Name         string
	Duration     time.Duration
	PixelsPerSec float64
	RaysPerSec   float64
	MemoryUsed   uint64
	Allocations  uint64
}

// RunBenchmark runs a simple inline benchmark and returns results
func RunBenchmark(name string, fn func()) *BenchmarkResult {
	// Get initial memory stats
	var memBefore runtime.MemStats
	runtime.ReadMemStats(&memBefore)

	// Reset render stats
	ResetRenderStats()

	// Run benchmark
	start := time.Now()
	fn()
	duration := time.Since(start)

	// Get final memory stats
	var memAfter runtime.MemStats
	runtime.ReadMemStats(&memAfter)

	rays := GlobalRenderStats.RayCount.Load()

	return &BenchmarkResult{
		Name:        name,
		Duration:    duration,
		RaysPerSec:  float64(rays) / duration.Seconds(),
		MemoryUsed:  memAfter.TotalAlloc - memBefore.TotalAlloc,
		Allocations: memAfter.Mallocs - memBefore.Mallocs,
	}
}

// PrintBenchmarkResult prints a benchmark result
func (r *BenchmarkResult) Print() {
	fmt.Printf("\n=== Benchmark: %s ===\n", r.Name)
	fmt.Printf("  Duration:       %s\n", FormatDuration(r.Duration))
	fmt.Printf("  Rays/sec:       %.2f M\n", r.RaysPerSec/1_000_000)
	fmt.Printf("  Memory used:    %s\n", formatBytes(r.MemoryUsed))
	fmt.Printf("  Allocations:    %d\n", r.Allocations)
	fmt.Println()
}

// BenchmarkRayIntersection benchmarks ray-AABB intersection
func BenchmarkRayAABBIntersection(b *testing.B) {
	ray := NewRay(Point3{0, 0, 0}, Vec3{1, 1, 1}, 0)
	aabb := AABB{
		X: NewInterval(-1, 1),
		Y: NewInterval(-1, 1),
		Z: NewInterval(-1, 1),
	}
	interval := NewInterval(0.001, 1000)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		aabb.Hit(ray, interval)
	}
}

// BenchmarkVec3Operations benchmarks vector operations
func BenchmarkVec3Operations(b *testing.B) {
	v1 := Vec3{1.0, 2.0, 3.0}
	v2 := Vec3{4.0, 5.0, 6.0}

	b.Run("Add", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = v1.Add(v2)
		}
	})

	b.Run("Dot", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = Dot(v1, v2)
		}
	})

	b.Run("Cross", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = Cross(v1, v2)
		}
	})

	b.Run("Normalize", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = v1.Unit()
		}
	})
}

// BenchmarkBVHConstruction benchmarks BVH construction
func BenchmarkBVHConstruction(b *testing.B) {
	// Create test objects
	objects := make([]Hittable, 100)
	for i := 0; i < 100; i++ {
		center := Point3{
			RandomDoubleRange(-10, 10),
			RandomDoubleRange(-10, 10),
			RandomDoubleRange(-10, 10),
		}
		objects[i] = NewSphere(center, 0.5, NewLambertian(Color{0.5, 0.5, 0.5}))
	}

	list := &HittableList{Objects: objects}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = NewBVHNodeFromList(list)
	}
}

// BenchmarkRayTracing benchmarks full ray tracing for a single pixel
func BenchmarkRayTracing(b *testing.B) {
	// Set up a simple scene
	world, camera := CornellBoxScene()
	camera.Initialize()
	bvh := NewBVHNodeFromList(world)

	ray := camera.GetRay(camera.ImageWidth/2, camera.ImageHeight/2)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = camera.RayColor(ray, camera.MaxDepth, bvh)
	}
}

// QuickBenchmark runs a quick performance test with minimal settings
func QuickBenchmark() *BenchmarkResult {
	fmt.Println("🏃 Running quick benchmark...")

	// Create a simple scene
	world, camera := CornellBoxScene()

	// Override with benchmark settings
	camera.ImageWidth = 160
	camera.ImageHeight = 90
	camera.SamplesPerPixel = 1
	camera.MaxDepth = 3
	camera.Initialize()

	bvh := NewBVHNodeFromList(world)

	result := RunBenchmark("QuickBenchmark", func() {
		// Render all pixels
		for j := 0; j < camera.ImageHeight; j++ {
			for i := 0; i < camera.ImageWidth; i++ {
				ray := camera.GetRay(i, j)
				_ = camera.RayColor(ray, camera.MaxDepth, bvh)
			}
		}
	})

	result.PixelsPerSec = float64(camera.ImageWidth*camera.ImageHeight) / result.Duration.Seconds()
	return result
}

// BenchmarkBVHTraversal benchmarks BVH traversal performance
func BenchmarkBVHTraversal(b *testing.B) {
	// Create a scene with many objects
	objects := make([]Hittable, 1000)
	for i := 0; i < 1000; i++ {
		center := Point3{
			RandomDoubleRange(-10, 10),
			RandomDoubleRange(-10, 10),
			RandomDoubleRange(-10, 10),
		}
		objects[i] = NewSphere(center, 0.2, NewLambertian(Color{0.5, 0.5, 0.5}))
	}
	list := &HittableList{Objects: objects}

	// Build BVH
	bvh := NewBVHNodeFromList(list)

	// Create test rays
	rays := make([]Ray, 100)
	for i := range rays {
		origin := Point3{
			RandomDoubleRange(-15, 15),
			RandomDoubleRange(-15, 15),
			RandomDoubleRange(-15, 15),
		}
		target := Point3{
			RandomDoubleRange(-5, 5),
			RandomDoubleRange(-5, 5),
			RandomDoubleRange(-5, 5),
		}
		dir := target.Sub(origin).Unit()
		rays[i] = NewRay(origin, dir, 0)
	}

	interval := NewInterval(0.001, 1000)

	rec := &HitRecord{}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		ray := rays[i%len(rays)]
		bvh.Hit(ray, interval, rec)
	}
}
