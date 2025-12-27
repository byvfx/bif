package rt

import (
	"math"
	"math/rand"
)

// Volume represents a constant density medium (fog, smoke, mist, etc.)
type Volume struct {
	boundary      Hittable
	negInvDensity float64
	phaseFunction Material
}

// NewVolume creates a volume with a texture
func NewVolume(boundary Hittable, density float64, tex Texture) *Volume {
	return &Volume{
		boundary:      boundary,
		negInvDensity: -1.0 / density,
		phaseFunction: NewIsotropic(tex),
	}
}

// NewVolumeFromColor creates a volume with a solid color
func NewVolumeFromColor(boundary Hittable, density float64, albedo Color) *Volume {
	return &Volume{
		boundary:      boundary,
		negInvDensity: -1.0 / density,
		phaseFunction: NewIsotropicFromColor(albedo),
	}
}

// Hit determines if a ray hits the volume
func (v *Volume) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	rec1 := &HitRecord{}
	rec2 := &HitRecord{}

	// First intersection with boundary
	if !v.boundary.Hit(r, UniverseInterval, rec1) {
		return false
	}

	// Second intersection (exiting boundary)
	if !v.boundary.Hit(r, NewInterval(rec1.T+0.0001, math.Inf(1)), rec2) {
		return false
	}

	// Clamp to ray interval
	if rec1.T < rayT.Min {
		rec1.T = rayT.Min
	}
	if rec2.T > rayT.Max {
		rec2.T = rayT.Max
	}

	if rec1.T >= rec2.T {
		return false
	}

	if rec1.T < 0 {
		rec1.T = 0
	}

	rayLength := r.Direction().Len()
	distanceInsideBoundary := (rec2.T - rec1.T) * rayLength
	hitDistance := v.negInvDensity * math.Log(rand.Float64())

	if hitDistance > distanceInsideBoundary {
		return false
	}

	rec.T = rec1.T + hitDistance/rayLength
	rec.P = r.At(rec.T)
	rec.Normal = Vec3{X: 1, Y: 0, Z: 0} // arbitrary
	rec.FrontFace = true                // arbitrary
	rec.Mat = v.phaseFunction

	return true
}

// BoundingBox returns the bounding box of the boundary
func (v *Volume) BoundingBox() AABB {
	return v.boundary.BoundingBox()
}
