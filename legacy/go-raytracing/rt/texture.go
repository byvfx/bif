package rt

import "math"

type Texture interface {
	Value(u, v float64, p Point3) Color
}

type SolidColor struct {
	Albedo Color
}

type CheckerTexture struct {
	invScale float64
	even     Texture
	odd      Texture
}

type NoiseTexture struct {
	noise *Perlin
	scale float64
}

func NewNoiseTexture(scale float64) *NoiseTexture {
	return &NoiseTexture{
		noise: NewPerlin(),
		scale: scale,
	}
}

func NewSolidColor(albedo Color) *SolidColor {
	return &SolidColor{
		Albedo: albedo,
	}
}

func NewSolidColorRGB(red, green, blue float64) *SolidColor {
	return &SolidColor{
		Albedo: Color{X: red, Y: green, Z: blue},
	}
}

func (s *SolidColor) Value(u, v float64, p Point3) Color {
	return s.Albedo
}

func NewCheckerTexture(scale float64, even, odd Texture) *CheckerTexture {
	return &CheckerTexture{
		invScale: 1.0 / scale,
		even:     even,
		odd:      odd,
	}
}

func NewCheckerTextureFromColors(scale float64, c1, c2 Color) *CheckerTexture {
	return NewCheckerTexture(
		scale,
		NewSolidColor(c1),
		NewSolidColor(c2),
	)
}

func (c *CheckerTexture) Value(u, v float64, p Point3) Color {
	// Add a small epsilon to avoid floating-point precision issues at boundaries
	const epsilon = 1e-4

	xInteger := int(math.Floor(c.invScale*p.X + epsilon))
	yInteger := int(math.Floor(c.invScale*p.Y + epsilon))
	zInteger := int(math.Floor(c.invScale*p.Z + epsilon))

	isEven := (xInteger+yInteger+zInteger)%2 == 0

	if isEven {
		return c.even.Value(u, v, p)
	}
	return c.odd.Value(u, v, p)
}

// TODO add option for turbulence
// TODO add different noise types and turbulences
func (tex *NoiseTexture) Value(u, v float64, p Point3) Color {
	s := tex.scale*p.Z + 10.0*tex.noise.Turb(p.Scale(tex.scale), 7)
	turbValue := 0.5 * (1.0 + math.Sin(s))
	return Color{X: 1, Y: 1, Z: 1}.Scale(turbValue)
}
