package tcp

import "testing"

func TestRouterBuilderStoresRoutes(t *testing.T) {
	r := NewRouter().AddUnit(1, 0).AddRange(2, 10, 1)
	if len(r.units) != 1 || r.units[0].unit != 1 || r.units[0].channel != 0 {
		t.Fatalf("unit routes = %+v", r.units)
	}
	if len(r.ranges) != 1 || r.ranges[0].min != 2 || r.ranges[0].max != 10 || r.ranges[0].channel != 1 {
		t.Fatalf("range routes = %+v", r.ranges)
	}
}
