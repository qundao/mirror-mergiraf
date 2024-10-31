type Bicycle struct {
	weight float64
	speed  float64
}

func (r Bicycle) Cost() float64 {
	return 1.34
}

type Car struct {
	turbulence float64
}

func (c Car) Cost() float64 {
	return 37.82
}
