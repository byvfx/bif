package rt

import (
	"fmt"
	"math"
)

// =============================================================================
// HDRI ENVIRONMENT MAP
// =============================================================================

// HDRIEnvironment represents an HDRI environment map with optional importance sampling
type HDRIEnvironment struct {
	image    *ImageLoader
	width    int
	height   int
	rotation float64 // Rotation in radians

	// Importance sampling data (optional)
	useImportanceSampling bool
	pdf                   []float64   // Per-pixel PDF (luminance-weighted)
	cdf                   []float64   // Cumulative distribution (flattened 2D)
	marginalCDF           []float64   // Marginal CDF for row selection
	conditionalCDFs       [][]float64 // Conditional CDF for each row
	totalPower            float64     // Total integrated luminance
}

// NewHDRIEnvironment creates a new HDRI environment map from a file
func NewHDRIEnvironment(filename string) *HDRIEnvironment {
	env := &HDRIEnvironment{
		useImportanceSampling: true,
		rotation:              0,
	}

	env.image = NewImageLoaderFromHDR(filename)
	if env.image.data == nil {
		fmt.Printf("Warning: Failed to load HDRI '%s'\n", filename)
		return env
	}

	env.width = env.image.Width()
	env.height = env.image.Height()

	// Build importance sampling distribution by default
	env.BuildDistribution()

	return env
}

// SetRotation sets the rotation of the environment map in degrees
func (env *HDRIEnvironment) SetRotation(degrees float64) {
	env.rotation = degrees * math.Pi / 180.0
}

// DisableImportanceSampling disables importance sampling (for debugging)
func (env *HDRIEnvironment) DisableImportanceSampling() {
	env.useImportanceSampling = false
	// Free memory
	env.pdf = nil
	env.cdf = nil
	env.marginalCDF = nil
	env.conditionalCDFs = nil
}

// IsValid returns true if the environment map loaded successfully
func (env *HDRIEnvironment) IsValid() bool {
	return env.image != nil && env.image.data != nil
}

// =============================================================================
// EQUIRECTANGULAR MAPPING
// =============================================================================

// DirectionToUV converts a 3D direction to equirectangular UV coordinates
func (env *HDRIEnvironment) DirectionToUV(dir Vec3) (u, v float64) {
	d := dir.Unit()

	// Spherical coordinates
	// phi (azimuth) from atan2, theta (elevation) from asin
	phi := math.Atan2(d.Z, d.X) // Range: [-π, π]
	theta := math.Asin(d.Y)     // Range: [-π/2, π/2]

	// Convert to UV [0, 1]
	u = 0.5 + phi/(2*math.Pi) // Longitude
	v = 0.5 - theta/math.Pi   // Latitude (inverted for top-down)

	// Apply rotation
	u = u + env.rotation/(2*math.Pi)

	// Wrap u to [0, 1]
	u = u - math.Floor(u)

	return u, v
}

// UVToDirection converts equirectangular UV coordinates to a 3D direction
func (env *HDRIEnvironment) UVToDirection(u, v float64) Vec3 {
	// Undo rotation
	u = u - env.rotation/(2*math.Pi)
	u = u - math.Floor(u)

	// Convert UV to spherical angles
	phi := (u - 0.5) * 2 * math.Pi // Azimuth: [-π, π]
	theta := (0.5 - v) * math.Pi   // Elevation: [-π/2, π/2]

	// Spherical to Cartesian
	cosTheta := math.Cos(theta)
	return Vec3{
		X: cosTheta * math.Cos(phi),
		Y: math.Sin(theta),
		Z: cosTheta * math.Sin(phi),
	}
}

// =============================================================================
// SAMPLING
// =============================================================================

// Sample returns the color for a given direction (with bilinear filtering)
func (env *HDRIEnvironment) Sample(dir Vec3) Color {
	if !env.IsValid() {
		// Return a default sky color if no HDRI
		return Color{X: 0.5, Y: 0.7, Z: 1.0}
	}

	u, v := env.DirectionToUV(dir)
	return env.image.PixelDataBilinear(u, v)
}

// SampleNearest returns the color using nearest-neighbor sampling (for debugging)
func (env *HDRIEnvironment) SampleNearest(dir Vec3) Color {
	if !env.IsValid() {
		return Color{X: 0.5, Y: 0.7, Z: 1.0}
	}

	u, v := env.DirectionToUV(dir)
	return env.image.PixelDataUV(u, v)
}

// =============================================================================
// IMPORTANCE SAMPLING
// =============================================================================

