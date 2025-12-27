package rt

import "math"

type Plane struct {
	Point  Point3
	Normal Vec3
	Mat    Material
	bbox   AABB
}

func NewPlane(point Point3, normal Vec3, mat Material) *Plane {
	return &Plane{
		Point:  point,
		Normal: normal.Unit(),
		Mat:    mat,
		bbox:   NewAABBFromIntervals(UniverseInterval, UniverseInterval, UniverseInterval),
	}
}
func (p *Plane) BoundingBox() AABB {
	return p.bbox
}

func (p *Plane) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	denom := Dot(p.Normal, r.Direction())

	if math.Abs(denom) < 1e-8 {
		return false
	}

	t := Dot(p.Point.Sub(r.Origin()), p.Normal) / denom
	if !rayT.Surrounds(t) {
		return false
	}
	rec.T = t
	rec.P = r.At(t)

	rec.SetFaceNormal(r, p.Normal)

	rec.Mat = p.Mat
	return true
}
