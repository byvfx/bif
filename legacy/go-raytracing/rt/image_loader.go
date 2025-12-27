package rt

import (
	"bufio"
	"fmt"
	"image"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"math"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

type ImageLoader struct {
	data             []Color
	imageWidth       int
	imageHeight      int
	bytesPerPixel    int
	bytesPerScanline int
	IsHDR            bool // True if loaded from HDR format
}

// NewImageLoader creates an empty image
func NewImageLoader() *ImageLoader {
	return &ImageLoader{
		bytesPerPixel: 3,
	}
}

func NewImageLoaderFromFile(filename string) *ImageLoader {
	img := NewImageLoader()

	path, err := FindAsset(filename, "images")
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: Could not resolve image file path '%s'.\n", filename)
		return img
	}
	img.Load(path)
	return img
}

// Load loads the linear (gamma=1) image data from the given file
func (img *ImageLoader) Load(filename string) bool {
	file, err := os.Open(filename)
	if err != nil {
		return false
	}
	defer file.Close()

	// Decode image (automatically detects format)
	decoded, _, err := image.Decode(file)
	if err != nil {
		return false
	}

	bounds := decoded.Bounds()
	img.imageWidth = bounds.Dx()
	img.imageHeight = bounds.Dy()
	img.bytesPerScanline = img.imageWidth * img.bytesPerPixel

	// Convert image to linear color space [0.0, 1.0]
	totalPixels := img.imageWidth * img.imageHeight
	img.data = make([]Color, totalPixels)

	for y := 0; y < img.imageHeight; y++ {
		for x := 0; x < img.imageWidth; x++ {
			r, g, b, _ := decoded.At(x+bounds.Min.X, y+bounds.Min.Y).RGBA()
			idx := y*img.imageWidth + x
			img.data[idx] = Color{
				X: LinearToGamma(float64(r) / 65535.0),
				Y: LinearToGamma(float64(g) / 65535.0),
				Z: LinearToGamma(float64(b) / 65535.0),
			}
		}
	}

	return true
}

func (img *ImageLoader) Width() int {
	if img.data == nil {
		return 0
	}
	return img.imageWidth
}

func (img *ImageLoader) Height() int {
	if img.data == nil {
		return 0
	}
	return img.imageHeight
}

func (img *ImageLoader) PixelData(x, y int) Color {
	// Return magenta if no image data
	if img.data == nil {
		return Color{X: 1.0, Y: 0.0, Z: 1.0}
	}

	// Clamp to valid range
	x = clamp(x, 0, img.imageWidth)
	y = clamp(y, 0, img.imageHeight)

	idx := y*img.imageWidth + x
	return img.data[idx]
}

// TODO do i need this clamp function i think i can just use math.Min math.Max
func clamp(x, low, high int) int {
	if x < low {
		return low
	}
	if x < high {
		return x
	}
	return high - 1
}

func FindAsset(filename string, assetType string) (string, error) {
	searchPaths := []string{
		filename,
		filepath.Join(assetType, filename),
		filepath.Join("assets", assetType, filename),
		filepath.Join("..", assetType, filename),
		filepath.Join("..", "assets", assetType, filename),
	}
	// Add hdri-specific paths
	if assetType == "hdri" {
		searchPaths = append(searchPaths,
			filepath.Join("hdri", filename),
			filepath.Join("assets", "hdri", filename),
			filepath.Join("..", "hdri", filename),
			filepath.Join("..", "assets", "hdri", filename),
		)
	}
	for _, path := range searchPaths {
		if _, err := os.Stat(path); err == nil {
			return path, nil
		}
	}
	return "", fmt.Errorf("asset file '%s' not found in any search paths", filename)
}

// =============================================================================
// HDR/RGBE FILE LOADING
// =============================================================================

// NewImageLoaderFromHDR creates an ImageLoader from a Radiance HDR file
func NewImageLoaderFromHDR(filename string) *ImageLoader {
	img := NewImageLoader()

	path, err := FindAsset(filename, "hdri")
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: Could not resolve HDR file path '%s'.\n", filename)
		return img
	}
	img.LoadHDR(path)
	return img
}

