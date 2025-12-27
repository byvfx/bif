package rt

import "math"

// =============================================================================
// MATERIAL INTERFACES
// =============================================================================

type Material interface {
	Scatter(rIn Ray, rec *HitRecord, attenuation *Color, scattered *Ray) bool
	Emitted(u, v float64, p Point3) Color
}

// PDFEvaluator provides probability density function evaluation for importance sampling
type PDFEvaluator interface {
	PDF(wi, wo, normal Vec3) float64
}

type MaterialProperties struct {
	isPureSpecular bool
	isEmissive     bool
	CanUseNEE      bool
}

type MaterialInfo interface {
	Properties() MaterialProperties
}

// =============================================================================
// LAMBERTIAN (DIFFUSE)
// =============================================================================

type Lambertian struct {
	tex Texture
}

func NewLambertian(albedo Color) *Lambertian {
	return &Lambertian{
		tex: NewSolidColor(albedo),
	}
}

func NewLambertianTexture(tex Texture) *Lambertian {
	return &Lambertian{
		tex: tex,
	}
}

func (l *Lambertian) Properties() MaterialProperties {
	return MaterialProperties{
		isPureSpecular: false,
		isEmissive:     false,
		CanUseNEE:      true,
	}
}

func (l *Lambertian) Scatter(rIn Ray, rec *HitRecord, attenuation *Color, scattered *Ray) bool {
	scatterDirection := rec.Normal.Add(RandomUnitVector())

	if scatterDirection.NearZero() {
		scatterDirection = rec.Normal
	}

	*scattered = NewRay(rec.P, scatterDirection, rIn.Time())
	*attenuation = l.tex.Value(rec.U, rec.V, rec.P)

	return true
}

func (l *Lambertian) PDF(wi, wo, normal Vec3) float64 {
	cosTheta := Dot(normal, wo)
	if cosTheta < 0 {
		return 0
	}
	return cosTheta / math.Pi
}

func (l *Lambertian) Emitted(u, v float64, p Point3) Color {
	return Color{X: 0, Y: 0, Z: 0}
}

// =============================================================================
// METAL (REFLECTIVE)
// =============================================================================

type Metal struct {
	Albedo Color
	Fuzz   float64
}

func NewMetal(albedo Color, fuzz float64) *Metal {
	if fuzz > 1 {
		fuzz = 1
	}
	return &Metal{
		Albedo: albedo,
		Fuzz:   fuzz,
	}
}

func (m *Metal) Properties() MaterialProperties {
	// Metals should NOT use NEE/MIS - light sampling adds incorrect diffuse appearance.
	// Even glossy metals should use pure BRDF sampling for correct specular reflections.
	// Only very rough metals (fuzz > 0.4) might benefit from NEE, but we disable it
	// entirely for metals to maintain proper metallic appearance.
	return MaterialProperties{
		isPureSpecular: m.Fuzz < 0.1, // More generous threshold for "pure specular"
		isEmissive:     false,
		CanUseNEE:      false, // Metals should never use NEE - causes diffuse appearance
	}
}

func (m *Metal) Scatter(rIn Ray, rec *HitRecord, attenuation *Color, scattered *Ray) bool {
	reflected := Reflect(rIn.Direction(), rec.Normal)
	reflected = reflected.Unit().Add(RandomUnitVector().Scale(m.Fuzz))
	*scattered = NewRay(rec.P, reflected, rIn.Time())
	*attenuation = m.Albedo
	return Dot(scattered.Direction(), rec.Normal) > 0
}

func (m *Metal) PDF(wi, wo, normal Vec3) float64 {
	if m.Fuzz == 0 {
		return 0 // Perfect specular reflection is a delta distribution
	}

	// For fuzzy metal, use a Phong-like distribution
	reflected := Reflect(wi.Scale(-1), normal)
	cosAlpha := Dot(reflected, wo)
	if cosAlpha < 0 {
		return 0
	}

	// Simplified Phong exponent based on fuzz (lower fuzz = higher exponent)
	exponent := (1.0 - m.Fuzz) * 50.0
	return (exponent + 1) / (2 * math.Pi) * math.Pow(cosAlpha, exponent)
}

func (m *Metal) Emitted(u, v float64, p Point3) Color {
	return Color{X: 0, Y: 0, Z: 0}
}

// =============================================================================
// DIELECTRIC (GLASS/REFRACTIVE)
// =============================================================================

