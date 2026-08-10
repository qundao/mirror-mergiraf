type Bicycle struct {
	weight float64
	speed  float64
}

type Car struct {
	turbulence float64
}

func (c Car) Cost() float64 {
	return 37.82
}
