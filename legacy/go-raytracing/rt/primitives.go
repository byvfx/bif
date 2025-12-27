package rt

import "math"

func Box(a, b Point3, mat Material) Hittable {
	sides := NewHittableList()

	min := Point3{
		X: math.Min(a.X, b.X),
		Y: math.Min(a.Y, b.Y),
		Z: math.Min(a.Z, b.Z),
	}
	max := Point3{
		X: math.Max(a.X, b.X),
		Y: math.Max(a.Y, b.Y),
		Z: math.Max(a.Z, b.Z),
	}

	dx := Vec3{X: max.X - min.X, Y: 0, Z: 0}
	dy := Vec3{X: 0, Y: max.Y - min.Y, Z: 0}
	dz := Vec3{X: 0, Y: 0, Z: max.Z - min.Z}

	// Front face
	sides.Add(NewQuad(Point3{X: min.X, Y: min.Y, Z: max.Z}, dx, dy, mat))
	// Right face
	sides.Add(NewQuad(Point3{X: max.X, Y: min.Y, Z: max.Z}, dz.Neg(), dy, mat))
	// Back face
	sides.Add(NewQuad(Point3{X: max.X, Y: min.Y, Z: min.Z}, dx.Neg(), dy, mat))
	// Left face
	sides.Add(NewQuad(Point3{X: min.X, Y: min.Y, Z: min.Z}, dz, dy, mat))
	// Top face
	sides.Add(NewQuad(Point3{X: min.X, Y: max.Y, Z: max.Z}, dx, dz.Neg(), mat))
	// Bottom face
	sides.Add(NewQuad(Point3{X: min.X, Y: min.Y, Z: min.Z}, dx, dz, mat))

	return sides
}

func Pyramid(baseCenter Point3, baseSize, height float64, mat Material) Hittable {

	sides := NewHittableList()

	// Base quad
	sides.Add(NewQuad(
		Point3{X: baseCenter.X - baseSize/2, Y: baseCenter.Y, Z: baseCenter.Z - baseSize/2},
		Vec3{X: baseSize, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: baseSize},
		mat,
	))

	// Four triangular sides
	apex := Point3{X: baseCenter.X, Y: baseCenter.Y + height, Z: baseCenter.Z}
	halfSize := baseSize / 2

	corners := []Point3{
		{X: baseCenter.X + halfSize, Y: baseCenter.Y, Z: baseCenter.Z - halfSize},
		{X: baseCenter.X + halfSize, Y: baseCenter.Y, Z: baseCenter.Z + halfSize},
		{X: baseCenter.X - halfSize, Y: baseCenter.Y, Z: baseCenter.Z + halfSize},
		{X: baseCenter.X - halfSize, Y: baseCenter.Y, Z: baseCenter.Z - halfSize},
	}

	for i := range 4 {
		sides.Add(NewTriangle(
			corners[i],
			corners[(i+1)%4],
			apex,
			mat,
		))
	}
	return sides
}