type Dielectric struct {
	RefractionIndex float64
}

func NewDielectric(refractionIndex float64) *Dielectric {
	return &Dielectric{
		RefractionIndex: refractionIndex,
	}
}

func (d *Dielectric) Properties() MaterialProperties {
	return MaterialProperties{
		isPureSpecular: true,
		isEmissive:     false,
		CanUseNEE:      false,
	}
}

func (d *Dielectric) Scatter(rIn Ray, rec *HitRecord, attenuation *Color, scattered *Ray) bool {
	*attenuation = Color{X: 1.0, Y: 1.0, Z: 1.0}

	var ri float64
	if rec.FrontFace {
		ri = 1.0 / d.RefractionIndex
	} else {
		ri = d.RefractionIndex
	}
	unitDirection := rIn.Direction().Unit()
	cosTheta := math.Min(Dot(unitDirection.Neg(), rec.Normal), 1.0)
	sinTheta := math.Sqrt(1.0 - cosTheta*cosTheta)
	cannotRefract := ri*sinTheta > 1.0

	var direction Vec3

	if cannotRefract || reflectance(cosTheta, ri) > RandomDouble() {
		direction = Reflect(unitDirection, rec.Normal)
	} else {
		direction = Refract(unitDirection, rec.Normal, ri)
	}
	*scattered = NewRay(rec.P, direction, rIn.Time())

	return true
}

func (d *Dielectric) PDF(wi, wo, normal Vec3) float64 {
	return 0 // Delta BSDF, cannot be importance sampled
}

func (d *Dielectric) Emitted(u, v float64, p Point3) Color {
	return Color{X: 0, Y: 0, Z: 0}
}

// =============================================================================
// DIFFUSE LIGHT (EMISSIVE)
// =============================================================================

type DiffuseLight struct {
	tex Texture
}

func NewDiffuseLight(tex Texture) *DiffuseLight {
	return &DiffuseLight{
		tex: tex,
	}
}

func NewDiffuseLightColor(emit Color) *DiffuseLight {
	return &DiffuseLight{
		tex: NewSolidColor(emit),
	}
}

func (dl *DiffuseLight) Properties() MaterialProperties {
	return MaterialProperties{
		isPureSpecular: false,
		isEmissive:     true,
		CanUseNEE:      false,
	}
}

func (dl *DiffuseLight) Scatter(rIn Ray, rec *HitRecord, attenuation *Color, scattered *Ray) bool {
	return false
}

func (dl *DiffuseLight) PDF(wi, wo, normal Vec3) float64 {
	return 0 // Emissive only, no scattering
}

func (dl *DiffuseLight) Emitted(u, v float64, p Point3) Color {
	return dl.tex.Value(u, v, p)
}

// =============================================================================
// ISOTROPIC (FOR VOLUMES)
// =============================================================================

// Isotropic scatters light uniformly in all directions (for volumes like fog, smoke)
type Isotropic struct {
	tex Texture
}

// NewIsotropic creates an isotropic material from a texture
func NewIsotropic(tex Texture) *Isotropic {
	return &Isotropic{tex: tex}
}

// NewIsotropicFromColor creates an isotropic material from a solid color
func NewIsotropicFromColor(albedo Color) *Isotropic {
	return &Isotropic{tex: NewSolidColor(albedo)}
}

func (i *Isotropic) Properties() MaterialProperties {
	return MaterialProperties{
		isPureSpecular: false,
		isEmissive:     false,
		CanUseNEE:      false,
	}
}

// Scatter scatters the ray in a random direction (uniform sphere)
func (i *Isotropic) Scatter(rIn Ray, rec *HitRecord, attenuation *Color, scattered *Ray) bool {
	*scattered = NewRay(rec.P, RandomUnitVector(), rIn.Time())
	*attenuation = i.tex.Value(rec.U, rec.V, rec.P)
	return true
}

func (i *Isotropic) PDF(wi, wo, normal Vec3) float64 {
	return 1.0 / (4.0 * math.Pi) // Uniform sphere PDF
}

func (i *Isotropic) Emitted(u, v float64, p Point3) Color {
	return Color{X: 0, Y: 0, Z: 0}
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

func reflectance(cosine, refractionIndex float64) float64 {
	r0 := (1 - refractionIndex) / (1 + refractionIndex)
	r0 = r0 * r0
	return r0 + (1-r0)*math.Pow(1-cosine, 5)
}