// BuildDistribution builds the importance sampling distribution
func (env *HDRIEnvironment) BuildDistribution() {
	if !env.IsValid() {
		return
	}

	width := env.width
	height := env.height
	totalPixels := width * height

	// Allocate PDF and CDFs
	env.pdf = make([]float64, totalPixels)
	env.marginalCDF = make([]float64, height+1)
	env.conditionalCDFs = make([][]float64, height)

	// Compute luminance-weighted PDF with sin(theta) correction
	env.totalPower = 0
	rowSums := make([]float64, height)

	for y := 0; y < height; y++ {
		// Compute sin(theta) for spherical correction
		// v = 0 is top (theta = π/2), v = 1 is bottom (theta = -π/2)
		// theta at this row
		v := (float64(y) + 0.5) / float64(height)
		theta := (0.5 - v) * math.Pi
		sinTheta := math.Cos(theta) // Note: cos(theta) because theta is elevation, not polar angle

		env.conditionalCDFs[y] = make([]float64, width+1)
		env.conditionalCDFs[y][0] = 0

		for x := 0; x < width; x++ {
			idx := y*width + x
			color := env.image.PixelData(x, y)

			// Luminance (Rec. 709)
			luminance := 0.2126*color.X + 0.7152*color.Y + 0.0722*color.Z

			// Weight by sin(theta) for uniform solid angle sampling
			weight := luminance * sinTheta
			if weight < 0 {
				weight = 0
			}

			env.pdf[idx] = weight
			rowSums[y] += weight
			env.totalPower += weight

			// Build conditional CDF for this row
			env.conditionalCDFs[y][x+1] = env.conditionalCDFs[y][x] + weight
		}
	}

	// Normalize conditional CDFs
	for y := 0; y < height; y++ {
		if rowSums[y] > 0 {
			for x := 0; x <= width; x++ {
				env.conditionalCDFs[y][x] /= rowSums[y]
			}
		}
	}

	// Build marginal CDF (for selecting rows)
	env.marginalCDF[0] = 0
	for y := 0; y < height; y++ {
		env.marginalCDF[y+1] = env.marginalCDF[y] + rowSums[y]
	}

	// Normalize marginal CDF
	if env.totalPower > 0 {
		for y := 0; y <= height; y++ {
			env.marginalCDF[y] /= env.totalPower
		}
		// Normalize PDF
		for i := range env.pdf {
			env.pdf[i] /= env.totalPower
		}
	}

	fmt.Printf("HDRI: Built importance sampling distribution (%dx%d, total power: %.2f)\n",
		width, height, env.totalPower)
}

// SampleDirection samples a direction from the HDRI using importance sampling
// Returns: direction, emission color, and PDF value
func (env *HDRIEnvironment) SampleDirection() (Vec3, Color, float64) {
	if !env.IsValid() || !env.useImportanceSampling || env.totalPower == 0 {
		// Fallback to uniform sphere sampling
		dir := RandomUnitVector()
		emission := env.Sample(dir)
		pdf := 1.0 / (4.0 * math.Pi)
		return dir, emission, pdf
	}

	// Sample row using marginal CDF (inverse transform sampling)
	xi1 := RandomDouble()
	y := env.searchCDF(env.marginalCDF, xi1)

	// Sample column using conditional CDF for selected row
	xi2 := RandomDouble()
	x := env.searchCDF(env.conditionalCDFs[y], xi2)

	// Convert to UV
	u := (float64(x) + 0.5) / float64(env.width)
	v := (float64(y) + 0.5) / float64(env.height)

	// Convert to direction
	dir := env.UVToDirection(u, v)

	// Get emission
	emission := env.image.PixelData(x, y)

	// Compute PDF
	pdf := env.PDF(dir)

	return dir, emission, pdf
}

// PDF returns the probability density for sampling a given direction
func (env *HDRIEnvironment) PDF(dir Vec3) float64 {
	if !env.IsValid() || !env.useImportanceSampling || env.totalPower == 0 {
		return 1.0 / (4.0 * math.Pi) // Uniform sphere
	}

	u, v := env.DirectionToUV(dir)

	// Get pixel coordinates
	x := int(u * float64(env.width))
	y := int(v * float64(env.height))

	// Clamp to valid range
	x = clamp(x, 0, env.width)
	y = clamp(y, 0, env.height)

	idx := y*env.width + x

	// Convert from per-pixel PDF to solid angle PDF
	// PDF_solidAngle = PDF_pixel * (width * height) / (4 * π)
	// But we also need to account for the sin(theta) factor
	theta := (0.5 - v) * math.Pi
	sinTheta := math.Cos(theta)
	if sinTheta < 1e-10 {
		sinTheta = 1e-10
	}

	// The stored PDF is already weighted by sin(theta), so we need to divide by it
	// and multiply by the Jacobian
	pdfSolidAngle := env.pdf[idx] * float64(env.width*env.height) / (2.0 * math.Pi * math.Pi * sinTheta)

	if pdfSolidAngle < 1e-10 {
		return 1e-10 // Avoid division by zero
	}

	return pdfSolidAngle
}

// searchCDF performs binary search on a CDF to find the sample index
func (env *HDRIEnvironment) searchCDF(cdf []float64, xi float64) int {
	n := len(cdf) - 1
	low, high := 0, n

	for low < high {
		mid := (low + high) / 2
		if cdf[mid+1] <= xi {
			low = mid + 1
		} else {
			high = mid
		}
	}

	// Clamp result
	if low >= n {
		low = n - 1
	}
	if low < 0 {
		low = 0
	}

	return low
}

// TotalPower returns the total integrated power of the environment map
func (env *HDRIEnvironment) TotalPower() float64 {
	return env.totalPower
}
