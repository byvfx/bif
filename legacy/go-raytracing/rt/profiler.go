package rt

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"runtime/trace"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// ProfileConfig holds profiling configuration
type ProfileConfig struct {
	Enabled      bool   // Enable profiling
	CPUProfile   bool   // Enable CPU profiling
	MemProfile   bool   // Enable memory profiling
	TraceEnabled bool   // Enable execution tracing
	BlockProfile bool   // Enable block profiling (goroutine blocking)
	OutputDir    string // Directory to save profile files
	SampleRate   int    // CPU profiling sample rate (default: 100Hz)
}

// DefaultProfileConfig returns a default profiling configuration
func DefaultProfileConfig() *ProfileConfig {
	return &ProfileConfig{
		Enabled:      false,
		CPUProfile:   true,
		MemProfile:   true,
		TraceEnabled: false,
		BlockProfile: false,
		OutputDir:    "profiles",
		SampleRate:   100,
	}
}

// Profiler manages profiling for the raytracer
type Profiler struct {
	config    *ProfileConfig
	cpuFile   *os.File
	traceFile *os.File
	startTime time.Time
	enabled   bool
	mu        sync.Mutex

	// Timing metrics
	timings   map[string]*TimingMetric
	timingsMu sync.RWMutex
}

// TimingMetric tracks timing for a specific operation
type TimingMetric struct {
	Name      string
	TotalTime time.Duration
	CallCount int64
	MinTime   time.Duration
	MaxTime   time.Duration
	mu        sync.Mutex
}

// RenderStats holds detailed render statistics
type RenderStats struct {
	TotalRenderTime   time.Duration
	BVHConstructTime  time.Duration
	RayCount          atomic.Int64
	BVHIntersections  atomic.Int64
	PrimitiveTests    atomic.Int64
	ShadowRays        atomic.Int64
	ReflectionBounces atomic.Int64
	PixelsRendered    atomic.Int64
	SamplesComputed   atomic.Int64
}

// Global render stats instance
var GlobalRenderStats = &RenderStats{}

// GlobalProfiler is the default profiler instance
var GlobalProfiler = NewProfiler(DefaultProfileConfig())

// NewProfiler creates a new profiler with the given configuration
func NewProfiler(config *ProfileConfig) *Profiler {
	return &Profiler{
		config:  config,
		timings: make(map[string]*TimingMetric),
	}
}

// Start begins profiling
func (p *Profiler) Start() error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if !p.config.Enabled {
		return nil
	}

	p.startTime = time.Now()
	p.enabled = true

	// Create output directory
	if err := os.MkdirAll(p.config.OutputDir, 0755); err != nil {
		return fmt.Errorf("failed to create profile directory: %w", err)
	}

	timestamp := time.Now().Format("2006-01-02_15-04-05")

	// Start CPU profiling
	if p.config.CPUProfile {
		cpuPath := filepath.Join(p.config.OutputDir, fmt.Sprintf("cpu_%s.pprof", timestamp))
		f, err := os.Create(cpuPath)
		if err != nil {
			return fmt.Errorf("failed to create CPU profile: %w", err)
		}
		p.cpuFile = f

		runtime.SetCPUProfileRate(p.config.SampleRate)
		if err := pprof.StartCPUProfile(f); err != nil {
			return fmt.Errorf("failed to start CPU profile: %w", err)
		}
		fmt.Printf("📊 CPU profiling started: %s\n", cpuPath)
	}

	// Start execution tracing
	if p.config.TraceEnabled {
		tracePath := filepath.Join(p.config.OutputDir, fmt.Sprintf("trace_%s.out", timestamp))
		f, err := os.Create(tracePath)
		if err != nil {
			return fmt.Errorf("failed to create trace file: %w", err)
		}
		p.traceFile = f

		if err := trace.Start(f); err != nil {
			return fmt.Errorf("failed to start trace: %w", err)
		}
		fmt.Printf("📊 Execution tracing started: %s\n", tracePath)
	}

	// Enable block profiling
	if p.config.BlockProfile {
		runtime.SetBlockProfileRate(1)
		fmt.Println("📊 Block profiling enabled")
	}

	return nil
}

