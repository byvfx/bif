package rt

type SceneConfig struct {
	GroundColor      Color
	SphereGridBounds struct{ MinA, MaxA, MinB, MaxB int }
	MovingSphereProb float64
	LambertProb      float64
	DielectricProb   float64
	MetalProb        float64
	LargeSpheresY    float64
}

func DefaultSceneConfig() SceneConfig {
	return SceneConfig{
		GroundColor: Color{X: 0.5, Y: 0.5, Z: 0.5},
		SphereGridBounds: struct {
			MinA int
			MaxA int
			MinB int
			MaxB int
		}{-10, 10, -10, 10},
		MovingSphereProb: 0,
		LambertProb:      0.3,
		DielectricProb:   0.3,
		MetalProb:        0.3,
		LargeSpheresY:    1.0,
	}
}

func RandomScene() (*HittableList, *Camera) {
	return RandomSceneWithConfig(DefaultSceneConfig())
}

func RandomSceneWithConfig(config SceneConfig) (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	groundChecker := NewCheckerTextureFromColors(
		0.32,
		config.GroundColor,
		Color{X: 0.9, Y: 0.9, Z: 0.9},
	)
	groundMaterial := NewLambertianTexture(groundChecker)

	// =============================================================================
	// GEOMETRY
	// =============================================================================
	world.Add(NewPlane(Point3{X: 0, Y: 0, Z: -1}, Vec3{X: 0, Y: 1, Z: 0}, groundMaterial))

	for a := config.SphereGridBounds.MinA; a < config.SphereGridBounds.MaxA; a++ {
		for b := config.SphereGridBounds.MinB; b < config.SphereGridBounds.MaxB; b++ {
			chooseMat := RandomDouble()
			center := Point3{
				X: float64(a) + 0.9*RandomDouble(),
				Y: 0.2,
				Z: float64(b) + 0.9*RandomDouble(),
			}

			if center.Sub(Point3{X: 4, Y: 0.2, Z: 0}).Len() > 0.9 {
				addRandomSphere(world, center, chooseMat, config)
			}
		}
	}
	addLargeSpheres(world, config.LargeSpheresY)

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(1200, 16.0/9.0).
		SetQuality(500, 50).
		SetPosition(
			Point3{X: 13, Y: 2, Z: 3},
			Point3{X: 0, Y: 0, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(20, 0.6, 10.0).
		EnableSkyGradient(true).
		Build()

	return world, camera
}
func addRandomSphere(world *HittableList, center Point3, chooseMat float64, config SceneConfig) {
	var sphereMaterial Material

	lambertThreshold := config.LambertProb
	metalThreshold := config.MetalProb + lambertThreshold
	dielectricThreshold := config.DielectricProb + metalThreshold

	if chooseMat < lambertThreshold {
		albedo := Color{
			X: RandomDouble() * RandomDouble(),
			Y: RandomDouble() * RandomDouble(),
			Z: RandomDouble() * RandomDouble(),
		}
		sphereMaterial = NewLambertian(albedo)
		center2 := center.Add(Vec3{X: 0, Y: RandomDoubleRange(0, 0.5), Z: 0})
		world.Add(NewMovingSphere(center, center2, 0.2, sphereMaterial))
	} else if chooseMat < metalThreshold {

		albedo := Color{
			X: 0.5 + RandomDouble()*0.5,
			Y: 0.5 + RandomDouble()*0.5,
			Z: 0.5 + RandomDouble()*0.5,
		}
		fuzz := RandomDouble() * 0.5
		sphereMaterial = NewMetal(albedo, fuzz)
		world.Add(NewSphere(center, 0.2, sphereMaterial))
	} else if chooseMat < dielectricThreshold {

		sphereMaterial = NewDielectric(1.5)
		world.Add(NewSphere(center, 0.2, sphereMaterial))
	}
}

func addLargeSpheres(world *HittableList, y float64) {
	// Glass sphere (center)
	material1 := NewDielectric(1.5)
	world.Add(NewSphere(Point3{X: 0, Y: y, Z: 0}, 1.0, material1))

	// Diffuse sphere (left)
	material2 := NewLambertian(Color{X: 0.4, Y: 0.2, Z: 0.1})
	world.Add(NewSphere(Point3{X: -4, Y: y, Z: 0}, 1.0, material2))

	// Metal sphere (right)
	material3 := NewMetal(Color{X: 0.7, Y: 0.6, Z: 0.5}, 0.0)
	world.Add(NewSphere(Point3{X: 4, Y: y, Z: 0}, 1.0, material3))
}

func CheckeredSpheresScene() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	checker := NewCheckerTextureFromColors(
		0.32,
		Color{X: 0.2, Y: 0.3, Z: 0.1},
		Color{X: 0.9, Y: 0.9, Z: 0.9},
	)
	checkerMaterial := NewLambertianTexture(checker)

	// =============================================================================
	// GEOMETRY
	// =============================================================================
	// Bottom sphere (at y=-10)
	world.Add(NewSphere(Point3{X: 0, Y: -10, Z: 0}, 10, checkerMaterial))

	// Top sphere (at y=10)
	world.Add(NewSphere(Point3{X: 0, Y: 10, Z: 0}, 10, checkerMaterial))

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(600, 16.0/9.0).
		SetQuality(100, 50).
		SetPosition(
			Point3{X: 13, Y: 2, Z: 3},
			Point3{X: 0, Y: 0, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(20, 0, 10).
		EnableSkyGradient(true).
		Build()

	return world, camera
}

func SimpleScene() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	materialGround := NewLambertian(Color{X: 0.8, Y: 0.8, Z: 0.0})
	materialCenter := NewLambertian(Color{X: 0.1, Y: 0.2, Z: 0.5})
	materialLeft := NewDielectric(1.5)
	materialBubble := NewDielectric(1.0 / 1.5)
	materialRight := NewMetal(Color{X: 0.8, Y: 0.6, Z: 0.2}, 0.0)

	// =============================================================================
	// GEOMETRY
	// =============================================================================
	world.Add(NewPlane(Point3{X: 0, Y: -0.5, Z: -1}, Vec3{X: 0, Y: 1, Z: 0}, materialGround))
	world.Add(NewSphere(Point3{X: 0, Y: 0, Z: -1}, 0.5, materialCenter))
	world.Add(NewSphere(Point3{X: -1, Y: 0, Z: -1}, 0.5, materialLeft))
	world.Add(NewSphere(Point3{X: -1, Y: 0, Z: -1}, 0.4, materialBubble))
	world.Add(NewSphere(Point3{X: 1, Y: 0, Z: -1}, 0.5, materialRight))

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(400, 16.0/9.0).
		SetQuality(100, 50).
		SetPosition(
			Point3{X: 0, Y: 0, Z: 2},
			Point3{X: 0, Y: 0, Z: -1},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(90, 0, 10).
		EnableSkyGradient(true).
		Build()

	return world, camera
}
func EarthScene() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	earthTexture := NewImageTexture("earthmap.jpg")
	earthSurface := NewLambertianTexture(earthTexture)

	// =============================================================================
	// GEOMETRY
	// =============================================================================
	globe := NewSphere(Point3{X: 0, Y: 0, Z: 0}, 2, earthSurface)
	world.Add(globe)

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(800, 16.0/9.0).
		SetQuality(100, 50).
		SetPosition(
			Point3{X: 0, Y: 0, Z: 12},
			Point3{X: 0, Y: 0, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(20, 0, 10).
		EnableSkyGradient(true).
		Build()

	return world, camera
}
func PerlinSpheresScene() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	pertext := NewNoiseTexture(4.0)
	perlMaterial := NewLambertianTexture(pertext)

	// =============================================================================
	// GEOMETRY
	// =============================================================================
	world.Add(NewSphere(Point3{X: 0, Y: 2, Z: 0}, 2, perlMaterial))
	world.Add(NewPlane(Point3{X: 0, Y: 0, Z: -1}, Vec3{X: 0, Y: 1, Z: 0}, perlMaterial))

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(600, 16.0/9.0).
		SetQuality(100, 50).
		SetPosition(
			Point3{X: 13, Y: 2, Z: -10},
			Point3{X: 0, Y: 1.5, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(20, 0, 10).
		EnableSkyGradient(true).
		Build()

	return world, camera
}
func QuadsScene() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	leftRed := NewLambertian(Color{X: 1.0, Y: 0.2, Z: 0.2})
	backGreen := NewLambertian(Color{X: 0.2, Y: 1.0, Z: 0.2})
	rightBlue := NewLambertian(Color{X: 0.2, Y: 0.2, Z: 1.0})
	upperOrange := NewLambertian(Color{X: 1.0, Y: 0.5, Z: 0.0})
	lowerTeal := NewLambertian(Color{X: 0.2, Y: 0.8, Z: 0.8})

	// =============================================================================
	// GEOMETRY
	// =============================================================================
	world.Add(NewQuad(Point3{X: -3, Y: -2, Z: 5}, Vec3{X: 0, Y: 0, Z: -4}, Vec3{X: 0, Y: 4, Z: 0}, leftRed))
	world.Add(NewQuad(Point3{X: -2, Y: -2, Z: 0}, Vec3{X: 4, Y: 0, Z: 0}, Vec3{X: 0, Y: 4, Z: 0}, backGreen))
	world.Add(NewQuad(Point3{X: 3, Y: -2, Z: 1}, Vec3{X: 0, Y: 0, Z: 4}, Vec3{X: 0, Y: 4, Z: 0}, rightBlue))
	world.Add(NewQuad(Point3{X: -2, Y: 3, Z: 1}, Vec3{X: 4, Y: 0, Z: 0}, Vec3{X: 0, Y: 0, Z: 4}, upperOrange))
	world.Add(NewQuad(Point3{X: -2, Y: -3, Z: 5}, Vec3{X: 4, Y: 0, Z: 0}, Vec3{X: 0, Y: 0, Z: -4}, lowerTeal))

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(400, 1.0).
		SetQuality(100, 50).
		SetPosition(
			Point3{X: 0, Y: 0, Z: 9},
			Point3{X: 0, Y: 0, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(80, 0, 10).
		EnableSkyGradient(true).
		Build()

	return world, camera
}

func PrimitivesScene() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	redMat := NewLambertian(Color{X: 0.8, Y: 0.1, Z: 0.1})
	greenMat := NewLambertian(Color{X: 0.1, Y: 0.8, Z: 0.1})
	blueMat := NewLambertian(Color{X: 0.1, Y: 0.1, Z: 0.8})
	metalMat := NewMetal(Color{X: 1.0, Y: 1.0, Z: 1.0}, 0)
	lightMat := NewDiffuseLight(NewSolidColor(Color{X: 2, Y: 2, Z: 2}))

	checkerMat := NewLambertianTexture(NewCheckerTextureFromColors(1.0,
		Color{X: 0.0, Y: 0.0, Z: 0.0},
		Color{X: 0.9, Y: 0.9, Z: 0.9}))

	// =============================================================================
	// GROUND PLANE (Infinite Plane with Checker Pattern)
	// =============================================================================
	world.Add(NewPlane(Point3{X: 0, Y: -1, Z: 0}, Vec3{X: 0, Y: 1, Z: 0}, checkerMat))

	// =============================================================================
	// LEFT: Circle (Disk)
	// =============================================================================
	world.Add(NewCircle(
		Point3{X: -5, Y: 0, Z: 0},
		Vec3{X: 0, Y: 1, Z: 0}, // Normal pointing at camera
		0.9,
		redMat,
	))

	// =============================================================================
	// CENTER-LEFT: Triangle Pyramid
	// =============================================================================
	pyramidX := -2.5
	pyramidHeight := 1.8
	pyramidBase := 1.4

	world.Add(Pyramid(Point3{X: pyramidX, Y: -1, Z: 0}, pyramidBase, pyramidHeight, greenMat))

	// =============================================================================
	// CENTER: Glass Sphere
	// =============================================================================
	world.Add(NewSphere(Point3{X: 0, Y: 0.6, Z: 0}, 0.8, NewDielectric(1.5)))

	// =============================================================================
	// CENTER-RIGHT: Quad Cube
	// =============================================================================
	cubeX := 2.5
	cubeSize := 1.0

	world.Add(Box(
		Point3{X: cubeX - cubeSize/2, Y: -1, Z: -cubeSize / 2},
		Point3{X: cubeX + cubeSize/2, Y: -1 + cubeSize, Z: cubeSize / 2},
		blueMat,
	))

	// =============================================================================
	// OVERHEAD LIGHT SOURCE
	// =============================================================================
	areaLight := NewQuad(
		Point3{X: -2, Y: 5, Z: -2},
		Vec3{X: 4, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 4},
		lightMat,
	)
	world.Add(areaLight)

	// =============================================================================
	// RIGHT: Metal Sphere
	// =============================================================================
	world.Add(NewSphere(Point3{X: 5, Y: 0.6, Z: 0}, 0.8, metalMat))

	camera := NewCameraBuilder().
		SetResolution(800, 16.0/9.0).
		SetQuality(300, 25).
		SetPosition(
			Point3{X: 0, Y: 2, Z: 10},
			Point3{X: 0, Y: 0, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(45, 0, 10).
		SetBackground(Color{0, 0, 0}).
		EnableSkyGradient(true).
		AddLight(areaLight).
		Build()

	return world, camera
}

// ==================================================================================
// HDRI Test Scene - Demonstrates HDRI environment mapping
// ==================================================================================
func HDRITestScene() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	glassMat := NewDielectric(1.5)
	mirrorMat := NewMetal(Color{X: 1.0, Y: 1.0, Z: 1.0}, 0.0)
	goldMat := NewMetal(Color{X: 1.0, Y: 0.84, Z: 0.0}, 0.1)
	groundMat := NewLambertianTexture(NewCheckerTextureFromColors(0.5,
		Color{X: 0.1, Y: 0.1, Z: 0.1},
		Color{X: 0.9, Y: 0.9, Z: 0.9}))

	// =============================================================================
	// GROUND PLANE
	// =============================================================================
	world.Add(NewPlane(Point3{X: 0, Y: 0, Z: 0}, Vec3{X: 0, Y: 1, Z: 0}, groundMat))

	// =============================================================================
	// SPHERES - Glass, Mirror, and Gold to show HDRI reflections
	// =============================================================================
	// Center: Glass sphere
	world.Add(NewSphere(Point3{X: 0, Y: 1, Z: 0}, 1.0, glassMat))

	// Left: Mirror sphere
	world.Add(NewSphere(Point3{X: -2.5, Y: 1, Z: 0}, 1.0, mirrorMat))

	// Right: Gold sphere
	world.Add(NewSphere(Point3{X: 2.5, Y: 1, Z: 0}, 1.0, goldMat))

	// Small glass spheres in front
	world.Add(NewSphere(Point3{X: -1.2, Y: 0.4, Z: 2}, 0.4, glassMat))
	world.Add(NewSphere(Point3{X: 1.2, Y: 0.4, Z: 2}, 0.4, glassMat))

	// =============================================================================
	// CAMERA with HDRI Environment
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(800, 16.0/9.0).
		SetQuality(200, 20).
		SetPosition(
			Point3{X: 0, Y: 2.5, Z: 8},
			Point3{X: 0, Y: 1, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(40, 0, 10).
		SetEnvironmentMap("assets/hdri/abandoned_hall_01_1k.hdr"). // Load HDRI environment
		SetEnvironmentRotation(0).
		SetPhantomHDRI(true). // Adjust rotation as needed
		Build()

	return world, camera
}

// ==================================================================================
// Cornell Box Scene
// ==================================================================================
func CornellBoxScene() (*HittableList, *Camera) {
	world := NewHittableList()

	whiteMat := NewLambertian(Color{X: 0.73, Y: 0.73, Z: 0.73})
	redMat := NewLambertian(Color{X: 0.65, Y: 0.05, Z: 0.05})
	greenMat := NewLambertian(Color{X: 0.12, Y: 0.45, Z: 0.15})
	lightMat := NewDiffuseLight(NewSolidColor(Color{X: 3, Y: 3, Z: 3}))

	areaLight := NewQuad(
		Point3{X: 213, Y: 554, Z: 227},
		Vec3{X: 130, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 105},
		lightMat,
	)
	world.Add(areaLight)

	// Walls
	world.Add(NewQuad(
		Point3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		greenMat,
	))
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		redMat,
	))
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		whiteMat,
	))
	world.Add(NewQuad(
		Point3{X: 555, Y: 555, Z: 555},
		Vec3{X: -555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: -555},
		whiteMat,
	))
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 555},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		whiteMat,
	))
	// Light already added above as areaLight

	//Boxes
	box1 := Box(
		Point3{X: 0, Y: 0, Z: 0},
		Point3{X: 165, Y: 330, Z: 165},
		whiteMat,
	)
	box1Xform := NewTransform().
		SetScale(Vec3{X: 1.0, Y: 1.0, Z: 1.0}).
		SetRotationY(15).
		SetPosition(Vec3{X: 265, Y: 0, Z: 295}).
		Apply(box1)
	world.Add(box1Xform)

	box2 := Box(
		Point3{X: 0, Y: 0, Z: 0},
		Point3{X: 165, Y: 165, Z: 165},
		whiteMat,
	)
	box2Xform := NewTransform().
		SetScale(Vec3{X: 1.0, Y: 1.0, Z: 1.0}).
		SetRotationY(-18).
		SetPosition(Vec3{X: 130, Y: 0, Z: 65}).
		Apply(box2)
	world.Add(box2Xform)

	// =============================================================================
	// ENCOMPASSING FOG VOLUME
	// =============================================================================
	// Create a box that fills the entire Cornell box interior
	fogBoundary := Box(
		Point3{X: 0, Y: 0, Z: 0},
		Point3{X: 555, Y: 555, Z: 555},
		whiteMat, // Material doesn't matter for the boundary, it's just for the volume
	)
	world.Add(NewVolumeFromColor(fogBoundary, 0.001, Color{X: 1, Y: 1, Z: 1}))

	camera := NewCameraBuilder().
		SetResolution(600, 1.0).
		SetQuality(500, 5).
		SetPosition(
			Point3{X: 278, Y: 278, Z: -800},
			Point3{X: 278, Y: 278, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(40, 0, 10).
		SetBackground(Color{0, 0, 0}).
		AddLight(areaLight).
		Build()

	return world, camera
}

func GlossyMetalTest() (*HittableList, *Camera) {
	world := NewHittableList()

	// Ground
	groundMat := NewLambertian(Color{X: 0.5, Y: 0.5, Z: 0.5})
	world.Add(NewPlane(Point3{X: 0, Y: 0, Z: 0}, Vec3{X: 0, Y: 1, Z: 0}, groundMat))

	// Three spheres with increasing glossiness
	smoothMetal := NewMetal(Color{X: 0.8, Y: 0.6, Z: 0.2}, 0.0)
	mediumMetal := NewMetal(Color{X: 0.8, Y: 0.6, Z: 0.2}, 0.2)
	roughMetal := NewMetal(Color{X: 0.8, Y: 0.6, Z: 0.2}, 0.5)

	world.Add(NewSphere(Point3{X: -2.5, Y: 1, Z: 0}, 1.0, smoothMetal))
	world.Add(NewSphere(Point3{X: 0, Y: 1, Z: 0}, 1.0, mediumMetal))
	world.Add(NewSphere(Point3{X: 2.5, Y: 1, Z: 0}, 1.0, roughMetal))

	// Area light
	lightMat := NewDiffuseLightColor(Color{X: 4, Y: 4, Z: 4})
	areaLight := NewQuad(
		Point3{X: -2, Y: 5, Z: -2},
		Vec3{X: 4, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 4},
		lightMat,
	)
	world.Add(areaLight)

	camera := NewCameraBuilder().
		SetResolution(640, 16.0/9.0).
		SetQuality(100, 10).
		SetPosition(
			Point3{X: 0, Y: 2, Z: 10},
			Point3{X: 0, Y: 1, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(40, 0, 10).
		SetBackground(BackgroundBlack).
		AddLight(areaLight).
		Build()

	return world, camera
}

func CornellBoxGlossy() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	whiteMat := NewLambertian(Color{X: 0.73, Y: 0.73, Z: 0.73})
	redMat := NewLambertian(Color{X: 0.65, Y: 0.05, Z: 0.05})
	greenMat := NewLambertian(Color{X: 0.12, Y: 0.45, Z: 0.15})

	// Glossy metals with different roughness
	goldShiny := NewMetal(Color{X: 1.0, Y: 0.84, Z: 0.0}, 0.05)     // Polished gold
	goldBrushed := NewMetal(Color{X: 1.0, Y: 0.84, Z: 0.0}, 0.15)   // Brushed gold
	silverRough := NewMetal(Color{X: 0.95, Y: 0.95, Z: 0.98}, 0.25) // Rough silver

	// Glass sphere for variety
	glassMat := NewDielectric(1.5)

	// Bright area light
	lightMat := NewDiffuseLightColor(Color{X: 15, Y: 15, Z: 15})

	// =============================================================================
	// CORNELL BOX WALLS
	// =============================================================================

	// Green wall (right)
	world.Add(NewQuad(
		Point3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		greenMat,
	))

	// Red wall (left)
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		redMat,
	))

	// White floor
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		whiteMat,
	))

	// White ceiling
	world.Add(NewQuad(
		Point3{X: 555, Y: 555, Z: 555},
		Vec3{X: -555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: -555},
		whiteMat,
	))

	// White back wall
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 555},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		whiteMat,
	))

	// =============================================================================
	// AREA LIGHT
	// =============================================================================
	areaLight := NewQuad(
		Point3{X: 213, Y: 554, Z: 227},
		Vec3{X: 130, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 105},
		lightMat,
	)
	world.Add(areaLight)

	// =============================================================================
	// GLOSSY METAL SPHERES
	// =============================================================================

	// Back row: Three gold spheres with increasing roughness
	world.Add(NewSphere(Point3{X: 150, Y: 100, Z: 400}, 100, goldShiny))   // Shiny
	world.Add(NewSphere(Point3{X: 278, Y: 100, Z: 400}, 100, goldBrushed)) // Brushed
	world.Add(NewSphere(Point3{X: 410, Y: 100, Z: 400}, 100, silverRough)) // Rough silver

	// Front: Large glass sphere
	world.Add(NewSphere(Point3{X: 278, Y: 130, Z: 180}, 130, glassMat))

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(600, 1.0).
		SetQuality(200, 5).
		SetPosition(
			Point3{X: 278, Y: 278, Z: -800},
			Point3{X: 278, Y: 200, Z: 200},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(40, 0, 10).
		SetBackground(BackgroundBlack).
		AddLight(areaLight).
		Build()

	return world, camera
}

