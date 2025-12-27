package rt

import (
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"sort"
	"sync"
	"sync/atomic"
	"time"

	"github.com/hajimehoshi/ebiten/v2"
	"github.com/hajimehoshi/ebiten/v2/ebitenutil"
	"github.com/hajimehoshi/ebiten/v2/text/v2"
	"golang.org/x/image/font/basicfont"
)

// Bucket represents a tile/region of the image to render
// RUST PORT NOTE: Simple value type, use #[derive(Copy, Clone)]
type Bucket struct {
	X      int // Starting X coordinate
	Y      int // Starting Y coordinate
	Width  int // Bucket width
	Height int // Bucket height
}

// BucketRenderer implements parallel tile-based rendering with progressive passes
// RUST PORT NOTE: Replace with:
// - Arc<Mutex<RgbaImage>> for framebuffer
// - AtomicI32 for counters
// - AtomicBool for flags
// - crossbeam::channel or rayon::par_iter for work distribution
type BucketRenderer struct {
	framebuffer    *image.RGBA
	camera         *Camera
	world          Hittable
	buckets        []Bucket
	completedCount atomic.Int32
	totalBuckets   int
	bucketSize     int
	completed      bool
	renderStart    time.Time
	renderEnd      time.Time
	numWorkers     int
	renderStarted  bool
	currentPass    int
	totalPasses    int
	passComplete   atomic.Bool
	mu             sync.Mutex // Protects framebuffer writes
}

func NewBucketRenderer(camera *Camera, world Hittable, bucketSize int, numWorkers int) *BucketRenderer {
	framebuffer := image.NewRGBA(image.Rect(0, 0, camera.ImageWidth, camera.ImageHeight))

	// Generate buckets
	buckets := generateBuckets(camera.ImageWidth, camera.ImageHeight, bucketSize)

	return &BucketRenderer{
		framebuffer:   framebuffer,
		camera:        camera,
		world:         world,
		buckets:       buckets,
		totalBuckets:  len(buckets),
		bucketSize:    bucketSize,
		completed:     false,
		renderStart:   time.Now(),
		numWorkers:    numWorkers,
		renderStarted: false,
		currentPass:   0,
		totalPasses:   3, // Preview (1 SPP) + Medium (SPP/4) + Final (full SPP)
	}
}

// generateBuckets creates a grid of buckets in spiral order (V-Ray style)
func generateBuckets(width, height, bucketSize int) []Bucket {
	var buckets []Bucket

	// First generate all buckets in grid order
	for y := 0; y < height; y += bucketSize {
		for x := 0; x < width; x += bucketSize {
			bw := min(bucketSize, width-x)
			bh := min(bucketSize, height-y)
			buckets = append(buckets, Bucket{
				X:      x,
				Y:      y,
				Width:  bw,
				Height: bh,
			})
		}
	}

	// Sort buckets in spiral order from center
	centerX := width / 2
	centerY := height / 2

	type bucketDist struct {
		bucket Bucket
		dist   float64
	}

	bucketDistances := make([]bucketDist, len(buckets))
	for i, b := range buckets {
		bx := b.X + b.Width/2
		by := b.Y + b.Height/2
		dx := float64(bx - centerX)
		dy := float64(by - centerY)
		dist := dx*dx + dy*dy
		bucketDistances[i] = bucketDist{bucket: b, dist: dist}
	}

	// Sort by distance from center (spiral out) - O(n log n)
	sort.Slice(bucketDistances, func(i, j int) bool {
		return bucketDistances[i].dist < bucketDistances[j].dist
	})

	// Extract sorted buckets
	sortedBuckets := make([]Bucket, len(buckets))
	for i, bd := range bucketDistances {
		sortedBuckets[i] = bd.bucket
	}

	return sortedBuckets
}

func (r *BucketRenderer) Update() error {
	if r.completed {
		return nil
	}

	// Start rendering on first update (protected by mutex)
	r.mu.Lock()
	if !r.renderStarted {
		r.renderStarted = true
		r.mu.Unlock()
		go r.renderMultiPass()
	} else {
		r.mu.Unlock()
	}

	// Check if current pass is complete and start next pass
	if r.passComplete.Load() && r.currentPass < r.totalPasses {
		r.passComplete.Store(false)
		r.completedCount.Store(0)
		r.currentPass++

		if r.currentPass < r.totalPasses {
			go r.renderPass()
		} else {
			// All passes done - currentPass is now equal to totalPasses
			r.completed = true
			r.renderEnd = time.Now()
			r.drawStatsToFramebuffer()
			_ = r.SaveImage("image.png")

			// Print render stats
			renderDuration := r.renderEnd.Sub(r.renderStart)
			PrintRenderStats(renderDuration, r.camera.ImageWidth, r.camera.ImageHeight)
		}
	}

	return nil
}

