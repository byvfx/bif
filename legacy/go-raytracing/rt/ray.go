package rt

type Ray struct {
	orig Point3
	dir  Vec3
	tm   float64
}

func NewRay(origin Point3, direction Vec3, time float64) Ray {
	return Ray{orig: origin, dir: direction, tm: time}
}

func (r Ray) Origin() Point3 {
	return r.orig
}

func (r Ray) Direction() Vec3 {
	return r.dir
}

func (r Ray) At(t float64) Point3 {
	return r.orig.Add(r.dir.Scale(t))
}

func (r Ray) Time() float64 {
	return r.tm
}
