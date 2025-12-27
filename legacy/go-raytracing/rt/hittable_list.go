package rt

type HittableList struct {
	Objects []Hittable
	bbox    AABB
}

func NewHittableList() *HittableList {
	return &HittableList{
		Objects: make([]Hittable, 0),
		bbox:    NewAABB(),
	}
}

// Add adds a hittable object to the list
func (hl *HittableList) Add(object Hittable) {
	hl.Objects = append(hl.Objects, object)
	hl.bbox = NewAABBFromBoxes(hl.bbox, object.BoundingBox())
}

func (hl *HittableList) BoundingBox() AABB {
	return hl.bbox
}

// Clear removes all objects from the list
func (hl *HittableList) Clear() {
	hl.Objects = hl.Objects[:0]
}

// Hit implements the Hittable interface for the list
func (hl *HittableList) Hit(r Ray, rayT Interval, rec *HitRecord) bool {
	tempRec := &HitRecord{}
	hitAnything := false
	closestSoFar := rayT.Max

	for _, object := range hl.Objects {
		if object.Hit(r, NewInterval(rayT.Min, closestSoFar), tempRec) {
			hitAnything = true
			closestSoFar = tempRec.T
			*rec = *tempRec
		}
	}

	return hitAnything
}
