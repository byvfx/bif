package rt

import "math"

type Quad struct {
	Q      Point3
	u      Vec3
	v      Vec3
	w      Vec3
	mat    Material
	bbox   AABB
	normal Vec3
	D      float64
}

func NewQuad(Q Point3, u, v Vec3, mat Material) *Quad {
	quad := &Quad{
		Q:   Q,
		u:   u,
		v:   v,
		mat: mat,
	}

	n := Cross(u, v)
	quad.normal = n.Unit()

	// Calculate the plane constant D
	quad.D = Dot(quad.normal, Q)
	quad.w = n.Scale(1.0 / Dot(n, n))

	quad.setBoundingBox()
	return quad
}
func (q *Quad) setBoundingBox() {
	bboxDiagonal1 := NewAABBFromPoints(q.Q, q.Q.Add(q.u).Add(q.v))
	bboxDiagonal2 := NewAABBFromPoints(q.Q.Add(q.u), q.Q.Add(q.v))
	q.bbox = NewAABBFromBoxes(bboxDiagonal1, bboxDiagonal2)
}

func (q *Quad) BoundingBox() AABB {
	return q.bbox
}

func (q *Quad) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	denom := Dot(q.normal, r.Direction())

	if math.Abs(denom) < 1e-8 {
		return false
	}

	// Return false if the hit point parameter t is outside the ray interval
	t := (q.D - Dot(q.normal, r.Origin())) / denom
	if !rayT.Contains(t) {
		return false
	}

	intersection := r.At(t)
	planarHitptVector := intersection.Sub(q.Q)
	alpha := Dot(q.w, Cross(planarHitptVector, q.v))
	beta := Dot(q.w, Cross(q.u, planarHitptVector))

	if !q.isInterior(alpha, beta, rec) {
		return false
	}
	rec.T = t
	rec.P = intersection
	rec.Mat = q.mat
	rec.SetFaceNormal(r, q.normal)

	return true
}
func (q *Quad) isInterior(a, b float64, rec *HitRecord) bool {
	unitInterval := NewInterval(0, 1)

	// Given the hit point in plane coordinates, return false if it is outside the
	// primitive, otherwise set the hit record UV coordinates and return true
	if !unitInterval.Contains(a) || !unitInterval.Contains(b) {
		return false
	}

	rec.U = a
	rec.V = b
	return true
}

// SamplePoint returns a random point on the quad surface
func (q *Quad) SamplePoint() Point3 {
	// Random barycentric coordinates [0,1] x [0,1]
	alpha := RandomDouble()
	beta := RandomDouble()
	return q.Q.Add(q.u.Scale(alpha)).Add(q.v.Scale(beta))
}

// Area returns the surface area of the quad
func (q *Quad) Area() float64 {
	return Cross(q.u, q.v).Len()
}

// PdfValue returns the probability density function value for sampling this quad
func (q *Quad) PdfValue(origin Point3, direction Vec3) float64 {
	rec := &HitRecord{}
	if !q.Hit(NewRay(origin, direction, 0), NewInterval(0.001, math.Inf(1)), rec) {
		return 0
	}

	distanceSquared := rec.T * rec.T * direction.Len2()
	cosine := math.Abs(Dot(direction, rec.Normal) / direction.Len())

	return distanceSquared / (cosine * q.Area())
}
