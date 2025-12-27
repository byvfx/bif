package rt

import "math"

// =============================================================================
// TRANSFORM BUILDER
// =============================================================================

type Transform struct {
	Scale    Vec3
	Rotation Vec3
	Position Vec3
}

func NewTransform() *Transform {
	return &Transform{
		Scale:    Vec3{X: 1, Y: 1, Z: 1},
		Rotation: Vec3{X: 0, Y: 0, Z: 0},
		Position: Vec3{X: 0, Y: 0, Z: 0},
	}
}

// Order: Scale -> Rotate (X->Y->Z) -> Translate
func (t *Transform) Apply(obj Hittable) Hittable {
	result := obj

	if t.Scale.X != 1.0 || t.Scale.Y != 1.0 || t.Scale.Z != 1.0 {
		result = NewScale(result, t.Scale)
	}

	if t.Rotation.X != 0 {
		result = Rx(result, t.Rotation.X)
	}
	if t.Rotation.Y != 0 {
		result = Ry(result, t.Rotation.Y)
	}
	if t.Rotation.Z != 0 {
		result = Rz(result, t.Rotation.Z)
	}

	if t.Position.X != 0 || t.Position.Y != 0 || t.Position.Z != 0 {
		result = NewTranslate(result, t.Position)
	}

	return result
}

func (t *Transform) SetScale(s Vec3) *Transform {
	t.Scale = s
	return t
}

func (t *Transform) SetUniformScale(s float64) *Transform {
	t.Scale = Vec3{X: s, Y: s, Z: s}
	return t
}

func (t *Transform) SetRotation(r Vec3) *Transform {
	t.Rotation = r
	return t
}

func (t *Transform) SetRotationY(angle float64) *Transform {
	t.Rotation.Y = angle
	return t
}

func (t *Transform) SetPosition(p Vec3) *Transform {
	t.Position = p
	return t
}

// =============================================================================
// INDIVIDUAL TRANSFORM PRIMITIVES
// =============================================================================

// Translate moves an object by an offset vector
type Translate struct {
	Obj    Hittable
	Offset Vec3
	bbox   AABB
}

func NewTranslate(obj Hittable, offset Vec3) *Translate {
	bbox := obj.BoundingBox().Translate(offset)
	return &Translate{
		Obj:    obj,
		Offset: offset,
		bbox:   bbox,
	}
}

func (t *Translate) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	offsetRay := NewRay(r.Origin().Sub(t.Offset), r.Direction(), r.Time())

	if !t.Obj.Hit(offsetRay, rayT, rec) {
		return false
	}

	rec.P = rec.P.Add(t.Offset)
	return true
}

func (t *Translate) BoundingBox() AABB {
	return t.bbox
}

// =============================================================================
// ROTATION TRANSFORMS
// =============================================================================

// RotateY rotates an object around the Y-axis
type RotateY struct {
	Obj      Hittable
	SinTheta float64
	CosTheta float64
	bbox     AABB
}

func Ry(obj Hittable, angle float64) *RotateY {
	radians := DegreesToRadians(angle)
	sinTheta := math.Sin(radians)
	cosTheta := math.Cos(radians)

	bbox := obj.BoundingBox()
	min := Point3{X: math.Inf(1), Y: math.Inf(1), Z: math.Inf(1)}
	max := Point3{X: math.Inf(-1), Y: math.Inf(-1), Z: math.Inf(-1)}

	for i := range 2 {
		for j := range 2 {
			for k := range 2 {
				x := float64(i)*bbox.X.Max + float64(1-i)*bbox.X.Min
				y := float64(j)*bbox.Y.Max + float64(1-j)*bbox.Y.Min
				z := float64(k)*bbox.Z.Max + float64(1-k)*bbox.Z.Min

				newX := cosTheta*x + sinTheta*z
				newZ := -sinTheta*x + cosTheta*z

				tester := Vec3{X: newX, Y: y, Z: newZ}

				min.X = math.Min(min.X, tester.X)
				max.X = math.Max(max.X, tester.X)
				min.Y = math.Min(min.Y, tester.Y)
				max.Y = math.Max(max.Y, tester.Y)
				min.Z = math.Min(min.Z, tester.Z)
				max.Z = math.Max(max.Z, tester.Z)
			}
		}
	}

	return &RotateY{
		Obj:      obj,
		SinTheta: sinTheta,
		CosTheta: cosTheta,
		bbox:     NewAABBFromPoints(min, max),
	}
}

