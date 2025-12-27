package rt

import "math"

//TODO add in a unifed noise function for textures and displacements similar to Maxon's noise implementation
//TODO add options for different noise types (Perlin, Simplex, Worley, etc.)
//TODO add fractal noise options (fBm, Turbulence, etc.)
type Perlin struct {
	randvec [256]Vec3
	permX   [256]int
	permY   [256]int
	permZ   [256]int
}

func NewPerlin() *Perlin {
	p := &Perlin{}

	for i := range 256 {
		p.randvec[i] = RandomVec3Range(-1, 1).Unit()
	}

	perlinGeneratePerm(&p.permX)
	perlinGeneratePerm(&p.permY)
	perlinGeneratePerm(&p.permZ)

	return p
}

func (p *Perlin) Noise(pt Point3) float64 {
	u := pt.X - math.Floor(pt.X)
	v := pt.Y - math.Floor(pt.Y)
	w := pt.Z - math.Floor(pt.Z)

	i := int(math.Floor(pt.X))
	j := int(math.Floor(pt.Y))
	k := int(math.Floor(pt.Z))

	var c [2][2][2]Vec3

	for di := range 2 {
		for dj := range 2 {
			for dk := range 2 {
				c[di][dj][dk] = p.randvec[p.permX[(i+di)&255]^
					p.permY[(j+dj)&255]^
					p.permZ[(k+dk)&255]]
			}
		}
	}

	return perlinInterp(c, u, v, w)
}

func (p *Perlin) Turb(pt Point3, depth int) float64 {
	accum := 0.0
	tempPt := pt
	weight := 1.0
	for i := 0; i < depth; i++ {
		accum += weight * p.Noise(tempPt)
		weight *= 0.5
		tempPt = tempPt.Scale(2)
	}
	return math.Abs(accum)
}

func perlinGeneratePerm(p *[256]int) {
	for i := range 256 {
		p[i] = i
	}
	permute(p, 256)
}

func permute(p *[256]int, n int) {
	for i := n - 1; i > 0; i-- {
		target := RandomInt(0, i)
		p[i], p[target] = p[target], p[i]
	}
}
func perlinInterp(c [2][2][2]Vec3, u, v, w float64) float64 {
	accum := 0.0
	for i := range 2 {
		for j := range 2 {
			for dk := range 2 {
				weightV := Vec3{u - float64(i), v - float64(j), w - float64(dk)}
				accum += (float64(i)*u + (1-float64(i))*(1-u)) *
					(float64(j)*v + (1-float64(j))*(1-v)) *
					(float64(dk)*w + (1-float64(dk))*(1-w)) *
					Dot(c[i][j][dk], weightV)
			}
		}
	}
	return accum
}
