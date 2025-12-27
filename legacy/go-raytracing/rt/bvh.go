package rt

import (
	"runtime"
	"sort"
	"sync"
)

// BVHNode represents a node in a Bounding Volume Hierarchy tree
// RUST PORT NOTE: Use Box<dyn Hittable> for left/right
// or consider enum-based dispatch for better performance:
// enum BVHNode { Leaf(Triangle), Branch { left: Box<BVH>, right: Box<BVH>, bbox: AABB } }
type BVHNode struct {
	left  Hittable
	right Hittable
	bbox  AABB
}

// BVHLeaf holds multiple primitives in a single leaf node
// This reduces tree depth and improves cache locality
type BVHLeaf struct {
	objects []Hittable
	bbox    AABB
}

func (l *BVHLeaf) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	hitAnything := false
	closest := rayT.Max

	for _, obj := range l.objects {
		if obj.Hit(r, NewInterval(rayT.Min, closest), rec) {
			hitAnything = true
			closest = rec.T
		}
	}
	return hitAnything
}

func (l *BVHLeaf) BoundingBox() AABB {
	return l.bbox
}

// bvhPrimitive caches bounding box and centroid for fast sorting
type bvhPrimitive struct {
	index    int
	bbox     AABB
	centroid Vec3
}

// Tuning constants
const (
	bvhParallelThreshold = 8192 // Min objects to spawn goroutines
	bvhLeafMaxSize       = 4    // Max primitives per leaf node
)

// bvhSemaphore limits concurrent goroutines during BVH construction
var bvhSemaphore chan struct{}
var bvhSemaphoreOnce sync.Once

func initBVHSemaphore() {
	bvhSemaphore = make(chan struct{}, runtime.NumCPU())
}

func NewBVHNodeFromList(list *HittableList) *BVHNode {
	objects := list.Objects
	return NewBVHNode(objects, 0, len(objects))
}

func NewBVHNode(objects []Hittable, start, end int) *BVHNode {
	bvhSemaphoreOnce.Do(initBVHSemaphore)

	n := end - start
	if n == 0 {
		return &BVHNode{}
	}

	// Pre-compute all bounding boxes and centroids in parallel
	primitives := make([]bvhPrimitive, n)

	// Parallel bbox computation for large sets
	if n > 10000 {
		numWorkers := runtime.NumCPU()
		chunkSize := (n + numWorkers - 1) / numWorkers
		var wg sync.WaitGroup

		for w := 0; w < numWorkers; w++ {
			wg.Add(1)
			startIdx := w * chunkSize
			endIdx := startIdx + chunkSize
			if endIdx > n {
				endIdx = n
			}
			go func(s, e int) {
				defer wg.Done()
				for i := s; i < e; i++ {
					bbox := objects[start+i].BoundingBox()
					primitives[i] = bvhPrimitive{
						index:    start + i,
						bbox:     bbox,
						centroid: bbox.Centroid(),
					}
				}
			}(startIdx, endIdx)
		}
		wg.Wait()
	} else {
		for i := 0; i < n; i++ {
			bbox := objects[start+i].BoundingBox()
			primitives[i] = bvhPrimitive{
				index:    start + i,
				bbox:     bbox,
				centroid: bbox.Centroid(),
			}
		}
	}

	return buildBVHNode(objects, primitives, runtime.NumCPU())
}

func buildBVHNode(objects []Hittable, primitives []bvhPrimitive, parallelDepth int) *BVHNode {
	n := len(primitives)

	// Compute bounds of all primitives
	bounds := primitives[0].bbox
	centroidBounds := NewAABBFromPoints(primitives[0].centroid, primitives[0].centroid)
	for i := 1; i < n; i++ {
		bounds = NewAABBFromBoxes(bounds, primitives[i].bbox)
		centroidBounds = NewAABBFromBoxes(centroidBounds,
			NewAABBFromPoints(primitives[i].centroid, primitives[i].centroid))
	}

	// Create leaf for small sets
	if n <= bvhLeafMaxSize {
		leaf := &BVHLeaf{
			objects: make([]Hittable, n),
			bbox:    bounds,
		}
		for i, p := range primitives {
			leaf.objects[i] = objects[p.index]
		}
		return &BVHNode{left: leaf, right: leaf, bbox: bounds}
	}

	// Choose split axis based on centroid spread
	axis := centroidBounds.LongestAxis()

	// Sort by centroid on chosen axis (faster than full bbox sort)
	sort.Slice(primitives, func(i, j int) bool {
		switch axis {
		case 0:
			return primitives[i].centroid.X < primitives[j].centroid.X
		case 1:
			return primitives[i].centroid.Y < primitives[j].centroid.Y
		default:
			return primitives[i].centroid.Z < primitives[j].centroid.Z
		}
	})

	mid := n / 2

	node := &BVHNode{bbox: bounds}

	// Parallel construction for large subtrees
	if parallelDepth > 0 && n >= bvhParallelThreshold {
		// Try to acquire semaphore slots BEFORE spawning goroutines
		// This prevents deadlock - if we can't get slots, go sequential
		gotLeft := false
		gotRight := false

		select {
		case bvhSemaphore <- struct{}{}:
			gotLeft = true
		default:
		}

		select {
		case bvhSemaphore <- struct{}{}:
			gotRight = true
		default:
		}

		if gotLeft && gotRight {
			// Both slots acquired - run in parallel
			var wg sync.WaitGroup
			wg.Add(2)

			go func() {
				defer wg.Done()
				defer func() { <-bvhSemaphore }()
				node.left = buildBVHNode(objects, primitives[:mid], parallelDepth-1)
			}()

			go func() {
				defer wg.Done()
				defer func() { <-bvhSemaphore }()
				node.right = buildBVHNode(objects, primitives[mid:], parallelDepth-1)
			}()

			wg.Wait()
		} else {
			// Release any acquired slots and run sequentially
			if gotLeft {
				<-bvhSemaphore
			}
			if gotRight {
				<-bvhSemaphore
			}
			node.left = buildBVHNode(objects, primitives[:mid], 0)
			node.right = buildBVHNode(objects, primitives[mid:], 0)
		}
	} else {
		node.left = buildBVHNode(objects, primitives[:mid], 0)
		node.right = buildBVHNode(objects, primitives[mid:], 0)
	}

	return node
}

func (b *BVHNode) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	GlobalRenderStats.BVHIntersections.Add(1)

	// First check if ray hits this node's bounding box
	if !b.bbox.Hit(r, rayT) {
		return false
	}

	// Check left child
	hitLeft := b.left.Hit(r, rayT, rec)

	// Check right child (only up to closest hit so far)
	// Avoid closure allocation by using simple conditional
	rightMax := rayT.Max
	if hitLeft {
		rightMax = rec.T
	}
	hitRight := b.right.Hit(r, Interval{Min: rayT.Min, Max: rightMax}, rec)

	return hitLeft || hitRight
}

// BoundingBox returns the bounding box for this BVH node
// C++: aabb bounding_box() const override { return bbox; }
func (b *BVHNode) BoundingBox() AABB {
	return b.bbox
}

// boxCompare returns a comparison function for sorting objects along a given axis
func boxCompare(axis int) func(a, b Hittable) bool {
	return func(a, b Hittable) bool {
		aInterval := a.BoundingBox().AxisInterval(axis)
		bInterval := b.BoundingBox().AxisInterval(axis)
		return aInterval.Min < bInterval.Min
	}
}
