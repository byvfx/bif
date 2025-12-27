package rt

import (
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"time"

	"github.com/hajimehoshi/ebiten/v2"
	"github.com/hajimehoshi/ebiten/v2/ebitenutil"
	"github.com/hajimehoshi/ebiten/v2/text/v2"
	"golang.org/x/image/font/basicfont"
)

type ProgressiveRenderer struct {
	framebuffer *image.RGBA
	camera      *Camera
	world       Hittable
	currentRow  int
	completed   bool
	renderStart time.Time
	renderEnd   time.Time
}

func NewProgressiveRenderer(camera *Camera, world Hittable) *ProgressiveRenderer {
	framebuffer := image.NewRGBA(image.Rect(0, 0, camera.ImageWidth, camera.ImageHeight))
	return &ProgressiveRenderer{
		framebuffer: framebuffer,
		camera:      camera,
		world:       world,
		currentRow:  0,
		completed:   false,
		renderStart: time.Now(), // Start timing when renderer is created
	}
}
func (r *ProgressiveRenderer) Update() error {
	if r.currentRow < r.camera.ImageHeight {
		r.renderScanline(r.currentRow)
		r.currentRow++
		if r.currentRow >= r.camera.ImageHeight && !r.completed {
			r.completed = true
			r.renderEnd = time.Now()
			r.drawStatsToFramebuffer()
			_ = r.SaveImage("image.png")

			// Print render stats with actual render time
			renderDuration := r.renderEnd.Sub(r.renderStart)
			PrintRenderStats(renderDuration, r.camera.ImageWidth, r.camera.ImageHeight)
		}
	}
	return nil
}

func (r *ProgressiveRenderer) Draw(screen *ebiten.Image) {
	screen.WritePixels(r.framebuffer.Pix)

	// Draw render settings in lower left corner
	r.drawRenderSettings(screen)
}

func (r *ProgressiveRenderer) drawRenderSettings(screen *ebiten.Image) {
	// Calculate progress
	progress := float64(r.currentRow) / float64(r.camera.ImageHeight) * 100.0
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

	// Draw black bar at bottom (30 pixels tall)
	barHeight := 30
	barY := r.camera.ImageHeight - barHeight
	bgColor := color.RGBA{R: 0, G: 0, B: 0, A: 255}

	for py := barY; py < r.camera.ImageHeight; py++ {
		for px := 0; px < r.camera.ImageWidth; px++ {
			r.framebuffer.Set(px, py, bgColor)
		}
	}

	// Display stats in a single line across the bar
	textY := barY + 10 // Center text vertically in the 30px bar
	spacing := 15

	var status string
	if r.completed {
		status = "COMPLETED"
	} else {
		status = fmt.Sprintf("Scanline: %d/%d", r.currentRow, r.camera.ImageHeight)
	}

	statsText := fmt.Sprintf("%dx%d | SPP:%d | Depth:%d | %.1f%% | %s | %s",
		r.camera.ImageWidth,
		r.camera.ImageHeight,
		r.camera.SamplesPerPixel,
		r.camera.MaxDepth,
		progress,
		FormatDuration(elapsed),
		status,
	)

	ebitenutil.DebugPrintAt(screen, statsText, spacing, textY)
}

// drawStatsToFramebuffer draws the render statistics directly onto the framebuffer for saving
func (r *ProgressiveRenderer) drawStatsToFramebuffer() {
	elapsed := r.renderEnd.Sub(r.renderStart)

	// Draw black bar at bottom (30 pixels tall)
	barHeight := 30
	barY := r.camera.ImageHeight - barHeight
	bgColor := color.RGBA{R: 0, G: 0, B: 0, A: 255}

	for py := barY; py < r.camera.ImageHeight; py++ {
		for px := 0; px < r.camera.ImageWidth; px++ {
			r.framebuffer.Set(px, py, bgColor)
		}
	}

	// Prepare stats text
	textColor := color.RGBA{R: 255, G: 255, B: 255, A: 255}
	face := text.NewGoXFace(basicfont.Face7x13)

	statsText := fmt.Sprintf("%dx%d | SPP:%d | Depth:%d | 100.0%% | %s",
		r.camera.ImageWidth,
		r.camera.ImageHeight,
		r.camera.SamplesPerPixel,
		r.camera.MaxDepth,
		FormatDuration(elapsed),
	)

	// Create temporary ebiten.Image from framebuffer
	tempImg := ebiten.NewImageFromImage(r.framebuffer)

	opts := &text.DrawOptions{}
	opts.GeoM.Translate(15, float64(barY+10)) // Position text in the bar (13px font height needs room)
	opts.ColorScale.ScaleWithColor(textColor)
	text.Draw(tempImg, statsText, face, opts)

	// Copy the result back to framebuffer
	tempImg.ReadPixels(r.framebuffer.Pix)
}

func (r *ProgressiveRenderer) Layout(w, h int) (int, int) {
	return r.camera.ImageWidth, r.camera.ImageHeight
}
func (r *ProgressiveRenderer) renderScanline(j int) {
	for i := 0; i < r.camera.ImageWidth; i++ {
		pixelColor := Color{X: 0, Y: 0, Z: 0}

		for sample := 0; sample < r.camera.SamplesPerPixel; sample++ {
			ray := r.camera.GetRay(i, j)
			pixelColor = pixelColor.Add(r.camera.RayColor(ray, r.camera.MaxDepth, r.world))
		}

		scale := 1.0 / float64(r.camera.SamplesPerPixel)
		pixelColor = pixelColor.Scale(scale)

		// Clamp using Interval
		intensity := NewInterval(0.0, 0.999)
		r.framebuffer.Set(i, j, color.RGBA{
			R: uint8(256 * intensity.Clamp(LinearToGamma(pixelColor.X))),
			G: uint8(256 * intensity.Clamp(LinearToGamma(pixelColor.Y))),
			B: uint8(256 * intensity.Clamp(LinearToGamma(pixelColor.Z))),
			A: 255,
		})
	}
}

func (r *ProgressiveRenderer) SaveImage(filename string) error {
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

func (r *ProgressiveRenderer) IsCompleted() bool {
	return r.completed
}

// GetRenderDuration returns the actual render time
func (r *ProgressiveRenderer) GetRenderDuration() time.Duration {
	if r.completed {
		return r.renderEnd.Sub(r.renderStart)
	}
	return time.Since(r.renderStart)
}
