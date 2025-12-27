package rt

import (
	"bufio"
	"fmt"
	"os"
	"strconv"
	"strings"
)

// LoadOBJ loads a Wavefront OBJ file and returns a BVH of the triangles
// RUST PORT NOTE: Consider using the 'obj' crate or 'tobj' for parsing
// Returns a pre-built BVH (not a flat list) for optimal performance
// with large meshes (hundreds of thousands of triangles)
func LoadOBJ(filename string, material Material) (Hittable, error) {
	file, err := os.Open(filename)
	if err != nil {
		return nil, fmt.Errorf("failed to open OBJ file: %w", err)
	}
	defer file.Close()

	var vertices []Point3
	var triangles []Hittable

	scanner := bufio.NewScanner(file)
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := strings.TrimSpace(scanner.Text())

		// Skip empty lines and comments
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		parts := strings.Fields(line)
		if len(parts) == 0 {
			continue
		}

		switch parts[0] {
		case "v":
			// Vertex position
			if len(parts) < 4 {
				return nil, fmt.Errorf("invalid vertex at line %d", lineNum)
			}
			x, err1 := strconv.ParseFloat(parts[1], 64)
			y, err2 := strconv.ParseFloat(parts[2], 64)
			z, err3 := strconv.ParseFloat(parts[3], 64)
			if err1 != nil || err2 != nil || err3 != nil {
				return nil, fmt.Errorf("invalid vertex coordinates at line %d", lineNum)
			}
			vertices = append(vertices, Point3{X: x, Y: y, Z: z})

		case "f":
			// Face - only process triangles
			if len(parts) < 4 {
				continue
			}

			// Parse vertex indices (handle f v1 v2 v3 or f v1/vt1/vn1 v2/vt2/vn2 v3/vt3/vn3)
			indices := make([]int, 0, len(parts)-1)
			for i := 1; i < len(parts); i++ {
				indexStr := strings.Split(parts[i], "/")[0] // Get vertex index (ignore texture/normal)
				idx, err := strconv.Atoi(indexStr)
				if err != nil {
					return nil, fmt.Errorf("invalid face index at line %d", lineNum)
				}
				// OBJ indices are 1-based
				if idx < 0 {
					// Negative indices count from the end
					idx = len(vertices) + idx + 1
				}
				indices = append(indices, idx-1) // Convert to 0-based
			}

			// Triangulate if needed (for quads or n-gons)
			for i := 1; i < len(indices)-1; i++ {
				idx0 := indices[0]
				idx1 := indices[i]
				idx2 := indices[i+1]

				// Validate indices
				if idx0 < 0 || idx0 >= len(vertices) ||
					idx1 < 0 || idx1 >= len(vertices) ||
					idx2 < 0 || idx2 >= len(vertices) {
					return nil, fmt.Errorf("vertex index out of bounds at line %d", lineNum)
				}

				v0 := vertices[idx0]
				v1 := vertices[idx1]
				v2 := vertices[idx2]

				triangle := NewTriangle(v0, v1, v2, material)
				triangles = append(triangles, triangle)
			}
		}
	}

	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("error reading OBJ file: %w", err)
	}

	fmt.Printf("Loaded OBJ: %d vertices, %d triangles\n", len(vertices), len(triangles))

	// Build BVH for the mesh
	fmt.Printf("Building BVH for mesh...\n")
	meshBVH := NewBVHNode(triangles, 0, len(triangles))
	fmt.Printf("BVH built successfully\n")

	return meshBVH, nil
}

// LoadOBJWithTransform loads an OBJ file and applies a transform
func LoadOBJWithTransform(filename string, material Material, transform *Transform) (Hittable, error) {
	mesh, err := LoadOBJ(filename, material)
	if err != nil {
		return nil, err
	}

	if transform != nil {
		return transform.Apply(mesh), nil
	}

	return mesh, nil
}