// CornellBoxLucy - Cornell Box with Lucy statue
func CornellBoxLucy() (*HittableList, *Camera) {
	world := NewHittableList()

	whiteMat := NewLambertian(Color{X: 0.73, Y: 0.73, Z: 0.73})
	redMat := NewLambertian(Color{X: 0.65, Y: 0.05, Z: 0.05})
	greenMat := NewLambertian(Color{X: 0.12, Y: 0.45, Z: 0.15})
	lightMat := NewDiffuseLight(NewSolidColor(Color{X: 15, Y: 15, Z: 15}))

	areaLight := NewQuad(
		Point3{X: 213, Y: 554, Z: 227},
		Vec3{X: 130, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 105},
		lightMat,
	)
	world.Add(areaLight)

	// Walls
	world.Add(NewQuad(
		Point3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		greenMat,
	))
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		redMat,
	))
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		whiteMat,
	))
	world.Add(NewQuad(
		Point3{X: 555, Y: 555, Z: 555},
		Vec3{X: -555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: -555},
		whiteMat,
	))
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 555},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		whiteMat,
	))

	// Load Lucy model ONCE and reuse with transforms (major performance optimization)
	lucyMat := NewLambertian(Color{X: 0.9, Y: 0.9, Z: 0.9})

	// Lucy bounds: [-465, -0.025, -267] to [465, 1597, 267]
	// Scale to fit in Cornell box (height ~400 units)
	scale := 0.15 // Smaller to fit multiple instances

	// Load the mesh once and build BVH once
	lucyMesh, err := LoadOBJ("assets/models/lucy_low.obj", lucyMat)
	if err != nil {
		panic(err)
	}

	// Create 10 angel instances in a grid pattern using transform wrappers
	positions := []struct {
		pos Vec3
		rot float64
	}{
		{Vec3{X: 150, Y: 0, Z: 150}, 45},
		{Vec3{X: 400, Y: 0, Z: 150}, 315},
		{Vec3{X: 150, Y: 0, Z: 400}, 135},
		{Vec3{X: 400, Y: 0, Z: 400}, 225},
		{Vec3{X: 278, Y: 0, Z: 278}, 0},
		{Vec3{X: 100, Y: 0, Z: 278}, 90},
		{Vec3{X: 450, Y: 0, Z: 278}, 270},
		{Vec3{X: 278, Y: 0, Z: 100}, 180},
		{Vec3{X: 278, Y: 0, Z: 450}, 0},
		{Vec3{X: 200, Y: 0, Z: 350}, 60},
	}

	// Reuse the same mesh with different transforms (10x faster loading)
	for _, inst := range positions {
		lucyInstance := NewTransform().
			SetScale(Vec3{X: scale, Y: scale, Z: scale}).
			SetRotationY(inst.rot).
			SetPosition(inst.pos).
			Apply(lucyMesh)

		world.Add(lucyInstance)
	}

	camera := NewCameraBuilder().
		SetResolution(600, 1.0).
		SetQuality(50, 5).
		SetPosition(
			Point3{X: 278, Y: 278, Z: -800},
			Point3{X: 278, Y: 278, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(40, 0, 10).
		SetBackground(Color{0, 0, 0}).
		AddLight(areaLight).
		Build()

	return world, camera
}

// CornellSmoke - Cornell Box with volumetric fog/smoke boxes
func CornellSmoke() (*HittableList, *Camera) {
	world := NewHittableList()

	// =============================================================================
	// MATERIALS
	// =============================================================================
	whiteMat := NewLambertian(Color{X: 0.73, Y: 0.73, Z: 0.73})
	redMat := NewLambertian(Color{X: 0.65, Y: 0.05, Z: 0.05})
	greenMat := NewLambertian(Color{X: 0.12, Y: 0.45, Z: 0.15})
	lightMat := NewDiffuseLight(NewSolidColor(Color{X: 3, Y: 3, Z: 3}))

	// =============================================================================
	// AREA LIGHT
	// =============================================================================
	areaLight := NewQuad(
		Point3{X: 113, Y: 554, Z: 127},
		Vec3{X: 330, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 305},
		lightMat,
	)
	world.Add(areaLight)

	// =============================================================================
	// WALLS
	// =============================================================================
	// Green wall (right)
	world.Add(NewQuad(
		Point3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		greenMat,
	))
	// Red wall (left)
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		redMat,
	))
	// White floor
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 0},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: 555},
		whiteMat,
	))
	// White ceiling
	world.Add(NewQuad(
		Point3{X: 555, Y: 555, Z: 555},
		Vec3{X: -555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 0, Z: -555},
		whiteMat,
	))
	// White back wall
	world.Add(NewQuad(
		Point3{X: 0, Y: 0, Z: 555},
		Vec3{X: 555, Y: 0, Z: 0},
		Vec3{X: 0, Y: 555, Z: 0},
		whiteMat,
	))

	// =============================================================================
	// SMOKE BOXES
	// =============================================================================
	// Tall box with black smoke
	box1 := Box(
		Point3{X: 0, Y: 0, Z: 0},
		Point3{X: 165, Y: 330, Z: 165},
		whiteMat,
	)
	box1Xform := NewTransform().
		SetRotationY(15).
		SetPosition(Vec3{X: 265, Y: 0, Z: 295}).
		Apply(box1)
	world.Add(NewVolumeFromColor(box1Xform, 0.01, Color{X: 0, Y: 0, Z: 0}))

	// Short box with white smoke
	box2 := Box(
		Point3{X: 0, Y: 0, Z: 0},
		Point3{X: 165, Y: 165, Z: 165},
		whiteMat,
	)
	box2Xform := NewTransform().
		SetRotationY(-18).
		SetPosition(Vec3{X: 130, Y: 0, Z: 65}).
		Apply(box2)
	world.Add(NewVolumeFromColor(box2Xform, 0.01, Color{X: 1, Y: 1, Z: 1}))

	// =============================================================================
	// CAMERA
	// =============================================================================
	camera := NewCameraBuilder().
		SetResolution(600, 1.0).
		SetQuality(150, 5).
		SetPosition(
			Point3{X: 278, Y: 278, Z: -800},
			Point3{X: 278, Y: 278, Z: 0},
			Vec3{X: 0, Y: 1, Z: 0},
		).
		SetLens(40, 0, 10).
		SetBackground(Color{X: 0, Y: 0, Z: 0}).
		AddLight(areaLight).
		Build()

	return world, camera
}
