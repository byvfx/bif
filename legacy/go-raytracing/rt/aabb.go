package rt

import "math"

type AABB struct {
	X, Y, Z Interval
}

var (
	EmptyAABB    = NewAABB()
	UniverseAABB = NewAABBFromIntervals(
		UniverseInterval,
		UniverseInterval,
		UniverseInterval,
	)
)

func NewAABB() AABB {
	return AABB{
		X: NewEmptyInterval(),
		Y: NewEmptyInterval(),
		Z: NewEmptyInterval(),
	}
}

func NewAABBFromIntervals(x, y, z Interval) AABB {
	box := AABB{X: x, Y: y, Z: z}
	box.padToMinimums()
	return box
}

func NewAABBFromPoints(a, b Point3) AABB {
	box := AABB{
		X: NewInterval(math.Min(a.X, b.X), math.Max(a.X, b.X)),
		Y: NewInterval(math.Min(a.Y, b.Y), math.Max(a.Y, b.Y)),
		Z: NewInterval(math.Min(a.Z, b.Z), math.Max(a.Z, b.Z)),
	}
	box.padToMinimums()
	return box
}

func NewAABBFromBoxes(box0, box1 AABB) AABB {
	return AABB{
		X: NewIntervalFromIntervals(box0.X, box1.X),
		Y: NewIntervalFromIntervals(box0.Y, box1.Y),
		Z: NewIntervalFromIntervals(box0.Z, box1.Z),
	}
}
func (box AABB) AxisInterval(n int) Interval {
	if n == 1 {
		return box.Y
	}
	if n == 2 {
		return box.Z
	}
	return box.X
}

func (box AABB) Hit(r Ray, rayT Interval) bool {
	rayOrig := r.Origin()
	rayDir := r.Direction()

	// Unrolled loop for X, Y, Z axes - avoids switch overhead in hot path
	// X axis
	adinv := 1.0 / rayDir.X
	t0 := (box.X.Min - rayOrig.X) * adinv
	t1 := (box.X.Max - rayOrig.X) * adinv
	if adinv < 0 {
		t0, t1 = t1, t0
	}
	if t0 > rayT.Min {
		rayT.Min = t0
	}
	if t1 < rayT.Max {
		rayT.Max = t1
	}
	if rayT.Max <= rayT.Min {
		return false
	}

	// Y axis
	adinv = 1.0 / rayDir.Y
	t0 = (box.Y.Min - rayOrig.Y) * adinv
	t1 = (box.Y.Max - rayOrig.Y) * adinv
	if adinv < 0 {
		t0, t1 = t1, t0
	}
	if t0 > rayT.Min {
		rayT.Min = t0
	}
	if t1 < rayT.Max {
		rayT.Max = t1
	}
	if rayT.Max <= rayT.Min {
		return false
	}

	// Z axis
	adinv = 1.0 / rayDir.Z
	t0 = (box.Z.Min - rayOrig.Z) * adinv
	t1 = (box.Z.Max - rayOrig.Z) * adinv
	if adinv < 0 {
		t0, t1 = t1, t0
	}
	if t0 > rayT.Min {
		rayT.Min = t0
	}
	if t1 < rayT.Max {
		rayT.Max = t1
	}
	if rayT.Max <= rayT.Min {
		return false
	}

	return true
}
func (box *AABB) padToMinimums() {
	delta := 0.0001
	if box.X.Size() < delta {
		box.X = box.X.Expand(delta)
	}
	if box.Y.Size() < delta {
		box.Y = box.Y.Expand(delta)
	}
	if box.Z.Size() < delta {
		box.Z = box.Z.Expand(delta)
	}
}

func (box AABB) Translate(offset Vec3) AABB {
	return NewAABBFromIntervals(
		box.X.Add(offset.X),
		box.Y.Add(offset.Y),
		box.Z.Add(offset.Z),
	)
}

// LongestAxis returns the index (0=X, 1=Y, 2=Z) of the axis with the longest extent
func (box AABB) LongestAxis() int {
	xSize := box.X.Size()
	ySize := box.Y.Size()
	zSize := box.Z.Size()

	if xSize > ySize && xSize > zSize {
		return 0
	} else if ySize > zSize {
		return 1
	}
	return 2
}

// Centroid returns the center point of the bounding box
func (box AABB) Centroid() Vec3 {
	return Vec3{
		X: (box.X.Min + box.X.Max) * 0.5,
		Y: (box.Y.Min + box.Y.Max) * 0.5,
		Z: (box.Z.Min + box.Z.Max) * 0.5,
	}
}