// Stop ends profiling and writes the results
func (p *Profiler) Stop() error {
	p.mu.Lock()
	defer p.mu.Unlock()

	if !p.enabled {
		return nil
	}

	timestamp := time.Now().Format("2006-01-02_15-04-05")

	// Stop CPU profiling
	if p.cpuFile != nil {
		pprof.StopCPUProfile()
		p.cpuFile.Close()
		fmt.Println("✓ CPU profile saved")
	}

	// Stop tracing
	if p.traceFile != nil {
		trace.Stop()
		p.traceFile.Close()
		fmt.Println("✓ Execution trace saved")
	}

	// Write memory profile
	if p.config.MemProfile {
		memPath := filepath.Join(p.config.OutputDir, fmt.Sprintf("mem_%s.pprof", timestamp))
		f, err := os.Create(memPath)
		if err != nil {
			return fmt.Errorf("failed to create memory profile: %w", err)
		}
		defer f.Close()

		runtime.GC() // Get up-to-date statistics
		if err := pprof.WriteHeapProfile(f); err != nil {
			return fmt.Errorf("failed to write memory profile: %w", err)
		}
		fmt.Printf("✓ Memory profile saved: %s\n", memPath)
	}

	// Write block profile
	if p.config.BlockProfile {
		blockPath := filepath.Join(p.config.OutputDir, fmt.Sprintf("block_%s.pprof", timestamp))
		f, err := os.Create(blockPath)
		if err != nil {
			return fmt.Errorf("failed to create block profile: %w", err)
		}
		defer f.Close()

		if err := pprof.Lookup("block").WriteTo(f, 0); err != nil {
			return fmt.Errorf("failed to write block profile: %w", err)
		}
		fmt.Printf("✓ Block profile saved: %s\n", blockPath)
	}

	// Write goroutine profile
	goroutinePath := filepath.Join(p.config.OutputDir, fmt.Sprintf("goroutine_%s.pprof", timestamp))
	f, err := os.Create(goroutinePath)
	if err == nil {
		defer f.Close()
		pprof.Lookup("goroutine").WriteTo(f, 0)
		fmt.Printf("✓ Goroutine profile saved: %s\n", goroutinePath)
	}

	p.enabled = false
	return nil
}

// StartTiming begins timing a named operation
func (p *Profiler) StartTiming(name string) func() {
	if !p.enabled {
		return func() {} // no-op
	}

	start := time.Now()
	return func() {
		elapsed := time.Since(start)
		p.recordTiming(name, elapsed)
	}
}

// recordTiming records a timing measurement
func (p *Profiler) recordTiming(name string, duration time.Duration) {
	p.timingsMu.Lock()
	metric, exists := p.timings[name]
	if !exists {
		metric = &TimingMetric{
			Name:    name,
			MinTime: duration,
			MaxTime: duration,
		}
		p.timings[name] = metric
	}
	p.timingsMu.Unlock()

	metric.mu.Lock()
	defer metric.mu.Unlock()

	metric.TotalTime += duration
	metric.CallCount++
	if duration < metric.MinTime {
		metric.MinTime = duration
	}
	if duration > metric.MaxTime {
		metric.MaxTime = duration
	}
}

// PrintTimingReport prints all recorded timing metrics
func (p *Profiler) PrintTimingReport() {
	p.timingsMu.RLock()
	defer p.timingsMu.RUnlock()

	if len(p.timings) == 0 {
		return
	}

	fmt.Println("\n" + strings.Repeat("═", 60))
	fmt.Println("                    TIMING REPORT")
	fmt.Println(strings.Repeat("═", 60))

	for name, metric := range p.timings {
		metric.mu.Lock()
		avgTime := time.Duration(0)
		if metric.CallCount > 0 {
			avgTime = metric.TotalTime / time.Duration(metric.CallCount)
		}
		fmt.Printf("  %-20s: Total: %12s | Calls: %8d | Avg: %12s\n",
			name, metric.TotalTime, metric.CallCount, avgTime)
		metric.mu.Unlock()
	}
	fmt.Println(strings.Repeat("═", 60))
}