// LoadHDR loads a Radiance HDR (.hdr) file
func (img *ImageLoader) LoadHDR(filename string) bool {
	file, err := os.Open(filename)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: Could not open HDR file '%s': %v\n", filename, err)
		return false
	}
	defer file.Close()

	reader := bufio.NewReader(file)

	// Parse header
	width, height, err := img.parseHDRHeader(reader)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: Invalid HDR header in '%s': %v\n", filename, err)
		return false
	}

	img.imageWidth = width
	img.imageHeight = height
	img.bytesPerScanline = width * 4
	img.IsHDR = true

	// Allocate pixel data
	totalPixels := width * height
	img.data = make([]Color, totalPixels)

	// Read scanlines
	for y := 0; y < height; y++ {
		err := img.readHDRScanline(reader, y)
		if err != nil {
			fmt.Fprintf(os.Stderr, "ERROR: Failed to read HDR scanline %d: %v\n", y, err)
			return false
		}
	}

	return true
}

// parseHDRHeader parses the Radiance HDR file header
func (img *ImageLoader) parseHDRHeader(reader *bufio.Reader) (width, height int, err error) {
	// Read first line - should contain #? signature
	line, err := reader.ReadString('\n')
	if err != nil {
		return 0, 0, fmt.Errorf("failed to read header: %v", err)
	}

	if !strings.HasPrefix(line, "#?") {
		return 0, 0, fmt.Errorf("not a valid Radiance HDR file (missing #? signature)")
	}

	// Read header lines until we find an empty line
	for {
		line, err = reader.ReadString('\n')
		if err != nil {
			return 0, 0, fmt.Errorf("unexpected end of header: %v", err)
		}

		line = strings.TrimSpace(line)

		// Empty line signals end of header
		if line == "" {
			break
		}

		// We can ignore FORMAT, EXPOSURE, etc. for basic loading
		// Just continue reading until empty line
	}

	// Read resolution line: -Y height +X width (most common format)
	line, err = reader.ReadString('\n')
	if err != nil {
		return 0, 0, fmt.Errorf("failed to read resolution: %v", err)
	}

	line = strings.TrimSpace(line)
	parts := strings.Fields(line)

	if len(parts) != 4 {
		return 0, 0, fmt.Errorf("invalid resolution format: %s", line)
	}

	// Parse -Y height +X width format
	if parts[0] == "-Y" && parts[2] == "+X" {
		height, err = strconv.Atoi(parts[1])
		if err != nil {
			return 0, 0, fmt.Errorf("invalid height: %s", parts[1])
		}
		width, err = strconv.Atoi(parts[3])
		if err != nil {
			return 0, 0, fmt.Errorf("invalid width: %s", parts[3])
		}
	} else if parts[0] == "+X" && parts[2] == "-Y" {
		// Alternative format: +X width -Y height
		width, err = strconv.Atoi(parts[1])
		if err != nil {
			return 0, 0, fmt.Errorf("invalid width: %s", parts[1])
		}
		height, err = strconv.Atoi(parts[3])
		if err != nil {
			return 0, 0, fmt.Errorf("invalid height: %s", parts[3])
		}
	} else {
		return 0, 0, fmt.Errorf("unsupported resolution format: %s", line)
	}

	return width, height, nil
}

// readHDRScanline reads a single scanline from the HDR file
func (img *ImageLoader) readHDRScanline(reader *bufio.Reader, y int) error {
	width := img.imageWidth

	// Read first 4 bytes to determine encoding
	header := make([]byte, 4)
	_, err := io.ReadFull(reader, header)
	if err != nil {
		return fmt.Errorf("failed to read scanline header: %v", err)
	}

	// Check for new RLE format: starts with 2, 2, then 16-bit width
	if header[0] == 2 && header[1] == 2 {
		// New RLE format
		scanlineWidth := int(header[2])<<8 | int(header[3])
		if scanlineWidth != width {
			return fmt.Errorf("scanline width mismatch: expected %d, got %d", width, scanlineWidth)
		}
		return img.readRLEScanline(reader, y, width)
	}

	// Old format or uncompressed - first 4 bytes are first pixel
	// Convert first pixel
	img.rgbeToColor(header, y, 0)

	// Read remaining pixels as raw RGBE
	for x := 1; x < width; x++ {
		pixel := make([]byte, 4)
		_, err := io.ReadFull(reader, pixel)
		if err != nil {
			return fmt.Errorf("failed to read pixel: %v", err)
		}
		img.rgbeToColor(pixel, y, x)
	}

	return nil
}

