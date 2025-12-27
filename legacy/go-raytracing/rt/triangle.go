package rt

import "math"

// Triangle represents a triangle primitive
// RUST PORT NOTE: This can be a simple struct with Copy/Clone
// Consider storing edge vectors to avoid recomputation in Hit()
type Triangle struct {
	v0, v1, v2 Point3 // Vertices
	normal     Vec3   // Face normal (pre-computed and unit length)
	mat        Material
	bbox       AABB
	D          float64 // Plane constant (unused - can be removed)
}

// NewTriangle creates a new triangle from three vertices
func NewTriangle(v0, v1, v2 Point3, mat Material) *Triangle {
	// Calculate edges
	edge1 := v1.Sub(v0)
	edge2 := v2.Sub(v0)

	// Calculate normal using cross product
	normal := Cross(edge1, edge2).Unit()

	tri := &Triangle{
		v0:     v0,
		v1:     v1,
		v2:     v2,
		normal: normal,
		mat:    mat,
	}

	// Calculate plane constant
	tri.D = Dot(tri.normal, v0)

	// Create bounding box encompassing all vertices
	minX := math.Min(v0.X, math.Min(v1.X, v2.X))
	maxX := math.Max(v0.X, math.Max(v1.X, v2.X))
	minY := math.Min(v0.Y, math.Min(v1.Y, v2.Y))
	maxY := math.Max(v0.Y, math.Max(v1.Y, v2.Y))
	minZ := math.Min(v0.Z, math.Min(v1.Z, v2.Z))
	maxZ := math.Max(v0.Z, math.Max(v1.Z, v2.Z))

	tri.bbox = NewAABBFromPoints(
		Point3{X: minX, Y: minY, Z: minZ},
		Point3{X: maxX, Y: maxY, Z: maxZ},
	)

	return tri
}

func (t *Triangle) BoundingBox() AABB {
	return t.bbox
}

// Hit uses the Möller-Trumbore algorithm for ray-triangle intersection
func (t *Triangle) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	edge1 := t.v1.Sub(t.v0)
	edge2 := t.v2.Sub(t.v0)

	h := Cross(r.Direction(), edge2)
	a := Dot(edge1, h)

	// Ray is parallel to triangle
	if math.Abs(a) < 1e-8 {
		return false
	}

	f := 1.0 / a
	s := r.Origin().Sub(t.v0)
	u := f * Dot(s, h)

	// Check if intersection is outside triangle (u parameter)
	if u < 0.0 || u > 1.0 {
		return false
	}

	q := Cross(s, edge1)
	v := f * Dot(r.Direction(), q)

	// Check if intersection is outside triangle (v parameter)
	if v < 0.0 || u+v > 1.0 {
		return false
	}

	// Calculate t parameter
	hitT := f * Dot(edge2, q)

	if !rayT.Contains(hitT) {
		return false
	}

	// Valid intersection found
	rec.T = hitT
	rec.P = r.At(hitT)
	rec.Mat = t.mat
	rec.SetFaceNormal(r, t.normal)

	// Set barycentric UV coordinates
	rec.U = u
	rec.V = v

	return true
}
