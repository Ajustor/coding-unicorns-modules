// Package main is a sample.
package main

import (
	"fmt"
	"strings"
)

type Shape interface {
	Area() float64
}

type Rect struct {
	W, H float64 `json:"w"`
}

func (r Rect) Area() float64 { return r.W * r.H }

func main() {
	shapes := []Shape{Rect{W: 2, H: 3}}
	total := 0.0
	for _, s := range shapes {
		total += s.Area()
	}
	ch := make(chan int, 1)
	go func() { ch <- 42 }()
	defer close(ch)
	fmt.Println(strings.ToUpper("total"), total, <-ch, nil, true, 'x', 0x1F)
}