// readRLEScanline reads an RLE-compressed scanline (new format)
func (img *ImageLoader) readRLEScanline(reader *bufio.Reader, y, width int) error {
	// Each component (R, G, B, E) is encoded separately
	scanline := make([][]byte, 4)
	for i := range scanline {
		scanline[i] = make([]byte, width)
	}

	// Read each component
	for component := 0; component < 4; component++ {
		x := 0
		for x < width {
			code, err := reader.ReadByte()
			if err != nil {
				return fmt.Errorf("failed to read RLE code: %v", err)
			}

			if code > 128 {
				// RLE run: next byte repeated (code - 128) times
				count := int(code) - 128
				value, err := reader.ReadByte()
				if err != nil {
					return fmt.Errorf("failed to read RLE value: %v", err)
				}
				for i := 0; i < count && x < width; i++ {
					scanline[component][x] = value
					x++
				}
			} else {
				// Raw run: read 'code' literal bytes
				count := int(code)
				for i := 0; i < count && x < width; i++ {
					value, err := reader.ReadByte()
					if err != nil {
						return fmt.Errorf("failed to read raw value: %v", err)
					}
					scanline[component][x] = value
					x++
				}
			}
		}
	}

	// Convert RGBE to Color for each pixel
	for x := 0; x < width; x++ {
		rgbe := []byte{scanline[0][x], scanline[1][x], scanline[2][x], scanline[3][x]}
		img.rgbeToColor(rgbe, y, x)
	}

	return nil
}

// rgbeToColor converts RGBE bytes to a linear HDR Color
func (img *ImageLoader) rgbeToColor(rgbe []byte, y, x int) {
	idx := y*img.imageWidth + x

	if rgbe[3] == 0 {
		// Zero exponent means black
		img.data[idx] = Color{X: 0, Y: 0, Z: 0}
		return
	}

	// RGBE to float conversion
	// value = (mantissa + 0.5) * 2^(exponent - 128 - 8)
	exponent := int(rgbe[3]) - 128 - 8
	scale := math.Ldexp(1.0, exponent)

	img.data[idx] = Color{
		X: (float64(rgbe[0]) + 0.5) * scale,
		Y: (float64(rgbe[1]) + 0.5) * scale,
		Z: (float64(rgbe[2]) + 0.5) * scale,
	}
}

// PixelDataUV returns pixel data with float UV coordinates (for bilinear filtering)
func (img *ImageLoader) PixelDataUV(u, v float64) Color {
	if img.data == nil {
		return Color{X: 1.0, Y: 0.0, Z: 1.0}
	}

	// Convert UV to pixel coordinates
	x := int(u * float64(img.imageWidth))
	y := int(v * float64(img.imageHeight))

	return img.PixelData(x, y)
}

// PixelDataBilinear returns bilinearly interpolated pixel data
func (img *ImageLoader) PixelDataBilinear(u, v float64) Color {
	if img.data == nil {
		return Color{X: 1.0, Y: 0.0, Z: 1.0}
	}

	// Convert to continuous pixel coordinates
	px := u*float64(img.imageWidth) - 0.5
	py := v*float64(img.imageHeight) - 0.5

	// Get integer and fractional parts
	x0 := int(math.Floor(px))
	y0 := int(math.Floor(py))
	x1 := x0 + 1
	y1 := y0 + 1

	fx := px - float64(x0)
	fy := py - float64(y0)

	// Wrap x coordinates for seamless horizontal tiling
	x0 = ((x0 % img.imageWidth) + img.imageWidth) % img.imageWidth
	x1 = ((x1 % img.imageWidth) + img.imageWidth) % img.imageWidth

	// Clamp y coordinates
	y0 = clamp(y0, 0, img.imageHeight)
	y1 = clamp(y1, 0, img.imageHeight)

	// Sample four corners
	c00 := img.PixelData(x0, y0)
	c10 := img.PixelData(x1, y0)
	c01 := img.PixelData(x0, y1)
	c11 := img.PixelData(x1, y1)

	// Bilinear interpolation
	c0 := c00.Scale(1 - fx).Add(c10.Scale(fx))
	c1 := c01.Scale(1 - fx).Add(c11.Scale(fx))

	return c0.Scale(1 - fy).Add(c1.Scale(fy))
}