func (r *BucketRenderer) renderMultiPass() {
	r.renderPass()
}

func (r *BucketRenderer) renderPass() {
	// Determine samples for this pass
	var samplesForPass int
	var depthForPass int

	switch r.currentPass {
	case 0:
		// Preview pass: 1 SPP, reduced depth
		samplesForPass = 1
		depthForPass = 3
	case 1:
		// Medium pass: 25% of target SPP
		samplesForPass = max(1, r.camera.SamplesPerPixel/4)
		depthForPass = max(3, r.camera.MaxDepth/2)
	case 2:
		// Final pass: Full quality
		samplesForPass = r.camera.SamplesPerPixel
		depthForPass = r.camera.MaxDepth
	default:
		samplesForPass = r.camera.SamplesPerPixel
		depthForPass = r.camera.MaxDepth
	}

	// Use buffered channel for better performance
	bucketChan := make(chan Bucket, r.numWorkers*2)

	// Start worker goroutines
	var wg sync.WaitGroup
	for i := 0; i < r.numWorkers; i++ {
		wg.Add(1)
		go func(workerID int) {
			defer wg.Done()
			r.workerMultiPass(bucketChan, samplesForPass, depthForPass)
		}(i)
	}

	// Feed buckets into channel (in spiral order)
	for _, bucket := range r.buckets {
		bucketChan <- bucket
	}
	close(bucketChan)

	wg.Wait()
	r.passComplete.Store(true)
}

func (r *BucketRenderer) renderParallel() {
	// Use buffered channel for better performance
	bucketChan := make(chan Bucket, r.numWorkers*2)

	// Start worker goroutines first
	var wg sync.WaitGroup
	for i := 0; i < r.numWorkers; i++ {
		wg.Add(1)
		go func(workerID int) {
			defer wg.Done()
			r.worker(bucketChan, workerID)
		}(i)
	}

	// Feed buckets into channel (in spiral order)
	for _, bucket := range r.buckets {
		bucketChan <- bucket
	}
	close(bucketChan)

	wg.Wait()
}

func (r *BucketRenderer) worker(buckets <-chan Bucket, workerID int) {
	for bucket := range buckets {
		r.renderBucket(bucket)
		r.completedCount.Add(1)
	}
}

func (r *BucketRenderer) workerMultiPass(buckets <-chan Bucket, samplesPerPixel int, maxDepth int) {
	for bucket := range buckets {
		r.renderBucketWithQuality(bucket, samplesPerPixel, maxDepth)
		r.completedCount.Add(1)
	}
}

func (r *BucketRenderer) renderBucket(bucket Bucket) {
	r.renderBucketWithQuality(bucket, r.camera.SamplesPerPixel, r.camera.MaxDepth)
}

func (r *BucketRenderer) renderBucketWithQuality(bucket Bucket, samplesPerPixel int, maxDepth int) {
	// Create temporary buffer for this bucket
	bucketBuffer := make([]color.RGBA, bucket.Width*bucket.Height)

	for localY := 0; localY < bucket.Height; localY++ {
		for localX := 0; localX < bucket.Width; localX++ {
			globalX := bucket.X + localX
			globalY := bucket.Y + localY

			pixelColor := Color{X: 0, Y: 0, Z: 0}

			// Sample the pixel
			for sample := 0; sample < samplesPerPixel; sample++ {
				ray := r.camera.GetRay(globalX, globalY)
				pixelColor = pixelColor.Add(r.camera.RayColor(ray, maxDepth, r.world))
				GlobalRenderStats.SamplesComputed.Add(1)
			}

			// Average and gamma correct
			scale := 1.0 / float64(samplesPerPixel)
			pixelColor = pixelColor.Scale(scale)

			intensity := NewInterval(0.0, 0.999)
			bucketBuffer[localY*bucket.Width+localX] = color.RGBA{
				R: uint8(256 * intensity.Clamp(LinearToGamma(pixelColor.X))),
				G: uint8(256 * intensity.Clamp(LinearToGamma(pixelColor.Y))),
				B: uint8(256 * intensity.Clamp(LinearToGamma(pixelColor.Z))),
				A: 255,
			}

			GlobalRenderStats.PixelsRendered.Add(1)
		}
	}

	// Write bucket to framebuffer (synchronized)
	r.mu.Lock()
	for localY := 0; localY < bucket.Height; localY++ {
		for localX := 0; localX < bucket.Width; localX++ {
			globalX := bucket.X + localX
			globalY := bucket.Y + localY
			r.framebuffer.Set(globalX, globalY, bucketBuffer[localY*bucket.Width+localX])
		}
	}
	r.mu.Unlock()
}