func (ry *RotateY) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	origin := r.Origin()
	direction := r.Direction()

	origin.X = ry.CosTheta*r.Origin().X - ry.SinTheta*r.Origin().Z
	origin.Z = ry.SinTheta*r.Origin().X + ry.CosTheta*r.Origin().Z

	direction.X = ry.CosTheta*r.Direction().X - ry.SinTheta*r.Direction().Z
	direction.Z = ry.SinTheta*r.Direction().X + ry.CosTheta*r.Direction().Z

	rotatedRay := NewRay(origin, direction, r.Time())

	if !ry.Obj.Hit(rotatedRay, rayT, rec) {
		return false
	}

	p := rec.P
	p.X = ry.CosTheta*rec.P.X + ry.SinTheta*rec.P.Z
	p.Z = -ry.SinTheta*rec.P.X + ry.CosTheta*rec.P.Z

	normal := rec.Normal
	normal.X = ry.CosTheta*rec.Normal.X + ry.SinTheta*rec.Normal.Z
	normal.Z = -ry.SinTheta*rec.Normal.X + ry.CosTheta*rec.Normal.Z

	rec.P = p
	rec.Normal = normal

	return true
}

func (ry *RotateY) BoundingBox() AABB {
	return ry.bbox
}

// RotateX rotates an object around the X-axis
type RotateX struct {
	Obj      Hittable
	SinTheta float64
	CosTheta float64
	bbox     AABB
}

func Rx(obj Hittable, angle float64) *RotateX {
	radians := DegreesToRadians(angle)
	sinTheta := math.Sin(radians)
	cosTheta := math.Cos(radians)

	bbox := obj.BoundingBox()
	min := Point3{X: math.Inf(1), Y: math.Inf(1), Z: math.Inf(1)}
	max := Point3{X: math.Inf(-1), Y: math.Inf(-1), Z: math.Inf(-1)}

	for i := range 2 {
		for j := range 2 {
			for k := range 2 {
				x := float64(i)*bbox.X.Max + float64(1-i)*bbox.X.Min
				y := float64(j)*bbox.Y.Max + float64(1-j)*bbox.Y.Min
				z := float64(k)*bbox.Z.Max + float64(1-k)*bbox.Z.Min

				newY := cosTheta*y - sinTheta*z
				newZ := sinTheta*y + cosTheta*z

				tester := Vec3{X: x, Y: newY, Z: newZ}

				min.X = math.Min(min.X, tester.X)
				max.X = math.Max(max.X, tester.X)
				min.Y = math.Min(min.Y, tester.Y)
				max.Y = math.Max(max.Y, tester.Y)
				min.Z = math.Min(min.Z, tester.Z)
				max.Z = math.Max(max.Z, tester.Z)
			}
		}
	}

	return &RotateX{
		Obj:      obj,
		SinTheta: sinTheta,
		CosTheta: cosTheta,
		bbox:     NewAABBFromPoints(min, max),
	}
}

func (rx *RotateX) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	origin := r.Origin()
	direction := r.Direction()

	origin.Y = rx.CosTheta*r.Origin().Y - rx.SinTheta*r.Origin().Z
	origin.Z = rx.SinTheta*r.Origin().Y + rx.CosTheta*r.Origin().Z

	direction.Y = rx.CosTheta*r.Direction().Y - rx.SinTheta*r.Direction().Z
	direction.Z = rx.SinTheta*r.Direction().Y + rx.CosTheta*r.Direction().Z

	rotatedRay := NewRay(origin, direction, r.Time())

	if !rx.Obj.Hit(rotatedRay, rayT, rec) {
		return false
	}

	p := rec.P
	p.Y = rx.CosTheta*rec.P.Y - rx.SinTheta*rec.P.Z
	p.Z = rx.SinTheta*rec.P.Y + rx.CosTheta*rec.P.Z

	normal := rec.Normal
	normal.Y = rx.CosTheta*rec.Normal.Y - rx.SinTheta*rec.Normal.Z
	normal.Z = rx.SinTheta*rec.Normal.Y + rx.CosTheta*rec.Normal.Z

	rec.P = p
	rec.Normal = normal

	return true
}

func (rx *RotateX) BoundingBox() AABB {
	return rx.bbox
}

// RotateZ rotates an object around the Z-axis
type RotateZ struct {
	Obj      Hittable
	SinTheta float64
	CosTheta float64
	bbox     AABB
}

