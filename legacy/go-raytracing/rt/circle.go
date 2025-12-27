package rt

import "math"

type Circle struct {
	center Point3
	normal Vec3
	radius float64
	mat    Material
	bbox   AABB
	D      float64
}

func NewCircle(center Point3, normal Vec3, radius float64, mat Material) *Circle {
	circle := &Circle{
		normal: normal.Unit(),
		center: center,
		radius: radius,
		mat:    mat,
	}
	circle.D = Dot(circle.normal, center)

	rvec := Vec3{X: radius, Y: radius, Z: radius}
	circle.bbox = NewAABBFromPoints(
		center.Sub(rvec),
		center.Add(rvec),
	)
	return circle
}
func (c *Circle) BoundingBox() AABB {
	return c.bbox
}

func (c *Circle) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	denom := Dot(c.normal, r.Direction())

	if math.Abs(denom) < 1e-8 {
		return false
	}

	t := (c.D - Dot(c.normal, r.Origin())) / denom
	if !rayT.Contains(t) {
		return false
	}

	intersection := r.At(t)
	distanceFromCenter := intersection.Sub(c.center).Len()

	if distanceFromCenter > c.radius {
		return false
	}

	rec.T = t
	rec.P = intersection
	rec.Mat = c.mat
	rec.SetFaceNormal(r, c.normal)

	var u, v Vec3
	if math.Abs(c.normal.Y) > 0.9 {
		u = Cross(Vec3{X: 1, Y: 0, Z: 0}, c.normal).Unit()
	} else {
		u = Cross(Vec3{X: 0, Y: 1, Z: 0}, c.normal).Unit()
	}
	v = Cross(c.normal, u)

	localPoint := intersection.Sub(c.center)
	x := Dot(localPoint, u)
	y := Dot(localPoint, v)

	rec.U = (x/c.radius + 1.0) * 0.5
	rec.V = (y/c.radius + 1.0) * 0.5

	return true
}