func (r *BucketRenderer) Draw(screen *ebiten.Image) {
	r.mu.Lock()
	screen.WritePixels(r.framebuffer.Pix)
	r.mu.Unlock()

	// Draw render settings
	r.drawRenderSettings(screen)
}

func (r *BucketRenderer) drawRenderSettings(screen *ebiten.Image) {
	completedBuckets := int(r.completedCount.Load())
	progress := float64(completedBuckets) / float64(r.totalBuckets) * 100.0
	if r.completed {
		progress = 100.0
	}

	// Calculate elapsed time
	var elapsed time.Duration
	if r.completed {
		elapsed = r.renderEnd.Sub(r.renderStart)
	} else {
		elapsed = time.Since(r.renderStart)
	}

	// Draw black bar at bottom
	barHeight := 30
	barY := r.camera.ImageHeight - barHeight
	bgColor := color.RGBA{R: 0, G: 0, B: 0, A: 255}

	r.mu.Lock()
	for py := barY; py < r.camera.ImageHeight; py++ {
		for px := 0; px < r.camera.ImageWidth; px++ {
			r.framebuffer.Set(px, py, bgColor)
		}
	}
	r.mu.Unlock()

	textY := barY + 10
	spacing := 15

	var status string
	var passName string

	switch r.currentPass {
	case 0:
		passName = "PREVIEW"
	case 1:
		passName = "REFINING"
	case 2:
		passName = "FINAL"
	default:
		passName = "RENDERING"
	}

	if r.completed {
		status = "COMPLETED"
	} else {
		status = fmt.Sprintf("%s | Buckets: %d/%d", passName, completedBuckets, r.totalBuckets)
		// Workers display commented out
		// status = fmt.Sprintf("%s | Buckets: %d/%d | Workers: %d", passName, completedBuckets, r.totalBuckets, r.numWorkers)
	}

	statsText := fmt.Sprintf("%dx%d | SPP:%d | Depth:%d | Pass:%d/%d | %.1f%% | %s | %s",
		r.camera.ImageWidth,
		r.camera.ImageHeight,
		r.camera.SamplesPerPixel,
		r.camera.MaxDepth,
		min(r.currentPass+1, r.totalPasses), // Cap at totalPasses when completed
		r.totalPasses,
		progress,
		FormatDuration(elapsed),
		status,
	)

	ebitenutil.DebugPrintAt(screen, statsText, spacing, textY)
}

func (r *BucketRenderer) drawStatsToFramebuffer() {
	elapsed := r.renderEnd.Sub(r.renderStart)

	barHeight := 30
	barY := r.camera.ImageHeight - barHeight
	bgColor := color.RGBA{R: 0, G: 0, B: 0, A: 255}

	for py := barY; py < r.camera.ImageHeight; py++ {
		for px := 0; px < r.camera.ImageWidth; px++ {
			r.framebuffer.Set(px, py, bgColor)
		}
	}

	textColor := color.RGBA{R: 255, G: 255, B: 255, A: 255}
	face := text.NewGoXFace(basicfont.Face7x13)

	statsText := fmt.Sprintf("%dx%d | SPP:%d | Depth:%d | 100.0%% | %s | Workers: %d",
		r.camera.ImageWidth,
		r.camera.ImageHeight,
		r.camera.SamplesPerPixel,
		r.camera.MaxDepth,
		FormatDuration(elapsed),
		r.numWorkers,
	)

	tempImg := ebiten.NewImageFromImage(r.framebuffer)
	opts := &text.DrawOptions{}
	opts.GeoM.Translate(15, float64(barY+10)) // Position text in the bar (13px font height needs room)
	opts.ColorScale.ScaleWithColor(textColor)
	text.Draw(tempImg, statsText, face, opts)
	tempImg.ReadPixels(r.framebuffer.Pix)
}

func (r *BucketRenderer) Layout(w, h int) (int, int) {
	return r.camera.ImageWidth, r.camera.ImageHeight
}

func (r *BucketRenderer) SaveImage(filename string) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	file, err := os.Create(filename)
	if err != nil {
		return fmt.Errorf("error creating image file: %w", err)
	}
	defer func(file *os.File) {
		err := file.Close()
		if err != nil {
			fmt.Fprintf(os.Stderr, "ERROR: Could not close file '%s': %v\n", filename, err)
		}
	}(file)

	if err := png.Encode(file, r.framebuffer); err != nil {
		return fmt.Errorf("error encoding PNG: %w", err)
	}

	fmt.Printf("\n✓ Image saved to %s\n", filename)
	return nil
}

func (r *BucketRenderer) IsCompleted() bool {
	return r.completed
}

func (r *BucketRenderer) GetRenderDuration() time.Duration {
	if r.completed {
		return r.renderEnd.Sub(r.renderStart)
	}
	return time.Since(r.renderStart)
}