func Rz(obj Hittable, angle float64) *RotateZ {
	radians := DegreesToRadians(angle)
	sinTheta := math.Sin(radians)
	cosTheta := math.Cos(radians)

	bbox := obj.BoundingBox()
	min := Point3{X: math.Inf(1), Y: math.Inf(1), Z: math.Inf(1)}
	max := Point3{X: math.Inf(-1), Y: math.Inf(-1), Z: math.Inf(-1)}

	for i := range 2 {
		for j := range 2 {
			for k := range 2 {
				x := float64(i)*bbox.X.Max + float64(1-i)*bbox.X.Min
				y := float64(j)*bbox.Y.Max + float64(1-j)*bbox.Y.Min
				z := float64(k)*bbox.Z.Max + float64(1-k)*bbox.Z.Min

				newX := cosTheta*x - sinTheta*y
				newY := sinTheta*x + cosTheta*y

				tester := Vec3{X: newX, Y: newY, Z: z}

				min.X = math.Min(min.X, tester.X)
				max.X = math.Max(max.X, tester.X)
				min.Y = math.Min(min.Y, tester.Y)
				max.Y = math.Max(max.Y, tester.Y)
				min.Z = math.Min(min.Z, tester.Z)
				max.Z = math.Max(max.Z, tester.Z)
			}
		}
	}

	return &RotateZ{
		Obj:      obj,
		SinTheta: sinTheta,
		CosTheta: cosTheta,
		bbox:     NewAABBFromPoints(min, max),
	}
}

func (rz *RotateZ) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	origin := r.Origin()
	direction := r.Direction()

	origin.X = rz.CosTheta*r.Origin().X - rz.SinTheta*r.Origin().Y
	origin.Y = rz.SinTheta*r.Origin().X + rz.CosTheta*r.Origin().Y

	direction.X = rz.CosTheta*r.Direction().X - rz.SinTheta*r.Direction().Y
	direction.Y = rz.SinTheta*r.Direction().X + rz.CosTheta*r.Direction().Y

	rotatedRay := NewRay(origin, direction, r.Time())

	if !rz.Obj.Hit(rotatedRay, rayT, rec) {
		return false
	}

	p := rec.P
	p.X = rz.CosTheta*rec.P.X - rz.SinTheta*rec.P.Y
	p.Y = rz.SinTheta*rec.P.X + rz.CosTheta*rec.P.Y

	normal := rec.Normal
	normal.X = rz.CosTheta*rec.Normal.X - rz.SinTheta*rec.Normal.Y
	normal.Y = rz.SinTheta*rec.Normal.X + rz.CosTheta*rec.Normal.Y

	rec.P = p
	rec.Normal = normal

	return true
}

func (rz *RotateZ) BoundingBox() AABB {
	return rz.bbox
}

// =============================================================================
// SCALE TRANSFORM
// =============================================================================

// Scale applies non-uniform scaling to an object
type Scale struct {
	Obj       Hittable
	Factor    Vec3
	InvFactor Vec3
	bbox      AABB
}

func NewScale(obj Hittable, factor Vec3) *Scale {
	invFactor := Vec3{
		X: 1.0 / factor.X,
		Y: 1.0 / factor.Y,
		Z: 1.0 / factor.Z,
	}

	bbox := obj.BoundingBox()
	min := Point3{
		X: bbox.X.Min * factor.X,
		Y: bbox.Y.Min * factor.Y,
		Z: bbox.Z.Min * factor.Z,
	}
	max := Point3{
		X: bbox.X.Max * factor.X,
		Y: bbox.Y.Max * factor.Y,
		Z: bbox.Z.Max * factor.Z,
	}

	if min.X > max.X {
		min.X, max.X = max.X, min.X
	}
	if min.Y > max.Y {
		min.Y, max.Y = max.Y, min.Y
	}
	if min.Z > max.Z {
		min.Z, max.Z = max.Z, min.Z
	}

	return &Scale{
		Obj:       obj,
		Factor:    factor,
		InvFactor: invFactor,
		bbox:      NewAABBFromPoints(min, max),
	}
}

func NewUniformScale(obj Hittable, factor float64) *Scale {
	return NewScale(obj, Vec3{X: factor, Y: factor, Z: factor})
}

func (s *Scale) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	origin := Point3{
		X: r.Origin().X * s.InvFactor.X,
		Y: r.Origin().Y * s.InvFactor.Y,
		Z: r.Origin().Z * s.InvFactor.Z,
	}
	direction := Vec3{
		X: r.Direction().X * s.InvFactor.X,
		Y: r.Direction().Y * s.InvFactor.Y,
		Z: r.Direction().Z * s.InvFactor.Z,
	}

	scaledRay := NewRay(origin, direction, r.Time())

	if !s.Obj.Hit(scaledRay, rayT, rec) {
		return false
	}

	rec.P = Point3{
		X: rec.P.X * s.Factor.X,
		Y: rec.P.Y * s.Factor.Y,
		Z: rec.P.Z * s.Factor.Z,
	}

	normal := Vec3{
		X: rec.Normal.X * s.InvFactor.X,
		Y: rec.Normal.Y * s.InvFactor.Y,
		Z: rec.Normal.Z * s.InvFactor.Z,
	}
	rec.Normal = normal.Unit()

	return true
}

func (s *Scale) BoundingBox() AABB {
	return s.bbox
}
