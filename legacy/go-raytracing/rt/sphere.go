package rt

import "math"

// Sphere represents a sphere object that can be hit by rays
type Sphere struct {
	Center Ray
	Radius float64
	Mat    Material
	bbox   AABB
}

// NewSphere creates a new sphere with the given center and radius and material
func NewSphere(center Point3, radius float64, mat Material) *Sphere {
	rvec := Vec3{X: radius, Y: radius, Z: radius}
	return &Sphere{
		Center: NewRay(center, Vec3{X: 0, Y: 0, Z: 0}, 0),
		Radius: math.Max(0, radius),
		Mat:    mat,
		bbox:   NewAABBFromPoints(center.Sub(rvec), center.Add(rvec)),
	}
}

func NewMovingSphere(center1, center2 Point3, radius float64, mat Material) *Sphere {
	rvec := Vec3{X: radius, Y: radius, Z: radius}
	velocity := center2.Sub(center1)

	// Bounding box at time=0 (center.at(0) returns center1)
	box1 := NewAABBFromPoints(center1.Sub(rvec), center1.Add(rvec))

	// Bounding box at time=1 (center.at(1) returns center2)
	box2 := NewAABBFromPoints(center2.Sub(rvec), center2.Add(rvec))

	// Combined bounding box for entire motion
	bbox := NewAABBFromBoxes(box1, box2)

	return &Sphere{
		Center: NewRay(center1, velocity, 0),
		Radius: math.Max(0, radius),
		Mat:    mat,
		bbox:   bbox,
	}
}

func (s *Sphere) BoundingBox() AABB {
	return s.bbox
}

func (s *Sphere) SphereCenter(time float64) Point3 {
	return s.Center.At(time)
}

func getSphereUV(p Point3) (u, v float64) {
	theta := math.Acos(-p.Y)
	phi := math.Atan2(-p.Z, p.X) + math.Pi
	u = phi / (2 * math.Pi)
	v = theta / math.Pi
	return u, v
}

// Hit implements the Hittable interface for Sphere

func (s *Sphere) Hit(r Ray, rayT Interval, rec *HitRecord) bool {

	sphereCenter := s.SphereCenter(r.Time())

	oc := sphereCenter.Sub(r.Origin())
	a := r.Direction().Len2()
	h := Dot(r.Direction(), oc)
	c := oc.Len2() - s.Radius*s.Radius

	discriminant := h*h - a*c
	if discriminant < 0 {
		return false
	}

	sqrtd := math.Sqrt(discriminant)

	root := (h - sqrtd) / a
	if !rayT.Surrounds(root) {
		root = (h + sqrtd) / a
		if !rayT.Surrounds(root) {
			return false
		}
	}

	rec.T = root
	rec.P = r.At(rec.T)
	outwardNormal := rec.P.Sub(sphereCenter).Div(s.Radius)
	rec.SetFaceNormal(r, outwardNormal)
	rec.U, rec.V = getSphereUV(outwardNormal)
	rec.Mat = s.Mat
	return true
}
