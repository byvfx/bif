package rt

// ImageTexture uses an image as a texture
// C++: class image_texture : public texture
type ImageTexture struct {
	image *ImageLoader
}

// NewImageTexture creates a texture from an image file
// C++: image_texture(const char* filename) : image(filename) {}
func NewImageTexture(filename string) *ImageTexture {
	return &ImageTexture{
		image: NewImageLoaderFromFile(filename),
	}
}

// NewImageTextureFromImage creates a texture from an ImageLoader
func NewImageTextureFromImage(image *ImageLoader) *ImageTexture {
	return &ImageTexture{
		image: image,
	}
}

// Value returns the color at the given texture coordinates
// C++: color value(double u, double v, const point3& p) const override
func (tex *ImageTexture) Value(u, v float64, p Point3) Color {
	// If we have no texture data, return solid cyan as a debugging aid
	if tex.image.Height() <= 0 {
		return Color{X: 0, Y: 1, Z: 1}
	}

	// Clamp input texture coordinates to [0,1] x [1,0]
	u = clampFloat(u, 0.0, 1.0)
	v = 1.0 - clampFloat(v, 0.0, 1.0) // Flip V to image coordinates

	// Convert to integer pixel coordinates
	i := int(u * float64(tex.image.Width()))
	j := int(v * float64(tex.image.Height()))

	return tex.image.PixelData(i, j)
}

// clampFloat clamps a float value to [min, max]
func clampFloat(x, min, max float64) float64 {
	if x < min {
		return min
	}
	if x > max {
		return max
	}
	return x
}