// PrintRenderStatsReport prints detailed render statistics
func PrintRenderStatsReport(stats *RenderStats, renderTime time.Duration) {
	rays := stats.RayCount.Load()
	samples := stats.SamplesComputed.Load()
	pixels := stats.PixelsRendered.Load()

	raysPerSecond := float64(0)
	samplesPerSecond := float64(0)
	if renderTime.Seconds() > 0 {
		raysPerSecond = float64(rays) / renderTime.Seconds()
		samplesPerSecond = float64(samples) / renderTime.Seconds()
	}

	fmt.Println("\n" + strings.Repeat("═", 60))
	fmt.Println("                 RENDER STATISTICS")
	fmt.Println(strings.Repeat("═", 60))
	fmt.Printf("  Total Render Time      : %s\n", FormatDuration(renderTime))
	fmt.Printf("  Pixels Rendered        : %d\n", pixels)
	fmt.Printf("  Total Samples          : %d\n", samples)
	fmt.Printf("  Total Rays Cast        : %d\n", rays)
	fmt.Printf("  BVH Intersections      : %d\n", stats.BVHIntersections.Load())
	fmt.Printf("  Primitive Tests        : %d\n", stats.PrimitiveTests.Load())
	fmt.Printf("  Shadow Rays            : %d\n", stats.ShadowRays.Load())
	fmt.Printf("  Reflection Bounces     : %d\n", stats.ReflectionBounces.Load())
	fmt.Println(strings.Repeat("─", 60))
	fmt.Printf("  Rays/Second            : %.2f M\n", raysPerSecond/1_000_000)
	fmt.Printf("  Samples/Second         : %.2f M\n", samplesPerSecond/1_000_000)
	fmt.Println(strings.Repeat("═", 60))
}

// ResetRenderStats resets all render statistics
func ResetRenderStats() {
	GlobalRenderStats.RayCount.Store(0)
	GlobalRenderStats.BVHIntersections.Store(0)
	GlobalRenderStats.PrimitiveTests.Store(0)
	GlobalRenderStats.ShadowRays.Store(0)
	GlobalRenderStats.ReflectionBounces.Store(0)
	GlobalRenderStats.PixelsRendered.Store(0)
	GlobalRenderStats.SamplesComputed.Store(0)
}

// MemStats returns current memory statistics
func MemStats() runtime.MemStats {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	return m
}

// PrintMemStats prints current memory usage
func PrintMemStats() {
	m := MemStats()
	fmt.Println("\n" + strings.Repeat("═", 60))
	fmt.Println("                  MEMORY STATISTICS")
	fmt.Println(strings.Repeat("═", 60))
	fmt.Printf("  Alloc (current)        : %s\n", formatBytes(m.Alloc))
	fmt.Printf("  TotalAlloc (cumulative): %s\n", formatBytes(m.TotalAlloc))
	fmt.Printf("  Sys (from OS)          : %s\n", formatBytes(m.Sys))
	fmt.Printf("  NumGC                  : %d\n", m.NumGC)
	fmt.Printf("  Heap Objects           : %d\n", m.HeapObjects)
	fmt.Printf("  Goroutines             : %d\n", runtime.NumGoroutine())
	fmt.Println(strings.Repeat("═", 60))
}

// formatBytes formats bytes in human readable form
func formatBytes(b uint64) string {
	const unit = 1024
	if b < unit {
		return fmt.Sprintf("%d B", b)
	}
	div, exp := uint64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.2f %cB", float64(b)/float64(div), "KMGTPE"[exp])
}

// Timer is a simple utility for timing code blocks
type Timer struct {
	name  string
	start time.Time
}

// NewTimer creates and starts a new timer
func NewTimer(name string) *Timer {
	return &Timer{
		name:  name,
		start: time.Now(),
	}
}

// Stop stops the timer and prints the elapsed time
func (t *Timer) Stop() time.Duration {
	elapsed := time.Since(t.start)
	fmt.Printf("⏱️  %s: %s\n", t.name, FormatDuration(elapsed))
	return elapsed
}

// Elapsed returns the elapsed time without stopping
func (t *Timer) Elapsed() time.Duration {
	return time.Since(t.start)
}
